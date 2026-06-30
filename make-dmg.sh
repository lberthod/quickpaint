#!/usr/bin/env bash
# Construit QuickPaint.app puis l'empaquette dans QuickPaint.dmg
# (avec un lien « Applications » pour l'installation par glisser-déposer).
set -euo pipefail
cd "$(dirname "$0")"

APP="QuickPaint.app"
DMG="QuickPaint.dmg"
VOL="QuickPaint"

# 1) Construire le bundle (build release + icône + Info.plist).
./make-app.sh

# 2) Signature ad-hoc (évite l'erreur « endommagé » en local ; sans Developer
#    ID, Gatekeeper demandera quand même « clic droit → Ouvrir » ailleurs).
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true

# 3) Dossier de mise en scène : l'app + un raccourci vers /Applications.
STAGING="$(mktemp -d)/dmg"
mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

# 4) Image disque compressée.
rm -f "$DMG"
hdiutil create -volname "$VOL" -srcfolder "$STAGING" -ov -format UDZO "$DMG" >/dev/null

echo "✓ $DMG prêt ($(du -h "$DMG" | cut -f1)) — glissez QuickPaint vers Applications."
