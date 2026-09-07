//! A small SVG renderer for icons: paths (every command), circles,
//! ellipses, rects, polygons, groups with inherited fill, opacity and
//! transforms, both fill rules, the `viewBox`. A gradient fill takes its
//! first stop's color. Strokes, masks, clips and text are not drawn.
//! Everything is our own: no external crates.

mod geom;
mod path;
mod raster;
mod xml;

use geom::{Affine, P};
use lntrn_image::Image;
use raster::{Canvas, FillRule};
use xml::Element;

/// What a shape inherits from the groups above it.
#[derive(Clone, Copy)]
struct Style {
    fill: Option<[u8; 4]>,
    opacity: f32,
    rule: FillRule,
    transform: Affine,
}

/// The color an attribute names, or `None` for `none`. `currentColor`
/// is black, as an icon with no context would show it.
fn color_of(s: &str, gradients: &[(String, [u8; 4])]) -> Option<[u8; 4]> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if let Some(rest) = s.strip_prefix("url(") {
        let id = rest.trim_end_matches(')').trim().trim_start_matches('#').trim_matches(['"', '\'']);
        return gradients.iter().find(|(k, _)| k == id).map(|(_, c)| *c).or(Some([128, 128, 128, 255]));
    }
    if let Some(hex) = s.strip_prefix('#') {
        let v = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 | 4 => {
                let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|x| x * 17);
                Some([d(0)?, d(1)?, d(2)?, if hex.len() == 4 { d(3)? } else { 255 }])
            }
            6 => Some([v(0)?, v(2)?, v(4)?, 255]),
            8 => Some([v(0)?, v(2)?, v(4)?, v(6)?]),
            _ => None,
        };
    }
    if let Some(rest) = s.strip_prefix("rgb").and_then(|r| r.trim_start_matches('a').strip_prefix('(')) {
        let parts: Vec<&str> = rest.trim_end_matches(')').split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
        let ch = |i: usize| -> Option<u8> {
            let p = parts.get(i)?.trim();
            if let Some(pct) = p.strip_suffix('%') { pct.parse::<f32>().ok().map(|v| (v * 2.55).round() as u8) } else { p.parse::<f32>().ok().map(|v| v.round() as u8) }
        };
        let a = parts.get(3).and_then(|p| p.parse::<f32>().ok()).map_or(255, |a| (a.clamp(0.0, 1.0) * 255.0) as u8);
        return Some([ch(0)?, ch(1)?, ch(2)?, a]);
    }
    let named = match s.to_ascii_lowercase().as_str() {
        "black" | "currentcolor" => [0, 0, 0, 255],
        "white" => [255, 255, 255, 255],
        "red" => [255, 0, 0, 255],
        "green" => [0, 128, 0, 255],
        "blue" => [0, 0, 255, 255],
        "yellow" => [255, 255, 0, 255],
        "gray" | "grey" => [128, 128, 128, 255],
        "orange" => [255, 165, 0, 255],
        _ => return None,
    };
    Some(named)
}

/// An attribute, or the same property from the `style` attribute.
fn prop(el: &Element, name: &str) -> Option<String> {
    if let Some(style) = el.attr("style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':')
                && k.trim() == name
            {
                return Some(v.trim().to_owned());
            }
        }
    }
    el.attr(name).map(str::to_owned)
}

fn number(s: &str) -> Option<f32> {
    s.trim().trim_end_matches("px").parse().ok()
}

/// Gradients by id, each reduced to its first stop's color.
fn gradients(root: &Element) -> Vec<(String, [u8; 4])> {
    let mut all = Vec::new();
    root.walk(&mut all);
    let mut out = Vec::new();
    for g in all.iter().filter(|e| e.name == "linearGradient" || e.name == "radialGradient") {
        let Some(id) = g.attr("id") else { continue };
        let mut stops = Vec::new();
        g.walk(&mut stops);
        let color = stops.iter().filter(|e| e.name == "stop").find_map(|st| prop(st, "stop-color").and_then(|c| color_of(&c, &[])));
        if let Some(c) = color {
            out.push((id.to_owned(), c));
        }
    }
    out
}

/// Draw `el` and what is under it.
fn draw(el: &Element, parent: Style, canvas: &mut Canvas, grads: &[(String, [u8; 4])]) {
    // Definitions, masks and clips are not shapes.
    if matches!(el.name.as_str(), "defs" | "mask" | "clipPath" | "linearGradient" | "radialGradient" | "symbol" | "title" | "desc" | "metadata" | "text") {
        return;
    }
    let mut st = parent;
    if let Some(f) = prop(el, "fill") {
        st.fill = color_of(&f, grads);
    }
    if let Some(o) = prop(el, "opacity").and_then(|v| number(&v)) {
        st.opacity *= o.clamp(0.0, 1.0);
    }
    if let Some(o) = prop(el, "fill-opacity").and_then(|v| number(&v)) {
        st.opacity *= o.clamp(0.0, 1.0);
    }
    if let Some(r) = prop(el, "fill-rule") {
        st.rule = if r.trim() == "evenodd" { FillRule::EvenOdd } else { FillRule::NonZero };
    }
    if let Some(t) = el.attr("transform") {
        st.transform = Affine::parse(t).then(&parent.transform);
    }
    let polys: Vec<Vec<P>> = match el.name.as_str() {
        "path" => el.attr("d").map(|d| path::flatten(d, &st.transform)).unwrap_or_default(),
        "rect" => {
            let (x, y) = (el.attr("x").and_then(number).unwrap_or(0.0), el.attr("y").and_then(number).unwrap_or(0.0));
            let (w, h) = (el.attr("width").and_then(number).unwrap_or(0.0), el.attr("height").and_then(number).unwrap_or(0.0));
            let rx = el.attr("rx").and_then(number).or_else(|| el.attr("ry").and_then(number)).unwrap_or(0.0).min(w * 0.5).min(h * 0.5);
            let d = if rx > 0.0 {
                format!("M{} {}h{}a{rx} {rx} 0 0 1 {rx} {rx}v{}a{rx} {rx} 0 0 1 -{rx} {rx}h-{}a{rx} {rx} 0 0 1 -{rx} -{rx}v-{}a{rx} {rx} 0 0 1 {rx} -{rx}z", x + rx, y, w - 2.0 * rx, h - 2.0 * rx, w - 2.0 * rx, h - 2.0 * rx)
            } else {
                format!("M{x} {y}h{w}v{h}h-{w}z")
            };
            path::flatten(&d, &st.transform)
        }
        "circle" | "ellipse" => {
            let (cx, cy) = (el.attr("cx").and_then(number).unwrap_or(0.0), el.attr("cy").and_then(number).unwrap_or(0.0));
            let r = el.attr("r").and_then(number);
            let (rx, ry) = (el.attr("rx").and_then(number).or(r).unwrap_or(0.0), el.attr("ry").and_then(number).or(r).unwrap_or(0.0));
            let d = format!("M{} {cy}a{rx} {ry} 0 1 0 {} 0a{rx} {ry} 0 1 0 -{} 0z", cx - rx, 2.0 * rx, 2.0 * rx);
            path::flatten(&d, &st.transform)
        }
        "polygon" | "polyline" => {
            let pts = el.attr("points").unwrap_or("");
            path::flatten(&format!("M{pts}z"), &st.transform)
        }
        _ => Vec::new(),
    };
    if !polys.is_empty()
        && let Some(c) = st.fill
    {
        canvas.fill(&polys, st.rule, c, st.opacity);
    }
    for child in &el.children {
        draw(child, st, canvas, grads);
    }
}

/// Render `svg` into a square of `size` pixels, the drawing fitted and
/// centered. `None` when it is not an SVG at all.
pub fn render(svg: &str, size: u32) -> Option<Image> {
    let root = xml::parse(svg)?;
    if root.name != "svg" {
        return None;
    }
    let vb: Vec<f32> = root.attr("viewBox").map(|v| v.split([' ', ',']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect()).unwrap_or_default();
    let (vx, vy, vw, vh) = if vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0 {
        (vb[0], vb[1], vb[2], vb[3])
    } else {
        let w = root.attr("width").and_then(number).unwrap_or(16.0);
        let h = root.attr("height").and_then(number).unwrap_or(w);
        (0.0, 0.0, w.max(1.0), h.max(1.0))
    };
    let s = size as f32 / vw.max(vh);
    let ox = (size as f32 - vw * s) * 0.5;
    let oy = (size as f32 - vh * s) * 0.5;
    let fit = Affine::translate(-vx, -vy).then(&Affine::scale(s, s)).then(&Affine::translate(ox, oy));
    let grads = gradients(&root);
    let mut canvas = Canvas::new(size as usize, size as usize);
    let base = Style { fill: Some([0, 0, 0, 255]), opacity: 1.0, rule: FillRule::NonZero, transform: fit };
    for child in &root.children {
        draw(child, base, &mut canvas, &grads);
    }
    Some(Image::new(size, size, canvas.rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_sum(img: &Image) -> u64 {
        img.rgba.chunks(4).map(|p| p[3] as u64).sum()
    }

    #[test]
    fn renders_shapes_with_inherited_style() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><g fill="#ff0000" opacity="0.5"><rect x="2" y="2" width="12" height="12"/></g><circle cx="8" cy="8" r="2" fill="none"/></svg>"##;
        let img = render(svg, 32).unwrap();
        assert_eq!((img.width, img.height), (32, 32));
        let px = img.pixel(16, 16);
        assert_eq!((px[0], px[1], px[2]), (255, 0, 0));
        assert!((120..=136).contains(&px[3]), "group opacity: {}", px[3]);
        assert_eq!(img.pixel(1, 1)[3], 0);
        assert!(render("<html/>", 8).is_none());
    }

    #[test]
    fn fits_the_viewbox_and_takes_gradient_stops() {
        let svg = r##"<svg viewBox="0 0 512 512"><defs><linearGradient id="g"><stop offset="0" stop-color="#00ff00"/></linearGradient></defs><path fill="url(#g)" d="M0 0H512V512H0z"/></svg>"##;
        let img = render(svg, 16).unwrap();
        assert_eq!(alpha_sum(&img), 16 * 16 * 255, "the box fills every pixel");
        assert_eq!(img.pixel(3, 3)[1], 255);
    }

    /// `LNTRN_SVG_DIR=/path cargo test -- --ignored`: every icon in a
    /// folder renders to something.
    #[test]
    #[ignore]
    fn renders_a_folder_of_icons() {
        let dir = std::env::var("LNTRN_SVG_DIR").expect("LNTRN_SVG_DIR");
        let (mut n, mut blank, mut failed) = (0, Vec::new(), Vec::new());
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "svg") {
                continue;
            }
            n += 1;
            let text = std::fs::read_to_string(&p).unwrap();
            match render(&text, 32) {
                Some(img) if alpha_sum(&img) > 0 => {}
                Some(_) => blank.push(p.file_name().unwrap().to_string_lossy().into_owned()),
                None => failed.push(p.file_name().unwrap().to_string_lossy().into_owned()),
            }
        }
        eprintln!("{n} icons: {} blank, {} failed", blank.len(), failed.len());
        eprintln!("blank: {:?}", &blank[..blank.len().min(40)]);
        eprintln!("failed: {:?}", &failed[..failed.len().min(20)]);
        assert!(failed.is_empty(), "every file parses");
        assert!(blank.len() * 20 < n, "fewer than 5% blank");
        // `LNTRN_SVG_SHEET=out.png`: the first 160 icons on one sheet, to look at.
        if let Ok(out) = std::env::var("LNTRN_SVG_SHEET") {
            let (cell, cols, size) = (40u32, 16u32, 32u32);
            let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
            names.sort();
            let rows = 10;
            let mut sheet = Image::solid(cols * cell, rows * cell, [40, 40, 44, 255]);
            for (k, p) in names.iter().take((cols * rows) as usize).enumerate() {
                let Some(img) = render(&std::fs::read_to_string(p).unwrap(), size) else { continue };
                let (ox, oy) = ((k as u32 % cols) * cell + 4, (k as u32 / cols) * cell + 4);
                for y in 0..size {
                    for x in 0..size {
                        let s = img.pixel(x, y);
                        let a = s[3] as u32;
                        let i = (((oy + y) * sheet.width + ox + x) * 4) as usize;
                        for c in 0..3 {
                            sheet.rgba[i + c] = ((s[c] as u32 * a + sheet.rgba[i + c] as u32 * (255 - a)) / 255) as u8;
                        }
                    }
                }
            }
            std::fs::write(out, lntrn_image::encode_png(&sheet)).unwrap();
        }
    }
}
