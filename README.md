<div align="center">
  <img src="assets/logo.png" alt="QuickPaint logo" width="160" />

  # QuickPaint

  A **touch-friendly** drawing editor for macOS, written in **Rust** with **egui/eframe**.
</div>

Designed to be as simple as Paint, but with modern features (layers, blend modes,
shapes, text, transforms).

Author: **Loïc Berthod** — <https://github.com/lberthod>

## Features

- **Drawing**: brush (width simulated from stroke speed, Catmull-Rom smoothing),
  **object** or **partial** eraser, paint bucket, color picker.
- **Shapes**: line, arrow, rectangle, ellipse, polygon, star
  (outline or filled, Shift constraint).
- **Pen** (Bézier curves) and editable **text**.
- **Selection**: move, **resize**, **rotate**, duplicate,
  align / distribute, **z-order** (bring to front / send to back).
- **Layers**: visibility, opacity, **blend modes** (multiply, screen…),
  reordering, **groups**, merge / flatten.
- **Images**: import + **paste (⌘V)**, move, **crop**, filters
  (brightness, grayscale, blur).
- **View**: touch zoom/pan, grid + snapping, fixed document size.
- **History**: non-linear (panel + jump straight to any state).
- **Export**: PNG, vector **SVG**; **project save** as `.json`.

## Build & run

```bash
cargo run --release
```

## Build the macOS app (`QuickPaint.app`)

```bash
./make-app.sh        # release build + .icns + QuickPaint.app bundle
open QuickPaint.app
```

## Distribute (`QuickPaint.dmg`)

```bash
./make-dmg.sh        # builds the app + a .dmg (drag to Applications)
```

> Without an Apple Developer account, the app is **ad-hoc** signed: on another
> machine, Gatekeeper will require a **right-click → Open** the first time.

## Architecture

`model` (data) · `input` (gesture capture) · `render` (egui rendering + tiny-skia
CPU compositor) · `history` (command-based undo/redo) · `tools` · `ui`.

## License

MIT © Loïc Berthod
