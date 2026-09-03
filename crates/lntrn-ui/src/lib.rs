//! Lantern UI (D016, D017).
//!
//! Two layers:
//! - **Retained**: [`Screen`] tiles the window into areas, each hosting one
//!   editor. It is plain data (an app may save it) and it changes only when
//!   the user splits, joins or drags a separator.
//! - **Immediate**: inside every region the widgets are re-declared on each
//!   rebuild through a [`Ui`] context, which lays them out, routes input and
//!   emits draw commands. Per-widget persistent state (caret, scroll offset,
//!   open popup) lives in [`UiState`], keyed by stable [`WidgetId`]s.
//!
//! A rebuild happens whenever an input event arrives; an idle app rebuilds
//! nothing and draws nothing.
//!
//! An app plugs in through the [`Host`] trait: it names its editors, draws
//! each area with a [`Ui`], and carries out the [`Action`]s its menus, keys
//! and context menus produce. [`Shell::frame`] does the rest. `lntrn-app`
//! wraps that in a window; `lntrn-demo` is a small complete host.

pub mod context_menu;
pub mod event;
pub mod file_browser;
pub mod gallery;
pub mod host;
pub mod icons;
pub mod id;
pub mod keymap;
pub mod panel;
pub mod persist;
pub mod popups;
pub mod prefs;
pub mod screen;
pub mod shell;
pub mod state;
pub mod testing;
pub mod theme;
pub mod titlebar;
pub mod toasts;
pub mod ui;
pub mod widgets;

pub use context_menu::{ContextMenu, Item, Tool};
pub use event::{Event, Key, Modifiers, MouseButton, WheelDelta};
pub use host::{Action, AreaCx, Capture, Dialog, Host, HostCx, Menu, MenuItem, ShellRequest, actions};
pub use icons::Icon;
pub use id::WidgetId;
pub use keymap::{KeyConfig, KeyItem, KeyMap, Trigger};
pub use prefs::Prefs;
pub use screen::{Area, AreaId, Axis, Screen};
pub use shell::{Shell, ShellOutput, WindowState};
pub use state::{CursorIcon, History, KeyPress, Snapshot, UiState};
pub use theme::{Metrics, Theme};
pub use titlebar::{ResizeEdge, WindowCommand};
pub use toasts::Toast;
pub use ui::{FILL, KeyStep, Response, Sense, Ui};
/// Re-exported: the handle [`Ui::image`] draws.
pub use lntrn_render::ImageHandle;
