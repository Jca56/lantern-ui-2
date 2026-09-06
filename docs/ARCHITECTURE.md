# Lantern UI — Architecture

Lantern UI is the framework every Lantern app draws with. It came out of
Lantern Prism (see `DECISIONS.md` for the reasoning, kept under the same
numbers the code comments cite) and lives here so Prism, Mix, Spark and
whatever comes next share one widget set, one text engine, one theme.

## 1. Principles

1. **Fully ours.** Two external crates, `wgpu` and `winit`. Everything else
   — math, containers, reflection, text, UI, the window loop — is here.
2. **Nothing depends upward.** A crate stands only on crates below it.
3. **Headless below `render`.** `math`, `core`, `props`, `image`, `text`
   build and test with no GPU and no window. `cargo test` passes on a
   machine with no display.
4. **Big text, big targets.** Accessibility is a requirement, not a theme.
5. **The app owns its data; the shell owns the window.** The shell never
   sees a document. It asks the app what to draw through one trait.

## 2. Crates

```
lntrn-math     Vec2/3/4, Mat3/4, Quat, Transform, Rect, Color, Aabb, Ray, Plane, Frustum. f64.
lntrn-core     Handle/Arena, Id, ChunkedVec (persistent array), jobs (pool, scope,
               parallel_for), Undo (snapshot stack), block_on, bytes (Pod), Pcg32, log.
lntrn-props    props! { struct } → Reflect: field ids, labels, ranges, subtypes, get/set
               by index, nested structs and lists, field-id-tagged serialization.
lntrn-image    PNG (all bit depths, Adam7) and JPEG (baseline + progressive) decoders.
lntrn-text     sfnt/TTF/CFF/variable fonts, COLR/CBDT emoji, system font discovery,
               GSUB/GPOS shaping, BiDi, line breaking, scanline raster, atlas packing.
lntrn-render   Gpu, SurfaceTarget, RenderGraph + TexturePool, Pass2d (one vertex stream
               for rects/lines/glyphs against one atlas), DrawList, WGSL preprocessor.
lntrn-ui       Event vocabulary, Screen (area tree), Ui (immediate-mode context) and the
               widgets, props panels, Theme/Prefs, title bar, popups, context menu,
               file browser, keymap, and the Shell that ties them to a Host.
lntrn-app      AppConfig, run(): winit window, GPU init, event translation, redraw on
               demand, cursor and window commands, render-graph hooks for 3D hosts.
lntrn-demo     Widget gallery + Preferences + Notes editors: the reference Host.
```

## 3. The shell and the host

```
                ┌───────────────────────── Shell<H> ─────────────────────────┐
  events ─────► │ title bar · areas (split/join/drag) · focus · popups ·      │ ──► ShellOutput
                │ keymap routing · requests                                  │     (cursor, clear,
                └────┬───────────────▲───────────────▲───────────────▲───────┘      window cmd, quit)
                     │ draw_body     │ menu()        │ run(action)   │ key(press)
                     ▼               │               │               │
                ┌──────────────────────── H: Host ────────────────────────────┐
                │ editors · labels · title · menus · palette · draw · run     │
                └─────────────────────────────────────────────────────────────┘
```

**Retained** — `Screen<E, S>` is a binary tree of splits whose leaves are
areas. Each area holds a stack of tabs, one shown at a time, and each tab
has an editor kind `E` (the host's enum) and a state `S` (the host's
per-tab data: a camera, a selection; `()` when none). It changes only
when the user splits, joins or drags a separator, or adds, closes or
switches a tab (`+` and `⋮` in the header, `shell.next_tab`). An editor
whose `Host::shows_header` is `false` gets no header at all; its body is
the whole area (U031).

**Immediate** — inside every header and body the host re-declares its
widgets each rebuild through a `Ui`: `ui.slider("Opacity", &mut v, 0.0,
1.0, 0.0)`. Per-widget memory (caret, scroll, drag origin, open popup)
lives in `UiState`, keyed by stable `WidgetId`s built from labels and
indices.

**Actions** — every menu row, palette entry, tool, and key binding
produces an `Action { id, args }`. The shell dispatches it: `shell.*` ids
(open menu, palette, toggle a preference, quit) it handles itself; the rest
go to `Host::run`. Keys work the same way: `Host::key` resolves a press
against its `KeyConfig` and returns the action.

**Requests** — while drawing or running, the host pushes `ShellRequest`s:
open a menu, the palette, a path or folder dialog, a context menu; flip a
preference; rebuild; quit. The shell applies them at the end of the rebuild.

**Closing** — the title bar's `×`, the window system's close, and
`ShellRequest::CloseWindow` all reach `Host::close_requested` first; a host
with unsaved work returns `false` and asks with a dialog whose button
quits or closes for real. The main window's close quits the app.

**Context menus** — the host builds a `ContextMenu` (title, tabs of items,
submenus, live `props!` panels with Apply, custom rows it draws itself, a
tool strip, a bar) from whatever was under the pointer and requests it.
After a tool runs, `Host::refresh_context_menu` brings it up to date.

**Dialogs** — a `Dialog` is a title, a body, buttons that run actions, and
optionally a `content` key: the shell then calls `Host::draw_item` with
it between the body and the buttons, so a rename asks for a name and an
export shows its settings, with ordinary widgets. The dialog sizes itself
to what was drawn.

**Capture** — a running tool (a modal operator) can claim presses and keys
through `Host::capture`; the shell still routes motion, releases and the
wheel to widgets and hands the tool every event through `Host::captured`.

**Rebuild on demand** — a rebuild happens only when an event arrives. A
popup closing or a value committing asks for one more (`rebuild_again`);
the app caps that at four before presenting.

## 3b. Keyboard, time and memory

**Focus** — every clickable widget registers itself as a Tab stop
(`Ui::focusable`). A Tab nobody consumed moves focus along that order at
the end of the frame; Enter and Space click the focused widget
(`Ui::key_click`); arrows step values (`Ui::key_step`); a ring shows only
after keyboard navigation and disappears on the next pointer press. A
scroll area brings a newly focused widget into view.

**Time** — `UiState::now` is the frame clock. `Ui::animate` eases a
per-widget value toward a target and keeps asking for frames
(`request_redraw_after`) until it rests; the harness sleeps with
`WaitUntil` in between and is idle otherwise. A thread with something
to show (a terminal's output, a finished export) wakes the loop through
the `lntrn_app::Waker` the host is handed at start (`AppHost::waker`);
wakes coalesce until the loop has turned, and every window rebuilds. Toasts and the busy bar use
the same mechanism. The `reduce_motion` preference turns all of it off:
values snap, toasts go when their time is up, busy bars hold still, and
nothing asks for a frame it does not need.

**Undo** — `lntrn_core::Undo<T>` is a stack of snapshots with coalescing
for bursts (a drag, fast typing). A document on `ChunkedVec` clones in
O(chunks), so a host keeps one snapshot per edit and wires Ctrl+Z to it;
the text widgets keep their own per-field history the same way.

**Memory** — per-widget state lives in a map keyed by widget id *and*
kind, so a number field keeps its typing buffer, its drag origin and its
undo history side by side. Editors replay keys and typed text in arrival
order (each carries a sequence number), so a fast "hello, Enter, world"
lands in the right order even inside one frame.

**Persistence** — `Prefs` is a `props!` struct saved as field-id-tagged
bytes (`persist::save`); the area tree is one line of text
(`Screen::describe`; tabs as `[Gallery|Notes*]`). `lntrn_app::run` loads both from
`~/.config/<app_id>/` and writes them back on exit.

**The outside world** — three things cross the window boundary, all in
the event vocabulary so the harness and the shell stay apart. The
*clipboard*: widgets copy into `UiState::set_clipboard` and paste from
`UiState::clipboard`; the harness pulls the system clipboard in before a
rebuild that carries a paste key and pushes ours out after a copy
(`lntrn_app::clipboard`, over the window's own Wayland connection with a
`wl_data_device` of ours, U018; in-app elsewhere), and serves other apps'
pastes from a private queue even while the window sits idle. Pictures
travel the same way as PNG (`UiState::clipboard_image`). *Drags* cross it
both ways: files dragged in arrive as `Event::FileHovered`/`FileDropped`
for a `Ui::drop_zone` or the host; a widget whose drag has gone a few
pixels (`Ui::drag_out_starts`) hands a `DragPayload` (text, files, a
picture) to `UiState::start_drag_out`, the harness starts the window
system's drag from the press's own serial, and `Event::DragEnded` stands
in for the button release the compositor keeps (U020). *Input methods*: a
composition arrives as `Event::ImePreedit`, the
focused text widget shows it inline at its caret with an underline and
reports the caret rect (`ShellOutput::ime`) so the harness can place the
candidate window; the committed text is an ordinary `Event::Text`.
*Files from outside*: `Event::FileHovered` / `FileDropped`, taken by the
harness's own data device on Wayland (with the pointer position, so a
`Ui::drop_zone` under the pointer takes them) and from winit on X11;
anything no zone takes reaches `Host::dropped` with the area under the
pointer.

## 4. The frame

```
winit event ──► translate ──► Event ──┐
                                      ▼
  shell.frame(host, events, window, scale, ws, text, draw)   ×1..4
                                      │
                           host.after_rebuild(gpu, shell)      (picking)
                                      ▼
  clear ──► host.render(RenderCx)  ──► Pass2d(draw list, atlas) ──► present
```

`Pass2d` draws the whole `DrawList` — panels, strokes, icons, glyph quads
and pictures — in one pass from one vertex stream against the text atlas,
with a clip stack and layers (0 areas, 1 popups, 2 context-menu content, 3
tooltips, 4 toasts). Pictures are `Images` uploaded once (sRGB, with
mipmaps) and referenced by handle; the pass binds one picture per stretch
of vertices that needs it, so a frame without pictures is still a single
draw. A 3D host adds its nodes to the render graph between the clear and
the UI pass; the UI composites over them.

**Two harnesses** share that frame: `lntrn_app::run` owns winit windows
(undecorated, the shell draws the frame), and `lntrn_app::Embedded` draws
into a window somebody else owns — a plugin editor in a DAW — from raw
window handles, taking events in our own vocabulary and reporting the
cursor and wake time back. An app may have any number of windows (U021):
each `Win` carries its own surface, clipboard, event queue and `Shell`
(with a `title` of its own), while the host, the GPU, the pictures and
the text engine are shared. `ShellRequest::OpenWindow(NewWindow)` opens
one with a layout in the saved layout's words; the area menu's *Open in
New Window* does it for one area. Closing the main window quits;
preferences changed in any window reach the others after the frame.

## 5. Text

`lntrn-text` is GPU-free (D018). It owns fonts, shaping, layout and
rasterization, and packs glyph coverage and color emoji into a CPU RGBA
atlas with dirty rects. `lntrn-render` uploads that atlas and draws the
quads `TextEngine::place` produces. Text lives in pixel space and `f32`
by design: it is a picture, not geometry.

## 6. Theme and metrics

`Theme` is a `props!` struct: colors plus logical sizes in multiples of
five (`text_size` 25, `widget_height` 45, `padding` 10, ...). The
Preferences editor edits it live because the props panel is generated
from its description. `Theme::metrics(scale)` turns it into physical
pixels once per frame; `scale` is the window's scale factor times the
user's UI scale preference. Line width comes from `border_width`.

The title bar, area headers, area bodies and raised controls are each a
`Gradient` (`lntrn_props::Gradient`: two colors, top and bottom, the
same one twice for flat), one theme row with one swatch that opens the
picker with a swatch for each end. `gradient` shades everything else
drawn from a single color (`Theme::shaded(color)` makes it a surface;
`top`/`bottom`/`highlight`/`shade` are the factor's helpers), and the
bevel on raised and recessed controls stays one pixel whatever
`border_width` says; only outlines and separators grow (U028, U029).

Themes have names (`themes.rs`): the presets in code, the user's as
`name.theme` text files under `~/.config/lantern-ui/themes/`, shared by
every app; the Preferences editor's Look section picks, saves and
deletes them (U030).

## 7. Testing without a window

`lntrn_ui::testing::Harness` holds a text engine, a draw list and a
`UiState`, records every widget's rect, and runs frames from queued
synthetic input: `click_on(id, |ui| ...)`, `drag(from, to, ...)`,
`key(...)`, `type_text(...)`, `advance(seconds)`. Widget ids are the
label path (`WidgetId::ROOT.with("list").with_index(3).with("Row 3")`).
`shell_frame` drives a whole `Shell` with a host the same way. Every widget
and shell feature has a headless test in `crates/lntrn-ui/tests/`.

The `debug_overlay` preference draws what the last rebuild cost (its
time and vertex count), the atlas size and glyph count, and the layout
cache's hit rate in the top-right corner (`Shell::stats` has the same
numbers). It asks for no frames of its own.

## 8. Adding a widget

A widget is a method on `Ui` in `lntrn-ui/src/widgets/`: allocate a rect
(`alloc`), hit-test it (`interact` with a `Sense`), read the `Response`,
draw with the theme helpers (`raised`, `recessed`, `fill_shaded`,
`text_in_rect`, ...), and keep any cross-frame memory in `UiState` by
`WidgetId`. Add it to the gallery so it can be poked at.

Anything that scrolls goes through `scroll_core` (`widgets/scroll.rs`):
it owns the wheel, the bars, the clip and scrolling keyboard focus into
view, and hands the content a `ScrollView` (viewport, offset, origin) so
big content can lay out only what shows. `virtual_list` and `table` are
built on it; a table's rows are declared by the caller from
`Table::visible()`, so ten thousand rows cost what the visible dozen do.

Hit order matters in immediate mode: the first `interact` that contains
the press claims it. A container that wants clicks *and* holds widgets
(a table row) hit-tests twice: once with `Sense::NONE` before its
children, for hover and the background, and once with `Sense::CLICK`
after them, so a widget in a cell wins over the row.
