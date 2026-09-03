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

## U004 — Theme carries no app-specific colors
**Status:** Accepted (Alva, 2026-09-03)
**Decision:** Prism's per-object-kind colors and its Object/Edit mode
color left the theme. A host that wants more colors keeps its own `props!`
struct and shows it in its Preferences editor beside the shell's `Prefs`.
The focus outline and the context menu's default outline are `theme.focus`;
a context menu may set its own `outline`.
