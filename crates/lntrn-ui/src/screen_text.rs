//! The area tree as text, for saving between runs: a leaf is `[name]`, a
//! split is `(h ratio first second)` or `(v ...)`.

use crate::screen::{Area, Axis, Node, NodeId, Screen};

impl<E: Copy + PartialEq, S: Default> Screen<E, S> {
    /// The tree as one line of text, for saving: a leaf is `[name]`, a
    /// split is `(h ratio first second)` or `(v ...)`; `name` comes from
    /// `name(editor)` with any `]` dropped.
    pub fn describe(&self, name: impl Fn(E) -> String) -> String {
        let mut out = String::new();
        self.describe_node(self.root, &name, &mut out);
        out
    }

    fn describe_node(&self, node: NodeId, name: &dyn Fn(E) -> String, out: &mut String) {
        match &self.nodes[node] {
            Some(Node::Leaf(area)) => {
                let n = self.area(*area).map(|a| name(a.editor)).unwrap_or_default();
                out.push('[');
                out.extend(n.chars().filter(|&c| c != ']' && c != '['));
                out.push(']');
            }
            Some(Node::Split { axis, ratio, children }) => {
                out.push_str(if *axis == Axis::Horizontal { "(h " } else { "(v " });
                out.push_str(&format!("{ratio:.4} "));
                self.describe_node(children[0], name, out);
                out.push(' ');
                self.describe_node(children[1], name, out);
                out.push(')');
            }
            None => out.push_str("[]"),
        }
    }

    /// A tree from [`Self::describe`]'s text; `editor(name)` maps names
    /// back (returning `None` for a name rejects the whole layout).
    pub fn from_description(text: &str, editor: impl Fn(&str) -> Option<E>) -> Option<Self> {
        let mut s = Self { nodes: Vec::new(), areas: Vec::new(), root: 0, active: None, maximized: None, layouts: Vec::new(), separators: Vec::new(), node_rects: Vec::new() };
        let mut chars = text.chars().peekable();
        let root = s.parse_node(&mut chars, &editor)?;
        skip_ws(&mut chars);
        if chars.next().is_some() {
            return None; // trailing junk
        }
        s.root = root;
        s.active = s.areas.iter().position(Option::is_some);
        Some(s)
    }

    fn parse_node(&mut self, chars: &mut std::iter::Peekable<std::str::Chars<'_>>, editor: &dyn Fn(&str) -> Option<E>) -> Option<NodeId> {
        skip_ws(chars);
        match chars.next()? {
            '[' => {
                let mut name = String::new();
                loop {
                    match chars.next()? {
                        ']' => break,
                        c => name.push(c),
                    }
                }
                let e = editor(&name)?;
                let area = self.alloc_area(Area { editor: e, state: S::default() });
                Some(self.alloc_node(Node::Leaf(area)))
            }
            '(' => {
                skip_ws(chars);
                let axis = match chars.next()? {
                    'h' => Axis::Horizontal,
                    'v' => Axis::Vertical,
                    _ => return None,
                };
                skip_ws(chars);
                let mut num = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let ratio: f64 = num.parse().ok()?;
                let first = self.parse_node(chars, editor)?;
                let second = self.parse_node(chars, editor)?;
                skip_ws(chars);
                if chars.next()? != ')' {
                    return None;
                }
                Some(self.alloc_node(Node::Split { axis, ratio: ratio.clamp(0.02, 0.98), children: [first, second] }))
            }
            _ => None,
        }
    }
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lntrn_math::Rect;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum K {
        Empty,
        Prefs,
        Gallery,
    }

    fn win() -> Rect {
        Rect::from_xywh(0.0, 0.0, 1000.0, 600.0)
    }

    #[test]
    fn describe_and_restore() {
        let mut s: Screen<K> = Screen::new(K::Gallery);
        let right = s.split(0, Axis::Horizontal, 0.62, K::Prefs).unwrap();
        s.split(right, Axis::Vertical, 0.6, K::Empty).unwrap();
        let name = |k: K| format!("{k:?}");
        let text = s.describe(name);
        assert_eq!(text, "(h 0.6200 [Gallery] (v 0.6000 [Prefs] [Empty]))");
        let back: Screen<K> = Screen::from_description(&text, |n| match n {
            "Gallery" => Some(K::Gallery),
            "Prefs" => Some(K::Prefs),
            "Empty" => Some(K::Empty),
            _ => None,
        })
        .unwrap();
        assert_eq!(back.describe(name), text);
        assert_eq!(back.area_count(), 3);
        assert_eq!(back.active, Some(0));
        let mut b = back;
        b.layout(win(), 45.0, 10.0);
        assert_eq!(b.layouts().len(), 3);
        assert!((b.layout_of(0).unwrap().rect.width() - 614.0).abs() <= 1.0, "62% of (1000 - gap)");
        // Unknown names, junk and truncation are rejected.
        let bad = |t: &str| Screen::<K>::from_description(t, |n| (n == "Prefs").then_some(K::Prefs));
        assert!(bad("(h 0.5 [Prefs] [Nope])").is_none());
        assert!(bad("(h 0.5 [Prefs]").is_none());
        assert!(bad("[Prefs] extra").is_none());
        assert!(bad("").is_none());
        assert!(bad("[Prefs]").is_some());
    }
}
