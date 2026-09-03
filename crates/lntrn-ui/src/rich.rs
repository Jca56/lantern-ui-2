//! Rich text: a small markdown for help panes, about boxes and dialog
//! bodies, laid out and drawn by [`Ui::rich_text`]. `# Heading` and
//! `## Subheading`, paragraphs parted by blank lines, `- ` bullets,
//! `1. ` numbers, `**bold**`, `*italic*`, `` `code` ``, `[label](target)`
//! links, fenced ``` code blocks, and `---` rules. Anything else is plain
//! text; an unclosed marker stays visible rather than eating what follows.

use lntrn_math::{Rect, Vec2};
use lntrn_text::TextStyle;

use crate::state::CursorIcon;
use crate::ui::{FILL, Sense, Ui};

/// One run of text with one look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    /// Where the run leads when clicked.
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Level 1 or 2.
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    List { ordered: bool, items: Vec<Vec<Span>> },
    Code(String),
    Rule,
}

/// What [`Ui::rich_text`] saw this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichResponse {
    /// A link's target, on the frame it was clicked.
    pub link_clicked: Option<String>,
    pub link_hovered: Option<String>,
    /// Where the text was laid out.
    pub rect: Rect,
}

/// The blocks of `md`.
pub fn parse(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut lines = md.lines();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.starts_with("```") {
            flush(&mut para, &mut blocks);
            let mut code = String::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push_str(l);
                code.push('\n');
            }
            blocks.push(Block::Code(code.trim_end_matches('\n').to_owned()));
        } else if t.is_empty() {
            flush(&mut para, &mut blocks);
        } else if t == "---" || t == "***" {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Rule);
        } else if let Some(rest) = t.strip_prefix("## ") {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Heading(2, inline(rest)));
        } else if let Some(rest) = t.strip_prefix("# ") {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Heading(1, inline(rest)));
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush(&mut para, &mut blocks);
            push_item(&mut blocks, false, inline(rest));
        } else if let Some(rest) = numbered(t) {
            flush(&mut para, &mut blocks);
            push_item(&mut blocks, true, inline(rest));
        } else {
            para.push(t);
        }
    }
    flush(&mut para, &mut blocks);
    blocks
}

fn flush(para: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if !para.is_empty() {
        blocks.push(Block::Paragraph(inline(&para.join(" "))));
        para.clear();
    }
}

fn push_item(blocks: &mut Vec<Block>, ordered: bool, item: Vec<Span>) {
    match blocks.last_mut() {
        Some(Block::List { ordered: o, items }) if *o == ordered => items.push(item),
        _ => blocks.push(Block::List { ordered, items: vec![item] }),
    }
}

/// `3. text` → `text`.
fn numbered(t: &str) -> Option<&str> {
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then(|| t[digits..].strip_prefix(". ")).flatten()
}

/// The runs of one line of markup.
pub fn inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut cur = Span::default();
    let mut buf = String::new();
    let mut i = 0;
    let find = |from: usize, pat: &[char]| chars[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from);
    while i < chars.len() {
        let c = chars[i];
        if c == '`' && let Some(end) = find(i + 1, &['`']) {
            push(&mut spans, &mut buf, &cur);
            spans.push(Span { text: chars[i + 1..end].iter().collect(), code: true, ..Span::default() });
            i = end + 1;
            continue;
        }
        if c == '*' {
            let double = chars.get(i + 1) == Some(&'*');
            let (len, open) = if double { (2, cur.bold) } else { (1, cur.italic) };
            // A marker hugging the text closes the emphasis it opened, or
            // opens one when a closer follows; a star with air on both
            // sides is a star.
            let can_close = i > 0 && !chars[i - 1].is_whitespace();
            let can_open = chars.get(i + len).is_some_and(|c| !c.is_whitespace());
            let closes_later = || (i + len..chars.len()).any(|j| chars[j..].starts_with(&chars[i..i + len]) && !chars[j - 1].is_whitespace());
            if (open && can_close) || (!open && can_open && closes_later()) {
                push(&mut spans, &mut buf, &cur);
                if double {
                    cur.bold = !cur.bold;
                } else {
                    cur.italic = !cur.italic;
                }
                i += len;
                continue;
            }
        }
        if c == '['
            && let Some(close) = find(i + 1, &[']'])
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = find(close + 2, &[')'])
        {
            push(&mut spans, &mut buf, &cur);
            spans.push(Span { text: chars[i + 1..close].iter().collect(), link: Some(chars[close + 2..end].iter().collect()), ..cur.clone() });
            i = end + 1;
            continue;
        }
        buf.push(c);
        i += 1;
    }
    push(&mut spans, &mut buf, &cur);
    spans
}

fn push(spans: &mut Vec<Span>, buf: &mut String, cur: &Span) {
    if !buf.is_empty() {
        spans.push(Span { text: std::mem::take(buf), ..cur.clone() });
    }
}

// ---- layout ------------------------------------------------------------------

/// Where the next word goes.
struct Flow {
    origin: Vec2,
    width: f64,
    x: f64,
    y: f64,
    indent: f64,
    /// Draw and interact; off to measure alone.
    draw: bool,
    links: usize,
    out: RichResponse,
}

impl Ui<'_> {
    /// Lay out and draw `md` (see [`crate::rich`]) across the available
    /// width. Links underline, light up under the pointer, and report a
    /// click.
    pub fn rich_text(&mut self, md: &str) -> RichResponse {
        let blocks = parse(md);
        let width = self.avail_width();
        let (height, _) = render(self, &blocks, Vec2::ZERO, width, false);
        let rect = self.alloc(Vec2::new(FILL, height));
        let (_, mut out) = render(self, &blocks, rect.min, width, true);
        out.rect = rect;
        out
    }

    /// The height `md` takes at `width`, without drawing it.
    pub fn rich_text_height(&mut self, md: &str, width: f64) -> f64 {
        render(self, &parse(md), Vec2::ZERO, width, false).0
    }
}

fn render(ui: &mut Ui, blocks: &[Block], origin: Vec2, width: f64, draw: bool) -> (f64, RichResponse) {
    let m = ui.m;
    let mut f = Flow { origin, width, x: origin.x, y: origin.y, indent: 0.0, draw, links: 0, out: RichResponse::default() };
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            f.y += m.gap;
        }
        match block {
            Block::Heading(level, spans) => {
                let style = if *level == 1 { ui.heading_style() } else { ui.text_style().bold() };
                flow(ui, &mut f, spans, &style);
            }
            Block::Paragraph(spans) => {
                let style = ui.text_style();
                flow(ui, &mut f, spans, &style);
            }
            Block::List { ordered, items } => {
                let style = ui.text_style();
                let indent = m.pad * 3.0;
                for (n, item) in items.iter().enumerate() {
                    if n > 0 {
                        f.y += m.gap * 0.5;
                    }
                    if draw {
                        let marker = if *ordered { format!("{}.", n + 1) } else { "•".to_owned() };
                        ui.text_at(&marker, &style, Vec2::new(origin.x + m.pad, f.y), 1.0e6, ui.theme.text_dim);
                    }
                    f.indent = indent;
                    flow(ui, &mut f, item, &style);
                }
                f.indent = 0.0;
            }
            Block::Code(code) => {
                let style = ui.mono_style();
                let lh = style.line_height() as f64;
                let lines = code.lines().count().max(1);
                let rect = Rect::from_min_size(Vec2::new(origin.x, f.y), Vec2::new(width, lines as f64 * lh + m.pad * 2.0));
                if draw {
                    ui.draw.rounded_rect(rect, m.radius, ui.theme.field);
                    ui.draw.push_clip(rect);
                    for (n, line) in code.lines().enumerate() {
                        ui.text_at(line, &style, Vec2::new(origin.x + m.pad, rect.min.y + m.pad + n as f64 * lh), 1.0e6, ui.theme.text);
                    }
                    ui.draw.pop_clip();
                }
                f.y = rect.max.y;
            }
            Block::Rule => {
                if draw {
                    ui.draw.hline(origin.x, origin.x + width, f.y + m.gap * 0.5, m.border, ui.theme.border_light);
                }
                f.y += m.gap;
            }
        }
    }
    (f.y - origin.y, f.out)
}

/// Words left to right, wrapping at the width, each in its span's look.
fn flow(ui: &mut Ui, f: &mut Flow, spans: &[Span], base: &TextStyle) {
    let lh = base.line_height() as f64;
    let left = f.origin.x + f.indent;
    let right = f.origin.x + f.width;
    f.x = left;
    for span in spans {
        let mut style = base.clone();
        if span.bold {
            style = style.bold();
        }
        if span.italic {
            style = style.italic();
        }
        if span.code {
            style = style.mono();
        }
        let color = if span.link.is_some() { ui.theme.accent } else { ui.theme.text };
        for word in span.text.split_inclusive(' ') {
            let shown = word.trim_end();
            let w = ui.measure(shown, &style);
            if f.x + w > right && f.x > left {
                f.x = left;
                f.y += lh;
            }
            let rect = Rect::from_min_size(Vec2::new(f.x, f.y), Vec2::new(w, lh));
            if f.draw && !shown.is_empty() {
                if span.code {
                    ui.draw.rounded_rect(Rect::new(rect.min - Vec2::new(2.0, 0.0), rect.max + Vec2::new(2.0, 0.0)), ui.m.radius * 0.5, ui.theme.field);
                }
                ui.text_at(shown, &style, rect.min, 1.0e6, color);
                if let Some(target) = &span.link {
                    let id = ui.id("link").with_index(f.links);
                    f.links += 1;
                    let r = ui.interact(id, rect, Sense::CLICK);
                    if r.hovered {
                        ui.state.cursor_icon = CursorIcon::Pointer;
                        f.out.link_hovered = Some(target.clone());
                    }
                    if r.clicked {
                        f.out.link_clicked = Some(target.clone());
                    }
                    let line = if r.hovered { ui.theme.text } else { ui.theme.accent };
                    ui.draw.hline(rect.min.x, rect.max.x, rect.max.y - ui.m.px(2.0), ui.m.px(1.5), line);
                }
            }
            f.x += ui.measure(word, &style);
        }
    }
    f.y += lh;
}
