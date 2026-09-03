# Lantern UI — Decision Log

Decisions inherited from Lantern Prism keep their Prism numbers, because
the code comments cite them (`D016`, `D017`, `D018`, ...). New decisions
made for the shared framework are numbered `U001`, `U002`, ...

## D001 — External dependencies: wgpu and winit only
**Status:** Accepted
**Decision:** Two external crates, in two crates: `wgpu` in `lntrn-render`,
`wgpu` + `winit` in `lntrn-app`. Everything else is written here.
**Why:** Fully ours. A dependency we did not write is a dependency we
cannot fix, resize, or make bigger.

## D004 — Precision: f64 on CPU, f32 as a transmission format
**Status:** Accepted
**Decision:** `lntrn-math` has no `f32` types. Geometry is `f64`; `to_gpu()`
converts at the upload boundary. Text is the exception: it is a picture,
not geometry, and lives in pixel space and `f32`.

## D006 — Reflection / property system is the keystone
**Status:** Accepted
**Decision:** `props!` describes a struct once: field ids, labels, defaults,
ranges, subtypes, docs. From that description come the UI (`Ui::props_panel`),
serialization (`lntrn_props::serial`), and whatever comes later. The theme
and preferences are `props!` structs, so the Preferences editor is free.

## D011 — Layered crates; GPU-free below `render`
**Status:** Accepted
```
lntrn-app       winit loop, wiring
lntrn-ui        shell, areas, widgets, panels (driven by props)
lntrn-render    render graph, wgpu wrap, 2D pass, shaders
lntrn-text      fonts, shaping, layout, raster, atlas packing
lntrn-image     PNG, JPEG
lntrn-props     reflection / property system
lntrn-core      containers, jobs, log
lntrn-math      vec / mat / quat / rect / color (f64)
```
Nothing depends upward. Everything below `lntrn-render` builds and tests
without a GPU or window. Files stay under 600 lines (flag at 500).

## D016 — UI model: immediate-mode widgets inside a retained area tree
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** The screen → area tree is retained (plain data an app may
save). Inside an area, widgets are re-declared on every rebuild (immediate
mode). Per-widget persistent state lives in a small map keyed by a stable
widget id. Props-driven auto panels are a walk over `Reflect` fields that
emits one widget per field.
**Why:** Blender's model. Least code, easiest to reason about, and auto
panels fall out for free. Redraw-on-demand is unaffected.
**Rejected:** Retained widget tree — two-way binding plumbing for every
props panel, far more code for the same result.

## D017 — Keyboard focus: click-to-focus, hover-focus as a preference
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** Keyboard events route to the last-clicked area; wheel routes
to the area under the cursor. The focused area draws a visible border. The
`focus_follows_mouse` preference switches to hover focus; one code path.

## D018 — Text: `lntrn-text` is GPU-free; one 2D pass
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** The text engine (own TTF/CFF parser, GSUB/GPOS shaping,
UAX#9/14/24/29, scanline rasterizer, variable fonts, COLR/CBDT emoji, glyph
atlas) owns atlas *packing* into a CPU RGBA image with dirty rects;
`lntrn-render` uploads that image and draws glyph quads through the same
2D draw-list pass that draws rects, lines and icons.
**Why:** One pipeline for all 2D means one vertex stream, one atlas
texture, one clip stack, and a 3D view composites into the same frame.
Keeping text GPU-free keeps it in the headless test set.

## D019 — Color pipeline: sRGB surface, linear shading, one conversion point
**Status:** Accepted
**Decision:** Theme colors are sRGB. The surface prefers an sRGB format;
`Color::to_linear` at the upload boundary is the one conversion.

## D023 — The right-click context menu is the primary discoverable surface
**Status:** Accepted
**Decision:** A titled panel with tabs, rows, submenus, live property
panels, a tool strip floating down its left edge, and an optional bar
above. Built by the host from whatever was under the pointer; an outside
press closes it *and* falls through, so one click selects the next thing.

## D035 — Menus on the title bar
**Status:** Accepted
**Decision:** The shell draws the window frame (undecorated window). The
host's named menus sit on the left of the title bar; the title and a
status line in the middle; minimize / maximize / close on the right; the
rest drags.

---

## U001 — Lift the framework out of Prism into `lntrn-*` 0.2.0
**Status:** Accepted (Alva, 2026-09-03)
**Decision:** The eight UI-side crates of Lantern Prism (`math`, `core`,
`props`, `image`, `text`, `render`, `ui`, `app`) move here under the
`lntrn-` prefix at version 0.2.0, whole — including the 3D math types and
the job system the UI does not need — so every Lantern project shares one
foundation. Lantern-DE's older `lntrn-text` / `lntrn-ui` are superseded;
the version bump keeps Cargo from confusing the two if a workspace ever
holds both.
**Why:** The first `lntrn-ui` was not good; Prism's is, and it was already
a port of `lntrn-text`. One home beats three copies drifting apart.

## U002 — The shell is generic over a `Host` trait
**Status:** Accepted (Alva, 2026-09-03)
**Decision:** Prism's shell reached into its document, operator executor
and viewport directly. Here `Shell<H: Host>` knows none of them. The host
provides: its editor kinds and labels, per-area state, the title and
menus, palette entries, how to draw each area, what an `Action` id does,
how to apply a props panel, how to draw a custom row, how to resolve a key,
whether a tool captures input, and how to refresh a context menu. Every
door — menu row, palette entry, tool, bar button, key binding, path dialog
— produces an `Action { id, args }`; `shell.*` ids the shell handles
itself, the rest go to `Host::run`. The host talks back with
`ShellRequest`s (open a menu / palette / path dialog / context menu, toggle
a preference, rebuild, quit) that the shell applies at the end of the
rebuild.
**Why:** The shell stays whole (title bar, areas, split/join/drag, focus,
popups, palette, file browser, context menu) and every app gets all of it
by implementing two required methods. Prism re-implements its editors on
top rather than patching a fork.
**Rejected:** Shipping only building blocks — every app rewrites the same
300-line frame loop, and they drift.

## U003 — Shaders live inside the crate that uses them
**Status:** Accepted (Alva, 2026-09-03)
**Decision:** `ui.wgsl` is embedded from `crates/lntrn-render/shaders/`,
not a workspace-level directory, so the crate works as a path dependency
from any repo. Apps with shaders of their own preprocess them with
`shader::load_with`, which lets them `#include` the built-in files.

## U005 — A headless harness, and widgets record their rects
**Status:** Accepted (2026-09-03)
**Decision:** `UiState::record_rects` makes every `interact` call remember
its rect; `testing::Harness` runs frames from synthetic events and clicks
widgets by id. Tests for widgets and the shell are headless integration
tests under `crates/lntrn-ui/tests/`.
**Why:** Alva tests the window; the framework must test itself. The
harness found real bugs the first day (below).

## U006 — Per-widget memory is keyed by (id, kind)
**Status:** Accepted (2026-09-03)
**Decision:** The memory map's key is the widget id *and* the kind of
memory. Before, a number field's typing buffer and drag origin shared one
slot and clobbered each other every frame, so dragging a value snapped it
toward zero. Inherited from Prism; caught by the harness.

## U007 — Keyboard focus is a Tab order the widgets build each frame
**Status:** Accepted (2026-09-03)
**Decision:** Widgets call `focusable(id, rect)`; the shell moves focus
along that order on an unconsumed Tab at the end of the frame; Enter and
Space click, arrows step, rings show only after keyboard use, scroll areas
scroll to the focused widget. Text fields leave Tab alone.
**Why:** Accessibility is a requirement (ARCHITECTURE §1). Building the
order from declaration keeps it right as panels change.

## U008 — Timed redraws, not a render loop
**Status:** Accepted (2026-09-03)
**Decision:** A widget that animates asks for a rebuild after a delay
(`request_redraw_after`); the shell reports the soonest; the harness sleeps
with `WaitUntil`. `Ui::animate` eases values and settles to silence.
**Why:** Zero percent idle stays true; hover fades, toasts and busy bars
still move.

## U009 — Edits replay in arrival order
**Status:** Accepted (2026-09-03)
**Decision:** Key presses and committed text each carry a sequence number
from the frame's event stream; editors take both as one ordered list.
**Why:** Keys and text were two streams; "hello, Enter, world" typed fast
enough to land in one frame came out as "\nhelloworld".

## U010 — Preferences as tagged bytes, layout as one line of text
**Status:** Accepted (2026-09-03)
**Decision:** `Prefs` saves through `lntrn_props::serial` behind a magic
prefix; the area tree saves as `(h 0.62 [Gallery] (v 0.6 [Prefs] [Notes]))`
with editor names the host maps back. Files live in
`~/.config/<app_id>/`, written atomically, loaded by `lntrn_app::run`.
**Why:** The serializer already survives renames and reorders; the layout
is small enough that a readable line beats a format.

## U011 — Pictures bind per run inside the one 2D pass
**Status:** Accepted (2026-09-03)
**Decision:** `Images` uploads sRGB textures with mipmaps and hands out
copyable handles. Image quads carry the image id in a spare vertex slot;
the pass splits the vertex stream into runs by image and binds group 1
per run. A frame with no pictures is still one draw call.
**Why:** No second pipeline, no sorting, no change to layers or clipping.

## U012 — An embedded harness for plugin editors
**Status:** Accepted (2026-09-03)
**Decision:** `lntrn_app::Embedded` runs the same shell and frame code
against a surface made from raw window handles, with events pushed by the
owner and no winit. The window harness and it share `frame.rs`.
**Why:** Lantern's VST3 plugins need this exact widget set inside a DAW's
window; keeping one frame path means the plugin face and the app face
cannot drift.

## U004 — Theme carries no app-specific colors
**Status:** Accepted (Alva, 2026-09-03)
**Decision:** Prism's per-object-kind colors and its Object/Edit mode
color left the theme. A host that wants more colors keeps its own `props!`
struct and shows it in its Preferences editor beside the shell's `Prefs`.
The focus outline and the context menu's default outline are `theme.focus`;
a context menu may set its own `outline`.

## U013 — The system clipboard through wl-clipboard, for now
**Status:** Superseded by U018 the same day: on Lantern-DE the tools
fall back to a focus-stealing window.
**Decision:** `lntrn_app::clipboard` spawns `wl-copy` and `wl-paste` when
they are on the PATH under Wayland: a read before any rebuild that carries
a paste key, a write after any rebuild in which a widget copied. Without
the tools the clipboard stays in-app. The widgets and the shell never know
which; they talk to `UiState::clipboard` only.
**Why:** The Lantern-DE apps speak `zwlr_data_control_v1` through the
`wayland-client` crate, which D001 rules out here. A from-scratch
data-control client over the Wayland socket is the planned replacement;
it slots in behind the same two functions.
**Rejected:** A crate. A second Wayland connection now (a day's work that
would delay the widgets every app is waiting for).

## U014 — Files from outside and input methods travel the event vocabulary
**Status:** Accepted (2026-09-03)
**Decision:** `Event::FileHovered` / `FileHoverLeft` / `FileDropped` and
`Event::ImePreedit` join the vocabulary. A `Ui::drop_zone` under the
pointer takes a drop; what no zone takes reaches `Host::dropped` with the
area under the pointer. A composition shows inline at the focused
widget's caret; the shell reports the caret rect so the harness can place
the input method's window.
**Wayland:** winit 0.30 delivers file drops on X11 only, so on Wayland
the harness takes them itself, over the data device it owns for the
clipboard (U018): it binds at version 3, accepts `text/uri-list` on
enter, negotiates a copy, reads the list while the drag still hovers,
and reports every motion as a pointer position, so drop zones and area
routing are exact there. On X11 winit's events stand, pointerless.

## U015 — Tables: the caller declares the visible rows and owns the data
**Status:** Accepted (2026-09-03)
**Decision:** `Ui::table` draws the header (widths remembered per table,
dragged at the grips, click to sort) and a body that scrolls both ways;
the caller lays rows out itself from `Table::visible()`, saying per row
whether it is selected and filling each cell with any widget through a
`Cell` (a `Ui` positioned in the cell, plus aligned `label`). The table
reports clicks, the sort the header asks for, and keyboard steps; the
caller sorts and selects.
**Why:** No data model to bind to means no borrow puzzle: the caller's
data is touched only inside its own closure, edited in place by cell
widgets. Selection and order stay wherever the app keeps them (a
document, an undo stack).
**Rejected:** A table that takes rows as strings (no widgets in cells, and
every app formats twice). A retained model with two-way binding (D016).

## U016 — Undo is a stack of snapshots
**Status:** Accepted (2026-09-03)
**Decision:** `lntrn_core::Undo<T>` keeps whole snapshots (capped, with
time-based coalescing for bursts). A host clones its document before an
edit and pushes the clone; Ctrl+Z swaps it back. No command objects.
**Why:** `ChunkedVec` makes a document clone O(chunks) and every edit
stamps a version, so snapshots are cheap and cache keys stay sound
across undo branches. One mechanism for every app, and for the text
widgets' own history.
**Rejected:** Command objects with `apply`/`revert` — every operation
written twice, and the second copy is where the bugs live.

## U017 — Accessibility switches live in the shell's preferences
**Status:** Accepted (2026-09-03)
**Decision:** `reduce_motion` (no easing, fades or sweeps) and
`debug_overlay` are shell preferences beside `ui_scale`. The framework
honours them itself — `Ui::animate`, toasts and busy bars read
`UiState::reduce_motion` — so a host gets them without writing a line.
Menu rows that toggle a preference show its state; the shell fills the
check in when the menu opens.
**Why:** Big text, big targets, and no motion the user did not ask for
are requirements (ARCHITECTURE §1), not features an app opts into.

## U018 — The clipboard on the window's own Wayland connection
**Status:** Accepted (2026-09-03)
**Decision:** `lntrn_app::wayland` speaks to the libwayland-client winit
already loaded (found again with `dlopen`, called through function
pointers, with interface tables of our own that are checked against
their signatures at compile time). It binds a `wl_data_device` and a
`wl_keyboard` of its own on the window's seat, on a private event queue
the harness dispatches once per loop turn. A copy sets a `wl_data_source`
with the latest keyboard serial and serves `send` from the queue; a paste
`receive`s the selection's offer through a socket pair with a timeout.
The embedded harness does the same with the owner's display handle.
**Why:** `wl-copy` and `wl-paste` fall back to an invisible focus-stealing
window when a compositor keeps `zwlr_data_control` from untrusted
clients, as Lantern-DE does. Every copy and paste took focus from the
window, and the focus bounce typed the shortcut's letter. The core
protocol needs no trust, no second connection and no window: it is what
every Wayland app does.
**Rejected:** A data-control client on a second connection (the same
trust filter blocks it). Reaching winit's proxies through the
`wayland-client` crate (D001).
**Learned:** winit's Wayland loop drops a wake-up that brought it no
events of its own when the app sits in plain `Wait`, and another app's
paste request is exactly such a wake-up: it lands on the clipboard's
queue alone. Left unanswered, the paster waits for an EOF that never
comes (Lantern-DE's terminal reads a paste with no timeout, so its
clipboard thread stuck for good). The harness therefore idles with a
far `WaitUntil` deadline instead of `Wait` whenever it has a system
clipboard to serve; `examples/clipboard_probe.rs` reproduces the hang
and the cure.
**Pictures:** the same source and offer carry `image/png`. A host puts an
`Image` in `UiState::set_clipboard_image` (pushed out through
`lntrn_image::encode_png`, a stored-deflate writer: exact pixels, no
compression) or sets `clipboard_image_wanted` and finds the decoded
picture in `clipboard_image` on the next rebuild.

## U019 — An area is a stack of tabs
**Status:** Accepted (2026-09-03)
**Decision:** `Area` holds `tabs: Vec<Tab { editor, state }>` and a
`current`; the header shows a strip once there are two, a `+` that lists
the editors, and a *Close Tab* row in its `⋮` menu. `shell.next_tab` and
`shell.prev_tab` cycle. The saved layout writes a leaf as
`[Gallery|Notes*]`, the shown tab starred; a one-tab leaf stays `[Name]`,
so old layout files still read. Per-tab state means a camera or a
selection survives switching tabs.
**Why:** Blender's workspaces without the workspace: the tree of splits
stays the layout, and a tab is just another editor in the same place.
**Rejected:** Tabs as a widget the host draws itself in every editor
(every app would rebuild the same strip and lose it in the saved layout).
