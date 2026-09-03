//! `UiState` on its own: event folding, focus walking, popups, the clipboard,
//! files and input methods.

use std::path::PathBuf;

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{Event, Key, Modifiers, MouseButton, UiState, WheelDelta, WidgetId};

#[test]
fn ingests_events() {
    let mut s = UiState::new();
    let p = Vec2::new(10.0, 20.0);
    s.begin_frame(
        &[
            Event::PointerMoved(p),
            Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE },
            Event::Wheel { delta: WheelDelta::Lines(Vec2::new(0.0, 1.0)), pos: p, mods: Modifiers::NONE },
            Event::Key { key: Key::Enter, pressed: true, repeat: false, mods: Modifiers::NONE },
            Event::Text("a".into()),
        ],
        40.0,
    );
    assert!(s.pressed && s.down && !s.released);
    assert_eq!(s.press_pos, p);
    assert_eq!(s.wheel, Vec2::new(0.0, 40.0));
    assert_eq!(s.text_input, vec![(4, "a".to_owned())]);
    assert!(s.take_key(|k| k.key == Key::Enter).is_some());
    assert!(s.take_key(|k| k.key == Key::Enter).is_none());
    s.active = Some(WidgetId::ROOT);
    s.end_frame();
    assert!(s.is_active(WidgetId::ROOT), "still held");
    s.begin_frame(&[Event::Button { button: MouseButton::Left, pressed: false, pos: p, mods: Modifiers::NONE }], 40.0);
    assert!(s.released && !s.down);
    s.end_frame();
    assert_eq!(s.active, None);
}

#[test]
fn double_click_and_popup_lifetime() {
    let mut s = UiState::new();
    let p = Vec2::ZERO;
    let press = Event::Button { button: MouseButton::Left, pressed: true, pos: p, mods: Modifiers::NONE };
    s.begin_frame(std::slice::from_ref(&press), 1.0);
    assert!(!s.double_click);
    s.begin_frame(&[press], 1.0);
    assert!(s.double_click);
    s.keep_popup(Rect::ZERO, 1);
    s.end_frame();
    assert!(s.popup.is_some());
    s.begin_frame(&[], 1.0);
    s.end_frame();
    assert!(s.popup.is_none(), "not kept → closed");
}

#[test]
fn tab_walks_focus_order() {
    let mut s = UiState::new();
    let (a, b, c) = (WidgetId::ROOT.with("a"), WidgetId::ROOT.with("b"), WidgetId::ROOT.with("c"));
    let tab = |mods| Event::Key { key: Key::Tab, pressed: true, repeat: false, mods };
    s.begin_frame(&[tab(Modifiers::NONE)], 1.0);
    s.focus_order.extend([a, b, c]);
    s.end_frame();
    assert_eq!(s.focus, Some(a), "nothing focused: Tab goes to the first");
    assert!(s.focus_visible && s.request_rebuild);
    s.begin_frame(&[tab(Modifiers::SHIFT)], 1.0);
    s.focus_order.extend([a, b, c]);
    s.end_frame();
    assert_eq!(s.focus, Some(c), "Shift+Tab wraps backwards");
    s.begin_frame(&[tab(Modifiers::NONE)], 1.0);
    s.focus_order.extend([a, b, c]);
    s.end_frame();
    assert_eq!(s.focus, Some(a), "and Tab wraps forwards");
    s.begin_frame(&[Event::Button { button: MouseButton::Left, pressed: true, pos: Vec2::ZERO, mods: Modifiers::NONE }], 1.0);
    assert!(!s.focus_visible, "a pointer press hides the rings");
    s.set_time(5.0);
    s.begin_frame(&[], 1.0);
    assert_eq!(s.now, 5.0);
    s.request_redraw_after(0.5);
    s.request_redraw_after(0.2);
    s.request_redraw_after(0.9);
    assert_eq!(s.wake_after, Some(0.2), "the soonest wins");
}

#[test]
fn clipboard_files_and_ime() {
    let mut s = UiState::new();
    assert!(!s.take_clipboard_dirty());
    s.set_clipboard("hello");
    assert_eq!(s.clipboard, "hello");
    assert!(s.take_clipboard_dirty());
    assert!(!s.take_clipboard_dirty(), "reported once");

    let p = PathBuf::from("/tmp/x.png");
    s.begin_frame(&[Event::FileHovered(p.clone())], 1.0);
    assert!(s.hovering_files);
    s.begin_frame(&[Event::FileDropped(p.clone())], 1.0);
    assert!(!s.hovering_files, "the drop ends the hover");
    assert_eq!(s.dropped_files, vec![p.clone()]);
    assert_eq!(s.take_dropped_files(), vec![p]);
    assert!(s.dropped_files.is_empty());
    s.begin_frame(&[Event::FileHovered(PathBuf::new()), Event::FileHoverLeft], 1.0);
    assert!(!s.hovering_files);

    s.begin_frame(&[Event::ImePreedit { text: "ni".into(), cursor: Some((2, 2)) }], 1.0);
    assert_eq!(s.ime_preedit, Some(("ni".to_owned(), Some((2, 2)))));
    s.begin_frame(&[], 1.0);
    assert!(s.ime_preedit.is_some(), "a composition outlives the frame");
    s.begin_frame(&[Event::Text("你".into())], 1.0);
    assert_eq!(s.ime_preedit, None, "a commit ends it");
    s.begin_frame(&[Event::ImePreedit { text: "a".into(), cursor: None }, Event::ImePreedit { text: String::new(), cursor: None }], 1.0);
    assert_eq!(s.ime_preedit, None, "an empty preedit clears it");
}

#[test]
fn pictures_on_the_clipboard() {
    let mut s = UiState::new();
    assert!(!s.take_clipboard_image_dirty());
    s.set_clipboard_image(lntrn_image::Image::solid(2, 2, [1, 2, 3, 255]));
    assert!(s.take_clipboard_image_dirty(), "the harness pushes it out");
    assert!(!s.take_clipboard_image_dirty(), "once");
    assert_eq!(s.clipboard_image.as_ref().map(|i| i.width), Some(2));
    assert!(!s.take_clipboard_dirty(), "a picture is not text");
    s.clipboard_image_wanted = true;
    assert!(s.clipboard_image_wanted);
}
