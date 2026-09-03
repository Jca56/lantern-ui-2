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
               parallel_for), block_on, bytes (Pod), Pcg32, log.
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
areas. Each area has an editor kind `E` (the host's enum) and a state `S`
(the host's per-area data: a camera, a selection; `()` when none). It
changes only when the user splits, joins or drags a separator.

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
open a menu, the palette, a path dialog, a context menu; flip a preference;
rebuild; quit. The shell applies them at the end of the rebuild.

**Context menus** — the host builds a `ContextMenu` (title, tabs of items,
submenus, live `props!` panels with Apply, custom rows it draws itself, a
tool strip, a bar) from whatever was under the pointer and requests it.
After a tool runs, `Host::refresh_context_menu` brings it up to date.

**Capture** — a running tool (a modal operator) can claim presses and keys
through `Host::capture`; the shell still routes motion, releases and the
wheel to widgets and hands the tool every event through `Host::captured`.

**Rebuild on demand** — a rebuild happens only when an event arrives. A
popup closing or a value committing asks for one more (`rebuild_again`);
the app caps that at four before presenting.

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

`Pass2d` draws the whole `DrawList` — panels, strokes, icons and glyph
quads — in one pass from one vertex stream against the text atlas, with a
clip stack and layers (0 areas, 1 popups, 2 context-menu content, 3
tooltips). A 3D host adds its nodes to the render graph between the clear
and the UI pass; the UI composites over them.

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
user's UI scale preference.

## 7. Adding a widget

A widget is a method on `Ui` in `lntrn-ui/src/widgets/`: allocate a rect
(`alloc`), hit-test it (`interact` with a `Sense`), read the `Response`,
draw with the theme helpers (`raised`, `recessed`, `fill_shaded`,
`text_in_rect`, ...), and keep any cross-frame memory in `UiState` by
`WidgetId`. Add it to the gallery so it can be poked at.
