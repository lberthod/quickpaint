# Changelog — QuickPaint

Versions alignées sur les sprints. Détail complet : [SPRINTS.md](SPRINTS.md)
et le journal git.

## 0.12.1 — juillet 2026 (Sprint 13.8 — suite du découpage de `app.rs`)

- `app/mod.rs` → `app/mod.rs` + `app/transform.rs` : la machine à états de
  transformation interactive de la sélection (poignées d'échelle/rotation,
  glissé, aperçu, undo dédié) extraite en sous-module, même schéma que
  `app/pen_edit.rs` (Sprint 12). `app/mod.rs` passe de 4 444 à 4 297 lignes.
  1 nouveau test unitaire (un scale/rotate en dessous du seuil de bruit ne
  pousse pas de commande d'undo).

## 0.12.0 — juillet 2026 (Sprint 12 — qualité, à partir de l'audit ANALYSE.md)

Détail complet et mesures : [SPRINTANALYSIS.md](SPRINTANALYSIS.md).

- **Fluidité** : le compositeur ne re-rastérise plus la surface peinte
  entière à chaque coup de pinceau pixel — cache incrémental par tuile
  (≈103× plus rapide sur un calque 4096×4096 déjà bien rempli, mesuré).
- **Export fidèle** : PNG/JPEG/WebP/PDF (simple et par lots) rendent le
  document à sa résolution native via le compositeur, au lieu de recadrer
  une capture d'écran du viewport — la résolution exportée ne dépend plus
  du zoom ni de la taille de la fenêtre.
- **Robustesse** : version de format projet, erreurs de chargement
  explicites (fichier corrompu, dimensions hors bornes) au lieu d'un échec
  silencieux, bornage du collage presse-papiers et des redimensionnements.
- **Maintenabilité** : `app.rs` → `app/mod.rs` + `app/pen_edit.rs` (édition
  de nœuds Bézier après coup extraite en sous-module, testable seule).

## 0.11.0 — juillet 2026 (Sprint 11)

- 10 nouveaux outils de retouche & composition : densité -/+ (dodge/burn),
  éponge (saturer/désaturer), flou & netteté localisés, estompe (smudge),
  règle/mesure, dessin en miroir rotatif, dégradé interactif au glisser.
- Icônes vectorielles Phosphor (nettes à toute taille) en remplacement des
  emojis système.
- Docs : ARCHITECTURE.md réécrit (état réel), ANALYSE.md (audit complet),
  LICENSE (MIT), CHANGELOG, CI GitHub Actions.

## 0.10.0 (Sprint 10)

- Bibliothèque d'assets embarquée, templates riches, presets de style nommés.

## 0.9.x (Sprint 9)

- Détourage en un clic (flood-fill + adoucissement), sélection globale
  (non contiguë) à la baguette, restauration.

## 0.8.0 (Sprint 8)

- Réglages continus non destructifs : niveaux, teinte/saturation, courbes ;
  outil correcteur (guérison).

## 0.7.0 (Sprint 7)

- Palette de couleurs personnalisable, raccourcis clavier configurables,
  export par lots multi-tailles.

## 0.6.0 (Sprint 6 + i18n)

- Moteur raster tuilé (fondation F1) : pinceau/gomme pixel, undo par tuile.
- Masques de calque peints, dégradés de remplissage, édition de nœuds Bézier
  après coup, booléens de chemins.
- i18n FR/EN complet, détection de la langue système.

## 0.1.0 → 0.5.0 (Sprints 1–5)

- MVP : trait lissé (Catmull-Rom), pression simulée par la vitesse, calques,
  undo/redo, export PNG.
- Sélection rectangle/lasso/baguette, texte riche, export multi-format
  (JPEG/WebP/PDF/SVG), règles, recadrage à ratio, masque d'écrêtage,
  filtres photo, modes de fusion, plume, grille/magnétisme.
- Release signée Developer ID + notarisée (DMG).
