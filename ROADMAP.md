# Audit fonctionnel & feuille de route — QuickPaint

Comparatif avec **Canva**, **Photoshop**, **Illustrator**, **GIMP**, puis TODO des
10 implémentations importantes à viser.

---

## 1. Matrice fonctionnelle

Légende : ✅ présent · 🟡 partiel · ❌ absent

| Domaine | QuickPaint | Canva | Photoshop | Illustrator | GIMP |
|---|---|---|---|---|---|
| Pinceau + lissage | ✅ (Catmull-Rom) | 🟡 | ✅ | ✅ | ✅ |
| Pression (simulée vitesse) | ✅ | ❌ | ✅ (stylet) | ✅ | ✅ |
| Gomme | ✅ (vectorielle/objet) | 🟡 | ✅ | ✅ | ✅ |
| Formes (ligne/rect/ellipse) | ✅ (contour+plein) | ✅ | ✅ | ✅ | ✅ |
| Texte riche (police/gras/aligne/contour) | ✅ | ✅ (cœur) | ✅ | ✅ | ✅ |
| **Sélection / déplacer / transformer** | ✅ (clic + rect/lasso/baguette) | ✅ | ✅ | ✅ (cœur) | ✅ |
| **Plume / courbes de Bézier** | ❌ | 🟡 | ✅ | ✅ (cœur) | ✅ |
| **Pot de peinture / remplissage zone** | 🟡 (formes pleines) | ✅ | ✅ | ✅ | ✅ |
| **Import d'image** | ❌ | ✅ | ✅ | ✅ | ✅ |
| Calques | ✅ (visi/opacité/ordre/renom) | ✅ | ✅ | ✅ | ✅ |
| **Modes de fusion (multiply…)** | ❌ | 🟡 | ✅ | ✅ | ✅ |
| **Compositing d'opacité réel** | 🟡 (par trait) | ✅ | ✅ | ✅ | ✅ |
| Undo / redo | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Copier / coller / dupliquer** | ❌ | ✅ | ✅ | ✅ | ✅ |
| Pipette | ✅ | ✅ | ✅ | ✅ | ✅ |
| Palette / couleurs récentes | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Sélecteur HSV / hex / alpha** | 🟡 (RVB+alpha) | ✅ | ✅ | ✅ | ✅ |
| **Anti-aliasing des traits** | ❌ | ✅ | ✅ | ✅ | ✅ |
| Zoom / pan | ✅ | ✅ | ✅ | ✅ | ✅ |
| Grille / repères / magnétisme | ✅ (grille + règles + magnétisme) | ✅ | ✅ | ✅ | ✅ |
| Taille de doc fixe / presets / recadrage | ✅ (presets + recadrage + ratio) | ✅ | ✅ | ✅ | ✅ |
| Export bitmap (PNG/JPG/WebP/PDF) | ✅ | 🟡 | ✅ | ✅ | ✅ |
| Export SVG (vectoriel) | ✅ | 🟡 | 🟡 | ✅ | 🟡 |
| Sauvegarde projet | ✅ (.json) | ✅ | ✅ (.psd) | ✅ (.ai) | ✅ (.xcf) |
| **Filtres (flou, etc.)** | ❌ | ✅ | ✅ | 🟡 | ✅ |
| **Presets de pinceau (dureté/type)** | ❌ | 🟡 | ✅ | ✅ | ✅ |

### Forces actuelles
Pipeline tactile soigné (lissage + pression simulée), modèle vectoriel multi-calques
non destructif, undo/redo robuste (id stables), rendu incrémental fluide, export PNG +
projet JSON. Base saine.

### Manques structurants
1. **Pas d'édition après coup** : aucun moyen de sélectionner/déplacer/supprimer un
   élément déjà posé (≠ Illustrator/PS).
2. **Pas de texte** (≠ Canva).
3. **Pas de couche raster** → pas de pot de peinture réel, d'import d'image, de filtres.
4. **Rendu non anti-aliasé** → bords crénelés.
5. **Document = fenêtre** → pas de taille fixe ni de recadrage propre pour l'export.

---

## 2. TODO — 10 implémentations importantes (priorisées)

Effort : S (petit) · M (moyen) · L (gros). Impact : ⭐ à ⭐⭐⭐.

> **Avancement :** #1, #2, #3, #4, #7 ✅ implémentés. (#5 amorcé via le raster d'image.)

### P0 — débloquent le plus de valeur

- [x] **1. Sélection + déplacement/transformation** — M, ⭐⭐⭐
  Outil flèche : cliquer un trait (hit-test existe déjà via `tools::hit`), boîte de
  sélection, déplacer, supprimer (Suppr), dupliquer (⌘D). Commandes undo
  `Move`/`Delete`/`Duplicate`. _Débloque toute l'édition._ Réf : Illustrator/PS.

- [x] **2. Outil Texte** — M, ⭐⭐⭐
  Nouvel élément `Text { pos, contenu, taille, couleur, police }` par calque. Saisie
  inline, rendu via `painter.text`, sérialisé, sélectionnable. _Atout n°1 de Canva._

- [x] **3. Taille de document fixe + presets + recadrage** — S/M, ⭐⭐
  Remplacer « le doc suit la fenêtre » par une taille réelle (presets : A4, 1080×1080,
  HD…), canvas centré avec damier autour, export = la zone du doc. _Prérequis d'un
  export propre et de Canva-like templates._

### P1 — qualité & complétude

- [x] **4. Anti-aliasing des traits** — M, ⭐⭐
  Bords adoucis (feathering façon egui, ou MSAA framebuffer). Gros gain visuel
  immédiat sur tout le dessin.

- [~] **5. Couche raster hybride** — L, ⭐⭐⭐
  Ajouter un type de calque pixel (`Vec<u8>` RGBA, déjà recommandé dans l'archi §3).
  _Prérequis_ de : pot de peinture réel, import d'image, filtres. Investissement
  structurant.

- [x] **6. Pot de peinture / remplissage de zone** — M, ⭐⭐ (dépend de #5)
  Flood-fill sur la couche raster (ou remplissage de région fermée). Réf : Paint/PS.

- [x] **7. Import d'image** — S/M, ⭐⭐ (dépend de #5)
  Placer un PNG/JPG comme calque raster (le crate `image` est déjà là). Réf : tous.

- [x] **8. Modes de fusion + compositing d'opacité réel** — M, ⭐⭐
  Rendre chaque calque dans une texture hors-écran puis composer (multiply/screen/
  overlay…). Corrige aussi la surimpression de l'opacité par-trait actuelle. Réf :
  PS/GIMP.

### P2 — finition pro

- [x] **9. Plume / courbes de Bézier** — L, ⭐⭐
  Tracé de chemins précis avec poignées, éditables. Cœur d'Illustrator ; complète bien
  notre modèle vectoriel.

- [x] **10. Grille + repères + magnétisme** — S/M, ⭐
  Grille optionnelle, snap aux points/grille, nudge clavier. Précision façon
  Illustrator/Canva.

### Backlog (au-delà du top 10)
✅ Export SVG (fait). copier/coller/dupliquer, sélecteur HSV/hex,
presets de pinceau (dureté/marqueur/aérographe), filtres (flou/luminosité),
panneau d'historique non linéaire, groupes de calques, alignement/répartition.

---

## 3. Ordre d'attaque conseillé
**1 → 3 → 2 → 4**, puis **5 → (6, 7, 8)**, puis **9, 10**.
La sélection (#1) et la taille de doc (#3) sont des fondations ; la couche raster (#5)
ouvre un second bloc de fonctionnalités (#6–8).
