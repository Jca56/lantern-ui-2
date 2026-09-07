//! Just enough XML for an SVG: elements, attributes (double or single
//! quoted, entities for the usual five), nesting; comments, doctypes and
//! text are skipped.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// Every element under this one, itself included, depth first.
    pub fn walk<'a>(&'a self, out: &mut Vec<&'a Element>) {
        out.push(self);
        for c in &self.children {
            c.walk(out);
        }
    }
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The root element of `text`, if it parses.
pub fn parse(text: &str) -> Option<Element> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    while i < b.len() {
        let Some(lt) = text[i..].find('<') else {
            break;
        };
        i += lt;
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |k| k + 3);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i += rest.find('>').map_or(rest.len(), |k| k + 1);
            continue;
        }
        if rest.starts_with("</") {
            i += rest.find('>').map_or(rest.len(), |k| k + 1);
            if let Some(done) = stack.pop() {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => root = Some(done),
                }
            }
            continue;
        }
        // An opening tag: name, attributes, maybe self-closing.
        let mut j = i + 1;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let mut el = Element { name: text[i + 1..j].to_owned(), ..Default::default() };
        let mut closed = false;
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            if b[j] == b'/' {
                closed = true;
                j += 1;
                continue;
            }
            let k0 = j;
            while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let key = text[k0..j].to_owned();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    let v0 = j + 1;
                    let mut v1 = v0;
                    while v1 < b.len() && b[v1] != q {
                        v1 += 1;
                    }
                    el.attrs.push((key, unescape(&text[v0..v1])));
                    j = (v1 + 1).min(b.len());
                } else {
                    let v0 = j;
                    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                        j += 1;
                    }
                    el.attrs.push((key, text[v0..j].to_owned()));
                }
            } else if !key.is_empty() {
                el.attrs.push((key, String::new()));
            } else {
                j += 1;
            }
        }
        i = j;
        if closed {
            match stack.last_mut() {
                Some(parent) => parent.children.push(el),
                None => root = Some(el),
            }
        } else {
            stack.push(el);
        }
    }
    // Unclosed tags close at the end.
    while let Some(done) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(done),
            None => root = Some(done),
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nests_and_reads_attributes() {
        let e = parse(r#"<?xml version="1.0"?><!-- c --><svg viewBox="0 0 16 16"><g fill='#f00'><path d="M0 0h1"/><circle r="2"></circle></g></svg>"#).unwrap();
        assert_eq!(e.name, "svg");
        assert_eq!(e.attr("viewBox"), Some("0 0 16 16"));
        assert_eq!(e.children.len(), 1);
        let g = &e.children[0];
        assert_eq!(g.attr("fill"), Some("#f00"));
        assert_eq!(g.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["path", "circle"]);
        assert_eq!(g.children[0].attr("d"), Some("M0 0h1"));
        let mut all = Vec::new();
        e.walk(&mut all);
        assert_eq!(all.len(), 4);
    }
}
