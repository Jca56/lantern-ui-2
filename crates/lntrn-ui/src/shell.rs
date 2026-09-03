//! The shell: one rebuild of the whole window. Lays the screen out, drives
//! separator drags and focus (D017), draws every area's header and body
//! through the [`Host`], routes leftover keys to it, hosts popups, carries
//! out the host's requests, and reports what the app should do next.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_render::DrawList;
use lntrn_text::TextEngine;

use crate::event::{Event, MouseButton};
use crate::file_browser::FileBrowser;
use crate::host::{AreaCx, Capture, Host, HostCx, ShellRequest};
use crate::id::WidgetId;
use crate::popups::{self, Popup, dispatch};
use crate::prefs::Prefs;
use crate::screen::{AreaId, Axis, Screen};
use crate::state::{CursorIcon, UiState};
use crate::theme::Metrics;
use crate::titlebar::WindowCommand;
use crate::ui::Ui;

/// What the app needs to know after a rebuild.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShellOutput {
    pub cursor: CursorIcon,
    /// Run another rebuild immediately (a popup closed, a value committed).
    pub rebuild_again: bool,
    /// Background to clear the window with.
    pub clear: Color,
    /// Something the title bar asked the window system to do.
    pub window_command: Option<WindowCommand>,
    pub quit: bool,
    /// Rebuild again after this many seconds even without input (an
    /// animation is running).
    pub wake_after: Option<f64>,
}

/// Facts about the window the shell cannot know on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowState {
    pub maximized: bool,
    pub focused: bool,
}

enum AreaAction<E> {
    Split(AreaId, Axis),
    Close(AreaId),
    Maximize(AreaId),
    SetEditor(AreaId, E),
}

pub struct Shell<H: Host> {
    pub screen: Screen<H::Editor, H::AreaState>,
    pub state: UiState,
    pub prefs: Prefs,
    pub(crate) popup: Option<Popup>,
    pub(crate) drag_sep: Option<usize>,
    pub(crate) toasts: Vec<crate::toasts::Toast>,
}

impl<H: Host> Shell<H> {
    /// One area hosting `editor`. Split [`Shell::screen`] for a richer
    /// starting layout.
    pub fn new(editor: H::Editor) -> Self {
        Self { screen: Screen::new(editor), state: UiState::new(), prefs: Prefs::default(), popup: None, drag_sep: None, toasts: Vec::new() }
    }

    /// Metrics for the current preferences at `window_scale`.
    pub fn metrics(&self, window_scale: f64) -> Metrics {
        self.prefs.theme.metrics(window_scale * self.prefs.ui_scale)
    }

    pub fn popup_open(&self) -> bool {
        self.popup.is_some()
    }

    /// The area layout as text, for saving (see [`Screen::describe`]).
    pub fn layout_description(&self, host: &H) -> String {
        self.screen.describe(|e| host.editor_id(e))
    }

    /// Replace the layout with a saved one. Editor names the host no
    /// longer knows become its first editor. `false` leaves the layout as
    /// it was.
    pub fn restore_layout(&mut self, host: &H, text: &str) -> bool {
        let fallback = host.editors().first().copied();
        match Screen::from_description(text, |id| host.editor_from_id(id).or(fallback)) {
            Some(screen) => {
                self.screen = screen;
                true
            }
            None => false,
        }
    }

    /// Open the host's named menu at `at`. Unknown names open nothing.
    pub fn open_menu(&mut self, host: &H, name: &str, at: Vec2) {
        self.popup = host.menu(name).map(|menu| Popup::Menu { title: menu.title, items: menu.items, pos: at });
        self.state.request_rebuild = true;
    }

    /// Carry out one request. Returns `true` to quit.
    pub fn request(&mut self, host: &H, r: ShellRequest) -> bool {
        let pointer = self.state.pointer;
        self.state.request_rebuild = true;
        match r {
            ShellRequest::Menu(name) => self.open_menu(host, &name, pointer),
            ShellRequest::MenuAt(name, at) => self.open_menu(host, &name, at),
            ShellRequest::Palette => self.popup = Some(Popup::Palette { query: String::new(), selected: 0 }),
            ShellRequest::PathDialog { action, save, suggest } => {
                let browser = FileBrowser::new(std::path::Path::new(&suggest), save);
                self.popup = Some(Popup::Path { action, browser });
            }
            ShellRequest::ContextMenu(menu) => self.popup = Some(Popup::Context(menu)),
            ShellRequest::Dialog(d) => self.popup = Some(Popup::Dialog(d)),
            ShellRequest::Toast(text) => self.toasts.push(crate::toasts::Toast { text, at: self.state.now }),
            ShellRequest::Maximize(area) => {
                if let Some(a) = area.or(self.screen.active) {
                    self.screen.toggle_maximize(a);
                }
            }
            ShellRequest::ClosePopup => {
                self.popup = None;
                self.state.focus = None;
            }
            ShellRequest::PrefToggle(field) => {
                use lntrn_props::{Reflect, Value};
                let prefs: &mut dyn Reflect = &mut self.prefs;
                if let Some(Value::Bool(on)) = prefs.get_by_name(&field) {
                    let _ = prefs.set_by_name(&field, Value::Bool(!on));
                }
            }
            ShellRequest::Rebuild => {}
            ShellRequest::Quit => return true,
        }
        false
    }

    /// One rebuild. `window` is the whole window in physical pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, host: &mut H, events: &[Event], window: Rect, window_scale: f64, ws: WindowState, text: &mut TextEngine, draw: &mut DrawList) -> ShellOutput {
        let theme = self.prefs.theme.clone();
        let m = self.metrics(window_scale);
        let mut requests: Vec<ShellRequest> = Vec::new();

        // A running tool owns button presses and keys; the UI still sees
        // motion (hover, cursor), releases (so a drag that started a tool
        // settles), and the middle button and wheel, so a view can be
        // navigated mid-tool — unless the tool claims the wheel.
        let capture = host.capture();
        let captured = capture != Capture::None;
        let wheel_captured = matches!(capture, Capture::Tool { wheel: true });
        let owned_by_tool = |e: &Event| {
            matches!(e, Event::Button { button: MouseButton::Left | MouseButton::Right, pressed: true, .. } | Event::Key { .. } | Event::Text(_))
                || (wheel_captured && matches!(e, Event::Wheel { .. }))
        };
        let ui_events: Vec<Event> = if captured { events.iter().filter(|e| !owned_by_tool(e)).cloned().collect() } else { Vec::new() };
        self.state.begin_frame(if captured { &ui_events } else { events }, m.widget_h);
        let pointer = self.state.pointer;
        if captured {
            host.captured(events, &mut HostCx { pointer, requests: &mut requests });
        }

        // Resize grabs along the undecorated edges come before everything.
        let mut window_command = self.resize_edges(window, m, ws);
        let edge_cursor = self.state.cursor_icon;
        let title = host.title();
        let status = host.status();
        let (area_rect, title_cmd, open_menu) = self.title_bar(draw, text, &theme, m, window, ws, &title, &status, host.title_menus());
        window_command = window_command.or(title_cmd);
        if let Some((name, at)) = open_menu {
            self.open_menu(host, &name, at);
        }
        let areas_window = area_rect;
        self.screen.layout(areas_window, m.header_h, m.sep);

        // ---- separators -------------------------------------------------
        let st = &mut self.state;
        if st.released {
            self.drag_sep = None;
        }
        if let Some(idx) = self.drag_sep {
            if st.down {
                self.screen.drag_separator(idx, st.pointer, Screen::<H::Editor, H::AreaState>::min_area_px(m.header_h));
                self.screen.layout(areas_window, m.header_h, m.sep);
            }
        } else if st.pressed
            && !st.press_claimed
            && st.popup.is_none()
            && self.popup.is_none()
            && let Some(idx) = self.screen.separator_at(st.press_pos)
        {
            self.drag_sep = Some(idx);
            st.press_claimed = true;
            st.active = Some(WidgetId::ROOT.with("separator"));
        }
        let hover_sep = self.drag_sep.or_else(|| {
            (st.popup.is_none() && st.active.is_none() && self.popup.is_none())
                .then(|| self.screen.separator_at(st.pointer))
                .flatten()
        });
        let sep_cursor = hover_sep.map(|i| match self.screen.separators()[i].axis {
            Axis::Horizontal => CursorIcon::EwResize,
            Axis::Vertical => CursorIcon::NsResize,
        });

        // ---- focus (D017) -------------------------------------------------
        if self.prefs.focus_follows_mouse {
            if st.pointer_in_window && self.popup.is_none() && !captured && let Some(a) = self.screen.area_at(st.pointer) {
                self.screen.active = Some(a);
            }
        } else if st.pressed
            && self.drag_sep.is_none()
            && let Some(a) = self.screen.area_at(st.press_pos)
            && st.popup.is_none_or(|(r, _)| !r.contains(st.press_pos))
        {
            self.screen.active = Some(a);
        }

        // ---- popup (drawn first so it claims the pointer) --------------
        let mut refresh_menu = false;
        if let Some(popup) = self.popup.as_mut() {
            let entries: Vec<(String, String)> = match popup {
                Popup::Palette { query, .. } => host.palette(query),
                _ => Vec::new(),
            };
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, window, window, WidgetId::ROOT.with("popup"), 0);
            ui.set_window_rect(window);
            let mut cx = HostCx { pointer, requests: &mut requests };
            let result = popups::draw(&mut ui, popup, window, &entries, host, &mut cx);
            ui.finish();
            if result.close {
                self.popup = None;
            } else {
                // Refreshed below, once the requests the tool raised have run.
                refresh_menu = result.refresh;
            }
        }

        // ---- areas ------------------------------------------------------
        let editors: Vec<H::Editor> = host.editors().to_vec();
        let labels: Vec<String> = editors.iter().map(|&k| host.editor_label(k).to_owned()).collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let layouts: Vec<_> = self.screen.layouts().to_vec();
        let maximized = self.screen.maximized;
        let mut actions = Vec::new();
        let mut changed_globals = false;
        for l in &layouts {
            let active = self.screen.active == Some(l.area);
            let Some(area) = self.screen.area_mut(l.area) else {
                continue;
            };
            let kind = area.editor;
            let base = WidgetId::ROOT.with_u64(l.area as u64);

            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.rect_gradient(l.header, theme.top(theme.header), theme.bottom(theme.header));
            draw.hline(l.header.min.x, l.header.max.x, l.header.min.y, m.border, theme.highlight(theme.header));
            draw.hline(l.header.min.x, l.header.max.x, l.header.max.y - m.border, m.border, theme.border_dark);
            if !host.paints_body(kind) {
                draw.rect(l.body, theme.panel);
            }
            draw.stroke_rect(l.rect, m.border, 0.0, theme.border_dark);
            draw.pop_clip();

            let content = Rect::new(
                Vec2::new(l.header.min.x + m.gap, l.header.min.y + ((l.header.height() - m.widget_h) * 0.5).round()),
                Vec2::new(l.header.max.x - m.gap, l.header.max.y),
            );
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, content, l.header, base.with("header"), 0);
            ui.set_window_rect(window);
            ui.row(|ui| {
                let mut idx = editors.iter().position(|&k| k == kind).unwrap_or(0);
                if ui.dropdown("editor", &mut idx, &label_refs) {
                    actions.push(AreaAction::SetEditor(l.area, editors[idx]));
                }
                let mut cx = AreaCx { area: l.area, state: &mut area.state, active, pointer, prefs: &mut self.prefs, requests: &mut requests };
                host.draw_header(kind, ui, &mut cx);
                let style = ui.text_style();
                let menu_w = ui.measure("⋮", &style) + ui.m.pad * 2.0;
                let spacer = (ui.avail_width() - menu_w - ui.m.gap).max(0.0);
                ui.alloc(Vec2::new(spacer, 1.0));
                let max_label = if maximized == Some(l.area) { "Restore" } else { "Maximize" };
                if let Some(i) = ui.menu_button("⋮", &[max_label, "Split Left | Right", "Split Top | Bottom", "Close Area"]) {
                    actions.push(match i {
                        0 => AreaAction::Maximize(l.area),
                        1 => AreaAction::Split(l.area, Axis::Horizontal),
                        2 => AreaAction::Split(l.area, Axis::Vertical),
                        _ => AreaAction::Close(l.area),
                    });
                }
            });
            ui.finish();

            let body_content = l.body.shrink(m.pad);
            let mut ui = Ui::new(draw, text, &theme, m, &mut self.state, body_content, l.body, base.with("body"), 0);
            ui.set_window_rect(window);
            let mut cx = AreaCx { area: l.area, state: &mut area.state, active, pointer, prefs: &mut self.prefs, requests: &mut requests };
            changed_globals |= host.draw_body(kind, &mut ui, &mut cx);
            ui.finish();
        }

        // Focused area outline, on top of its content.
        if let Some(active) = self.screen.active
            && let Some(l) = self.screen.layout_of(active)
        {
            draw.set_layer(0);
            draw.push_clip_absolute(l.rect);
            draw.stroke_rect(l.rect, m.focus_border, 0.0, theme.focus);
            draw.pop_clip();
        }
        self.draw_toasts(draw, text, &theme, m, window);

        // ---- keys no widget consumed go to the host -----------------------
        if self.popup.is_none() && !captured {
            let leftover: Vec<_> = self.state.keys.drain(..).collect();
            let editor = self.screen.active_editor();
            let mut cx = HostCx { pointer, requests: &mut requests };
            for k in leftover {
                if let Some(action) = host.key(k, editor) {
                    dispatch(host, &action, &mut cx);
                    self.state.request_rebuild = true;
                }
            }
        }

        let mut quit = false;
        for r in requests {
            quit |= self.request(host, r);
        }

        // A tool in the context menu changed something it displays. Now that
        // its requests have been applied, let the host bring the menu up to
        // date (its tabs keep their place).
        if refresh_menu && let Some(Popup::Context(menu)) = self.popup.as_mut() {
            host.refresh_context_menu(menu);
            self.state.request_rebuild = true;
        }

        for a in actions {
            match a {
                AreaAction::Split(area, axis) => {
                    if let Some(kind) = self.screen.area(area).map(|a| a.editor) {
                        self.screen.split(area, axis, 0.5, kind);
                    }
                }
                AreaAction::Close(area) => {
                    self.screen.join(area);
                }
                AreaAction::Maximize(area) => self.screen.toggle_maximize(area),
                AreaAction::SetEditor(area, kind) => {
                    if let Some(a) = self.screen.area_mut(area) {
                        a.editor = kind;
                    }
                }
            }
            self.state.request_rebuild = true;
        }
        if changed_globals {
            self.state.request_rebuild = true;
        }

        self.state.end_frame();
        let cursor = if captured {
            CursorIcon::Grabbing
        } else if edge_cursor != CursorIcon::Default {
            edge_cursor
        } else {
            sep_cursor.unwrap_or(self.state.cursor_icon)
        };
        ShellOutput { cursor, rebuild_again: self.state.request_rebuild, clear: theme.bg, window_command, quit, wake_after: self.state.wake_after }
    }
}
