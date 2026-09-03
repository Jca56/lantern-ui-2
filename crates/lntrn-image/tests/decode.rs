//! Decoder tests against the fixtures in `tests/fixtures/` (regenerate with
//! `make_fixtures.py`). PNG must match the reference pixels exactly; JPEG
//! must land within ±3 per channel of libjpeg-turbo's output (Pillow's
//! decode) with alpha exactly 255. Every fixture is also fed back truncated
//! and corrupted to prove the decoders never panic.

use lntrn_image::{Format, ImageError, decode, inflate::inflate};

macro_rules! fixture {
    ($name:literal) => {
        ($name, include_bytes!(concat!("fixtures/", $name)) as &[u8])
    };
}

/// (name, encoded, zlib-compressed expected RGBA)
macro_rules! case {
    ($enc:literal, $ref:literal) => {
        ($enc, include_bytes!(concat!("fixtures/", $enc)) as &[u8], include_bytes!(concat!("fixtures/", $ref)) as &[u8])
    };
}

const PNGS: &[(&str, &[u8], &[u8])] = &[
    case!("rgb8.png", "rgb8.rgba.z"),
    case!("rgba8.png", "rgba8.rgba.z"),
    case!("rgba8_stored.png", "rgba8_stored.rgba.z"),
    case!("rgba8_adam7.png", "rgba8_adam7.rgba.z"),
    case!("rgba8_pillow.png", "rgba8_pillow.rgba.z"),
    case!("l8.png", "l8.rgba.z"),
    case!("la8.png", "la8.rgba.z"),
    case!("rgb8_trns.png", "rgb8_trns.rgba.z"),
    case!("gray1.png", "gray1.rgba.z"),
    case!("gray2.png", "gray2.rgba.z"),
    case!("gray4.png", "gray4.rgba.z"),
    case!("gray1_adam7.png", "gray1_adam7.rgba.z"),
    case!("p8_trns.png", "p8_trns.rgba.z"),
    case!("p4_trns.png", "p4_trns.rgba.z"),
    case!("p2.png", "p2.rgba.z"),
    case!("rgb16.png", "rgb16.rgba.z"),
    case!("rgb16_trns.png", "rgb16_trns.rgba.z"),
    case!("gray16_trns.png", "gray16_trns.rgba.z"),
    case!("rgba16_adam7.png", "rgba16_adam7.rgba.z"),
    case!("rgba8_2x2.png", "rgba8_2x2.rgba.z"),
    case!("rgba8_2x2_adam7.png", "rgba8_2x2_adam7.rgba.z"),
    case!("rgb8_1x1.png", "rgb8_1x1.rgba.z"),
];

const JPEGS: &[(&str, &[u8], &[u8])] = &[
    case!("base_444_q95.jpg", "base_444_q95.rgba.z"),
    case!("base_422_q95.jpg", "base_422_q95.rgba.z"),
    case!("base_420_q95.jpg", "base_420_q95.rgba.z"),
    case!("base_420_q30.jpg", "base_420_q30.rgba.z"),
    case!("gray_q90.jpg", "gray_q90.rgba.z"),
    case!("prog_420_q85.jpg", "prog_420_q85.rgba.z"),
    case!("prog_444_q95.jpg", "prog_444_q95.rgba.z"),
    case!("restart_420_q85.jpg", "restart_420_q85.rgba.z"),
    case!("prog_restart_q85.jpg", "prog_restart_q85.rgba.z"),
    case!("rgb_keep_q95.jpg", "rgb_keep_q95.rgba.z"),
    case!("sample_1x2_q90.jpg", "sample_1x2_q90.rgba.z"),
    case!("tiny_2x2_420.jpg", "tiny_2x2_420.rgba.z"),
    case!("tiny_1x1.jpg", "tiny_1x1.rgba.z"),
];

const UNSUPPORTED: &[(&str, &[u8])] =
    &[fixture!("unsupported_arith.jpg"), fixture!("unsupported_12bit.jpg")];

fn reference(z: &[u8]) -> Vec<u8> {
    inflate(z, 1 << 16).expect("reference inflates")
}

/// Largest per-channel difference, and where it happened.
fn max_diff(name: &str, got: &[u8], want: &[u8], width: u32) -> (i32, String) {
    assert_eq!(got.len(), want.len(), "{name}: pixel count");
    let mut worst = (0i32, String::new());
    for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
        let d = (g as i32 - w as i32).abs();
        if d > worst.0 {
            let (px, ch) = (i / 4, i % 4);
            let (x, y) = (px as u32 % width, px as u32 / width);
            worst = (d, format!("{name}: at ({x},{y}) channel {ch}: got {g}, want {w}"));
        }
    }
    worst
}

#[test]
fn png_fixtures_decode_exactly() {
    for &(name, file, z) in PNGS {
        let img = decode(file).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = reference(z);
        let (d, at) = max_diff(name, &img.rgba, &want, img.width);
        assert_eq!(d, 0, "{at}");
    }
}

#[test]
fn png_transparency_lands_where_expected() {
    let img = decode(PNGS[7].1).unwrap();
    assert_eq!((img.width, img.height), (37, 29));
    assert_eq!(img.pixel(25, 10), [40, 180, 90, 0]); // colour-keyed flat block
    assert_eq!(img.pixel(2, 20)[3], 255);
    let img = decode(PNGS[1].1).unwrap();
    assert_eq!(img.pixel(2, 2)[3], 0); // transparent corner
    assert_eq!(img.pixel(36, 28)[3], 255);
    assert_eq!(img.pixel(18, 20)[3], 0); // alpha ramp starts at zero
}

#[test]
fn jpeg_fixtures_within_tolerance() {
    for &(name, file, z) in JPEGS {
        let img = decode(file).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = reference(z);
        let (d, at) = max_diff(name, &img.rgba, &want, img.width);
        eprintln!("{name}: max diff {d}");
        assert!(d <= 3, "{at} (max diff {d})");
        assert!(img.rgba.chunks_exact(4).all(|p| p[3] == 255), "{name}: alpha");
    }
}

#[test]
fn unsupported_jpegs_are_reported_not_mangled() {
    for &(name, file) in UNSUPPORTED {
        match decode(file) {
            Err(ImageError::Unsupported(_)) => {}
            other => panic!("{name}: expected Unsupported, got {other:?}"),
        }
    }
    assert_eq!(decode(b"nope"), Err(ImageError::UnknownFormat));
}

#[test]
fn sniffs_fixture_formats() {
    for &(_, file, _) in PNGS {
        assert_eq!(Format::sniff(file), Some(Format::Png));
    }
    for &(_, file, _) in JPEGS {
        assert_eq!(Format::sniff(file), Some(Format::Jpeg));
    }
}

/// Feed every fixture back truncated at every length and with bytes
/// flipped; the only acceptable outcomes are `Ok` or `Err`.
#[test]
fn truncated_and_corrupted_files_never_panic() {
    let files = PNGS.iter().chain(JPEGS).map(|&(n, f, _)| (n, f)).chain(UNSUPPORTED.iter().copied());
    let mut seed = 0x9E37_79B9u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for (name, file) in files {
        for cut in 0..file.len() {
            let _ = decode(&file[..cut]);
        }
        let mut copy = file.to_vec();
        for (i, &orig) in file.iter().enumerate() {
            copy[i] ^= 0x55;
            let _ = decode(&copy);
            copy[i] = orig;
        }
        for _ in 0..200 {
            let mut copy = file.to_vec();
            for _ in 0..1 + rnd() % 4 {
                let i = rnd() as usize % copy.len();
                copy[i] = rnd() as u8;
            }
            let cut = copy.len() - (rnd() as usize % copy.len().min(64));
            let _ = decode(&copy[..cut]);
        }
        let _ = name;
    }
}
