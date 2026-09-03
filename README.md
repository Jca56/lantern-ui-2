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
lntrn-image    PNG and JPEG decoders                          pure std
lntrn-props    reflection: describe a struct once             pure std
lntrn-core     handles, arenas, chunked arrays, jobs, log     pure std
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
a header onto another area to swap them, Ctrl+Space to maximize one.
Right-click the gallery, press F3 for the palette, Tab through everything,
edit the theme live in Preferences (it is saved for next time), rebind
keys in Key Bindings, open a picture from the File menu.

## What is in the box

- **Widgets**: labels, headings, paragraphs, buttons, icon buttons,
  toggles, tabs, sliders, drag numbers, knobs, dropdowns, menu buttons,
  selectables, text fields, a multi-line text area with undo, colour
  picker, tree view, progress bars, collapsing sections, scroll areas,
  columns, pictures, props-driven panels, a keymap editor.
- **Shell**: title bar with menus, resizable and swappable areas, maximize,
  command palette, file browser, modal dialogs, toasts, context menus with
  tool strips, keyboard focus with rings and scroll-into-view, an in-app
  clipboard, timed redraws for animation, preferences and layout saved
  between runs.
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
`run`), build a `Shell`, and call `lntrn_app::run`. `crates/lntrn-demo` is
the smallest complete example; `docs/ARCHITECTURE.md` explains the pieces.

## House rules

- Big text, big targets. Sizes are logical pixels in multiples of five.
- Files under 600 lines, flagged at 500.
- Own it before you import it: the only external crates are `wgpu` and
  `winit`, and only in `lntrn-render` and `lntrn-app`.
