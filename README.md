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
- **Pen** (Bézier curves) and **rich text** (proportional / monospace font,
  faux-bold, left/center/right alignment, outline).
- **Selection**: click, **rectangle (marquee)**, **lasso**, **magic wand**
  (by color); move, **resize**, **rotate**, duplicate, align / distribute,
  **z-order** (bring to front / send to back).
- **Layers**: visibility, opacity, **blend modes** (multiply, screen…),
  reordering, **groups**, merge / flatten.
- **Images**: import + **paste (⌘V)**, move, **crop** (free or ratio-constrained
  1:1 / 4:3 / 16:9 / A4), filters (brightness, grayscale, blur).
- **View**: touch zoom/pan, grid + snapping, **rulers**, fixed document size.
- **History**: non-linear (panel + jump straight to any state).
- **Export**: **PNG, JPEG, WebP, PDF**, vector **SVG**; **project save** as `.json`.

## Build & run

```bash
cargo run --release
```

## Install

Download **[QuickPaint.dmg](QuickPaint.dmg)**, open it and drag **QuickPaint** to
your Applications folder. Signed and notarized — no Gatekeeper warning.

## Architecture

`model` (data) · `input` (gesture capture) · `render` (egui rendering + tiny-skia
CPU compositor) · `history` (command-based undo/redo) · `tools` · `ui`.

## License

MIT © Loïc Berthod
