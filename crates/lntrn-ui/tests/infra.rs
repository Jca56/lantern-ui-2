//! Reduced motion, the debug overlay and frame stats, headless.

use std::cell::Cell;

use lntrn_ui::testing::Harness;
use lntrn_ui::{Action, AreaCx, Host, HostCx, Shell, ShellRequest, Ui, WidgetId};

#[test]
fn reduced_motion_snaps_animations() {
    let mut h = Harness::new(400.0, 300.0);
    let id = WidgetId::ROOT.with("fade");
    let target = Cell::new(0.0);
    let seen = Cell::new(0.0);
    let f = |ui: &mut Ui| {
        seen.set(ui.animate(id, target.get(), 0.5));
    };
    h.frame(f);
    target.set(1.0);
    h.advance(0.05);
    h.frame(f);
    assert!(seen.get() > 0.0 && seen.get() < 1.0, "easing: {}", seen.get());
    assert!(h.state.wake_after.is_some(), "keeps asking for frames");
    h.state.reduce_motion = true;
    h.advance(0.05);
    h.frame(f);
    assert_eq!(seen.get(), 1.0, "snapped");
    assert_eq!(h.state.wake_after, None, "and rests");
}

struct Plain;

impl Host for Plain {
    type Editor = u8;
    type AreaState = ();
    fn editors(&self) -> &[u8] {
        &[0]
    }
    fn editor_label(&self, _: u8) -> &str {
        "Main"
    }
    fn title(&self) -> String {
        "Plain".into()
    }
    fn draw_body(&mut self, _: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        ui.label("hi");
        false
    }
    fn run(&mut self, _: &Action, _: &mut HostCx) {}
}

#[test]
fn the_debug_overlay_draws_when_asked_and_stats_count_rebuilds() {
    let mut h = Harness::new(900.0, 600.0);
    let mut shell: Shell<Plain> = Shell::new(0);
    let mut host = Plain;
    h.shell_frame(&mut shell, &mut host);
    let plain = shell.stats().vertices;
    assert_eq!(shell.stats().frames, 1);
    assert!(shell.stats().rebuild_ms >= 0.0);
    shell.prefs.debug_overlay = true;
    h.shell_frame(&mut shell, &mut host);
    assert!(shell.stats().vertices > plain + 100, "the overlay's panel and lines: {} vs {plain}", shell.stats().vertices);
    assert_eq!(shell.stats().frames, 2);
    shell.prefs.debug_overlay = false;
    h.shell_frame(&mut shell, &mut host);
    assert_eq!(shell.stats().vertices, plain, "gone again");
}

#[test]
fn reduced_motion_lets_toasts_go_without_fading() {
    let mut h = Harness::new(900.0, 600.0);
    let mut shell: Shell<Plain> = Shell::new(0);
    let mut host = Plain;
    shell.prefs.reduce_motion = true;
    h.shell_frame(&mut shell, &mut host);
    shell.request(&mut host, ShellRequest::Toast("Saved".into()));
    let out = h.shell_frame(&mut shell, &mut host);
    assert!(out.wake_after.is_some_and(|w| w > 3.9), "wakes only when the toast is due to go: {:?}", out.wake_after);
    h.advance(4.1);
    let out = h.shell_frame(&mut shell, &mut host);
    assert!(shell.toasts().is_empty());
    assert_eq!(out.wake_after, None);
}
