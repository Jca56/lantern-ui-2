# Lantern UI

The shared UI framework for every Lantern project. Rust, `wgpu`, `winit` —
and nothing else. Born in Lantern Prism, lifted out so any app can stand
on it: a math library, a reflection system, a from-scratch text engine, a 2D
draw-list renderer, an immediate-mode widget set inside a retained area
tree, and a window harness.

```
lntrn-app      winit loop, GPU wiring, event translation      (wgpu, winit)
lntrn-ui       shell, areas, widgets, panels, theme, Host trait
lntrn-render   wgpu wrapper, render graph, 2D pass, shaders   (wgpu)
lntrn-text     fonts, shaping, layout, raster, atlas          GPU-free
lntrn-image    PNG and JPEG decoders, a PNG writer            pure std
lntrn-props    reflection: describe a struct once             pure std
lntrn-core     handles, arenas, chunked vecs, jobs, undo, log pure std
lntrn-math     f64 vectors, matrices, quaternions, rects      pure std
lntrn-demo     the widget gallery and a reference host
```

Nothing depends upward. Everything below `lntrn-render` builds and tests
with no GPU and no display.

## Try it

```sh
cargo run -p lntrn-demo
```

Split and join areas from the `⋮` menu in any header, drag the gaps, drag
a header onto another area to swap them, Ctrl+Space to maximize one, `+`
in a header to stack another editor in it as a tab (Ctrl+Tab cycles).
Right-click the gallery, press F3 for the palette, Tab through everything,
edit the theme live in Preferences (it is saved for next time), rebind
keys in Key Bindings, open a picture from the File menu or drop one on
the window. Slide along the title bar to switch menus; arrow keys walk
them.

## What is in the box

- **Widgets**: labels, headings, paragraphs, buttons, icon buttons (47
  procedural icons, no icon font), toggles, radio groups, tabs, sliders,
  range sliders, vertical faders, an XY pad, drag numbers, spinners,
  knobs, dropdowns, an editable combo box, menu buttons, selectables,
  text fields (placeholders, passwords), a multi-line text area with
  undo, colour picker, tree view, tables with sortable and resizable
  columns whose cells are widgets, virtual lists (ten thousand rows cost
  the same as ten), scroll areas that go down or both ways, level
  meters, waveforms, a curve editor, progress bars, collapsing sections,
  columns, pictures, props-driven panels, a keymap editor.
- **Shell**: title bar with menus (rules, greyed rows, submenus, key
  hints, keyboard navigation), resizable and swappable areas that stack
  editors as tabs, maximize,
  command palette, file browser, modal dialogs (with the host's own
  widgets inside when it wants them), toasts, context menus with
  tool strips, keyboard focus with rings and scroll-into-view, the system
  clipboard (text and pictures), input methods, files dragged in from
  other apps, timed redraws for
  animation (or none, with the reduce-motion preference), a debug overlay
  of what a rebuild costs, preferences and layout saved between runs.
- **Harnesses**: a winit window (`lntrn_app::run`) and an embedded view
  for plugin editors (`lntrn_app::Embedded`) that draws into a window the
  owner supplies.
- **Testing**: `lntrn_ui::testing::Harness` runs any widget code headless
  with synthetic clicks, drags and keys, so behaviour is tested without a
  window.

## Use it from another project

Path dependencies, versions pinned once in the workspace:

```toml
[workspace.dependencies]
lntrn-ui  = { path = "../lantern-ui-2/crates/lntrn-ui" }
lntrn-app = { path = "../lantern-ui-2/crates/lntrn-app" }
```

Then implement `lntrn_ui::Host` (two required methods: `draw_body` and
`run`), build a `Shell`, and call `lntrn_app::run`.

The quickest start is `crates/lntrn-app/examples/template.rs`: one
editor, a menu, a key binding, the palette, and the shell's preferences
editor in about a hundred commented lines. See it run with
`cargo run -p lntrn-app --example template`, then copy it into a new
crate's `src/main.rs` with this manifest:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[dependencies]
lntrn-app = { path = "../lantern-ui-2/crates/lntrn-app" }
lntrn-ui  = { path = "../lantern-ui-2/crates/lntrn-ui" }
```

`crates/lntrn-demo` is the full tour; `docs/ARCHITECTURE.md` explains
the pieces.

## House rules

- Big text, big targets. Sizes are logical pixels in multiples of five.
- Files under 600 lines, flagged at 500.
- Own it before you import it: the only external crates are `wgpu` and
  `winit`, and only in `lntrn-render` and `lntrn-app`.
