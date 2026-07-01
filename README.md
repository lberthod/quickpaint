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

## Build the macOS app (`QuickPaint.app`)

```bash
./make-app.sh        # release build + .icns + QuickPaint.app bundle
open QuickPaint.app
```

## Distribute (`QuickPaint.dmg`)

Three distribution modes, controlled entirely by environment variables:

### 1 — Ad-hoc (local / testing)

No Apple account needed. Gatekeeper will ask for a **right-click → Open** on
another machine the first time.

```bash
./make-dmg.sh
```

### 2 — Self-signed Developer ID (no notarization)

Removes the ad-hoc limitation; Gatekeeper still prompts once on other Macs but
the identity is verified. Requires a **Developer ID Application** certificate in
your keychain (Xcode → Settings → Accounts → Manage Certificates).

```bash
# find your identity
security find-identity -v -p codesigning | grep "Developer ID"

SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" ./make-dmg.sh
```

### 3 — Fully signed + notarized (recommended for distribution)

The app opens with **zero warnings** on any Mac. Requires a paid Apple Developer
account ($99/year).

```bash
# Step 1 — generate an app-specific password at appleid.apple.com
#           (Apple ID → Security → App-Specific Passwords)

# Step 2 — store credentials in the keychain (one-time)
xcrun notarytool store-credentials "quickpaint-notary" --apple-id "you@example.com" --team-id "TEAMID" --password "xxxx-xxxx-xxxx-xxxx"

# Step 3 — build, sign, notarize and staple in one command
SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" NOTARY_PROFILE="quickpaint-notary" ./make-dmg.sh

# Step 4 — verify (should print "source=Notarized Developer ID")
spctl --assess --type open --context context:primary-signature -v QuickPaint.dmg
```

To find your Team ID:
```bash
security find-identity -v -p codesigning | grep "Developer ID"
# → "Developer ID Application: Your Name (ABCDE12345)" — ABCDE12345 is the Team ID
```

## Architecture

`model` (data) · `input` (gesture capture) · `render` (egui rendering + tiny-skia
CPU compositor) · `history` (command-based undo/redo) · `tools` · `ui`.

## License

MIT © Loïc Berthod
