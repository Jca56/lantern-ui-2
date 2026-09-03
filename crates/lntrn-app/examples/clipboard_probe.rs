//! A manual probe for the system clipboard: opens a small window, copies a
//! line from inside it and pastes it from outside twice, the second time
//! after the window has sat idle and unfocused for three seconds with
//! nothing arriving but the paste itself. With `picture` as the argument
//! it copies a picture instead and pastes it from outside as PNG. Prints
//! what happened and exits (0 when everything came back right). Run it
//! under `WAYLAND_DEBUG=1` to see every message.
//!
//! One leg per run: a paste from outside takes keyboard focus for a
//! moment, and a copy from a window without focus is refused.
//!
//! ```sh
//! cargo run -p lntrn-app --example clipboard_probe
//! cargo run -p lntrn-app --example clipboard_probe picture
//! ```

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use lntrn_app::{AppConfig, AppHost, run};
use lntrn_image::Image;
use lntrn_ui::{Action, AreaCx, Host, HostCx, Shell, Ui};

const TEXT: &str = "lantern probe 42";

struct Probe {
    started: Instant,
    stage: u8,
    picture: bool,
    /// The picture leg finished (well or not).
    picture_done: Arc<AtomicBool>,
}

/// `wl-paste` of `mime`, given five seconds.
fn paste(mime: &str) -> (Vec<u8>, f64) {
    let since = Instant::now();
    let out = Command::new("timeout").args(["5", "wl-paste", "-n", "-t", mime]).output().map(|o| o.stdout).unwrap_or_default();
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
        let t = self.started.elapsed().as_secs_f64();
        match self.stage {
            // Let the window get focus, then copy a picture from inside.
            0 if t > 0.8 && self.picture => {
                let mut img = Image::solid(64, 48, [0, 0, 0, 255]);
                for (i, px) in img.rgba.chunks_mut(4).enumerate() {
                    px[0] = (i % 64 * 4) as u8;
                    px[1] = (i / 64 * 5) as u8;
                }
                ui.state.set_clipboard_image(img);
                let done = Arc::clone(&self.picture_done);
                std::thread::spawn(move || {
                    let (bytes, secs) = paste("image/png");
                    let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']);
                    let back = lntrn_image::decode(&bytes).ok();
                    let ok = png && back.as_ref().is_some_and(|b| b.width == 64 && b.height == 48 && b.pixel(3, 0) == [12, 0, 0, 255]);
                    println!("picture paste: {} bytes, png {png}, decodes {} in {secs:.2}s: {}", bytes.len(), back.is_some(), if ok { "OK" } else { "WRONG" });
                    done.store(true, Ordering::SeqCst);
                    std::process::exit(if ok { 0 } else { 1 });
                });
                self.stage = 3;
            }
            // Or copy text and go idle for real.
            0 if t > 0.8 => {
                ui.state.set_clipboard(TEXT);
                self.stage = 2;
                {
                    std::thread::spawn(|| {
                        let (first, t1) = paste("text/plain");
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let (second, t2) = paste("text/plain");
                        let (first, second) = (String::from_utf8_lossy(&first).into_owned(), String::from_utf8_lossy(&second).into_owned());
                        println!("first text paste: {first:?} in {t1:.2}s");
                        println!("second text paste, window idle and unfocused: {second:?} in {t2:.2}s");
                        std::process::exit(if first == TEXT && second == TEXT { 0 } else { 1 });
                    });
                }
            }
            0 => ui.state.request_redraw_after(0.05),
            // The picture leg is waiting on wl-paste; give it ten seconds.
            3 => {
                if t > 10.0 && !self.picture_done.load(Ordering::SeqCst) {
                    println!("picture paste: no answer");
                    std::process::exit(2);
                }
                ui.state.request_redraw_after(0.2);
            }
            // Nothing more to draw: the loop sleeps until the paste wakes it.
            _ => {}
        }
        false
    }
    fn run(&mut self, _: &Action, _: &mut HostCx) {}
}

impl AppHost for Probe {}

fn main() {
    let picture = std::env::args().nth(1).as_deref() == Some("picture");
    let probe = Probe { started: Instant::now(), stage: 0, picture, picture_done: Arc::new(AtomicBool::new(false)) };
    let config = AppConfig { title: "Lantern clipboard probe".into(), app_id: "lntrn-clipboard-probe".into(), size: (420.0, 200.0), maximized: false, persist: false, ..AppConfig::default() };
    run(config, probe, Shell::new(0));
}
