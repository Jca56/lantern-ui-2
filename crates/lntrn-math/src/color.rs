//! RGBA color with `f64` channels in `[0, 1]`. Values are **sRGB-encoded**
//! unless a name says `linear`; the GPU side decides how to interpret them.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

    #[inline]
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// `0xRRGGBB`, opaque.
    #[inline]
    pub const fn hex(rgb: u32) -> Self {
        Self::from_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }

    /// `0xRRGGBBAA`.
    #[inline]
    pub const fn hexa(rgba: u32) -> Self {
        Self::from_u8((rgba >> 24) as u8, (rgba >> 16) as u8, (rgba >> 8) as u8, rgba as u8)
    }

    #[inline]
    pub const fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, a as f64 / 255.0)
    }

    #[inline]
    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    #[inline]
    pub const fn with_alpha(self, a: f64) -> Self {
        Self { a, ..self }
    }

    /// Scale alpha by `f`.
    #[inline]
    pub fn fade(self, f: f64) -> Self {
        self.with_alpha(self.a * f)
    }

    /// Channel-wise interpolation, not clamped.
    pub fn lerp(self, o: Color, t: f64) -> Color {
        Color::rgba(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }

    /// Brighten (`f > 1`) or darken (`f < 1`) the RGB channels, clamped.
    pub fn scale_rgb(self, f: f64) -> Color {
        Color::rgba(
            (self.r * f).clamp(0.0, 1.0),
            (self.g * f).clamp(0.0, 1.0),
            (self.b * f).clamp(0.0, 1.0),
            self.a,
        )
    }

    pub fn srgb_to_linear(c: f64) -> f64 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    pub fn linear_to_srgb(c: f64) -> f64 {
        if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }

    /// Decode sRGB channels to linear light. Alpha is untouched.
    pub fn to_linear(self) -> Color {
        Color::rgba(
            Self::srgb_to_linear(self.r),
            Self::srgb_to_linear(self.g),
            Self::srgb_to_linear(self.b),
            self.a,
        )
    }

    /// Encode linear-light channels to sRGB. Alpha is untouched.
    pub fn to_srgb(self) -> Color {
        Color::rgba(
            Self::linear_to_srgb(self.r),
            Self::linear_to_srgb(self.g),
            Self::linear_to_srgb(self.b),
            self.a,
        )
    }

    /// RGB multiplied by alpha (what a premultiplied blend state wants).
    pub fn premultiplied(self) -> Color {
        Color::rgba(self.r * self.a, self.g * self.a, self.b * self.a, self.a)
    }

    /// Relative luminance of the *linear* channels (Rec. 709 weights).
    pub fn luminance_linear(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    #[inline]
    pub fn to_gpu(self) -> [f32; 4] {
        [self.r as f32, self.g as f32, self.b as f32, self.a as f32]
    }

    /// From hue (`0..1` around the wheel, red at 0), saturation and value,
    /// opaque.
    pub fn from_hsv(h: f64, s: f64, v: f64) -> Color {
        let h6 = h.rem_euclid(1.0) * 6.0;
        let i = h6.floor();
        let f = h6 - i;
        let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match i as i32 % 6 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Color::rgb(r, g, b)
    }

    /// Hue (`0..1`), saturation and value. Hue is 0 for greys.
    pub fn to_hsv(self) -> [f64; 3] {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let d = max - min;
        let s = if max > 0.0 { d / max } else { 0.0 };
        let h = if d <= 0.0 {
            0.0
        } else if max == self.r {
            ((self.g - self.b) / d).rem_euclid(6.0)
        } else if max == self.g {
            (self.b - self.r) / d + 2.0
        } else {
            (self.r - self.g) / d + 4.0
        } / 6.0;
        [h, s, max]
    }

    /// `#RRGGBB`, or `#RRGGBBAA` when not opaque.
    pub fn to_hex_string(self) -> String {
        let [r, g, b, a] = self.to_u8();
        if a == 255 { format!("#{r:02X}{g:02X}{b:02X}") } else { format!("#{r:02X}{g:02X}{b:02X}{a:02X}") }
    }

    /// Parse `#RGB`, `#RRGGBB` or `#RRGGBBAA` (the `#` is optional).
    pub fn parse_hex(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
        match s.len() {
            3 => {
                let n = |i: usize| u8::from_str_radix(&s[i..i + 1], 16).ok().map(|v| v * 17);
                Some(Color::from_u8(n(0)?, n(1)?, n(2)?, 255))
            }
            6 => Some(Color::from_u8(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Color::from_u8(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    pub fn approx_eq(self, o: Color, eps: f64) -> bool {
        (self.r - o.r).abs() <= eps
            && (self.g - o.g).abs() <= eps
            && (self.b - o.b).abs() <= eps
            && (self.a - o.a).abs() <= eps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_and_hex_round_trip() {
        for (name, c) in [("red", Color::RED), ("teal", Color::hex(0x1E9A8A)), ("grey", Color::rgb(0.5, 0.5, 0.5)), ("amber", Color::hex(0xFFB733)), ("navy", Color::hex(0x0A1F5C))] {
            let [h, s, v] = c.to_hsv();
            let back = Color::from_hsv(h, s, v);
            assert!(back.approx_eq(c, 1e-9), "{name}: {c:?} -> {h} {s} {v} -> {back:?}");
        }
        assert_eq!(Color::RED.to_hsv(), [0.0, 1.0, 1.0]);
        assert_eq!(Color::GREEN.to_hsv()[0], 1.0 / 3.0);
        assert_eq!(Color::rgb(0.3, 0.3, 0.3).to_hsv()[1], 0.0, "grey has no saturation");
        assert!(Color::from_hsv(1.25, 1.0, 1.0).approx_eq(Color::from_hsv(0.25, 1.0, 1.0), 1e-12), "hue wraps");
        assert_eq!(Color::hex(0xFFB733).to_hex_string(), "#FFB733");
        assert_eq!(Color::hexa(0xFFB73380).to_hex_string(), "#FFB73380");
        assert_eq!(Color::parse_hex("#ffb733"), Some(Color::hex(0xFFB733)));
        assert_eq!(Color::parse_hex("FFB733"), Some(Color::hex(0xFFB733)));
        assert_eq!(Color::parse_hex("#f00"), Some(Color::RED));
        assert_eq!(Color::parse_hex("#FFB73380"), Some(Color::hexa(0xFFB73380)));
        assert_eq!(Color::parse_hex("#ggg"), None);
        assert_eq!(Color::parse_hex("#12345"), None);
    }

    #[test]
    fn hex_roundtrip() {
        let c = Color::hex(0x1A2B3C);
        assert_eq!(c.to_u8(), [0x1A, 0x2B, 0x3C, 0xFF]);
        let c = Color::hexa(0x1A2B3C80);
        assert_eq!(c.to_u8(), [0x1A, 0x2B, 0x3C, 0x80]);
        assert_eq!(Color::WHITE.to_u8(), [255; 4]);
        assert_eq!(Color::TRANSPARENT.to_u8(), [0; 4]);
    }

    #[test]
    fn transfer_functions() {
        for v in [0.0, 0.001, 0.04, 0.2, 0.5, 0.9, 1.0] {
            let back = Color::linear_to_srgb(Color::srgb_to_linear(v));
            assert!((back - v).abs() < 1e-12, "{v}");
        }
        // Middle grey: sRGB 0.5 is about 21.4% linear light.
        assert!((Color::srgb_to_linear(0.5) - 0.214041).abs() < 1e-5);
        assert!(Color::rgb(0.5, 0.5, 0.5).to_linear().to_srgb().approx_eq(Color::rgb(0.5, 0.5, 0.5), 1e-12));
    }

    #[test]
    fn blending_helpers() {
        let c = Color::rgba(1.0, 0.5, 0.0, 0.5);
        assert_eq!(c.premultiplied(), Color::rgba(0.5, 0.25, 0.0, 0.5));
        assert_eq!(c.fade(0.5).a, 0.25);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 0.25), Color::rgb(0.25, 0.25, 0.25));
        assert_eq!(Color::rgb(0.5, 0.5, 0.5).scale_rgb(4.0), Color::WHITE);
        assert_eq!(Color::WHITE.luminance_linear(), 1.0);
        assert_eq!(c.to_gpu(), [1.0f32, 0.5, 0.0, 0.5]);
    }
}
