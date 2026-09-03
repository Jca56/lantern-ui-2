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
**Status:** Accepted (Alva, 2026-09-03)
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
**Known limit:** winit 0.30 delivers file drops on X11 only, and even
there without pointer updates during the drag, so drop zones and area
routing are best-effort until winit (or our own harness) does better on
Wayland. The tests cover the whole path so it lights up unchanged when it
does.
