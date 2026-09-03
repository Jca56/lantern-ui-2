//! A manual probe for the system clipboard: opens a small window, copies a
//! line from inside it, then pastes it from outside with `wl-paste` twice,
//! the second time after the window has sat idle and unfocused for three
//! seconds with nothing arriving but the paste itself. Prints how long each
//! paste took and exits (0 when both came back right). Run it under
//! `WAYLAND_DEBUG=1` to see every message.
//!
//! ```sh
//! cargo run -p lntrn-app --example clipboard_probe
//! ```

use std::process::Command;
use std::time::Instant;

use lntrn_app::{AppConfig, AppHost, run};
use lntrn_ui::{Action, AreaCx, Host, HostCx, Shell, Ui};

const TEXT: &str = "lantern probe 42";

struct Probe {
    started: Instant,
    copied: bool,
}

/// `wl-paste`, given five seconds.
fn paste() -> (String, f64) {
    let since = Instant::now();
    let out = Command::new("timeout").args(["5", "wl-paste", "-n"]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_else(|e| format!("error: {e}"));
    (out, since.elapsed().as_secs_f64())
}

impl Host for Probe {
    type Editor = u8;
    type AreaState = ();
    fn editors(&self) -> &[u8] {
        &[0]
    }
    fn editor_label(&self, _: u8) -> &str {
        "Probe"
    }
    fn title(&self) -> String {
        "Lantern clipboard probe".into()
    }
    fn draw_body(&mut self, _: u8, ui: &mut Ui, _: &mut AreaCx<()>) -> bool {
        ui.label("Probing the clipboard…");
        if self.copied {
            // Nothing more to draw: the loop goes to sleep for real.
            return false;
        }
        if self.started.elapsed().as_secs_f64() < 0.8 {
            // Let the window get focus first.
            ui.state.request_redraw_after(0.05);
            return false;
        }
        ui.state.set_clipboard(TEXT);
        self.copied = true;
        std::thread::spawn(|| {
            let (first, t1) = paste();
            std::thread::sleep(std::time::Duration::from_secs(3));
            let (second, t2) = paste();
            println!("first paste: {first:?} in {t1:.2}s");
            println!("second paste, window idle and unfocused: {second:?} in {t2:.2}s");
            std::process::exit(if first == TEXT && second == TEXT { 0 } else { 1 });
        });
        false
    }
    fn run(&mut self, _: &Action, _: &mut HostCx) {}
}

impl AppHost for Probe {}

fn main() {
    let probe = Probe { started: Instant::now(), copied: false };
    let config = AppConfig { title: "Lantern clipboard probe".into(), app_id: "lntrn-clipboard-probe".into(), size: (420.0, 200.0), maximized: false, persist: false, ..AppConfig::default() };
    run(config, probe, Shell::new(0));
}
