<div align="center">
  <img src="assets/logo.png" alt="QuickPaint logo" width="160" />

  # QuickPaint

  A **touch-friendly** drawing editor for macOS, written in **Rust** with **egui/eframe**.
</div>

Designed to be as simple as Paint, but with modern features (layers, blend modes,
shapes, text, transforms).

Author: **Loïc Berthod** — <https://github.com/lberthod>

## Features

- **Drawing**: vector brush (width simulated from stroke speed, Catmull-Rom
  smoothing), plus a tiled **raster engine** (F1) with pixel **brush**,
  **eraser**, **clone stamp** (⌥+click = source), and a real flood-fill
  **paint bucket** — all with per-tile undo.
- **Shapes**: line, arrow, rectangle, ellipse, polygon, star
  (outline or filled, Shift constraint), with **linear/radial gradient fills**.
- **Pen** (Bézier curves) with **node editing after the fact** — double-click a
  pen path to reopen and reshape its anchors/handles — and **path booleans**
  (union / subtract / intersect) on selected filled shapes.
- **Rich text**: system fonts (lazy-loaded), faux-bold, left/center/right
  alignment, outline.
- **Selection**: click, **rectangle (marquee)**, **lasso**, **magic wand**
  (by color); move, **resize**, **rotate**, duplicate, align / distribute
  (with **smart guides**/snapping), **z-order**, **copy/paste style**.
- **Layers**: visibility, opacity, **blend modes** (multiply, screen…),
  reordering, **groups**, merge / flatten, **clipping masks**, **painted layer
  masks**, non-destructive **adjustment layers** (brightness/contrast/
  saturation/sharpen/invert/grayscale).
- **Images**: import + **paste (⌘V)**, move, **crop** (free or ratio-constrained
  1:1 / 4:3 / 16:9 / A4), filters (brightness, grayscale, blur).
- **Templates**: built-in gallery of common Canva-like formats (social posts,
  presentation, print…) plus custom-sized documents.
- **View**: touch zoom/pan, grid + snapping, **rulers**, fixed document size.
- **History**: non-linear (panel + jump straight to any state).
- **Export**: **PNG, JPEG, WebP, PDF**, vector **SVG**; **project save** as `.json`.
- **Languages**: **FR/EN** UI, detected from the system locale at launch
  (switchable anytime from the menu bar, preference persisted).

## Build & run

```bash
cargo run --release
```

## Install

Download **[QuickPaint.dmg](QuickPaint.dmg)**, open it and drag **QuickPaint** to
your Applications folder. Signed and notarized — no Gatekeeper warning.

## Architecture

`model` (data) · `input` (gesture capture) · `render` (egui rendering + tiny-skia
CPU compositor) · `history` (command-based undo/redo) · `tools` · `ui` · `i18n`
(FR/EN string resolution).

## License

MIT © Loïc Berthod
