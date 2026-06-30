#!/usr/bin/env bash
# Construit QuickPaint.app : build release → icône .icns → bundle .app.
set -euo pipefail
cd "$(dirname "$0")"

APP="QuickPaint.app"
BIN="quickpaint"

echo "▸ Build release"
cargo build --release

echo "▸ Génération de l'icône (.icns)"
ICONSET="$(mktemp -d)/QuickPaint.iconset"
mkdir -p "$ICONSET"
for sz in 16 32 128 256 512 1024; do
  ./target/release/$BIN --dump-icon "$ICONSET/icon_${sz}x${sz}.png" "$sz" >/dev/null
done
./target/release/$BIN --dump-icon "$ICONSET/icon_16x16@2x.png"   32   >/dev/null
./target/release/$BIN --dump-icon "$ICONSET/icon_32x32@2x.png"   64   >/dev/null
./target/release/$BIN --dump-icon "$ICONSET/icon_128x128@2x.png" 256  >/dev/null
./target/release/$BIN --dump-icon "$ICONSET/icon_256x256@2x.png" 512  >/dev/null
./target/release/$BIN --dump-icon "$ICONSET/icon_512x512@2x.png" 1024 >/dev/null
iconutil -c icns "$ICONSET" -o /tmp/QuickPaint.icns

echo "▸ Assemblage du bundle"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$BIN" "$APP/Contents/MacOS/$BIN"
cp /tmp/QuickPaint.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>QuickPaint</string>
    <key>CFBundleDisplayName</key>     <string>QuickPaint</string>
    <key>CFBundleExecutable</key>      <string>quickpaint</string>
    <key>CFBundleIdentifier</key>      <string>com.lberthod.quickpaint</string>
    <key>CFBundleIconFile</key>        <string>AppIcon</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>0.1.0</string>
    <key>CFBundleVersion</key>         <string>0.1.0</string>
    <key>LSMinimumSystemVersion</key>  <string>10.15</string>
    <key>NSHighResolutionCapable</key> <true/>
    <key>NSHumanReadableCopyright</key> <string>© Loïc Berthod</string>
</dict>
</plist>
PLIST

echo "✓ $APP prêt — lancez : open $APP"
