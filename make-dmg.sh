#!/usr/bin/env bash
# Construit QuickPaint.app puis l'empaquette dans QuickPaint.dmg
# (avec un lien « Applications » pour l'installation par glisser-déposer).
set -euo pipefail
cd "$(dirname "$0")"

APP="QuickPaint.app"
DMG="QuickPaint.dmg"
VOL="QuickPaint"

# Variables d'environnement (toutes optionnelles) :
#   SIGN_IDENTITY  → "Developer ID Application: Nom (TEAMID)" : signature
#                    Developer ID (sinon make-app.sh fait une signature ad-hoc).
#   NOTARY_PROFILE → nom du profil enregistré via
#                    `xcrun notarytool store-credentials` : déclenche la
#                    notarisation Apple + l'agrafage du ticket.
# Exportées pour que le sous-processus make-app.sh en hérite.
export SIGN_IDENTITY="${SIGN_IDENTITY:-}"
export NOTARY_PROFILE="${NOTARY_PROFILE:-}"

# 1) Construire ET signer le bundle (la signature est gérée par make-app.sh
#    selon SIGN_IDENTITY : Developer ID si défini, sinon ad-hoc).
./make-app.sh

# 2) Dossier de mise en scène : l'app + un raccourci vers /Applications.
STAGING="$(mktemp -d)/dmg"
mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

# 3) Image disque compressée.
rm -f "$DMG"
hdiutil create -volname "$VOL" -srcfolder "$STAGING" -ov -format UDZO "$DMG" >/dev/null

# 4) Distribution : si on a une identité Developer ID, signer le .dmg lui-même ;
#    si un profil de notarisation est fourni, notariser puis agrafer le ticket
#    (l'app s'ouvre alors sans « clic droit → Ouvrir » sur n'importe quel Mac).
if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "▸ Signature du .dmg"
  codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
fi
if [[ -n "$NOTARY_PROFILE" ]]; then
  echo "▸ Notarisation (peut prendre quelques minutes)…"
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  echo "▸ Agrafage du ticket"
  xcrun stapler staple "$DMG"
  spctl --assess --type open --context context:primary-signature -v "$DMG" || true
fi

echo "✓ $DMG prêt ($(du -h "$DMG" | cut -f1)) — glissez QuickPaint vers Applications."
