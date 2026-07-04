# Architecture — QuickPaint

> État actuel du code (juillet 2026). Le document de conception d'origine
> (pré-projet, cible ViewSonic TD1655) a été remplacé par cette version qui
> décrit ce qui est **réellement implémenté**. L'historique des décisions
> vit dans le journal git.

## 1. Vue en couches

```
┌────────────────────────────────────────────────────────────┐
│ ui/          toolbar, layers, footer            (egui)     │
├────────────────────────────────────────────────────────────┤
│ app/         état global (mod.rs), machine à états des     │
│              outils, sélection/transformation, dialogues ; │
│              pen_edit.rs = édition de nœuds Bézier après    │
│              coup, extrait en sous-module (ANALYSE.md §12.5)│
├────────────────────────────────────────────────────────────┤
│ tools/       brush, eraser, pen, shape, bucket, boolean,   │
│              guides, filter, assets, eyedropper, hit       │
├────────────────────────────────────────────────────────────┤
│ history.rs   pattern Command — undo/redo non linéaire      │
├────────────────────────────────────────────────────────────┤
│ model/       document, layer, stroke, text, image,         │
│              raster (tuiles 256×256)                       │
├────────────────────────────────────────────────────────────┤
│ input/       capture, lissage (EMA + Catmull-Rom),         │
│              pression simulée (vitesse → épaisseur)        │
├────────────────────────────────────────────────────────────┤
│ render/      compositor tiny-skia (CPU), ribbon,           │
│              canvas, text                                  │
├────────────────────────────────────────────────────────────┤
│ transverse   i18n (FR/EN), export, project, svg, fonts,    │
│              keybindings, icon                             │
└────────────────────────────────────────────────────────────┘
```

Principe directeur : **le modèle ne sait pas qu'il est affiché**. `model/`,
`tools/`, `history` et `input/` se testent sans fenêtre — c'est là que vivent
les ~96 tests unitaires.

## 2. Modèle de document (hybride vectoriel + raster)

- **Vectoriel** : chaque calque porte des `Stroke` (points + style + dégradé
  optionnel + ancres Bézier conservées pour la réédition), des `TextItem` et
  des `ImageItem`. Tout élément reste éditable après coup (logique
  Illustrator).
- **Raster tuilé** ([model/raster.rs](src/model/raster.rs)) : les pixels
  peints (pinceau/gomme pixel, pot de peinture, clonage, correcteur,
  dodge/burn/éponge/flou/netteté/estompe) vivent dans des **tuiles 256×256
  allouées à la demande** (`HashMap<(i32,i32), Tile>`). Un calque vide ne
  coûte rien ; un coup de pinceau ne touche que les tuiles traversées.
  Persistance : contenu aplati en PNG borné à sa boîte englobante (le tuilage
  est un détail d'édition, pas de format).
- **Calques** : visibilité, opacité, modes de fusion, groupes, masque
  d'écrêtage, masque peint (réutilise le moteur raster en niveaux de gris),
  calques d'ajustement non destructifs (appliqués au compositing).

## 3. Pipeline d'entrée

```
évènement OS → capture → lissage EMA + Catmull-Rom → vitesse→épaisseur → ruban
```

Pas de pression matérielle (cible d'origine : stylet passif) : l'épaisseur du
trait est **simulée par la vitesse** (lent = épais). Un trait à épaisseur
variable est rendu comme un **ruban** (deux bords décalés de ±épaisseur/2,
bouts ronds) — [render/ribbon.rs](src/render/ribbon.rs).

La vraie pression stylet (tablettes Wacom…) nécessiterait de contourner
winit/egui-winit (NSEvent custom) — investigué, scopé « L », en backlog
(SPRINTS.md 13.7).

## 4. Rendu

- **À l'écran** : egui peint les éléments vectoriels ; MSAA 4× pour
  l'anti-aliasing matériel.
- **Compositing par calque** ([render/compositor.rs](src/render/compositor.rs)) :
  tiny-skia (CPU) réalise modes de fusion, masques, dégradés et calques
  d'ajustement, avec **cache bitmap par calque** invalidé sélectivement.
  Seul le trait en cours est recalculé à chaque frame. Le contenu peint
  (pinceau/gomme pixel...) d'un calque « sale » est lui-même patché **tuile
  par tuile** dans un cache persistant plutôt que ré-aplati en entier à
  chaque dab (`RasterTileCache`, ANALYSE.md §12.1 — ≈103× plus rapide sur un
  calque déjà bien rempli, mesuré). Reste au backlog : propager ce
  découpage jusqu'au compositing multi-calques (fusion/écrêtage séquentiels).
- **Export** ([`Compositor::render_to_rgba`](src/render/compositor.rs)) :
  rend le document à sa résolution **native** (`doc.size`) via ce même
  chemin de composition, indépendamment du zoom/de la taille de la fenêtre à
  l'écran (ANALYSE.md §12.2 — remplace l'ancien export par capture d'écran
  du viewport, qui plafonnait la résolution exportée à celle de la fenêtre).

## 5. Undo / redo (pattern Command)

[history.rs](src/history.rs) — chaque action est une commande réversible.
Deux choix structurants :

- **Ids stables** : les commandes référencent calques/éléments par id, jamais
  par index — suppression/réordonnancement ne corrompent pas la pile ; une
  commande sur un calque disparu devient un no-op.
- **Undo par tuile** : `Command::PaintRaster` ne clone que les tuiles
  touchées par le geste (avant/après) — le mécanisme .xcf/.psd.

L'historique est **non linéaire** : panneau listant les états, saut direct à
n'importe lequel.

## 6. Persistance & export

- **Projet** : `.json` (serde) — images et raster embarqués en PNG base64.
  Format v2 prévu (zip JSON + PNG séparés, façon .ora) : le base64 gonfle
  les fichiers (SPRINTS.md 13.5). Version de format déjà en place
  (`Document::format_version`, ANALYSE.md §12.3) pour préparer cette
  migration.
- **Export** : PNG/JPEG/WebP, PDF mono-page (writer minimal maison,
  DCTDecode), SVG vectoriel, export par lots multi-tailles.
- **Préférences** : `~/Library/Application Support/QuickPaint/settings.json`
  (langue, palette, raccourcis, presets de style) — local uniquement.

## 7. i18n

[i18n.rs](src/i18n.rs) — `t("Nouveau", "New")` : les deux langues voyagent
côte à côte au site d'appel (jamais désynchronisées). Détection de la langue
système au démarrage, préférence persistée. Arbitrage assumé : optimal pour
2 langues, à revoir si une 3ᵉ arrive.

## 8. Packaging & distribution

1. `cargo build --release` (LTO, opt-level 3)
2. `cargo bundle --release` → `QuickPaint.app`
3. `codesign --deep --options runtime -s "Developer ID Application: …"`
4. `hdiutil`/`create-dmg` → `QuickPaint.dmg`
5. `xcrun notarytool submit … --wait` puis `xcrun stapler staple`

Le DMG signé/notarisé est publié via les GitHub Releases.

## 9. Contraintes produit (non négociables)

**100 % local** (aucun serveur), **mono-utilisateur** (pas de collaboration),
**aucune API externe** (pas de cloud, pas de télémétrie). Vérifiable :
aucune dépendance réseau dans l'arbre de crates.
