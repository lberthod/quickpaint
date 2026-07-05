# audit_next.md — Audit fonctionnel QuickPaint (5 juillet 2026)

Audit du code source par rapport à une nouvelle checklist produit, plus large
que les précédentes (`audit_newxxx.md`/`audit_sprint_xx.md`, retirées une
fois actées — voir le journal git). Statuts : **✅ Implémenté** · **🟡
Partiel** · **❌ Absent**, avec pointeur de code ou « introuvable ».

Méthode : lecture directe du code (`grep`/lecture de fichiers), pas
d'estimation à partir des seuls noms de fonctionnalités.

---

## Fichiers & formats ouverts

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 1 | Nouveau document, tailles prédéfinies et personnalisées | 🟡 | `new_document()`/`new_document_sized()` (app/mod.rs) + galerie de modèles (`templates()`, ui/toolbar.rs) — présélections social/impression/écran, mais aucune n'est littéralement « 4K » (3840×2160). |
| 2 | Ouverture PNG, JPEG, BMP, TIFF, GIF, WebP | ✅ | `project.rs:81` — toutes acceptées via la crate `image`. |
| 3 | Import SVG (rendu vectoriel) | ❌ | Seul l'export SVG existe (`svg.rs`) ; aucune fonction d'import SVG trouvée. |
| 4 | Format natif avec calques (conteneur ouvert type zip+JSON) | 🟡 | Format natif = JSON pur (`project.rs`, `serde_json`), pas un conteneur zip+JSON — fonctionnellement équivalent (calques éditables persistés) mais pas le conteneur littéralement demandé. |
| 5 | Export PNG avec canal alpha | ✅ | `export.rs:112`. |
| 6 | Export JPEG avec réglage de qualité | ✅ | Curseur 1–100 (`app/mod.rs`, `export.rs:118`). |
| 7 | Export WebP (avec/sans perte) | 🟡 | Export WebP existe mais **toujours sans perte** (crate `image`) — le WebP *lossy* demanderait `libwebp`, écarté par décision produit (voir `audit_sprint_xx.md`, retiré). |
| 8 | Export TIFF et BMP | ✅ | Via la crate `image` (features Cargo.toml). |
| 9 | Export GIF (statique et animé) | ❌ | Import GIF supporté, **aucun export GIF** (ni statique ni animé) — seuls PNG/JPG/WebP/PDF/SVG sont exportables. |
| 10 | Export SVG des éléments vectoriels | ✅ | `svg.rs` — traits, images (base64), textes, opacité de calque. |
| 11 | Export PDF (rasterisé ou vectoriel) | 🟡 | PDF toujours **rasterisé** (JPEG encapsulé, `export.rs:122`) — pas de PDF vectoriel. |
| 12 | Export par lots (plusieurs fichiers) | ✅ | `save_batch()` (`export.rs:46`). |
| 13 | Export multi-tailles en un clic | ✅ | `BatchExportState` (`app/mod.rs`) + UI dédiée. |
| 14 | Export d'une zone sélectionnée uniquement | ❌ | L'export porte toujours sur le document entier. |
| 15 | Aperçu et poids estimé avant export | ❌ | Aucun aperçu ni estimation de poids avant export. |
| 16 | Profils d'export réutilisables (presets) | 🟡 | Les tailles du batch export sont réutilisables dans la session, mais pas de profils nommés/persistés (qualité + format + destination combinés). |
| 17 | Suppression optionnelle des métadonnées | ❌ | Aucune gestion de métadonnées EXIF/PNG à l'export. |
| 18 | Glisser-déposer de fichiers | ❌ | Aucun gestionnaire de drag-and-drop de fichiers trouvé dans la boucle d'événements. |
| 19 | Import depuis le presse-papiers | ✅ | `paste_image()` via `arboard` (`app/mod.rs`). |
| 20 | Sauvegarde automatique + récupération après crash | ✅ | `project.rs` — autosave périodique vers `recovery.json`, restauration proposée au démarrage. |

**Score : 11 ✅ / 6 🟡 / 3 ❌** *(sur 20 — un item peut compter dans les deux
premières colonnes selon la lecture)*

---

## Calques & composition

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 21 | Calques multiples empilables | ✅ | `Document.layers: Vec<Layer>`. |
| 22 | Réordonnancement par glisser-déposer | ✅ | `dnd_drag_source`/`dnd_drop_zone` (`ui/layers.rs`). |
| 23 | Opacité réglable par calque | ✅ | `layer.opacity` + curseur. |
| 24 | Modes de fusion (multiply, screen, overlay…) | ✅ | `BlendMode` — Normal/Multiply/Screen/Overlay/Darken/Lighten (6 modes). |
| 25 | Masque de calque (niveaux de gris) | ✅ | `layer.mask: Option<RasterLayer>`, peint en niveaux de gris. |
| 26 | Masque d'écrêtage | ✅ | `layer.clip: bool`. |
| 27 | Groupes de calques | ✅ | `layer.group: Option<String>` + repli/dépli dans l'UI. |
| 28 | Verrouillage (position, pixels, transparence) | 🟡 | `layer.locked: bool` bloque peinture/édition de contenu (ajouté récemment) — mais c'est un verrou **global** (« tout ou rien »), pas granulaire par type (position seule / pixels seuls / transparence seule comme Photoshop). |
| 29 | Visibilité par calque | ✅ | `layer.visible` + icône œil. |
| 30 | Renommage et code couleur | 🟡 | Renommage fonctionnel ; **pas de code couleur** (étiquette de couleur sur le calque, à la Photoshop/Figma). |
| 31 | Duplication de calque | ✅ | `duplicate_layer()`. |
| 32 | Fusion de calques / aplatissement | ✅ | `merge_down()` / `flatten()`. |
| 33 | Calque de remplissage (uni, dégradé, motif) | ❌ | Les calques d'ajustement existent, mais pas de calque de remplissage dédié (couleur unie / dégradé / motif en tant que contenu de calque). |
| 34 | Calques de réglage non-destructifs | ✅ | `layer.adjustment: Option<Adjustment>` — Niveaux, Courbes, Teinte/Saturation, Exposition, Vibrance, Balance des blancs, Réduction de bruit, Flou gaussien/mouvement/bokeh, Duotone, Distorsion, Aberration chromatique, Warp Arc. |
| 35 | Styles de calque (ombre, contour, lueur) | ✅ | `LayerStyle` — DropShadow/Stroke/Glow (interne/externe). |
| 36 | Alignement et distribution de calques | ❌ | L'alignement/distribution existe pour les **éléments** (traits/textes/images) sélectionnés, pas pour les calques eux-mêmes en tant qu'entités. |
| 37 | Vignettes de prévisualisation | ❌ | La liste des calques affiche nom + compteur d'éléments, pas de miniature rendue. |
| 38 | Recherche/filtre dans la liste des calques | ❌ | Aucun champ de recherche/filtre dans le panneau de calques. |

**Score : 11 ✅ / 2 🟡 / 4 ❌**

---

## Dessin & peinture

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 39 | Pinceau à taille et dureté réglables | ✅ | `brush.width` + dureté (pinceau pixel). |
| 40 | Crayon pixel-perfect | 🟡 | Existe comme préréglage de pinceau (« Crayon fin ») et via le pinceau pixel, pas comme outil dédié séparé. |
| 41 | Gomme avec opacité | ✅ | `Eraser`/`PixelEraser`, opacité via l'état de l'outil. |
| 42 | Pot de remplissage avec tolérance | ✅ | `ActiveTool::Bucket` + curseur de tolérance. |
| 43 | Dégradé linéaire, radial, conique | ✅ | `GradientKind::{Linear, Radial, Conic}`. |
| 44 | Aérographe | ❌ | Aucun outil aérographe (dépôt progressif tant que le clic est maintenu) trouvé. |
| 45 | Outil doigt / étalement | ✅ | `ActiveTool::Smudge`. |
| 46 | Tampon de duplication (clone) | ✅ | `ActiveTool::CloneStamp`. |
| 47 | Densité + / − | ✅ | `Dodge`/`Burn`. |
| 48 | Éponge (saturation locale) | ✅ | `Saturate`/`Desaturate`. |
| 49 | Bibliothèque de brosses | 🟡 | 4 préréglages embarqués (Feutre, Crayon fin, Aquarelle douce, Calligraphie) + sauvegarde/chargement custom — pas de galerie visuelle riche avec import de fichiers de brosses tiers. |
| 50 | Import de brosses depuis image (motif → pinceau) | ❌ | Aucune fonction de création de brosse à partir d'une image. |
| 51 | Dynamique de brosse selon la pression du stylet | ✅ | `input/pressure.rs::width_for_pressure()`. |
| 52 | Support tablette graphique / stylet | ✅ | Pression réelle via événements `Touch` d'egui, repli sur simulation vitesse. |
| 53 | Stabilisation du tracé | ✅ | Lissage EMA réglable (`input/smoothing.rs`). |
| 54 | Symétrie (miroir, radiale, mandala) | 🟡 | Miroir/rotation à N axes (2 à 12, réglable) autour du centre — couvre l'usage « mandala » basique, mais ce n'est pas un mode « radiale » distinct avec réflexion **et** rotation combinées comme Krita/Procreate. |
| 55 | Règles, guides et grille magnétiques | ✅ | Grille, règles, aimantation (`snap()`, `tools/guides.rs`). |
| 56 | Prévisualisation du contour de brosse | ❌ | Pas de curseur en forme de contour de brosse (cercle de la taille du pinceau) pendant le dessin. |

**Score : 12 ✅ / 3 🟡 / 3 ❌**

---

## Sélection & découpe

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 57 | Sélection rectangle et ellipse | ✅ | `SelectMode::{Rect, Ellipse}`. |
| 58 | Lasso libre et lasso polygonal | 🟡 | Lasso libre (`SelectMode::Lasso`) présent ; pas de variante « polygonale » distincte (clic par sommet plutôt que glissé continu). |
| 59 | Baguette magique (tolérance réglable) | ✅ | `magic_wand()` + curseur de tolérance. |
| 60 | Sélection par plage de couleurs | ✅ | Mode global de la baguette magique (`flood_global`, non contigu). |
| 61 | Ajout / soustraction / intersection de sélection | 🟡 | **Seul l'ajout** (Maj + glisser) est implémenté (`additive: bool` dans les fonctions de sélection) — pas de soustraction ni d'intersection. |
| 62 | Contour progressif (feather) | ❌ | `soft_edge()` existe mais seulement pour l'outil Détourage, pas exposé comme opération générique sur une sélection. |
| 63 | Dilater / contracter la sélection | ❌ | Aucune fonction grow/shrink trouvée. |
| 64 | Inversion de sélection | ❌ | Aucune fonction d'inversion trouvée. |
| 65 | Enregistrer / charger une sélection | ✅ | `save_named_selection()`/`load_named_selection()`. |
| 66 | Recadrage libre et par ratio | ✅ | Ratios libre/1:1/4:3/16:9/A4. |
| 67 | Redressement d'horizon | ✅ | Curseur d'angle -45°..45° + `straighten_and_crop()`. |
| 68 | Découpe automatique des bords vides (trim) | ❌ | Aucune fonction de recadrage automatique des bords transparents. |

**Score : 7 ✅ / 2 🟡 / 4 ❌**

---

## Retouche photo

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 69 | Luminosité / contraste | ✅ | `Filter::{Brighter, Darker, Contrast}`. |
| 70 | Exposition | ✅ | `Adjustment::Exposure { ev }`. |
| 71 | Niveaux (noir/blanc/gris) | ✅ | `Adjustment::Levels { black, white, gamma }`. |
| 72 | Courbes RVB | ✅ | `Adjustment::Curves` (3 points ancrés, canal composite). |
| 73 | Teinte / saturation / vibrance | ✅ | `HueSaturation` + `Vibrance`. |
| 74 | Balance des couleurs | ✅ | `WhiteBalance { temp, tint }` couvre l'essentiel (pas de balance séparée ombres/tons moyens/hautes lumières façon Photoshop). |
| 75 | Température / balance des blancs | ✅ | Idem (même ajustement). |
| 76 | Conversion N&B avec mixage de canaux | ❌ | `Filter::Grayscale` utilise la luminance standard (Rec. 601), pas de mixeur de canaux réglable par canal. |
| 77 | Netteté (accentuation / passe-haut) | ✅ | `Filter::Sharpen` (noyau 3×3). |
| 78 | Réduction de bruit | ✅ | `Adjustment::Denoise` (lissage bilatéral). |
| 79 | Suppression yeux rouges | ✅ | `reduce_red_eye()`. |
| 80 | Correction de distorsion et vignettage | 🟡 | Distorsion radiale (barrel/pincushion) présente ; le vignettage n'existe qu'intégré au filtre Vintage, pas comme réglage de vignette indépendant. |
| 81 | Histogramme en temps réel | ✅ | Étendu au canevas entier récemment (`canvas_histogram()`). |
| 82 | Comparaison avant / après | ✅ | Bouton « Avant (maintenir) ». |
| 83 | Auto-correction en un clic | ❌ | Aucune fonction d'auto-niveaux/auto-contraste trouvée. |

**Score : 12 ✅ / 1 🟡 / 2 ❌**

---

## Filtres & effets

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 84 | Flou gaussien | ✅ | `Adjustment::GaussianBlur` (noyau séparable). |
| 85 | Flou de mouvement et flou radial/zoom | 🟡 | Flou de mouvement directionnel présent ; **pas de flou radial/zoom** (effet vitesse/explosion depuis un point). |
| 86 | Pixelisation / mosaïque | ❌ | Aucun filtre de pixelisation trouvé. |
| 87 | Détection de contours (Sobel / Canny) | 🟡 | Sobel seul (utilisé par Croquis et BD) ; pas de Canny (suppression non-maximale + hystérésis). |
| 88 | Posterisation et seuil (effet BD) | ✅ | `Filter::Comic` (posterisation 5 niveaux + contours Sobel). |
| 89 | Grain et bruit ajoutable | ✅ | `Filter::FilmGrain` (bruit procédural déterministe). |
| 90 | Vignette artistique | 🟡 | Intégrée au filtre Vintage uniquement, pas réglable seule. |
| 91 | Duotone / bichromie | ✅ | `Adjustment::Duotone`. |
| 92 | Halftone (trame) | ❌ | Aucun effet de trame (points façon impression offset) trouvé. |
| 93 | Distorsions (vague, sphère, tourbillon) | ❌ | Seule la distorsion radiale barrel/pincushion existe — pas de vague, sphère, ni tourbillon. |
| 94 | Import de LUT `.cube` | ✅ | `tools/lut.rs` — interpolation trilinéaire, intensité réglable. |
| 95 | Intensité réglable + aperçu direct pour chaque filtre | ✅ | Tous les `Adjustment` ont des paramètres continus + aperçu en direct (calque de réglage). |

**Score : 6 ✅ / 3 🟡 / 3 ❌**

---

## Texte, vectoriel, couleur & moteur

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 96 | Outil texte (polices système, contour, ombre, texte sur courbe) | ✅ | `TextItem` — polices système, contour, ombre, texte sur arc. |
| 97 | Formes vectorielles + plume (Bézier) + opérations booléennes | ✅ | Formes géométriques, `tools/pen.rs` (Bézier), `tools/boolean.rs` (Union/Soustraction/Intersection). |
| 98 | Sélecteur de couleur, pipette, nuanciers, extraction de palette | 🟡 | Pipette + nuanciers personnalisés présents ; **pas d'extraction automatique de palette** depuis une image. |
| 99 | Transformations (rotation, échelle, inclinaison, perspective, warp) | 🟡 | Rotation, échelle, perspective (homographie 4 coins) et warp Arc confirmés ; **inclinaison (skew/shear) introuvable** dans le code — aucune fonction de cisaillement trouvée. |
| 100 | Rendu accéléré GPU (via wgpu) | ❌ | `eframe = { version = "0.29", features = ["accesskit"] }` (Cargo.toml) — features par défaut, donc backend **glow** (OpenGL), pas `wgpu`. Le compositeur logiciel (`tiny-skia`) est lui strictement CPU. Aucune accélération GPU explicite via wgpu. |
| 101 | Historique d'annulation illimité | ✅ | `history.rs` — pile sans limite fixe. |
| 102 | Raccourcis personnalisables | ✅ | `keybindings.rs` — persistés dans `settings.json`. |

**Score : 5 ✅ / 2 🟡 / 1 ❌**

---

## Synthèse globale

| Statut | Nombre approximatif | % (sur ~102 items) |
|---|---|---|
| ✅ Implémenté | 64 | ~63 % |
| 🟡 Partiel | 19 | ~19 % |
| ❌ Absent | 20 | ~20 % |

### Ce qui manque complètement (❌), par ordre d'impact utilisateur probable

**Format & export**
- Import SVG (seul l'export existe)
- Export GIF (statique et animé)
- Export d'une zone sélectionnée uniquement
- Aperçu/poids estimé avant export
- Suppression de métadonnées à l'export
- Glisser-déposer de fichiers

**Sélection**
- Soustraction/intersection de sélection (seul l'ajout existe)
- Contour progressif (feather) en opération générique de sélection
- Dilater/contracter la sélection
- Inversion de sélection
- Découpe automatique des bords vides (trim)

**Calques**
- Calque de remplissage (uni/dégradé/motif)
- Alignement/distribution des calques eux-mêmes
- Vignettes de prévisualisation des calques
- Recherche/filtre dans la liste des calques

**Dessin**
- Aérographe
- Import de brosse depuis une image
- Prévisualisation du contour de brosse

**Filtres & effets**
- Pixelisation/mosaïque
- Halftone (trame)
- Distorsions vague/sphère/tourbillon
- Flou radial/zoom
- Auto-correction en un clic
- Mixeur de canaux pour le N&B

**Moteur**
- Rendu GPU via `wgpu` spécifiquement (le rendu UI utilise `glow`/OpenGL par défaut d'eframe ; le compositeur photo reste CPU par choix architectural — voir ARCHITECTURE.md)
- Inclinaison (skew/shear) dans les transformations

### Points d'attention

1. **Sélection : les opérations booléennes sont la lacune la plus surprenante.**
   Seul l'ajout (Maj+glisser) existe ; pas de soustraction, intersection, ni
   inversion. C'est une fonctionnalité de base attendue dans tout éditeur
   d'image dès qu'on a plusieurs modes de sélection (rectangle/ellipse/
   lasso/baguette) — les avoir sans pouvoir les combiner est une limite
   nette.

2. **Rendu GPU : réponse dépend de la définition.** Le rendu de l'interface
   (`egui`) passe par le backend `glow` d'eframe, qui est bien accéléré GPU
   via OpenGL — mais la checklist demande spécifiquement `wgpu`, absent. Le
   compositeur photo (calques, filtres, texte rastérisé) est volontairement
   **CPU** (`tiny-skia`), un choix architectural documenté, pas un oubli.

3. **Formats d'export : les manques sont cohérents avec le reste du
   positionnement du projet** (pas de dépendances système lourdes) — GIF
   animé et SVG import demanderaient des bibliothèques supplémentaires,
   à mettre en balance avec la philosophie « peu de dépendances » déjà
   assumée pour HEIC/RAW/WebP lossy.

4. **Calques de remplissage et alignement de calques** sont les deux manques
   « calques » les plus simples à combler (pas de nouvelle dépendance,
   architecture déjà en place pour les calques d'ajustement/groupes).

---

*Audit réalisé par lecture du code source uniquement (deux passes de
relecture croisée pour vérifier les points ambigus : backend de rendu,
inclinaison, opérations de sélection). Les points 🟡/❌ les plus
susceptibles d'affecter une communication produit externe sont ceux des
sections Sélection et Filtres & effets — à vérifier manuellement avant toute
annonce publique de fonctionnalité.*
