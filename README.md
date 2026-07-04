<div align="center">
  <img src="assets/logo.png" alt="QuickPaint logo" width="160" />

  # QuickPaint

  A **touch-friendly** drawing & photo-editing app for macOS, written in
  **Rust** with **egui/eframe**. **100 % local** — no account, no cloud,
  no telemetry.
</div>

Designed to be as simple as Paint, with the core of PhotoFiltre (light photo
retouching) and Canva (fast composition), built on the architectural
foundations of the big editors: a tiled raster engine (GIMP/Photoshop),
always-editable vector objects (Illustrator), and non-destructive adjustments
(Photoshop).

Author: **Loïc Berthod** — <https://github.com/lberthod>

## Features

- **Drawing**: vector brush (width simulated from stroke speed, Catmull-Rom
  smoothing), plus a tiled **raster engine** with pixel **brush**, **eraser**,
  **clone stamp** (⌥+click = source), **healing brush**, a real flood-fill
  **paint bucket**, and one-click **cutout** (flood-fill + feathering) — all
  with per-tile undo.
- **Local retouching tools**: **dodge/burn**, **sponge**
  (saturate/desaturate), localized **blur/sharpen**, **smudge**,
  **measure ruler**, rotational **mirror drawing**, drag-to-place
  **interactive gradient**.
- **Shapes**: line, arrow, rectangle, ellipse, polygon, star (outline or
  filled, Shift constraint), with **linear/radial gradient fills**.
- **Pen** (Bézier curves) with **node editing after the fact** — double-click
  a pen path to reopen and reshape its anchors/handles — and **path booleans**
  (union / subtract / intersect) on selected filled shapes.
- **Rich text**: system fonts (lazy-loaded), faux-bold, left/center/right
  alignment, outline.
- **Selection**: click, **rectangle (marquee)**, **lasso**, **magic wand**
  (by color, contiguous or global); move, **resize**, **rotate**, duplicate,
  align / distribute (with **smart guides**/snapping), **z-order**,
  **copy/paste style**, named **style presets**.
- **Layers**: visibility, opacity, **blend modes** (multiply, screen…),
  reordering, **groups**, merge / flatten, **clipping masks**, **painted
  layer masks**, non-destructive **adjustment layers** (levels, curves,
  hue/saturation, brightness/contrast, sharpen, invert, grayscale…).
- **Images**: import + **paste (⌘V)**, move, **crop** (free or
  ratio-constrained 1:1 / 4:3 / 16:9 / A4), image & canvas **resize**,
  filters.
- **Templates & assets**: built-in gallery of common formats (social posts,
  presentation, print…), embedded asset/picto library, custom-sized documents.
- **View**: touch zoom/pan, grid + snapping, **rulers**, fixed document size.
- **History**: non-linear (panel + jump straight to any state).
- **Export**: **PNG, JPEG, WebP, PDF**, vector **SVG**, **batch multi-size
  export**; **project save** as `.json`.
- **Customization**: editable color palette, configurable keyboard shortcuts.
- **Languages**: **FR/EN** UI, detected from the system locale at launch
  (switchable anytime from the menu bar, preference persisted).

## Non-goals (by design)

No cloud sync, no real-time collaboration, no third-party API calls, no
telemetry. Everything works offline, on one machine, without an account.

## Install

Grab **QuickPaint.dmg** from the
[Releases page](https://github.com/lberthod/quickpaint/releases), open it and
drag **QuickPaint** to your Applications folder. Signed and notarized — no
Gatekeeper warning.

## Build & run from source

```bash
cargo run --release
```

Requires a recent stable Rust toolchain. Tests: `cargo test` (96 tests, no
window needed — the model, tools and history layers are UI-independent).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — layers, data model, input pipeline,
  rendering, undo/redo design.
- [UX_SPRINTS.md](UX_SPRINTS.md) — UI/UX and functionality audit with a
  5-sprint execution plan, grounded in real screenshots and code review.
- [CHANGELOG.md](CHANGELOG.md) — release history.
- [packaging/SANDBOX_NOTES.md](packaging/SANDBOX_NOTES.md) — Mac App Store
  sandbox validation notes.

Older planning documents (feature roadmap, sprint-by-sprint log, the initial
technical audit) were retired once acted upon — see the git log for that
history.

## License

MIT © Loïc Berthod — see [LICENSE](LICENSE).
