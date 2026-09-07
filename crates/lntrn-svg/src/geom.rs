//! Points and affine transforms, as SVG writes them.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

impl P {
    pub const ZERO: P = P { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// `c` mirrored through this point (the smooth-curve control).
    pub fn reflect(self, c: P) -> P {
        P::new(2.0 * self.x - c.x, 2.0 * self.y - c.y)
    }
}

/// `[a b c d e f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn apply(&self, p: P) -> P {
        let m = self.0;
        P::new(m[0] * p.x + m[2] * p.y + m[4], m[1] * p.x + m[3] * p.y + m[5])
    }

    /// `self` applied first, then `outer`.
    pub fn then(&self, outer: &Affine) -> Affine {
        let (a, o) = (self.0, outer.0);
        Affine([o[0] * a[0] + o[2] * a[1], o[1] * a[0] + o[3] * a[1], o[0] * a[2] + o[2] * a[3], o[1] * a[2] + o[3] * a[3], o[0] * a[4] + o[2] * a[5] + o[4], o[1] * a[4] + o[3] * a[5] + o[5]])
    }

    pub fn translate(x: f32, y: f32) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(x: f32, y: f32) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// An SVG `transform` attribute: a list of matrix, translate, scale,
    /// rotate, skewX and skewY, applied right to left as the spec says.
    pub fn parse(text: &str) -> Affine {
        let mut result = Affine::IDENTITY;
        let mut rest = text.trim();
        while let Some(open) = rest.find('(') {
            let name = rest[..open].trim().trim_start_matches(',').trim();
            let Some(close) = rest[open..].find(')') else { break };
            let args: Vec<f32> = rest[open + 1..open + close].split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
            let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
            let t = match name {
                "matrix" if args.len() >= 6 => Affine([a(0), a(1), a(2), a(3), a(4), a(5)]),
                "translate" => Affine::translate(a(0), a(1)),
                "scale" => Affine::scale(a(0), if args.len() > 1 { a(1) } else { a(0) }),
                "rotate" => {
                    let (s, c) = a(0).to_radians().sin_cos();
                    let r = Affine([c, s, -s, c, 0.0, 0.0]);
                    if args.len() >= 3 { Affine::translate(-a(1), -a(2)).then(&r).then(&Affine::translate(a(1), a(2))) } else { r }
                }
                "skewX" => Affine([1.0, 0.0, a(0).to_radians().tan(), 1.0, 0.0, 0.0]),
                "skewY" => Affine([1.0, a(0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
                _ => Affine::IDENTITY,
            };
            // Listed transforms apply right to left: the next one goes inside.
            result = t.then(&result);
            rest = &rest[open + close + 1..];
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_compose_like_svg() {
        let t = Affine::parse("translate(10,0) scale(2)");
        let p = t.apply(P::new(1.0, 1.0));
        assert_eq!((p.x, p.y), (12.0, 2.0), "scale first, then translate");
        let r = Affine::parse("rotate(90)");
        let p = r.apply(P::new(1.0, 0.0));
        assert!((p.x).abs() < 1e-5 && (p.y - 1.0).abs() < 1e-5);
        let m = Affine::parse("matrix(1 0 0 1 5 6)");
        assert_eq!(m.apply(P::ZERO), P::new(5.0, 6.0));
    }
}
