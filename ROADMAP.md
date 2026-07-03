# Audit fonctionnel & feuille de route — QuickPaint

Objectif stratégique : atteindre le **niveau d'usage de PhotoFiltre** (retouche photo
légère) et de **Canva** (composition rapide formes + texte + images), en s'appuyant
sur la **logique architecturale des grands** — GIMP/Photoshop pour le moteur raster,
Illustrator pour l'édition vectorielle après coup.

On ne vise **pas** la parité avec Photoshop/GIMP/Illustrator (écosystèmes de 20–30 ans) :
on leur **emprunte leurs fondations** pour battre les outils de la même catégorie de
poids (PhotoFiltre, Paintbrush, Canva-cœur, markup Preview).

---

## 1. Matrice fonctionnelle (à jour — juillet 2026)

Légende : ✅ présent · 🟡 partiel · ❌ absent

| Domaine | QuickPaint | PhotoFiltre | Canva | Photoshop | Illustrator | GIMP |
|---|---|---|---|---|---|---|
| Pinceau + lissage | ✅ (Catmull-Rom, AA) | 🟡 | 🟡 | ✅ | ✅ | ✅ |
| Pression | 🟡 (simulée vitesse) | ❌ | ❌ | ✅ (stylet) | ✅ | ✅ |
| Gomme (objet + partielle) | ✅ | ✅ | 🟡 | ✅ | ✅ | ✅ |
| Formes (ligne/flèche/rect/ellipse/polygone/étoile) | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| Plume / courbes de Bézier | ✅ (tracé) 🟡 (pas d'édition de nœuds après coup) | ❌ | 🟡 | ✅ | ✅ (cœur) | ✅ |
| Texte riche (police/gras/alignement/contour) | ✅ | 🟡 | ✅ (cœur) | ✅ | ✅ | ✅ |
| Polices système / vraie typo | ✅ (fontdb, chargement paresseux) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Sélection (clic / rectangle / lasso / baguette) | ✅ | ✅ | 🟡 | ✅ | ✅ | ✅ |
| Déplacer / redimensionner / pivoter / dupliquer | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| Aligner / répartir / z-order | ✅ | ❌ | ✅ | ✅ | ✅ | 🟡 |
| Calques (visi/opacité/ordre/groupes/fusion) | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| Modes de fusion | ✅ (6 modes) | 🟡 | 🟡 | ✅ (27) | ✅ | ✅ |
| Masque d'écrêtage | ✅ | ❌ | 🟡 | ✅ | ✅ | ✅ |
| Masque de calque peint | ✅ (moteur raster réutilisé) | ❌ | 🟡 | ✅ | ✅ | ✅ |
| **Calque raster peignable** | ✅ (tuilé, undo par tuile) | ✅ | ❌ | ✅ | 🟡 | ✅ |
| Pot de peinture (flood-fill pixel) | ✅ (écrit dans la couche raster, borné au canevas) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tampon de clonage | ✅ (⌥+clic = source, décalage constant) | ❌ | ❌ | ✅ | ❌ | ✅ |
| Import d'image + coller (⌘V) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Recadrage (libre + ratios) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Filtres (lum./contraste/satur./netteté/flou/négatif/N&B) | ✅ (destructif **ou** en calque d'ajustement) | ✅ | ✅ | ✅ | 🟡 | ✅ |
| **Niveaux / courbes / teinte-saturation** | 🟡 (presets non destructifs ; pas encore de réglages continus) | ✅ (cœur) | 🟡 | ✅ | ❌ | ✅ |
| **Tampon de clonage / correcteur** | ❌ | ✅ | 🟡 | ✅ | ❌ | ✅ |
| Redimensionner image / canevas | 🟡 (recadrage seul) | ✅ (cœur) | ✅ | ✅ | ✅ | ✅ |
| **Templates / bibliothèque d'assets** | 🟡 (galerie de formats ✅, pictos ❌) | ❌ | ✅ (cœur) | 🟡 | 🟡 | ❌ |
| Guides intelligents (snap objet↔objet) | ✅ (bords/centres + canevas, lignes magenta) | ❌ | ✅ (cœur) | ✅ | ✅ | 🟡 |
| Undo / redo (historique non linéaire) | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| Sélecteur couleur HSV / hex | 🟡 (RVB+alpha) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Zoom / pan tactile | ✅ | ❌ | ✅ | ✅ | ✅ | 🟡 |
| Export PNG/JPEG/WebP/PDF | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Export SVG | ✅ | ❌ | 🟡 | 🟡 | ✅ | 🟡 |
| Sauvegarde projet | ✅ (.json) | ✅ (.pfi) | ✅ | ✅ (.psd) | ✅ (.ai) | ✅ (.xcf) |
| Ouverture PSD (interop) | ❌ | 🟡 | 🟡 | ✅ | 🟡 | ✅ |
| Gestion couleur (ICC) / i18n / App Store | ❌ | 🟡 | ✅ | ✅ | ✅ | 🟡 |

### Lecture de la matrice

- **vs PhotoFiltre** : il manque le bloc **retouche pixel** — calque raster peignable,
  niveaux/courbes, clonage, redimensionnement d'image. Tout le reste est là ou mieux.
- **vs Canva** : il manque le bloc **productivité de composition** — templates,
  assets, guides intelligents, vraies polices. Le moteur (calques, formes, texte,
  alignement) est déjà là.
- **vs la logique GIMP/PS/AI** : ce qui nous sépare n'est pas une liste de filtres,
  c'est **trois fondations** : (F1) un moteur raster tuilé, (F2) l'édition vectorielle
  non destructive après coup, (F3) le pipeline d'ajustements non destructifs.

---

## 2. La solution : 3 fondations empruntées aux grands

> Principe : chaque fondation est un investissement unique qui débloque une grappe
> de fonctionnalités. C'est exactement comme ça que sont construits GIMP (core
> raster + GEGL), Photoshop (compositing tuilé + calques d'ajustement) et
> Illustrator (modèle objet éditable en permanence).

### F1 — Moteur raster tuilé (logique **GIMP/Photoshop**) — L, ⭐⭐⭐

Nouveau type de calque `Raster { tiles: HashMap<(i32,i32), Tile256> }` : pixels RGBA
prémultipliés en tuiles de 256×256, dirty-rects, undo par tuile (on ne clone que
les tuiles touchées — c'est le mécanisme .xcf/.psd). Le compositor actuel
(tiny-skia, cache par calque) reste : on remplace juste « re-rasteriser tout le
calque » par « re-composer les tuiles sales ».

**Débloque :** pinceau pixel (dureté/opacité/flux), gomme pixel, pot de peinture
réel, tampon de clonage, correcteur, masque de calque peint, et règle le goulot de
perf sur grands documents. _Prérequis de tout le bloc PhotoFiltre._

### F2 — Édition vectorielle après coup (logique **Illustrator**) — M/L, ⭐⭐⭐

Les éléments posés restent éditables en permanence : double-clic sur un trait de
plume → poignées de Bézier réapparaissent (le modèle `pen.rs` garde déjà les
ancres) ; double-clic sur une forme → paramètres (rayon, côtés, plein/contour)
rééditables ; texte déjà rééditable ✅. Ajouter : opérations booléennes de chemins
(union/soustraction/intersection — via `tiny-skia`/lyon), dégradés de remplissage.

**Débloque :** le réflexe Illustrator « rien n'est jamais figé », les formes
composées, les logos/pictos propres.

### F3 — Ajustements non destructifs (logique **Photoshop**) — M, ⭐⭐⭐

Les filtres actuels **détruisent** les pixels de l'image. Remplacer par des
**calques d'ajustement** : `Adjustment { kind: Levels|Curves|HueSat|…, params }`
appliqués au compositing (le compositor par calque s'y prête déjà — c'est une
passe de plus dans `rebuild`). Un ajustement écrêté (le `clip` existe ✅) ne
touche que le calque du dessous.

**Débloque :** niveaux, courbes, teinte/saturation, balance des couleurs —
réversibles, re-réglables — le cœur de PhotoFiltre en mieux (lui est destructif).

---

## 3. TODO priorisé — atteindre PhotoFiltre puis Canva

Effort : S · M · L. Impact : ⭐ à ⭐⭐⭐.

### P0 — Cap PhotoFiltre (retouche photo légère)

- [x] **1. F1 : moteur raster tuilé** — L, ⭐⭐⭐ (fondation, voir §2) : tuiles
      256×256 allouées à la demande ([model/raster.rs](src/model/raster.rs)),
      undo par tuile (`Command::PaintRaster`), persistance PNG paresseuse
      (comme `ImageItem`), intégré au compositeur (rendu sous les éléments
      vectoriels du calque). ✅
- [x] **2. Pinceau & gomme pixel** (dureté réglable, couleur/opacité
      partagées avec le pinceau vectoriel, pression simulée branchée) — M,
      ⭐⭐⭐ (dépend de #1) : outils **Pinceau pixel** / **Gomme pixel** dans la
      barre d'outils. ✅
- [x] **3. F3 : calques d'ajustement** — M, ⭐⭐⭐ : les 9 filtres existants
      ([tools/filter.rs](src/tools/filter.rs)) réutilisés en direct au
      compositing plutôt qu'appliqués aux pixels — réversible, re-réglable à
      tout moment via le menu **Calque › Ajouter un calque d'ajustement** ou
      le sélecteur du panneau de calques ; respecte l'écrêtage (n'affecte
      alors que le calque du dessous) et l'opacité (mélange original/filtré).
      Niveaux/courbes continus restent au backlog (les 9 filtres sont des
      presets discrets, pas des réglages continus). ✅
- [x] **4. Redimensionner image / taille du canevas** — S, ⭐⭐⭐ : menu **Image**
      (dialogues « Redimensionner l'image » avec proportions liées + presets %,
      « Taille du canevas » avec ancrage 9 positions), annulable (`SetDoc`).
      _Le geste n°1 de PhotoFiltre._ ✅
- [x] **5. Tampon de clonage** — M, ⭐⭐ (dépend de #1) : ⌥+clic définit la source,
      le glissé peint en échantillonnant la couche raster avec un **décalage
      constant** (figé au début du geste, comme GIMP/Photoshop). ✅ Correcteur
      (guérison automatique, sans décalage figé) laissé au backlog — nécessite
      un algorithme de mélange de texture plus élaboré.
- [x] **6. Sélecteur HSV + hex** — S, ⭐ : champ `#RGB/#RRGGBB/#RRGGBBAA` à côté
      de la pastille (picker HSV egui). ✅

### P1 — Cap Canva (composition rapide)

- [x] **7. Polices système** — M, ⭐⭐⭐ : `fontdb` scanne les dossiers système
      au démarrage ([fonts.rs](src/fonts.rs)), une police n'est chargée dans
      egui (`Context::set_fonts`) qu'au moment où elle est choisie. Sélecteur
      avec filtre dans la barre d'options Texte ; le rendu (live **et**
      compositeur CPU) suit automatiquement puisque les deux passent déjà par
      le même `FontId`/`FontFamily` egui. _Condition sine qua non du
      « Canva-like »._ ✅
- [x] **8. Guides intelligents** — M, ⭐⭐⭐ : snap objet↔objet (bords, centres)
      pendant le glissé de la sélection, plus bords/centre du canevas offerts
      gratuitement (même mécanisme). Lignes magenta affichées le temps du
      geste ([tools/guides.rs](src/tools/guides.rs), pur et testé — 4 tests).
      Réutilise la géométrie de boîtes englobantes déjà utilisée par
      l'alignement. Espacements égaux (distribution pendant le drag, ≠ le
      bouton « Répartir » déjà existant) laissés au backlog. ✅
- [x] **9a. Galerie de modèles** — S, ⭐⭐ : **Fichier › Nouveau depuis un
      modèle…** — 11 formats groupés par catégorie (réseaux sociaux, impression,
      écran : post Instagram, story/reel, bannières FB/YouTube, miniature
      YouTube, affiche A4, carte de visite/postale, présentation 16:9…).
      Fenêtre-galerie en grille, un clic = nouveau document à cette taille
      ([toolbar.rs](src/ui/toolbar.rs) `TEMPLATES`/`template_gallery`). ✅
- [ ] **9b. Bibliothèque d'assets** — M, ⭐⭐ (backlog) : panneau de pictos SVG
      de base et formes composées réutilisables, au-delà des formes
      paramétriques déjà existantes (polygone/étoile).
- [x] **10. Pipette de style** — S/M, ⭐⭐ : **Édition › Copier/Coller le style**
      (⌥⌘C / ⌥⌘V) — couleur, épaisseur, remplissage pour formes/traits ;
      police, gras, alignement, contour en plus pour le texte. Un trait garde
      sa géométrie, seul le style change. Styles de texte **nommés** (presets
      réutilisables sans passer par copier/coller) laissés au backlog.
- [x] **11. Dégradés de remplissage** (linéaire/radial) — M, ⭐⭐ (fait partie
      de F2) : `Stroke.gradient` optionnel (2 arrêts, dimensionné sur la
      boîte englobante de chaque forme), rendu via les shaders natifs de
      tiny-skia au compositing ([compositor.rs](src/render/compositor.rs)
      `gradient_shader`). Menu **Édition › Dégradé** (Linéaire/Radial/Retirer)
      sur la sélection — ne s'applique qu'aux formes pleines (`Rempli`).
      Dégradé sur le **texte** laissé au backlog (nécessiterait un shader par
      glyphe dans `raster_text`, plus complexe qu'un remplissage de chemin).
      ✅

### P2 — Logique Illustrator & finitions pro

- [x] **12. F2 : édition de nœuds après coup** — M/L, ⭐⭐⭐ (voir §2) :
      `Stroke.anchors` conserve le `PenPath` (ancres + poignées Bézier) au lieu
      de le jeter après l'échantillonnage ([tools/pen.rs](src/tools/pen.rs)).
      Double-clic sur un trait Plume avec l'outil Sélection rouvre l'édition
      (`try_start_pen_edit`) : ancres/poignées affichées en orange
      (`paint_pen_edit`), glissables (`hit_test_pen_node` / `apply_pen_drag`),
      ré-échantillonnage live du trait à chaque frame. `Échap` ou double-clic
      hors chemin referme l'édition ; undo/redo dédié
      (`Command::EditPenPath`) capture l'état avant/après du geste complet
      ([app.rs](src/app.rs), [history.rs](src/history.rs)). Bug corrigé en
      testant : `drag_started()` peut déjà refléter une position du pointeur
      avancée dans le glissé (plusieurs évènements souris fusionnés avant la
      première frame observée) — le hit-test du nœud ciblé utilise donc le
      point de pression réel (`ctx.input(|i| i.pointer.press_origin())`) et
      non la position courante, sans quoi un glissé rapide sur une petite
      poignée pouvait la manquer entièrement. ✅
- [x] **13. Booléens de chemins** (union/soustraction/intersection) — M, ⭐⭐ :
      `geo-clipper` (Clipper 2D) sur les deux formes pleines sélectionnées
      ([tools/boolean.rs](src/tools/boolean.rs)) ; menu **Édition › 🔷
      Booléens**, actif seulement si exactement 2 formes « Rempli » sont
      sélectionnées. Chaque polygone résultat (une soustraction peut séparer
      une forme en deux) redevient un `Stroke` plein indépendant, avec le
      style du trait le plus profond (z le plus petit = *subject*, l'autre =
      *clip*). MVP : seul le contour extérieur est conservé, pas de trous
      (limite du modèle `Stroke`, un seul anneau de points) — documenté en
      commentaire. Undo/redo réutilise `Command::SplitStrokes` (déjà
      invertible : retire des indices, réinsère un ensemble de traits) plutôt
      qu'une nouvelle variante. ✅
- [x] **14. Masque de calque peint** — M, ⭐⭐ (dépend de #1) : `Layer.mask`
      réutilise directement le moteur raster tuilé comme surface peignable en
      niveaux de gris (blanc = visible, noir = masqué ; un pixel jamais peint
      reste visible, comme un masque neuf). Bouton **🎭 Ajouter un masque**
      dans le panneau de calques + case **Éditer le masque** qui redirige le
      pinceau/gomme pixel existants vers le masque au lieu du contenu.
      Multiplie l'alpha du calque au compositing ([compositor.rs](src/render/compositor.rs)
      `apply_mask`, même technique que le masque d'écrêtage `clip_alpha`).
      `Command::PaintRaster` généralisé avec une cible Contenu/Masque pour
      l'undo par tuile. ✅
- [ ] **15. Pression réelle du stylet** — L (révisé, était noté M), ⭐⭐ :
      investigation faite — winit 0.30 (utilisé par eframe 0.29) capte bien
      `pressureChangeWithEvent:` côté macOS mais seulement pour le *Force
      Touch* du trackpad (`WindowEvent::TouchpadPressure`), et **egui-winit
      l'ignore explicitement** (`// TODO(emilk)` dans `lib.rs`, l'évènement
      n'est jamais transmis à egui). Une vraie tablette graphique (Wacom
      etc.) expose sa pression via l'API `NSEvent.tabletPoint`/`pressure`,
      que winit ne capte pas du tout — il faudrait un `NSView` custom
      (objc2 + raw-window-handle, bypass du pipeline winit/egui-winit standard)
      pour l'intercepter. Correctement scopé, c'est donc un chantier
      « fork/patch du pipeline d'évènements », pas une lecture NSEvent
      ponctuelle : reclassé en **L**. Le fallback vitesse actuel reste en
      place ; repris seulement si la valeur pour l'utilisateur (peu
      d'utilisateurs de tablette graphique sur macOS visés ici) justifie
      l'investissement.
- [ ] **16. Import PSD (lecture)** — L, ⭐ : crate `psd` ; calques raster + texte
      basiques. Interop d'appel, pas une priorité.

### Transversal (n'importe quand, forte valeur)

- [x] **i18n EN/FR** — S/M, ⭐⭐⭐ : [i18n.rs](src/i18n.rs) — `t(fr, en)` résolu à
      la volée sur la langue courante (pas de table de clés séparée : les deux
      versions voyagent côte à côte au site d'appel, donc jamais désynchronisées).
      Détection de la langue système au démarrage (`defaults read -g AppleLocale`
      sur macOS, repli `LANG`/`LC_ALL`), préférence explicite persistée dans
      `~/Library/Application Support/QuickPaint/settings.json`. Sélecteur
      **FR/EN** dans la barre de menu ([toolbar.rs](src/ui/toolbar.rs)). Toute
      l'UI est couverte : menus, barre d'outils, panneaux calques/historique,
      dialogues (redimensionner, modèles), messages de statut, libellés
      annuler/rétablir. ✅
- [ ] **Mac App Store** — M, ⭐⭐⭐ : la découvrabilité vaut plus que toute
      fonctionnalité. Sandbox + entitlements (rfd est déjà compatible).
- [x] **Perf compositor (premier passage)** : l'atlas de glyphes egui
      (`ctx.fonts(|f| f.image())`, plusieurs Mo en f32) était cloné à **chaque**
      appel de `rebuild()` — donc à chaque frame pendant une peinture raster/pixel
      (pinceau pixel, gomme pixel, tampon, masque), même sur un document sans
      texte. Récupéré paresseusement maintenant : une seule fois par appel, et
      seulement si un calque redevenu obsolète contient réellement du texte
      ([compositor.rs](src/render/compositor.rs) `rebuild`). Le vrai fond du
      sujet — recomposition **plein cadre** à chaque dab au lieu d'un rectangle
      sale — reste au backlog : ça suppose de propager des dirty-rects à travers
      le compositing par calque (ordre d'écrêtage/fusion séquentiel), plus gros
      chantier que ce premier nettoyage. Mesure sur doc 4K/10 calques toujours à
      faire.
- [ ] **Format projet v2** : garder .json mais images en fichiers séparés dans un
      .zip (comme .ora/.sketch) — le base64 explose la taille des projets.

---

## 4. Ordre d'attaque conseillé

**#4 (resize) → #6 (HSV) → #1 (raster tuilé) → #2 (pinceau pixel) → #3 (ajustements)**
= parité PhotoFiltre.
Puis **#7 (polices) → #8 (guides) → #9 (templates)** = cœur Canva.
Puis **#12/#13** (logique Illustrator) et le transversal en continu.

Jalon 1 « PhotoFiltre du Mac » : #1–#6 + i18n.
Jalon 2 « Canva de poche hors-ligne » : #7–#10 + App Store.
Jalon 3 « atelier vectoriel » : #11–#13.

---

## 5. Historique — top 10 initial (terminé)

Le premier plan (sélection, texte, doc fixe, AA, raster d'images, pot de peinture,
import, modes de fusion, plume, grille/magnétisme) est **entièrement livré** en
5 sprints + release signée/notarisée. Voir le journal git pour le détail.
