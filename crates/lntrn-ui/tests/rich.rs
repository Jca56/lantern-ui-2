//! Rich text (U022): the markup parses into blocks and spans, and the
//! widget lays it out, wraps, and reports link clicks.

use std::cell::RefCell;

use lntrn_math::Vec2;
use lntrn_ui::rich::{Block, Span, inline, parse};
use lntrn_ui::testing::Harness;
use lntrn_ui::{Ui, WidgetId};

fn plain(text: &str) -> Span {
    Span { text: text.into(), ..Span::default() }
}

#[test]
fn inline_markup_becomes_spans() {
    assert_eq!(inline("plain"), vec![plain("plain")]);
    assert_eq!(inline("a **b** c"), vec![plain("a "), Span { text: "b".into(), bold: true, ..Span::default() }, plain(" c")]);
    assert_eq!(inline("*i* and `x`"), vec![Span { text: "i".into(), italic: true, ..Span::default() }, plain(" and "), Span { text: "x".into(), code: true, ..Span::default() }]);
    assert_eq!(inline("see [docs](a/b.md)."), vec![plain("see "), Span { text: "docs".into(), link: Some("a/b.md".into()), ..Span::default() }, plain(".")]);
    assert_eq!(inline("**bold *and italic***"), vec![Span { text: "bold ".into(), bold: true, ..Span::default() }, Span { text: "and italic".into(), bold: true, italic: true, ..Span::default() }]);
    // Markers that close nothing stay as they are.
    assert_eq!(inline("2 * 3 and *open"), vec![plain("2 * 3 and *open")]);
    assert_eq!(inline("a `tick"), vec![plain("a `tick")]);
    assert_eq!(inline("[not a link] (x)"), vec![plain("[not a link] (x)")]);
    assert_eq!(inline("`**not bold**`"), vec![Span { text: "**not bold**".into(), code: true, ..Span::default() }], "code keeps its stars");
}

#[test]
fn blocks_parse() {
    let md = "# Title\nfirst line\nsecond line\n\n- one\n- two\n\n1. a\n2. b\n\n```\ncode here\n  indented\n```\n---\n## Sub\ntail";
    let blocks = parse(md);
    assert_eq!(blocks.len(), 8, "{blocks:#?}");
    assert_eq!(blocks[0], Block::Heading(1, vec![plain("Title")]));
    assert_eq!(blocks[1], Block::Paragraph(vec![plain("first line second line")]), "lines of one paragraph join");
    assert_eq!(blocks[2], Block::List { ordered: false, items: vec![vec![plain("one")], vec![plain("two")]] });
    assert_eq!(blocks[3], Block::List { ordered: true, items: vec![vec![plain("a")], vec![plain("b")]] });
    assert_eq!(blocks[4], Block::Code("code here\n  indented".into()), "code keeps its indentation");
    assert_eq!(blocks[5], Block::Rule);
    assert_eq!(blocks[6], Block::Heading(2, vec![plain("Sub")]));
    assert_eq!(blocks[7], Block::Paragraph(vec![plain("tail")]));
    assert!(parse("").is_empty());
    assert_eq!(parse("```\nnever closed").len(), 1, "an open fence runs to the end");
}

#[test]
fn text_wraps_and_links_click() {
    let md = "A paragraph long enough to wrap around at least once in a narrow column of text. Then [a link](target) at the end.";
    let mut h = Harness::new(800.0, 600.0);
    let mut narrow_h = 0.0;
    let mut wide_h = 0.0;
    h.frame(|ui: &mut Ui| {
        narrow_h = ui.rich_text_height(md, 300.0);
        wide_h = ui.rich_text_height(md, 3000.0);
    });
    let lh = h.metrics().widget_h;
    assert!(narrow_h > wide_h && wide_h > 0.0, "narrow {narrow_h} wide {wide_h}");
    assert!(wide_h < lh * 2.0, "one line when it fits: {wide_h}");
    let clicked = RefCell::new(None);
    let f = |ui: &mut Ui| {
        let r = ui.rich_text(md);
        if let Some(t) = r.link_clicked {
            *clicked.borrow_mut() = Some(t);
        }
    };
    h.click_on(WidgetId::ROOT.with("link").with_index(0), f);
    assert_eq!(*clicked.borrow(), Some("target".to_owned()), "the link's first word is its click target");
    // A click on plain text is not a link.
    let rect = h.rect_of(WidgetId::ROOT.with("link").with_index(0)).unwrap();
    *clicked.borrow_mut() = None;
    h.click_at(Vec2::new(rect.min.x - 200.0, rect.center().y), f);
    assert_eq!(*clicked.borrow(), None);
}
