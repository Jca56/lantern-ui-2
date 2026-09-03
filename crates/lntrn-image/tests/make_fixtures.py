#!/usr/bin/env python3
"""Regenerate the decoder test fixtures in tests/fixtures/.

Every encoded file gets a sibling `.rgba.z`: the expected RGBA8 pixels, top
row first, zlib-compressed to keep the tree small (the tests inflate them
with prism-image's own inflater). PNGs are written by the tiny encoder below
(Pillow cannot emit Adam7, sub-byte gray or 16-bit RGB) and their expected
pixels come from the source samples; Pillow's own decode is cross-checked
wherever it is trustworthy.
JPEGs come from Pillow (libjpeg-turbo) and cjpeg, with Pillow's decode as
the reference.

Needs: python3, Pillow, cjpeg (libjpeg-turbo). Run from anywhere.
"""

import io
import os
import struct
import subprocess
import zlib

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "fixtures")
W, H = 37, 29
FLAT = (40, 180, 90)


def base_pixel(x, y):
    """Gradient + white diagonal + one flat block + varying alpha."""
    r, g, b = int(255 * x / (W - 1)), int(255 * y / (H - 1)), 128
    if abs(x - y) <= 1:
        r, g, b = 255, 255, 255
    if 20 <= x < 32 and 6 <= y < 16:
        r, g, b = FLAT
    a = 255
    if x >= 18 and y >= 18:
        a = int(255 * (x - 18) / (W - 19))
    if x < 6 and y < 6:
        a = 0
    return r, g, b, a


def base_image(w=W, h=H):
    img = Image.new("RGBA", (w, h))
    img.putdata([base_pixel(x, y) for y in range(h) for x in range(w)])
    return img


def write(name, data):
    with open(os.path.join(OUT, name), "wb") as f:
        f.write(data)


def write_ref(name, rgba):
    write(os.path.splitext(name)[0] + ".rgba.z", zlib.compress(rgba, 9))


# ── PNG encoder ─────────────────────────────────────────────────────────────

CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
ADAM7 = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]


def pack_row(samples, depth):
    if depth == 8:
        return bytes(samples)
    if depth == 16:
        return b"".join(struct.pack(">H", s) for s in samples)
    out, acc, nbits = bytearray(), 0, 0
    for s in samples:
        acc, nbits = (acc << depth) | s, nbits + depth
        if nbits == 8:
            out.append(acc)
            acc, nbits = 0, 0
    if nbits:
        out.append(acc << (8 - nbits))
    return bytes(out)


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def filter_row(cur, prev, bpp, ftype):
    out = bytearray(len(cur))
    for i, v in enumerate(cur):
        left = cur[i - bpp] if i >= bpp else 0
        up = prev[i]
        upleft = prev[i - bpp] if i >= bpp else 0
        pred = [0, left, up, (left + up) // 2, paeth(left, up, upleft)][ftype]
        out[i] = (v - pred) & 0xFF
    return bytes(out)


def encode_rows(rows, depth, channels, mode):
    """Filter + pack the scanlines of one pass. `mode` picks filters."""
    bpp = max(1, channels * depth // 8)
    out = bytearray()
    prev = None
    for y, samples in enumerate(rows):
        cur = pack_row(samples, depth)
        if prev is None:
            prev = bytes(len(cur))
        if mode == "cycle":
            ftype = y % 5
        elif mode == "none":
            ftype = 0
        else:  # adaptive: smallest sum of signed residuals
            cands = [(sum(min(b, 256 - b) for b in filter_row(cur, prev, bpp, f)), f) for f in range(5)]
            ftype = min(cands)[1]
        out.append(ftype)
        out += filter_row(cur, prev, bpp, ftype)
        prev = cur
    return bytes(out)


def chunk(ctype, body):
    crc = zlib.crc32(ctype + body) & 0xFFFFFFFF
    return struct.pack(">I", len(body)) + ctype + body + struct.pack(">I", crc)


def write_png(name, w, h, ctype, depth, rows, palette=None, trns=None, interlace=False, mode="cycle", level=9):
    """`rows[y]` is the list of samples for scanline y (w * channels of them)."""
    channels = CHANNELS[ctype]
    if interlace:
        raw = bytearray()
        for x0, y0, dx, dy in ADAM7:
            sub = []
            for y in range(y0, h, dy):
                row = []
                for x in range(x0, w, dx):
                    row += rows[y][x * channels:(x + 1) * channels]
                sub.append(row)
            if sub and sub[0]:
                raw += encode_rows(sub, depth, channels, mode)
        raw = bytes(raw)
    else:
        raw = encode_rows(rows, depth, channels, mode)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, ctype, 0, 0, 1 if interlace else 0))
    if palette is not None:
        png += chunk(b"PLTE", b"".join(bytes(c) for c in palette))
    if trns is not None:
        png += chunk(b"tRNS", trns)
    png += chunk(b"IDAT", zlib.compress(raw, level))
    png += chunk(b"IEND", b"")
    write(name, png)
    return png


def expected_png(w, h, ctype, depth, rows, palette=None, trns=None):
    """What the PNG spec says those samples mean, as RGBA8."""
    channels = CHANNELS[ctype]

    def to8(v):
        return {1: v * 255, 2: v * 85, 4: v * 17, 8: v, 16: v >> 8}[depth]

    out = bytearray()
    for y in range(h):
        for x in range(w):
            px = rows[y][x * channels:(x + 1) * channels]
            if ctype == 0:
                a = 0 if trns is not None and struct.unpack(">H", trns)[0] == px[0] else 255
                out += bytes([to8(px[0])] * 3 + [a])
            elif ctype == 2:
                key = struct.unpack(">HHH", trns) if trns is not None else None
                a = 0 if key == tuple(px) else 255
                out += bytes([to8(px[0]), to8(px[1]), to8(px[2]), a])
            elif ctype == 3:
                a = trns[px[0]] if trns is not None and px[0] < len(trns) else 255
                out += bytes(palette[px[0]]) + bytes([a])
            elif ctype == 4:
                out += bytes([to8(px[0])] * 3 + [to8(px[1])])
            else:
                out += bytes(to8(v) for v in px)
    return bytes(out)


def png_fixture(name, w, h, ctype, depth, rows, palette=None, trns=None, check_pil=True, **kw):
    png = write_png(name, w, h, ctype, depth, rows, palette, trns, **kw)
    want = expected_png(w, h, ctype, depth, rows, palette, trns)
    write_ref(name, want)
    if check_pil:
        got = Image.open(io.BytesIO(png)).convert("RGBA").tobytes()
        assert got == want, f"Pillow disagrees on {name}"


def rgba_rows(img):
    w, h = img.size
    data = list(img.get_flattened_data())
    return [[v for px in data[y * w:(y + 1) * w] for v in px] for y in range(h)]


def gray(px):
    r, g, b = px[:3]
    return (r * 299 + g * 587 + b * 114 + 500) // 1000


def make_pngs():
    base = base_image()
    w, h = base.size
    rgba = rgba_rows(base)
    rgb = [[v for i, v in enumerate(row) if i % 4 != 3] for row in rgba]
    l8 = [[gray(row[i:i + 4]) for i in range(0, len(row), 4)] for row in rgba]
    la8 = [[v for i in range(0, len(row), 4) for v in (gray(row[i:i + 4]), row[i + 3])] for row in rgba]

    png_fixture("rgb8.png", w, h, 2, 8, rgb, mode="cycle", level=6)
    png_fixture("rgba8.png", w, h, 6, 8, rgba, mode="adaptive", level=9)
    png_fixture("rgba8_stored.png", w, h, 6, 8, rgba, mode="none", level=0)
    png_fixture("rgba8_adam7.png", w, h, 6, 8, rgba, mode="adaptive", interlace=True)
    png_fixture("l8.png", w, h, 0, 8, l8, mode="cycle")
    png_fixture("la8.png", w, h, 4, 8, la8, mode="adaptive")
    png_fixture("rgb8_trns.png", w, h, 2, 8, rgb, trns=struct.pack(">HHH", *FLAT), check_pil=False)

    # Low bit depth gray.
    for depth in (1, 2, 4):
        rows = [[v >> (8 - depth) for v in row] for row in l8]
        png_fixture(f"gray{depth}.png", w, h, 0, depth, rows, mode="cycle")
    rows = [[v >> 7 for v in row] for row in l8]
    png_fixture("gray1_adam7.png", w, h, 0, 1, rows, mode="adaptive", interlace=True)

    # Palette: 16 entries, alpha for the first 10 of them.
    palette = [((i * 37) & 255, (i * 91) & 255, (i * 149) & 255) for i in range(16)]
    trns = bytes(range(0, 250, 25))
    pal_rows = [[((x * 3 + y * 5) // 4) % 16 for x in range(w)] for y in range(h)]
    png_fixture("p8_trns.png", w, h, 3, 8, pal_rows, palette=palette, trns=trns, mode="adaptive")
    png_fixture("p4_trns.png", w, h, 3, 4, pal_rows, palette=palette, trns=trns, mode="cycle")
    png_fixture("p2.png", w, h, 3, 2, [[v % 4 for v in row] for row in pal_rows], palette=palette[:4])

    # 16-bit: high byte is the base value, low byte is noise the decoder
    # must drop. The flat block stays noise-free so a colour key can hit it.
    def wide(y, x, v):
        return v * 257 if 20 <= x < 32 and 6 <= y < 16 else v * 256 + (x * 7 + y * 13) % 256

    rgb16 = [[wide(y, x, v) for x in range(w) for v in rgb[y][x * 3:x * 3 + 3]] for y in range(h)]
    png_fixture("rgb16.png", w, h, 2, 16, rgb16, mode="adaptive")
    key = struct.pack(">HHH", *(v * 257 for v in FLAT))
    png_fixture("rgb16_trns.png", w, h, 2, 16, rgb16, trns=key, check_pil=False)
    l16 = [[wide(y, x, v) for x, v in enumerate(row)] for y, row in enumerate(l8)]
    flat_gray = gray(FLAT) * 257
    png_fixture("gray16_trns.png", w, h, 0, 16, l16, trns=struct.pack(">H", flat_gray), check_pil=False)
    rgba16 = [[wide(y, x, v) for x in range(w) for v in rgba[y][x * 4:x * 4 + 4]] for y in range(h)]
    png_fixture("rgba16_adam7.png", w, h, 6, 16, rgba16, mode="cycle", interlace=True)

    # Tiny pictures: every Adam7 pass but the last is empty at 1×1 / 2×2.
    tiny = [[10, 20, 30, 255, 40, 50, 60, 128], [70, 80, 90, 0, 100, 110, 120, 255]]
    png_fixture("rgba8_2x2.png", 2, 2, 6, 8, tiny)
    png_fixture("rgba8_2x2_adam7.png", 2, 2, 6, 8, tiny, interlace=True)
    png_fixture("rgb8_1x1.png", 1, 1, 2, 8, [[200, 100, 50]])

    # One straight from Pillow's encoder, for a second opinion on filters.
    buf = io.BytesIO()
    base.save(buf, "PNG", optimize=True)
    write("rgba8_pillow.png", buf.getvalue())
    write_ref("rgba8_pillow.png", base.tobytes())


# ── JPEG ────────────────────────────────────────────────────────────────────

def jpeg_ref(name, data):
    write(name, data)
    img = Image.open(io.BytesIO(data))
    img.load()
    write_ref(name, img.convert("RGBA").tobytes())


def pil_jpeg(name, img, **kw):
    buf = io.BytesIO()
    img.save(buf, "JPEG", **kw)
    jpeg_ref(name, buf.getvalue())


def cjpeg(name, img, *args):
    ppm = io.BytesIO()
    img.save(ppm, "PPM")
    data = subprocess.run(["cjpeg", *args], input=ppm.getvalue(), capture_output=True, check=True).stdout
    write(name, data)
    return data


def make_jpegs():
    base = base_image().convert("RGB")
    pil_jpeg("base_444_q95.jpg", base, quality=95, subsampling=0)
    pil_jpeg("base_422_q95.jpg", base, quality=95, subsampling=1)
    pil_jpeg("base_420_q95.jpg", base, quality=95, subsampling=2)
    pil_jpeg("base_420_q30.jpg", base, quality=30, subsampling=2)
    pil_jpeg("gray_q90.jpg", base.convert("L"), quality=90)
    pil_jpeg("prog_420_q85.jpg", base, quality=85, subsampling=2, progressive=True)
    pil_jpeg("prog_444_q95.jpg", base, quality=95, subsampling=0, progressive=True)
    pil_jpeg("restart_420_q85.jpg", base, quality=85, subsampling=2, restart_marker_blocks=1)
    pil_jpeg("prog_restart_q85.jpg", base, quality=85, subsampling=2, progressive=True, restart_marker_rows=1)
    pil_jpeg("rgb_keep_q95.jpg", base, quality=95, subsampling=0, keep_rgb=True)
    pil_jpeg("tiny_2x2_420.jpg", base_image(2, 2).convert("RGB"), quality=90, subsampling=2)
    pil_jpeg("tiny_1x1.jpg", base_image(1, 1).convert("RGB"), quality=90, subsampling=2)
    # 4:4:0 (1×2) only cjpeg will produce.
    jpeg_ref("sample_1x2_q90.jpg", cjpeg("sample_1x2_q90.jpg", base, "-quality", "90", "-sample", "1x2"))
    # Out-of-scope encodings: the decoder must refuse them politely.
    cjpeg("unsupported_arith.jpg", base_image(2, 2).convert("RGB"), "-arithmetic")
    cjpeg("unsupported_12bit.jpg", base_image(2, 2).convert("RGB"), "-precision", "12")


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    for f in os.listdir(OUT):
        os.remove(os.path.join(OUT, f))
    make_pngs()
    make_jpegs()
    total = sum(os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT))
    print(f"{len(os.listdir(OUT))} files, {total} bytes")
