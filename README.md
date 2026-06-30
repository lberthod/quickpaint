# QuickPaint

Éditeur de dessin **tactile** pour macOS, écrit en **Rust** avec **egui/eframe**.
Pensé pour être simple comme Paint, mais avec des fonctions modernes (calques,
modes de fusion, formes, texte, transformations).

Auteur : **Loïc Berthod** — <https://github.com/lberthod>

## Fonctionnalités

- **Dessin** : pinceau (épaisseur simulée par la vitesse, lissage Catmull-Rom),
  gomme **objet** ou **partielle**, pot de peinture, pipette.
- **Formes** : ligne, flèche, rectangle, ellipse, polygone, étoile
  (contour ou rempli, contrainte Maj).
- **Plume** (courbes de Bézier) et **texte** éditable.
- **Sélection** : déplacer, **redimensionner**, **tourner**, dupliquer,
  aligner / répartir, **ordre de superposition** (premier plan / arrière-plan).
- **Calques** : visibilité, opacité, **modes de fusion** (produit, écran…),
  réordonnancement, **groupes**, fusion / aplatissement.
- **Images** : import + **coller (⌘V)**, déplacement, **recadrage**, filtres
  (luminosité, N&B, flou).
- **Vue** : zoom/pan tactile, grille + magnétisme, taille de document fixe.
- **Historique** non linéaire (panneau + retour direct à un état).
- **Export** : PNG, **SVG** vectoriel ; **sauvegarde de projet** `.json`.

## Compiler & lancer

```bash
cargo run --release
```

## Construire l'app macOS (`QuickPaint.app`)

```bash
cargo build --release
# génère l'icône, l'.icns puis assemble le bundle
./make-app.sh   # voir le script (build + iconutil + bundle)
open QuickPaint.app
```

## Architecture

`model` (données) · `input` (capture du geste) · `render` (rendu egui + compositeur
CPU tiny-skia) · `history` (undo/redo par commandes) · `tools` · `ui`.

## Licence

MIT © Loïc Berthod
