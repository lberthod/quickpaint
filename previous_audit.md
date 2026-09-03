# previous_audit.md — Audits historiques QuickPaint (fusion)

> Fusion de 4 audits précédemment séparés (`audit_next.md`, `audit_aout.md`,
> `audit_uix_expert.md`, `audit_100_features.md`), conservés ici comme
> sections distinctes, dans l'ordre chronologique. Contenu inchangé — seuls
> les noms de fichiers ont été mis à jour dans les renvois croisés (code
> source et autres docs) pour pointer vers ce fichier unique.

**Sommaire** :
1. [audit_next — audit fonctionnel (5 juillet 2026)](#section-audit_next)
2. [audit_aout — audit technique & plan d'optimisation (29 août 2026)](#section-audit_aout)
3. [audit_uix_expert — avis critique UI/UX (29 août 2026)](#section-audit_uix_expert)
4. [audit_100_features — QuickPaint vs Canva/PS/AI/GIMP (29 août - 1er septembre 2026)](#section-audit_100_features)

---


<a id="section-audit_next"></a>

## audit_next — Audit fonctionnel QuickPaint (5 juillet 2026)

Audit du code source par rapport à une nouvelle checklist produit, plus large
que les précédentes (`audit_newxxx.md`/`audit_sprint_xx.md`, retirées une
fois actées — voir le journal git). Statuts : **✅ Implémenté** · **🟡
Partiel** · **❌ Absent**, avec pointeur de code ou « introuvable ».

Méthode : lecture directe du code (`grep`/lecture de fichiers), pas
d'estimation à partir des seuls noms de fonctionnalités.

---

### Fichiers & formats ouverts

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 1 | Nouveau document, tailles prédéfinies et personnalisées | 🟡 | `new_document()`/`new_document_sized()` (app/mod.rs) + galerie de modèles (`templates()`, ui/toolbar.rs) — présélections social/impression/écran, mais aucune n'est littéralement « 4K » (3840×2160). |
| 2 | Ouverture PNG, JPEG, BMP, TIFF, GIF, WebP | ✅ | `project.rs:81` — toutes acceptées via la crate `image`. GIF corrigé au Sprint L.6 : la feature `gif` n'était pas compilée (Cargo.toml), le décodage échouait en silence malgré le filtre de fichiers l'annonçant. |
| 3 | Import SVG (rendu vectoriel) | ✅ | Sprint L.5 : `svg_import.rs` (nouveau module, via `usvg`) — path/rect/circle/ellipse/line/polyline/polygon/texte/groupes/transforms convertis en `Stroke`/`TextItem` éditables (nouveau document, comme l'import PSD). Dégradés/motifs/clipPath/filtres hors scope (repli documenté), police système générique au lieu de la police exacte du SVG. |
| 4 | Format natif avec calques (conteneur ouvert type zip+JSON) | 🟡 | Format natif = JSON pur (`project.rs`, `serde_json`), pas un conteneur zip+JSON — fonctionnellement équivalent (calques éditables persistés) mais pas le conteneur littéralement demandé. |
| 5 | Export PNG avec canal alpha | ✅ | `export.rs:112`. |
| 6 | Export JPEG avec réglage de qualité | ✅ | Curseur 1–100 (`app/mod.rs`, `export.rs:118`). |
| 7 | Export WebP (avec/sans perte) | 🟡 | Export WebP existe mais **toujours sans perte** (crate `image`) — le WebP *lossy* demanderait `libwebp`, écarté par décision produit (voir `audit_sprint_xx.md`, retiré). |
| 8 | Export TIFF et BMP | ✅ | Via la crate `image` (features Cargo.toml). |
| 9 | Export GIF (statique et animé) | ✅ | Sprint L.6 : export GIF **statique** (`ExportFormat::Gif`, feature `gif` de la crate `image` activée — corrige au passage un bug latent : l'import GIF annoncé ✅ ne décodait en réalité rien, la feature n'était pas compilée) et **animé** — `Document::frames: Vec<AnimationFrame>` (instantané complet de la pile de calques par frame), panneau « Animation » (ajout/suppression/réordonnancement/délai par frame), export GIF animé via `image::codecs::gif`. Chaque opération de frame passe par l'undo général (`Command::SetDoc`), donc annulable. |
| 10 | Export SVG des éléments vectoriels | ✅ | `svg.rs` — traits, images (base64), textes, opacité de calque. |
| 11 | Export PDF (rasterisé ou vectoriel) | ✅ | Sprint L.7 : `pdf_vector.rs`, nouveau module — traits/formes en opérateurs de dessin PDF réels, texte en police standard (Helvetica/Bold, WinAnsiEncoding), images en XObject JPEG. Le PDF rasterisé (`export.rs`) reste disponible séparément (plus rapide, taille de fichier prévisible). |
| 12 | Export par lots (plusieurs fichiers) | ✅ | `save_batch()` (`export.rs:46`). |
| 13 | Export multi-tailles en un clic | ✅ | `BatchExportState` (`app/mod.rs`) + UI dédiée. |
| 14 | Export d'une zone sélectionnée uniquement | ✅ | Sprint L.1 : dialogue d'aperçu, case « Exporter uniquement la sélection » (recadrage post-rendu à la boîte englobante). |
| 15 | Aperçu et poids estimé avant export | ✅ | Sprint L.2 : miniature + poids en Ko/Mo, encodé une seule fois en mémoire (réutilisé tel quel à l'écriture finale). |
| 16 | Profils d'export réutilisables (presets) | ✅ | Sprint L.8 : `ExportProfile` (format + qualité + tailles), nommé et persisté comme `brush_presets`/`style_presets`. |
| 17 | Suppression optionnelle des métadonnées | ✅ | Vérifié (Sprint L.3) : l'export part toujours d'un buffer RGBA fraîchement rendu par le compositeur, jamais des octets du fichier source — aucune métadonnée n'est donc jamais écrite, par construction. Documenté dans `export.rs`, pas de case à cocher nécessaire. |
| 18 | Glisser-déposer de fichiers | ✅ | Sprint L.4 : `egui::Event`/`i.raw.dropped_files`, dispatché par extension (image/psd/json/svg). |
| 19 | Import depuis le presse-papiers | ✅ | `paste_image()` via `arboard` (`app/mod.rs`). |
| 20 | Sauvegarde automatique + récupération après crash | ✅ | `project.rs` — autosave périodique vers `recovery.json`, restauration proposée au démarrage. |

**Score : 17 ✅ / 3 🟡 / 0 ❌** *(sur 20 — un item peut compter dans les deux
premières colonnes selon la lecture)*

---

### Calques & composition

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 21 | Calques multiples empilables | ✅ | `Document.layers: Vec<Layer>`. |
| 22 | Réordonnancement par glisser-déposer | ✅ | `dnd_drag_source`/`dnd_drop_zone` (`ui/layers.rs`). |
| 23 | Opacité réglable par calque | ✅ | `layer.opacity` + curseur. |
| 24 | Modes de fusion (multiply, screen, overlay…) | ✅ | `BlendMode` — Normal/Multiply/Screen/Overlay/Darken/Lighten (6 modes). |
| 25 | Masque de calque (niveaux de gris) | ✅ | `layer.mask: Option<RasterLayer>`, peint en niveaux de gris. |
| 26 | Masque d'écrêtage | ✅ | `layer.clip: bool`. |
| 27 | Groupes de calques | ✅ | `layer.group: Option<String>` + repli/dépli dans l'UI. |
| 28 | Verrouillage (position, pixels, transparence) | ✅ | `layer.locked` (verrou global, inchangé) + deux verrous granulaires indépendants et cumulables : `layer.lock_position` (bloque `push_move`, le glisser-déplacer, sans bloquer la peinture) et `layer.lock_alpha` (restaure l'alpha d'origine des tuiles touchées à la fin du geste, dans `commit_raster_stroke` — peindre ne peut plus rendre opaque un pixel transparent, ni la gomme en rendre un transparent). Périmètre : `lock_position` ne couvre que le glisser sur le canevas (`push_move`), pas `align()`/`reorder()` ; `lock_alpha` ne couvre que le contenu peint (pas le masque de calque). |
| 29 | Visibilité par calque | ✅ | `layer.visible` + icône œil. |
| 30 | Renommage et code couleur | ✅ | Sprint I.5 : `layer.color_tag: Option<[u8;3]>` (palette 8 couleurs) + renommage. |
| 31 | Duplication de calque | ✅ | `duplicate_layer()`. |
| 32 | Fusion de calques / aplatissement | ✅ | `merge_down()` / `flatten()`. |
| 33 | Calque de remplissage (uni, dégradé, motif) | ✅ | Sprint I.1 : `Layer::new_fill` + `FillKind::{Solid, Linear, Radial}` (pas de motif/pattern). |
| 34 | Calques de réglage non-destructifs | ✅ | `layer.adjustment: Option<Adjustment>` — Niveaux, Courbes, Teinte/Saturation, Exposition, Vibrance, Balance des blancs, Réduction de bruit, Flou gaussien/mouvement/bokeh/radial, Duotone, Distorsion, Aberration chromatique, Warp Arc/Vague/Sphère/Tourbillon, Vignette, Mixeur N&B, Pixelisation, Halftone. |
| 35 | Styles de calque (ombre, contour, lueur) | ✅ | `LayerStyle` — DropShadow/Stroke/Glow (interne/externe). |
| 36 | Alignement et distribution de calques | ✅ | Sprint I.2 : `align_layer_to_document()` aligne le contenu entier d'un calque au document (6 modes). Distribution entre plusieurs calques : `distribute_layers()`, à partir d'une sélection multi-calque dans le panneau (`layer_multi_select`, ⇧/⌘+clic sur un nom de calque) — au moins 3 calques non vides, les deux extrêmes (par centre de boîte englobante) restent fixes, les autres sont espacés uniformément ; un seul pas d'annulation (`Command::SetDoc`). |
| 37 | Vignettes de prévisualisation | ✅ | Sprint I.3 : miniature par calque (`Compositor::layer_thumbnail`, cache réutilisé, invalidé par hash). |
| 38 | Recherche/filtre dans la liste des calques | ✅ | Sprint I.4 : champ de filtre (révélé au-delà de 8 calques). |

**Score : 17 ✅ / 0 🟡 / 0 ❌**

---

### Dessin & peinture

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 39 | Pinceau à taille et dureté réglables | ✅ | `brush.width` + dureté (pinceau pixel). |
| 40 | Crayon pixel-perfect | ✅ | `ActiveTool::Pencil` : outil dédié dans la barre d'outils, dessine exactement comme le Pinceau (même chemin `handle_draw`, non spécialisé) — la sélection du bouton applique automatiquement le préréglage « Crayon fin » (trait fin, bord net, peu de lissage). Cosmétique/ergonomique par choix, pas un nouveau moteur de dessin. |
| 41 | Gomme avec opacité | ✅ | `Eraser`/`PixelEraser`, opacité via l'état de l'outil. |
| 42 | Pot de remplissage avec tolérance | ✅ | `ActiveTool::Bucket` + curseur de tolérance. |
| 43 | Dégradé linéaire, radial, conique | ✅ | `GradientKind::{Linear, Radial, Conic}`. |
| 44 | Aérographe | ✅ | Sprint J.1 : `ActiveTool::Airbrush`, dépose en continu à intervalles réguliers tant que le clic est maintenu. |
| 45 | Outil doigt / étalement | ✅ | `ActiveTool::Smudge`. |
| 46 | Tampon de duplication (clone) | ✅ | `ActiveTool::CloneStamp`. |
| 47 | Densité + / − | ✅ | `Dodge`/`Burn`. |
| 48 | Éponge (saturation locale) | ✅ | `Saturate`/`Desaturate`. |
| 49 | Bibliothèque de brosses | 🟡 | 4 préréglages embarqués (Feutre, Crayon fin, Aquarelle douce, Calligraphie) + sauvegarde/chargement custom + import d'image comme tampon (Sprint J.2) — pas de galerie visuelle riche avec import de fichiers de brosses tiers (.abr etc.). |
| 50 | Import de brosses depuis image (motif → pinceau) | ✅ | Sprint J.2 : `BrushStamp::from_rgba` (luminance = couverture), échantillonné par le Pinceau pixel à la place de la formule circulaire. |
| 51 | Dynamique de brosse selon la pression du stylet | ✅ | `input/pressure.rs::width_for_pressure()`. |
| 52 | Support tablette graphique / stylet | ✅ | Pression réelle via événements `Touch` d'egui, repli sur simulation vitesse. |
| 53 | Stabilisation du tracé | ✅ | Lissage EMA réglable (`input/smoothing.rs`). |
| 54 | Symétrie (miroir, radiale, mandala) | 🟡 | Miroir/rotation à N axes (2 à 12, réglable) autour du centre — couvre l'usage « mandala » basique, mais ce n'est pas un mode « radiale » distinct avec réflexion **et** rotation combinées comme Krita/Procreate. |
| 55 | Règles, guides et grille magnétiques | ✅ | Grille, règles, aimantation (`snap()`, `tools/guides.rs`). |
| 56 | Prévisualisation du contour de brosse | ✅ | Sprint J.3 : `paint_cursor()` étendu au Pinceau/Gomme pixel et à l'Aérographe (existait déjà pour le pinceau vectoriel). |

**Score : 16 ✅ / 1 🟡 / 0 ❌**

---

### Sélection & découpe

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 57 | Sélection rectangle et ellipse | ✅ | `SelectMode::{Rect, Ellipse}`. |
| 58 | Lasso libre et lasso polygonal | 🟡 | Lasso libre (`SelectMode::Lasso`) présent ; pas de variante « polygonale » distincte (clic par sommet plutôt que glissé continu). |
| 59 | Baguette magique (tolérance réglable) | ✅ | `magic_wand()` + curseur de tolérance. |
| 60 | Sélection par plage de couleurs | ✅ | Mode global de la baguette magique (`flood_global`, non contigu). |
| 61 | Ajout / soustraction / intersection de sélection | ✅ | Sprint G.1 : `SelectionCombine { Replace, Add, Subtract, Intersect }`, Maj=Add, Alt=Subtract, Maj+Alt=Intersect. |
| 62 | Contour progressif (feather) | ✅ | Sprint H : masque de sélection en pixels (`PaintApp::selection_mask`, `tools/selection_mask.rs`) + `feather_selection()` (flou boîte du canal de couverture). |
| 63 | Dilater / contracter la sélection | ✅ | Sprint H : `dilate_selection()`/`contract_selection()` (filtre morphologique max/min sur un disque), même masque en pixels que le feather. |
| 64 | Inversion de sélection | ✅ | Sprint G.2 : `invert_selection()` (⌘⇧I + menu Édition). |
| 65 | Enregistrer / charger une sélection | ✅ | `save_named_selection()`/`load_named_selection()`. |
| 66 | Recadrage libre et par ratio | ✅ | Ratios libre/1:1/4:3/16:9/A4. |
| 67 | Redressement d'horizon | ✅ | Curseur d'angle -45°..45° + `straighten_and_crop()`. |
| 68 | Découpe automatique des bords vides (trim) | ✅ | Sprint G.3 : `trim_bounds()` + « Rogner les bords vides » (menu Image). |

**Score : 12 ✅ / 0 🟡 / 0 ❌**

---

### Retouche photo

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 69 | Luminosité / contraste | ✅ | `Filter::{Brighter, Darker, Contrast}`. |
| 70 | Exposition | ✅ | `Adjustment::Exposure { ev }`. |
| 71 | Niveaux (noir/blanc/gris) | ✅ | `Adjustment::Levels { black, white, gamma }`. |
| 72 | Courbes RVB | ✅ | `Adjustment::Curves` (3 points ancrés, canal composite). |
| 73 | Teinte / saturation / vibrance | ✅ | `HueSaturation` + `Vibrance`. |
| 74 | Balance des couleurs | ✅ | `WhiteBalance { temp, tint }` couvre l'essentiel (pas de balance séparée ombres/tons moyens/hautes lumières façon Photoshop). |
| 75 | Température / balance des blancs | ✅ | Idem (même ajustement). |
| 76 | Conversion N&B avec mixage de canaux | ✅ | Sprint K.7 : `Adjustment::ChannelMixerBw { r, g, b }`, poids réglables par canal. |
| 77 | Netteté (accentuation / passe-haut) | ✅ | `Filter::Sharpen` (noyau 3×3). |
| 78 | Réduction de bruit | ✅ | `Adjustment::Denoise` (lissage bilatéral). |
| 79 | Suppression yeux rouges | ✅ | `reduce_red_eye()`. |
| 80 | Correction de distorsion et vignettage | ✅ | Distorsion radiale (barrel/pincushion) + Sprint K.5 : `Adjustment::Vignette` indépendant. |
| 81 | Histogramme en temps réel | ✅ | Étendu au canevas entier récemment (`canvas_histogram()`). |
| 82 | Comparaison avant / après | ✅ | Bouton « Avant (maintenir) ». |
| 83 | Auto-correction en un clic | ✅ | Sprint K.8 : `Filter::AutoLevels` (étire l'histogramme par canal, percentiles 1/99). |

**Score : 15 ✅ / 0 🟡 / 0 ❌**

---

### Filtres & effets

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 84 | Flou gaussien | ✅ | `Adjustment::GaussianBlur` (noyau séparable). |
| 85 | Flou de mouvement et flou radial/zoom | ✅ | Flou de mouvement directionnel + Sprint K.4 : `Adjustment::RadialBlur` (effet vitesse/explosion). |
| 86 | Pixelisation / mosaïque | ✅ | Sprint K.1 : `Adjustment::Pixelate { block }`. |
| 87 | Détection de contours (Sobel / Canny) | ✅ | Sobel (utilisé par Croquis et BD) **et** Canny : `Filter::Canny` — lissage, gradients Sobel avec direction, suppression non maximale, double seuil + hystérésis (`tools/filter.rs::canny_edges`). |
| 88 | Posterisation et seuil (effet BD) | ✅ | `Filter::Comic` (posterisation 5 niveaux + contours Sobel). |
| 89 | Grain et bruit ajoutable | ✅ | `Filter::FilmGrain` (bruit procédural déterministe). |
| 90 | Vignette artistique | ✅ | Sprint K.5 : `Adjustment::Vignette { amount }`, extrait du filtre Vintage en réglage autonome. |
| 91 | Duotone / bichromie | ✅ | `Adjustment::Duotone`. |
| 92 | Halftone (trame) | ✅ | Sprint K.2 : `Adjustment::Halftone { cell, angle }`. |
| 93 | Distorsions (vague, sphère, tourbillon) | ✅ | Sprint K.3 : `Adjustment::{Wave, Sphere, Vortex}`. |
| 94 | Import de LUT `.cube` | ✅ | `tools/lut.rs` — interpolation trilinéaire, intensité réglable. |
| 95 | Intensité réglable + aperçu direct pour chaque filtre | ✅ | Tous les `Adjustment` ont des paramètres continus + aperçu en direct (calque de réglage). |

**Score : 12 ✅ / 0 🟡 / 0 ❌**

---

### Texte, vectoriel, couleur & moteur

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 96 | Outil texte (polices système, contour, ombre, texte sur courbe) | ✅ | `TextItem` — polices système, contour, ombre, texte sur arc. |
| 97 | Formes vectorielles + plume (Bézier) + opérations booléennes | ✅ | Formes géométriques, `tools/pen.rs` (Bézier), `tools/boolean.rs` (Union/Soustraction/Intersection). |
| 98 | Sélecteur de couleur, pipette, nuanciers, extraction de palette | ✅ | Sprint M.1 : `tools::palette::extract_palette()` (histogramme grossier + pics), bouton « Extraire d'une image ». |
| 99 | Transformations (rotation, échelle, inclinaison, perspective, warp) | ✅ | Sprint M.2 : `Command::Shear` + poignées dédiées (losanges) sur la boîte de sélection — cisaillement point par point pour les traits ; textes/images n'ont pas de champ d'inclinaison dans le modèle actuel, seule leur ancre se déplace (comme pour Scale/Rotate). |
| 100 | Rendu accéléré GPU (via wgpu) | ❌ | `eframe = { version = "0.29", features = ["accesskit"] }` (Cargo.toml) — features par défaut, donc backend **glow** (OpenGL), pas `wgpu`. Le compositeur logiciel (`tiny-skia`) est lui strictement CPU. Aucune accélération GPU explicite via wgpu. |
| 101 | Historique d'annulation illimité | ✅ | `history.rs` — pile sans limite fixe. |
| 102 | Raccourcis personnalisables | ✅ | `keybindings.rs` — persistés dans `settings.json`. |

**Score : 7 ✅ / 0 🟡 / 1 ❌**

---

### Synthèse globale

| Statut | Nombre (sur 102 items) | % |
|---|---|---|
| ✅ Implémenté | 95 | ~93 % |
| 🟡 Partiel | 6 | ~6 % |
| ❌ Absent | 1 | ~1 % |

*(Mis à jour après les Sprints G, H, K, I, J, M et L (complet, y compris L.5/L.6/L.7), puis une
session dédiée aux quatre derniers points optionnels — voir [CHANGELOG.md](CHANGELOG.md) : G
a réglé 61/64/68 (sélection, opérations d'ensemble) ; H a réglé 62/63 (masque de sélection en
pixels — feather, dilater/contracter) ; K a réglé 76/80/83/85/86/90/92/93 (filtres & effets) ; I
a réglé 30/33/37/38 (calques) ; J a réglé 44/50/56 (dessin) ; M a réglé 98/99
(couleur/transformations) ; L a réglé 3/9/14/15/16/17/18/11 (export, dont l'import SVG
vectoriel, le PDF vectoriel et le GIF animé — `Document::frames`, panneau « Animation », export
via `image::codecs::gif`). Session suivante : 36 (distribution multi-calque —
`layer_multi_select` + `distribute_layers`), 87/K.6 (Canny — `Filter::Canny`), 40 (Crayon —
`ActiveTool::Pencil`), 28 (verrouillage granulaire — `lock_position`/`lock_alpha`). Seul
100/N.1 (rendu GPU wgpu) reste, décision d'architecture non tranchée.)*

#### Ce qui manque complètement (❌), par ordre d'impact utilisateur probable

**Format & export**
(tout traité par le Sprint L — voir [CHANGELOG.md](CHANGELOG.md), y
compris l'import SVG vectoriel, le PDF vectoriel et le GIF statique **et**
animé)

**Sélection**
(tout traité par les Sprints G et H — voir [CHANGELOG.md](CHANGELOG.md) :
opérations d'ensemble, inversion, trim, et désormais feather/dilater/
contracter via un vrai masque de sélection en pixels)

**Calques**
(tout traité — Sprint I, puis distribution multi-calque (point 36) et
verrouillage granulaire (point 28) dans une session dédiée aux quatre
derniers points optionnels, voir [CHANGELOG.md](CHANGELOG.md))

**Dessin**
(tout traité — Sprint J, puis l'outil Crayon dédié (point 40) dans la même
session)

**Filtres & effets**
(tout traité — Sprint K, puis Canny (point 87/K.6) dans la même session)

**Moteur**
- Rendu GPU via `wgpu` spécifiquement (le rendu UI utilise `glow`/OpenGL par défaut d'eframe ; le compositeur photo reste CPU par choix architectural — voir ARCHITECTURE.md)

#### Points d'attention

1. **Sélection : complète, y compris feather/dilater/contracter.** ✅
   Résolu par les Sprints G et H (voir [CHANGELOG.md](CHANGELOG.md)) :
   soustraction/intersection (Alt/Maj+Alt), inversion (⌘⇧I) et trim des
   bords vides (Sprint G) ; contour progressif et dilater/contracter
   (Sprint H) via un vrai masque de sélection en pixels
   (`PaintApp::selection_mask`, `tools/selection_mask.rs`), peuplé
   directement depuis la géométrie du geste de sélection
   (rectangle/ellipse/lasso — pixel-précis ; baguette magique : approximé
   par union des boîtes englobantes des éléments retenus, limite
   documentée). Intégré au Pinceau/Gomme pixel et à l'Aérographe
   (`RasterLayer::stamp`/`stamp_custom`/`stroke_segment` acceptent
   désormais un masque optionnel qui multiplie leur couverture) ; aperçu
   visuel en teinte semi-transparente hors sélection (option la moins
   coûteuse évoquée dans l'audit initial, pas de vraie animation « fourmis
   en mouvement »). Le pot de peinture et les autres outils raster
   (tampon de clonage, densité +/-, éponge…) ne respectent pas encore ce
   masque — périmètre volontairement limité au point d'intégration le
   plus net (pinceau pixel), à étendre plus tard si le besoin se confirme.

2. **Filtres & effets : complet.** ✅ Résolu par le Sprint K
   (voir [CHANGELOG.md](CHANGELOG.md)) : pixelisation, halftone,
   vague/sphère/tourbillon, flou radial, vignette autonome, mixeur de
   canaux N&B, auto-correction. La détection de contours Canny (point
   87/K.6) a suivi : `Filter::Canny` (`tools/filter.rs`) — lissage 3×3,
   gradients Sobel avec direction (pas seulement la magnitude de
   `sobel_magnitude`, utilisée par Croquis/BD), suppression non maximale le
   long de la direction du gradient, puis double seuil + hystérésis
   (8-connexité) — contours fins et continus là où Sobel seul produirait
   des bords plus épais/bruités.

3. **Rendu GPU : réponse dépend de la définition.** Le rendu de l'interface
   (`egui`) passe par le backend `glow` d'eframe, qui est bien accéléré GPU
   via OpenGL — mais la checklist demande spécifiquement `wgpu`, absent. Le
   compositeur photo (calques, filtres, texte rastérisé) est volontairement
   **CPU** (`tiny-skia`), un choix architectural documenté, pas un oubli.

4. **Calques : complet.** ✅ Résolu par le Sprint I (voir
   [CHANGELOG.md](CHANGELOG.md)) : calque de remplissage (uni/dégradé),
   code couleur, vignettes de prévisualisation, recherche/filtre, et
   alignement du contenu d'un calque par rapport au document. Distribution
   entre plusieurs calques (point 36) et verrouillage granulaire (point 28)
   ont suivi : sélection multi-calque dans le panneau (`layer_multi_select`,
   ⇧/⌘+clic sur un nom de calque) puis `distribute_layers()` (au moins 3
   calques non vides, les deux extrêmes par centre de boîte englobante
   restent fixes) ; `layer.lock_position` (bloque `push_move`, le
   glisser-déplacer, sans bloquer la peinture) et `layer.lock_alpha`
   (restaure l'alpha d'origine des tuiles touchées à la fin du geste de
   peinture — un pixel transparent ne peut plus devenir opaque, ni
   l'inverse), indépendants du verrou global existant (`layer.locked`,
   inchangé) et cumulables avec lui.

5. **Dessin : complet.** ✅ Résolu par le Sprint J (voir
   [CHANGELOG.md](CHANGELOG.md)) : aérographe, import de brosse depuis
   une image (tampon en niveaux de gris), prévisualisation du contour de
   brosse étendue au pinceau/gomme pixel et à l'aérographe. L'outil Crayon
   dédié (point 40) a suivi : `ActiveTool::Pencil`, qui dessine exactement
   comme le Pinceau (même chemin `handle_draw`, non spécialisé pour ce
   nouvel outil) — la sélection du bouton dans la barre d'outils applique
   automatiquement le préréglage « Crayon fin » déjà existant. Choix
   délibérément cosmétique/ergonomique (bouton dédié plus visible qu'un
   préréglage caché dans un menu), pas un second moteur de dessin.

6. **Texte, vectoriel, couleur : complet.** ✅ Résolu par le Sprint M (voir
   [CHANGELOG.md](CHANGELOG.md)) : extraction de palette depuis une
   image (point 98) et cisaillement/skew (point 99, via des poignées
   dédiées sur la boîte de sélection). Pour le skew, seuls les traits sont
   véritablement déformés point par point ; textes/images n'ont pas de champ
   d'inclinaison dans le modèle actuel (seule leur ancre se déplace, comme
   pour Scale/Rotate) — limite technique documentée dans le code, pas un
   oubli.

7. **Export : complet, y compris les points qui demandaient une décision.**
   ✅ Résolu par le Sprint L (voir [CHANGELOG.md](CHANGELOG.md)) : export
   d'une zone sélectionnée, aperçu + poids estimé, profils d'export nommés,
   glisser-déposer de fichiers, import SVG vectoriel éditable
   (`svg_import.rs`, via `usvg`), export PDF vectoriel (`pdf_vector.rs`),
   export GIF statique **et animé** ; suppression de métadonnées vérifiée
   comme déjà satisfaite par construction. En ajoutant le GIF, régression
   latente trouvée et corrigée au passage : la feature `gif` de la crate
   `image` n'était pas activée dans `Cargo.toml`, donc l'import GIF déjà
   annoncé ✅ dans un audit précédent ne décodait en réalité rien — corrigé
   et couvert par un test de régression.

   Le GIF **animé** a introduit le seul vrai changement de modèle de données
   de cette session : `Document::frames: Vec<AnimationFrame>`, où chaque
   frame est un instantané complet de la pile de calques (pas une timeline
   de keyframes par calque — choix volontairement le plus simple des deux
   options évoquées dans l'audit précédent). Vide par défaut, donc aucun
   effet sur les documents existants. Chaque opération de frame (ajouter/
   supprimer/réordonner/changer de frame) passe par l'undo général
   (`Command::SetDoc`), pas un système dédié. Point d'attention corrigé au
   passage : l'encodage/décodage du raster peint et des masques
   (`encode_all_images`/`apply_loaded`) ne parcourait que la frame active —
   sans le correctif, sauvegarder puis rouvrir un projet animé aurait
   silencieusement vidé le contenu peint des frames non actives ; couvert
   par un test de régression dédié.

8. **Import SVG : limites assumées de la conversion vectorielle.** Police
   système générique plutôt que la police exacte du SVG (pas d'embarquement
   de police arbitraire) ; dégradés/motifs de remplissage repliés sur une
   couleur unie ; `clipPath`/masques/filtres SVG ignorés (le contenu reste
   visible, juste sans l'effet). Un élément avec fill *et* stroke à la fois
   perd son contour (le modèle `Stroke` ne porte qu'un style à la fois) —
   heuristique de priorité basée sur l'aire réelle du tracé (une ligne sans
   aire garde son contour même si `fill` résout par défaut à noir).

---

*Audit réalisé par lecture du code source uniquement (deux passes de
relecture croisée pour vérifier les points ambigus : backend de rendu,
inclinaison, opérations de sélection). Les points 🟡/❌ les plus
susceptibles d'affecter une communication produit externe sont ceux des
sections Sélection et Filtres & effets — à vérifier manuellement avant toute
annonce publique de fonctionnalité.*


---


<a id="section-audit_aout"></a>

## audit_aout — Audit technique & plan d'optimisation (29 août 2026)

Consolide un audit technique frais (build/tests/clippy/robustesse, ce jour)
avec l'état déjà connu et documenté ailleurs (`sprint.md`, CHANGELOG.md,
`deployappstore.md`) pour produire **une seule liste d'actions
priorisée**, orientée « rendre l'app plus rapide/robuste et livrable à de
vrais utilisateurs ».

Ne re-décrit pas ce qui est déjà fait — voir [previous_audit.md](previous_audit.md)
et [CHANGELOG.md](CHANGELOG.md) (0.20.0) pour le détail fonctionnel
(score global : quasi 100 % des ~120 items produit couverts).

---

### Constat du jour (29 août 2026)

- `cargo check` : OK, 43 warnings (type-inference `f32`/`f64`, cosmétiques,
  aucun impact runtime).
- `cargo clippy` : OK, 81 warnings — tous des suggestions de style
  (`as_chunks` au lieu de `chunks_exact` sur le code pixel manuel, 2 types
  très imbriqués dans `app/mod.rs`, un `?` à simplifier, une boucle
  réécrivable en `while let`). **Aucun lint de correctness/sécurité.**
- `cargo test` : **325/325 tests verts**, 1 ignoré, 0 échec.
- Aucun `unsafe`, aucun `panic!`/`todo!`/`unimplemented!`, aucun TODO/FIXME
  oublié dans `src/`.
- 135 `.unwrap()`/`.expect()` au total, mais quasi tous en tests ; les
  chemins réels d'I/O utilisateur (`project.rs`, `psd_import.rs`,
  `svg_import.rs`, `export.rs`, `native_menu.rs`) n'en ont **aucun** hors
  tests.
- `git ls-files` propre : `QuickPaint.app`/`QuickPaint.dmg` bien ignorés,
  pas d'artefact binaire tracké.
- `src/app/mod.rs` : 5043 lignes (avait été redescendu à 4531 par le
  sprint T3 — a regonflé avec les sprints O-U, cf. [sprint.md](sprint.md)).

---

### P0 — Bloquant pour une vraie mise à disposition utilisateurs

#### 1. Fusionner ou abandonner la branche `egui-upgrade`
État : branche à jour côté code (0.29 → 0.34, `glow` forcé, 0 warning
clippy, 299 tests verts à l'époque), mais **non fusionnée** dans `main`
(voir [sprint.md](sprint.md) T4). Un audit ne doit pas laisser une branche
de mise à jour de dépendance majeure pourrir indéfiniment : soit on la
termine, soit on documente pourquoi elle est abandonnée.

Reste à faire avant fusion (bloquant, décrit dans `sprint.md`) :
- **Test VoiceOver réel** (lecteur d'écran actif) — jamais fait, la feature
  `accesskit` est censée le supporter mais rien ne le prouve.
- **Décision explicite sur `.cargo/audit.toml`** : la branche remonte 11
  advisories `cargo audit` (9 confirmées Linux-only via `muda`, absentes du
  binaire macOS réel ; 2 réelles et non corrigibles — `ttf-parser`/
  `rustybuzz`, compilées sur macOS, projet non maintenu en amont). Il faut
  trancher : les documenter comme acceptées (avec justification écrite) ou
  chercher un contournement (fork, vendoring, alternative à `usvg`).
- Reconstruire/notariser le DMG depuis `main` après fusion.

**Pourquoi ça compte** : rester sur egui 0.29 indéfiniment coupe l'app des
correctifs de sécurité/accessibilité amont, et une branche qui diverge de
`main` depuis des semaines devient de plus en plus coûteuse à rebaser.

**Scope du merge, vérifié le 29 août 2026** (`git merge-tree`, aucun
merge réel effectué — la branche diverge de `main` depuis le 12 juillet,
`main` a 6 commits d'avance dont les sprints O-U et le nettoyage warnings
de ce jour) : 13 fichiers modifiés des deux côtés, mais seulement **5
conflits réels** (le reste se résout tout seul en 3-way) :
- `sprint.md` — divergence de contenu (deux réécritures indépendantes du
  même doc de suivi) : trivial, garder la version `main` (plus récente/à
  jour) et ré-intégrer à la main les points encore pertinents de la
  version `egui-upgrade` si besoin.
- `src/app/canvas_overlay.rs` (×2) — **conflit réel** : la rotation de
  canevas (#93, Sprint R, arrivée sur `main` après la divergence) dessine
  les cadres de sélection/recadrage en polygone (`Shape::closed_line`)
  plutôt qu'en `Rect` axis-aligned, alors que la branche a déjà migré ces
  mêmes appels vers la nouvelle signature egui 0.34
  (`rect_stroke(rect, radius, stroke, StrokeKind::Middle)`, un paramètre
  `StrokeKind` en plus). Il faut réappliquer la logique polygone de
  `main` par-dessus la nouvelle signature de `rect_stroke`, pas juste
  prendre un côté.
- `src/app/mod.rs` — **conflit réel** : la branche migre le point d'entrée
  `App::update(&Context, …)` → `App::ui(&mut Ui, …)` (eframe 0.30+,
  `ctx` récupéré via `top_ui.ctx().clone()`). Nécessite de rebrancher
  tout le corps de la fonction (autosave, panels) sur ce nouveau point
  d'entrée sans rien perdre des sprints O-U ajoutés sur `main` entre
  temps dans la même fonction.
- `src/render/text.rs` — **conflit réel** : migration d'API
  `ctx.fonts(|f| f.layout_job(...))` → `ctx.fonts_mut(|f|
  f.layout_no_wrap(text.clone(), font_id.clone(), …))` (API de layout de
  police changée entre egui 0.29 et 0.34).

Aucun de ces 3 conflits réels n'est mécanique — chacun demande de
comprendre à la fois le comportement métier ajouté sur `main` (rotation
de canevas, sprints O-U) et la nouvelle API egui 0.34, puis de vérifier
visuellement le résultat (c'est exactement pourquoi le test VoiceOver +
une repasse visuelle manuelle sont listés comme préalables dans
`sprint.md` T4.2). À traiter comme un vrai chantier de rebase, pas comme
un `git merge` à lancer et corriger au fil de l'eau.

#### 2. ✅ Tranché (30 août 2026) — Pas d'App Store, Developer ID/DMG uniquement
Décision explicite du porteur de projet : distribution **Developer ID +
DMG uniquement**, l'App Store n'est plus visé. `appstore_setup.md` et
`deployappstore.md` retirés du dépôt en conséquence (git history les
conserve si le sujet revient un jour).

**Conséquence directe pour la suite** : sans contrainte App Sandbox, la
voie plugins natifs tiers / scripting (voir `previous_audit.md`,
items #97-98) n'est plus bloquée par une incompatibilité de distribution —
c'était la seule réserve technique identifiée sur ce point.

#### 3. Distribution Developer ID (hors App Store) — vérifier que c'est à jour
`make-app.sh`/`make-dmg.sh` existent et fonctionnent (le DMG de 5,2 Mo à la
racine en atteste). À vérifier avant toute diffusion à un utilisateur
externe :
- Le DMG root (`QuickPaint.dmg`, 10 août 2026) est-il **notarisé** et
  signé avec le certificat actuel, ou juste un build de test local ? S'il
  doit être distribué tel quel, refaire un build+notarisation propre
  depuis `main` à jour plutôt que de réutiliser cet artefact.
- Un utilisateur qui double-clique un `.app` non notarisé reçoit
  aujourd'hui l'avertissement Gatekeeper « développeur non identifié » —
  premier obstacle d'usabilité pour un tiers qui n'est pas vous.

---

### P1 — Optimisation & robustesse (à faire, faible risque)

#### 4. ✅ Fait (29 août 2026) — Nettoyage des 43 warnings `f32`/`f64`
`cargo fix --bin quickpaint --allow-dirty` a suffixé les littéraux
concernés (`1.0_f32` au lieu de `1.0`) dans `canvas_overlay.rs` (28),
`app/mod.rs` (4), `pen_edit.rs` (4), `ui/layers.rs` (3), `ui/toolbar.rs`
(4). `cargo check` est désormais **0 warning**. Diff purement mécanique
(aucun changement de comportement), 325/325 tests toujours verts après.

#### 5. Clippy : 38 warnings restants, volontairement non touchés
`cargo clippy --fix` n'a rien pu appliquer automatiquement : les
suggestions `as_chunks` (35 occurrences dans `tools/filter.rs`,
`tools/lut.rs`, `tools/palette.rs`, `tools/bucket.rs`, `tools/brush.rs`,
`render/compositor.rs`) et le `while let` de `selection_mask.rs:226` sont
marquées non machine-applicables par clippy (changeraient l'ergonomie de
retour — tuple `as_chunks` vs itérateur `chunks_exact` — sur du code pixel
chaud). Les 2 « type très complexe » (`app/mod.rs:390`/`3511`) restent
aussi en l'état. **Décision** : ne pas toucher à la main sans bénéfice
mesurable — ce sont des lints de style, pas de correctness, sur du code
testé ; le risque de régression pixel-perfect (filtres) dépasse la valeur
d'un lint silencé. À reconsidérer seulement si une mise à jour clippy les
rend machine-applicables.

#### 6. ~~Vérifier le cas `Pixmap::new(0, 0)` dans le compositeur~~ — vérifié, non-problème
`src/render/compositor.rs:641` (`tint_from_alpha`) et ses appelants
(`apply_drop_shadow`/`apply_layer_stroke`/`apply_glow`, via
`apply_layer_styles`) reçoivent `w`/`h` provenant de la même fonction
`compose()`, qui fait déjà `Pixmap::new(w, h)?` à la ligne 199 — un
document de largeur/hauteur nulle fait déjà échouer (`?`) toute la
composition bien avant d'atteindre `apply_layer_styles`. Le `.unwrap()` de
`tint_from_alpha` est donc protégé par un garde-fou antérieur dans le même
appelant, pas un risque de panic réel. Aucune action nécessaire.

#### 7. Refactor différé : `app/mod.rs` (5043 lignes) et `ui/toolbar.rs` (2995 lignes)
Le sprint T3 avait déjà extrait 9 sous-modules (`selection.rs`,
`layers_ops.rs`, `io.rs`, `shortcuts.rs`, `raster_paint.rs`,
`export_ops.rs`, `canvas_overlay.rs`, `bucket_cutout.rs`,
`canvas_input.rs`) et conclu que le reste (struct `PaintApp`, `Default`,
`update()`, `on_exit()`) est le cœur non fragmentable de l'app. Les
sprints O-U (transform, animation, texte, ajustements) ont fait regonfler
le fichier de 4531 → 5043 lignes en y ajoutant de la logique neuve plutôt
que dans des sous-modules dédiés. **Action concrète** : les blocs liés à
`transform.rs`/`animation.rs`/`pen_edit.rs` déjà extraits en fichiers
séparés existent — vérifier qu'aucune logique récente (rotation de
canevas #93, symétrie #50, courbes libres #73) n'a été laissée dans
`mod.rs` par accident plutôt que dans son module dédié. Pas critique,
mais évite que le fichier ne redevienne le point de friction que T3 avait
résolu.

#### 8. ✅ Fait (29 août 2026) — Mesures de perf, plus d'optimisation à l'aveugle

`criterion` n'est pas utilisable tel quel : le crate n'a qu'une cible
`[[bin]]` (`src/main.rs`), pas de cible `[lib]` séparée à laquelle un
fichier `benches/` externe puisse se lier — en ajouter une est une
restructuration du crate (déplacer les `mod` de `main.rs` vers un
`lib.rs`), pas une action mécanique, donc pas faite sans confirmation
explicite. À la place : deux tests internes `#[ignore]` (`cargo test
--release -- --ignored --nocapture`), dans `src/app/mod.rs` —
`compose_stays_reasonably_fast_on_a_large_document` et
`undo_redo_stays_reasonably_fast_over_a_long_session` — avec un seuil
large (régression franche, pas un budget strict) et `eprintln!` du temps
réel mesuré.

**Résultats mesurés (build release, ce Mac)** :
- **Undo/redo, 500 traits vectoriels, 1000 opérations** (500 annuler + 500
  rétablir) : **50 µs au total**. Confirme que `history.rs` ne clone bien
  que des deltas pour les commandes vectorielles (`AddStroke` etc.), pas
  le document entier — la question posée dans ce point d'audit est
  tranchée, ce n'est pas un point sensible.
- **Composition complète à froid, document 4000×3000, 20 calques × 50
  traits (1000 traits)** : **~900 ms**. C'est le pire cas (tous les
  calques invalidés en même temps — ouverture de document, pas la
  peinture normale où seul le calque actif se réinvalide par trait grâce
  au cache par hash de `Compositor`). Pas assez alarmant pour justifier un
  chantier `wgpu` (Sprint N, toujours volontairement non engagé), mais à
  garder en tête si des utilisateurs rapportent un temps d'ouverture
  perceptible sur de très gros documents multi-calques.

**Conclusion** : pas de goulot d'étranglement caché trouvé qui justifierait
une optimisation immédiate — l'architecture existante (deltas d'historique,
cache de composition par hash, tuiles 256×256) fait déjà ce qu'elle est
censée faire. Sprint N (GPU) reste le seul levier si le cas d'usage
« gros document multi-calques » devient réellement un problème rapporté,
pas un fix cosmétique à faire préventivement.

---

### P2 — Usabilité (utilisateur final, pas développeur)

#### 9. Accessibilité VoiceOver — jamais validée
Mentionné en P0 côté branche `egui-upgrade`, mais reste vrai même sur
`main` : l'arbre `accesskit` est censé être construit automatiquement par
egui, mais rien dans les tests ne le vérifie (impossible à tester
unitairement — nécessite un lecteur d'écran actif). Pour une app qui vise
un usage tactile/accessible (cf. nom du dépôt), c'est le point d'usabilité
le plus significatif non couvert.

#### 10. Premier lancement — pas de tutoriel/onboarding constaté
`i18n.rs`/`app/mod.rs` gèrent les réglages persistés, mais rien dans le
code parcouru n'indique un onboarding pour un nouvel utilisateur (galerie
de modèles au démarrage : oui, via `templates()` — mais pas d'aide
contextuelle sur les outils). À vérifier avec un test manuel « premier
lancement sur un Mac vierge, sans lire le README » : est-ce que les
raccourcis (`keybindings.rs`), la barre d'outils, les calques sont
découvrables sans documentation externe ?

#### 11. Limites connues à ré-exposer clairement dans l'UI, pas seulement le code
Plusieurs limites sont documentées en commentaire de code mais invisibles
pour l'utilisateur au moment où il les rencontre (risque de confusion
« bug » alors que c'est une limite assumée) :
- Rotation du canevas ≠ 0° désactive règles/guides/pot de peinture/
  détourage — le menu Vue mentionne la limite (bien), à vérifier que
  l'outil lui-même donne un retour (curseur désactivé + tooltip) plutôt
  qu'un clic silencieusement sans effet.
- Flip de document sur du texte : glyphes non inversés (resteraient
  illisibles) — comportement correct, mais un utilisateur qui flip un
  calque texte et voit le texte non-miroir pourrait croire à un bug si
  ce n'est pas indiqué au moment du flip.
- Export SVG/PDF vectoriel du texte : pas de multi-ligne ni d'espacement
  de caractères — si un document texte riche est exporté en PDF vectoriel,
  la perte de fidélité doit être signalée avant l'export (boîte de
  dialogue), pas découverte après coup dans le fichier généré.

#### 12. Gestes multi-touch écran tactile — décision produit à formaliser
CHANGELOG.md (0.20.0, Sprint R) note que le trackpad est couvert (pinch/pan)
mais qu'un **écran tactile** natif (NSEvent hors winit) reste un chantier
non engagé, faute de décision produit. Si le nom du projet
(« paint_tactile ») implique un usage sur écran tactile réel (type
tablette graphique tactile ou écran tactile de bureau), c'est
potentiellement la fonctionnalité manquante la plus alignée avec l'intention
du produit — à re-questionner explicitement plutôt qu'à laisser en
sommeil.

---

### Ce qui n'a PAS besoin d'action (déjà solide, à ne pas re-auditer)

- Couverture fonctionnelle : ~120 items produit, quasi 100 % ✅ (voir
  `previous_audit.md`, CHANGELOG.md 0.20.0).
- Format natif, import/export multi-formats, calques, historique,
  filtres, texte, animation : tous couverts avec tests dédiés.
- Hygiène dépôt : `.gitignore` correct, pas de secret, pas de dépendance
  réseau, pas de télémétrie (non-goal assumé et respecté).
- Décisions produit déjà tranchées et closes (à ne pas rouvrir sans
  nouvelle info) : WebP lossy refusé, export PSD non supporté, import
  `.abr` non supporté, MP4 remplacé par APNG — toutes documentées dans le
  README avec justification technique.

---

### Ordre d'attaque recommandé

1. ✅ **P1.4** (warnings `f32`/`f64`) — fait le 29 août 2026, mécanique,
   0 régression (325/325 tests verts).
2. ✅ **P1.6** (garde-fou `Pixmap::new`) — vérifié le 29 août 2026 : déjà
   protégé par un `?` antérieur dans `compose()`, non-problème, aucune
   action nécessaire.
3. **P0.1** (egui-upgrade : VoiceOver + décision audit.toml + merge) —
   dette qui grossit avec le temps. Reste à faire, hors portée d'un agent
   seul (test VoiceOver nécessite un lecteur d'écran actif).
4. **P0.3** (vérifier/refaire le DMG notarisé) — bloquant dès qu'un vrai
   utilisateur externe doit installer l'app.
5. **P2.9** (test VoiceOver réel) — dépend d'un lecteur d'écran actif, à
   planifier avec le porteur de projet.
6. **P2.12** — décision produit pure (écran tactile) : à trancher
   explicitement, pas à deviner. *(P0.2 App Store tranché le 30 août
   2026 — voir section 2.)*
7. **P1.7** — refactor de `app/mod.rs`, seulement si un besoin concret
   (régression de lisibilité) apparaît.

*P1.5 (clippy `as_chunks`) volontairement laissé tel quel — voir section
5 ci-dessus. P1.8 (perf) fait le 29 août 2026 — voir section 8 : pas de
goulot d'étranglement trouvé, rien à optimiser dans l'immédiat.*


---


<a id="section-audit_uix_expert"></a>

## audit_uix_expert — Avis critique UI/UX expert (29 août 2026)

Angle : critique de designer produit / UX senior, pas d'audit de code. Basé
sur la lecture directe de la construction de l'interface (`app/mod.rs`,
`ui/toolbar.rs`, `ui/layers.rs`, `keybindings.rs`, `i18n.rs`) — pas de
capture d'écran (permission Enregistrement d'écran non accordée à ce
jour), donc certains constats sont formulés comme des **hypothèses à
vérifier visuellement**, pas des certitudes. Là où c'est le cas, c'est dit
explicitement.

Verdict global : c'est une interface **conçue par itération réelle sur
des retours d'usage** (le journal de corrections C2-C10 dans le code en
atteste — panneau calques figé corrigé, icônes undo/redo remplacées,
message d'erreur repeint en rouge…), ce qui est rare et sain. Mais cette
démarche reste **ad hoc, pas systématique** : aucune évaluation
heuristique formelle, aucun test utilisateur documenté, et au moins un
choix de design entre en contradiction frontale avec le nom même du
projet (« tactile »). Le détail ci-dessous.

---

### 🔴 Critique n°1 — Les tooltips comme seul mécanisme de découverte des 32 outils est incompatible avec un usage tactile

**Fait** : chaque bouton d'outil est une icône seule (police Phosphor,
`ui/toolbar.rs:2110`/`2238-2245`), sans label texte visible. Le nom de
l'outil et son raccourci n'apparaissent qu'au survol (`on_hover_text`).

**Pourquoi c'est un problème sérieux ici précisément** : un tooltip
déclenché par `hover` **n'existe pas au doigt**. Sur un écran tactile, un
tap est soit un clic, soit rien — il n'y a pas d'état intermédiaire
« survol » qui laisse le temps de lire un texte d'aide avant de
déclencher l'action. Un stylet peut simuler un hover selon le pilote,
mais ce n'est pas garanti pour tous les modèles. Résultat concret : sur
un projet dont le nom promet un usage tactile, **32 outils sur 32** sont
actuellement indiscoverables au doigt sans essai-erreur ou lecture
préalable de documentation externe. C'est le genre de défaut qui ne se
voit jamais en testant à la souris (ce qui explique probablement qu'il
n'a pas été détecté par les corrections C2-C10, toutes formulées comme
des observations à la souris/clavier).

**Test à faire** : poser l'app sur un vrai écran tactile ou une tablette
graphique en mode tactile, chronométrer un utilisateur qui n'a jamais vu
l'interface : combien de temps pour identifier correctement 5 outils sur
32 sans aide externe ?

**Piste de correction** (pas à décider par un agent — implication produit) :
soit un mode « libellés visibles » activable, soit un premier lancement
qui affiche les noms 3 secondes, soit une palette de commandes recherchable
(texte tapé → outil), qui ne dépend d'aucun hover.

---

### 🔴 Critique n°2 — Rouge/vert utilisé comme seul signal dans au moins deux endroits sensibles

**Faits** :
- Onion skin (Sprint U) : frame N-1 en teinte rouge, N+1 en teinte verte
  (`app/mod.rs:3703-3704`).
- Statut de fond (succès/échec d'une action) : vert vs rouge
  (`info()`/`fail()`, `app/mod.rs:2153-2163`).

**Pourquoi c'est un problème** : rouge/vert est la paire de couleurs la
moins distinguable pour ~8% des hommes (deutéranopie/protanopie — la
forme la plus fréquente de daltonisme). Pour l'onion skin, un utilisateur
concerné ne peut simplement pas dire quelle frame est avant/après sans
lire les tooltips à chaque fois — ça casse la fonctionnalité, pas juste
l'esthétique. Pour le statut succès/échec, le texte accompagne
probablement la couleur (donc dégradation gracieuse), mais l'onion skin
n'a a priori pas ce filet de sécurité textuel en continu à l'écran.

**Test à faire** : simuler deutéranopie (Sim Daltonism ou équivalent) sur
le panneau Animation avec onion skin actif — vérifier si les deux frames
restent distinguables sans la couleur seule (forme, opacité différente,
etc.).

**Piste de correction** : différencier aussi par un second canal (motif
en pointillés vs plein, ou orange/bleu plutôt que rouge/vert — paire
beaucoup plus sûre), pas seulement la teinte.

---

### 🟠 Critique n°3 — Densité d'icônes dans le panneau de calques : jusqu'à 7 éléments visuels par ligne, certains apparaissant/disparaissant selon l'état

**Fait** : une ligne de calque peut afficher, dans l'ordre : poignée de
drag, œil, cadenas global, cadenas position (si actif), cadenas alpha (si
actif), pastille de couleur, miniature 20×20, nom éditable +
suffixes (`(N traits)`, `%opacité`, préfixe `[clip]`) — `ui/layers.rs:191-320`.

**Pourquoi c'est un problème** : deux défauts cumulés — (1) c'est déjà
dense pour une ligne de liste probablement haute de 30-40px ; (2) les
icônes de verrou granulaire **n'existent visuellement que si elles sont
déjà activées**. Un nouvel utilisateur qui n'a jamais activé
`lock_position`/`lock_alpha` ne voit jamais leur icône et ne peut donc
pas deviner que la fonctionnalité existe en scannant l'interface — elle
n'est découvrable que via un menu contextuel ou une documentation externe.
C'est l'inverse du principe *visibility of system status* : le système a
un état caché qui n'a pas de représentation visuelle par défaut.

**Test à faire** : demander à quelqu'un qui n'a pas lu ce document de
verrouiller uniquement la position d'un calque (pas les pixels) sans
lui dire où chercher. Mesurer s'il trouve le clic-droit/menu adéquat.

**Piste de correction** : une icône « fantôme » (grisée, faible opacité)
visible en permanence pour signaler l'existence de l'option, qui se
« remplit » une fois activée — plutôt que absente/présente.

---

### 🟠 Critique n°4 — 32 couleurs d'accent codées en dur par outil : signal ou bruit ?

**Fait** : `tool_accent()` (`ui/toolbar.rs:2160-2194`) assigne une couleur
RVB fixe et distincte à chacun des 32 outils, indépendante du thème
clair/sombre.

**Pourquoi c'est ambigu sans capture d'écran** : le code ne dit pas si
cette couleur s'affiche en permanence sur l'icône (dans ce cas, 32
teintes différentes sur une seule barre créent un bruit visuel qui nuit
au regroupement par catégorie déjà mis en place) ou seulement à l'état
actif/survolé (dans ce cas c'est un renfort de mémorisation musculaire
utile — « le pinceau est toujours orange »). C'est le point le plus
important à trancher visuellement en priorité, parce que la réponse
change complètement le verdict (atout vs défaut).

**Test à faire** : capture d'écran de la barre d'outils au repos (aucun
outil sélectionné) — si les 32 couleurs sont déjà visibles, c'est un vrai
problème de hiérarchie visuelle à corriger (les groupes créés pour
lutter contre le désordre de l'ancienne barre plate seraient
partiellement annulés par ce bruit chromatique).

---

### 🟡 Critique n°5 — ~~Manque de labels de catégorie visibles~~ — corrigé, constat partiellement inexact

**Correction du constat initial** : en vérifiant le code pour appliquer un
correctif, `tools_row()` (`ui/toolbar.rs:2080-2086`) montre que le chevron
de repli **a déjà** un `on_hover_text(format!("{label} ..."))` avec le nom
de catégorie — l'info n'est pas aussi absente que le premier passage de
lecture l'a laissé penser (l'agent d'exploration avait trouvé
`TOOL_CATEGORY_TITLES`, utilisé dans la fenêtre d'aide, mais pas relu ce
second site). Reste vrai : cette info n'est visible qu'au survol, pas en
permanence — un utilisateur qui scanne la barre sans survoler chaque
chevron ne voit toujours que des séparateurs anonymes. Non corrigé plus
avant (afficher le nom en permanence demanderait de la place horizontale
supplémentaire dans une barre déjà dense — arbitrage produit, pas un fix
mécanique).

---

### 🟡 Critique n°6 — Sliders par défaut pour un usage annoncé « tactile »

**Fait** : au moins 38 `Slider`/`Checkbox`/`ComboBox` dans la barre
d'options contextuelle (`ui/toolbar.rs`, ex. taille/dureté/intensité de
pinceau lignes 2702-2757). Ce sont des sliders standards egui, dont la
zone de préhension est calibrée pour un curseur de souris (précision au
pixel), pas pour un doigt (zone de contact ~8-10mm, bien plus imprécise).

**Pourquoi c'est pertinent** : sans mesure de la largeur réelle des
sliders à l'écran, impossible de conclure avec certitude, mais c'est un
angle mort classique quand une UI est développée et testée principalement
à la souris (ce qui semble être le cas ici, vu que toutes les corrections
C2-C10 documentées sont des observations d'usage souris/clavier). Un
réglage de dureté de pinceau au 1/100e près est un geste précis — au
doigt, sur un slider de 150px de large, chaque incrément vaut un peu plus
d'1px, ce qui est bien en dessous de la résolution tactile humaine.

**Test à faire** : mesurer la largeur effective des sliders à l'écran (en
points), comparer à la cible Apple HIG pour les contrôles tactiles
(44×44pt minimum pour une cible fiable au doigt) — probablement en
dessous pour la hauteur de la piste du slider, même si sa largeur peut
suffire.

---

### 🟢 Points forts constatés (à ne pas perdre en corrigeant le reste)

- **Thème dynamique complet** (Système/Clair/Sombre, `UiTheme`,
  `app/mod.rs:778`) sans `Visuals::light()` forcé résiduel — propre.
- **Historique de corrections UX documenté dans le code** (C2 à C10) :
  rare et précieux — montre qu'un vrai retour d'usage a déjà été
  intégré (fond figé du panneau calques, icônes undo/redo, couleur
  d'erreur, menu contextuel canvas, fichiers récents…). C'est la base
  d'un vrai processus de design continu, à formaliser plutôt qu'à
  laisser en commentaires épars.
- **Tooltip qui affiche le raccourci effectif**, pas juste le nom
  (`ui/toolbar.rs:2238-2244`) — bon réflexe pour les utilisateurs avancés
  au clavier/souris (ne résout pas la Critique n°1 pour le tactile, mais
  bon point pour ce canal-là).
- **Réordonnancement de calques en glisser-déposer** a remplacé des
  boutons ▲/▼ à clic unique (commentaire `ui/layers.rs:177-182`) —
  décision UX qui va dans le bon sens (geste direct plutôt qu'indirection
  par bouton), cohérente avec un usage tactile, à l'inverse de la
  Critique n°1.
- **i18n minimaliste mais fonctionnelle** (`t("fr", "en")` inline,
  `i18n.rs`) : pas scalable à 10 langues, mais honnête pour un produit à
  2 langues — pas de sur-ingénierie.

---

### Ce qui manque structurellement (process, pas pixels)

1. **Aucune évaluation heuristique formelle** (type Nielsen 10 heuristics)
   n'a jamais été faite — les corrections C2-C10 sont des observations
   ponctuelles, pas une passe systématique. Une évaluation heuristique
   complète prendrait une demi-journée et trouverait probablement
   d'autres points que les six ci-dessus.
2. **Aucun test utilisateur documenté** avec un vrai utilisateur naïf
   (pas le porteur de projet). Toutes les corrections listées semblent
   venir de l'auto-observation du développeur, ce qui a un angle mort
   connu : on ne voit pas ses propres réflexes acquis comme des obstacles.
3. **Aucune vérification d'accessibilité visuelle** (contraste, daltonisme,
   VoiceOver — déjà noté dans `previous_audit.md` P2.9) au-delà du thème
   clair/sombre.
4. **Pas de définition de cible tactile explicite** (44×44pt Apple HIG) —
   le nom du projet suggère un usage tactile mais rien dans le code ne
   référence de contrainte de taille de cible tactile documentée.

---

### Plan de test recommandé (priorisé)

1. **Capture d'écran réelle** de la barre d'outils au repos + un calque
   sélectionné — tranche immédiatement les Critiques n°4 (bruit
   chromatique) et confirme/infirme n°3, n°5 visuellement. *(Nécessite la
   permission Enregistrement d'écran sur ce Mac.)*
2. **Test tactile réel** (tablette graphique tactile ou écran tactile,
   pas souris) : 10 minutes, utilisateur naïf, tâche « trouve l'outil
   Pinceau, puis verrouille juste sa position sur un calque ». Valide
   Critique n°1 et n°3 en conditions réelles.
3. **Simulation daltonisme** sur l'onion skin et les messages de statut
   (Critique n°2).
4. **Mesure de contraste** (WCAG AA, ratio 4.5:1 texte normal) sur les
   tooltips et le texte du footer dans les deux thèmes clair/sombre —
   pas vérifié par cet audit, à faire.
5. **Évaluation heuristique complète** (Nielsen) sur l'ensemble de
   l'app — celle-ci ne couvre que 6 points remarqués depuis le code, une
   passe visuelle systématique en trouvera d'autres.

---

### Note de méthode

Cet avis vient de la lecture du code de construction de l'UI, pas d'un
usage réel de l'app — les critiques n°1, n°2, n°3, n°5 sont des faits de
construction directement vérifiables dans le code (peu de marge d'erreur).
La critique n°4 et n°6 sont des hypothèses raisonnables mais **non
confirmées visuellement** — à vérifier en priorité avant d'investir du
temps de correction dessus, pour ne pas corriger un problème qui n'existe
peut-être pas à l'écran.

---

### Correctifs appliqués (29 août 2026)

| # | Critique | Statut | Ce qui a été fait |
|---|---|---|---|
| 1 | Icônes seules, découverte au survol uniquement | ✅ Corrigé | Réglage persisté « Afficher les noms des outils » (menu Vue, `ui/toolbar.rs`) : quand activé, chaque bouton d'outil (52×44 au lieu de 34×30) affiche son nom court sous l'icône, sans dépendre du survol. Désactivé par défaut pour ne pas alourdir l'usage souris/clavier existant — l'utilisateur (ou un premier lancement détectant un usage tactile, à décider séparément) doit l'activer. |
| 2 | Rouge/vert seul pour l'onion skin et les statuts | ✅ Corrigé | Onion skin : orange/bleu (paire sûre pour le daltonisme rouge-vert) au lieu de rouge/vert, tooltip et tooltip du menu Animation mis à jour en conséquence. Statuts succès/échec : icône (✓/⚠) ajoutée en plus de la couleur dans le footer, pour un second canal non-chromatique. |
| 3 | Icônes de verrou granulaire invisibles tant qu'inactives | ⚠️ Non corrigé, ré-examiné | Vérifié en relisant le code (`ui/layers.rs:224-229`) : c'est un choix documenté et assumé (« pas une 3e icône permanente pour un cas d'usage plus rare »), pas un oubli — et le réglage reste accessible dans le panneau « Calque actif » toujours visible, pas caché derrière un menu obscur. Le problème de découvrabilité pure reste réel mais moins grave que formulé initialement ; corriger demanderait de renverser une décision produit déjà prise consciemment, pas juste un fix mécanique. Laissé tel quel. |
| 4 | 32 couleurs d'accent visibles en permanence (bruit visuel) | ✅ Corrigé, confirmé réel | Le code confirmait bien le problème (icône peinte dans la couleur d'accent même au repos, pas seulement au survol/sélection — `tool_button`/`shape_family_selector`, `ui/toolbar.rs`). Corrigé : couleur neutre du thème (`ui.visuals().text_color()`) au repos, accent réservé au survol et à la sélection, où il sert de confirmation plutôt que de fond permanent. |
| 5 | Pas de label de catégorie visible dans la barre | ↩️ Constat corrigé | Le hover du chevron de groupe portait déjà le nom de catégorie — l'audit initial avait raté ce site. Rien à corriger, le fait a été rectifié dans la section correspondante ci-dessus. |
| 6 | Sliders potentiellement trop fins pour un doigt | ⏸️ Non corrigé, en attente | Nécessite une mesure de la largeur/hauteur réelle des sliders à l'écran (capture requise, permission non accordée) avant de pouvoir dimensionner un correctif sans deviner — redimensionner à l'aveugle risquerait de casser la mise en page dense de la barre d'options pour un problème peut-être déjà acceptable en pratique. |

**Validation** : `cargo check` et `cargo clippy` propres (0 nouveau
warning), 325/325 tests toujours verts après ces changements.

---

### Vérification visuelle réelle (30 août 2026)

Permission Enregistrement d'écran obtenue depuis la rédaction ci-dessus —
première vraie session de capture d'écran de l'app en fonctionnement
(build release, fenêtre 1500×950pt), pas seulement une lecture de code.

#### Confirmé conforme aux correctifs déjà appliqués
- **Critique n°4 (bruit chromatique)** : confirmé visuellement corrigé —
  au repos, tous les boutons d'outils sont en gris/blanc neutre ; seul
  l'outil actif (testé : Pinceau, Texte, Sélection) ressort en couleur
  pleine (orange, rose/rouge, bleu selon l'outil). Plus de « arc-en-ciel »
  de 32 couleurs simultanées.
- **Panneau de calques, état par défaut** : avec un seul calque sans tag
  couleur ni verrou actif, la ligne reste sobre (poignée, œil, cadenas,
  nom) — la densité redoutée ne se manifeste que dans les états avancés
  (tags + verrous multiples), pas par défaut.
- **Groupes de la barre repliés** : confirmé visuellement — 3 chevrons
  `>` nus (aucun texte visible), exactement comme prévu par la lecture de
  code précédente (le nom de catégorie n'existe qu'au survol).

#### 🔴 Nouveau bug trouvé et corrigé : icônes Gras/Italique invisibles
En basculant sur l'outil Texte, les boutons Gras/Italique de la barre
d'options s'affichaient comme **deux carrés vides** (glyphes manquants,
« tofu »). Cause : le code utilisait les caractères Unicode "𝐆"/"𝐼" du
bloc *Mathematical Alphanumeric Symbols* (`ui/toolbar.rs:2358-2363`),
absents de la police par défaut d'egui — exactement la même famille de
défaut que celle déjà corrigée une fois pour les icônes undo/redo
(constat C9 historique). **Corrigé** : remplacés par les icônes Phosphor
`TEXT_B`/`TEXT_ITALIC` (déjà utilisées ailleurs dans l'app), rendu vérifié
par une seconde capture — les icônes "B"/"I" s'affichent correctement.
`cargo test` : 325/325 toujours verts après le correctif.

#### 🟡 Nouveau constat : les barres d'options tiennent systématiquement sur 2 lignes
Vérifié sur l'outil Texte (Taille/Police/Gras/Italique/Interlignage/
Espacement/Police système/Aligner sur une ligne, puis Couleur/Contour/
Ombre/Sur courbe sur une seconde) et l'outil Sélection (Mode/Couleur/
Ordre/Aligner/Rogner/Supprimer sur une ligne, une seconde ligne partiellement
visible en dessous) : **même à une largeur de fenêtre confortable
(1500pt), aucune des barres d'options testées ne tient sur une seule
ligne**. Sur une fenêtre plus étroite (usage réaliste sur un écran plus
petit ou une tablette), ça grimperait probablement à 3-4 lignes,
mangeant d'autant l'espace de canevas disponible — un vrai point de
friction pour un outil qui se veut tactile/rapide, distinct des critiques
déjà listées plus haut. Pas corrigé (redesign de la densité d'options
par outil, hors scope d'un correctif ponctuel).

#### Note technique : automatisation de fenêtre peu fiable dans cet environnement
`click at {x,y}` via System Events s'est avéré instable dans cette
session — plusieurs tentatives ont déclenché des clics sur d'autres
applications ouvertes sur le bureau (VibeIDE, QuickTime Player) au lieu
de QuickPaint, sans rapport avec les coordonnées visées. Aucune perte de
données constatée (juste des changements de focus/fenêtre), mais **à
éviter pour de futures vérifications visuelles** — se limiter aux
raccourcis clavier (fiables, testés à plusieurs reprises sans incident)
et aux captures d'écran plutôt qu'à des clics synthétiques par coordonnées.


---


<a id="section-audit_100_features"></a>

## audit_100_features — QuickPaint vs. Canva / Photoshop / Illustrator / GIMP (29 août 2026)

Nouvel audit, indépendant des précédents (`previous_audit.md`, CHANGELOG.md) :
100 fonctionnalités représentatives du
superset Canva + Photoshop + Illustrator + GIMP, **choisies avant toute
lecture de code**, puis confrontées au code réel de QuickPaint par lecture
directe (grep + lecture de fichiers, pas d'estimation). Statuts :
**✅ Implémenté** · **🟡 Partiel** · **❌ Absent**.

**Score global : 72 ✅ / 6 🟡 / 22 ❌ sur 100 — mis à jour le 1er septembre 2026 (initialement 62/12/26 le 29 août). Corrigés : BMP #1, soulignement #61, Unsharp Mask #68, Refine Edge #38, Perspective #87, Redressement #88, Plage de couleurs #34, Pointillés #55, Grille des tiers #90, Plein écran #17, Texte→tracés #64, Kit de marque #92. Vérifié infaisable sans réécrire le moteur de police d'egui : Polices variables #65. Reporté à une session dédiée (trop gros pour cette passe) : Macros #96 / Traitement par lots #99.**

---

### 1. Fichiers & Formats — 7 ✅ / 1 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 1 | Import PNG/JPEG/TIFF/BMP/GIF/WebP | ✅ *(corrigé le 30 août 2026)* | `image` compilée avec les features `png,jpeg,webp,tiff,gif,bmp` — les 6 formats sont réellement décodables, [Cargo.toml](Cargo.toml) |
| 2 | Import PSD (calques natifs) | 🟡 | [psd_import.rs:16](src/psd_import.rs:16) reconstruit calques/opacité/visibilité/blend mode, mais aplatit les groupes et ignore masques/texte éditable |
| 3 | Import AI/EPS | ❌ | Introuvable |
| 4 | Import SVG éditable | ✅ | [svg_import.rs](src/svg_import.rs) via `usvg`, chemins/cercles/groupes/texte → `Stroke`/`TextItem` |
| 5 | Import RAW appareil photo | ❌ | Aucune dépendance RAW |
| 6 | Export PDF vectoriel | ✅ | [pdf_vector.rs](src/pdf_vector.rs), opérateurs de tracé réels (pas une image embarquée) |
| 7 | Export multi-formats/tailles en un clic | ✅ | [export.rs:84](src/export.rs:84) `save_batch` + `batch_export_window` |
| 8 | Glisser-déposer de fichiers | ✅ | [app/shortcuts.rs:39](src/app/shortcuts.rs:39) `handle_dropped_files` |
| 9 | Presse-papiers (copier/coller image) | ✅ | [app/io.rs:80](src/app/io.rs:80) via `arboard` |
| 10 | Sauvegarde auto + récupération crash | ✅ | [project.rs:110-147](src/project.rs:110) |

### 2. Document & Canevas — 6 ✅ / 0 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 11 | Nouveau document, tailles prédéfinies | ✅ | [ui/toolbar.rs:19-49](src/ui/toolbar.rs:19) `templates()` |
| 12 | Plans de travail multiples (artboards) | ❌ | Le "plan de travail" trouvé n'est qu'un pasteboard visuel autour d'un canevas unique, pas des zones indépendantes multiples |
| 13 | Redimensionnement de canevas non destructif | ✅ | [app/mod.rs:2822](src/app/mod.rs:2822) `resize_canvas` avec ancrage, annulable |
| 14 | Règles et guides | ✅ | Guides manuels persistés, [model/document.rs:451](src/model/document.rs:451) |
| 15 | Grille + magnétisme | ✅ | `snap()`, [tools/guides.rs:24](src/tools/guides.rs:24) |
| 16 | Zoom fluide + pan | ✅ | [app/mod.rs:2719](src/app/mod.rs:2719) |
| 17 | Mode plein écran / sans distraction | ✅ *(corrigé le 1er septembre 2026)* | `PaintApp::toggle_distraction_free` — plein écran natif + masquage des 3 panneaux, Échap pour sortir |
| 18 | Rotation du canevas (affichage) | ✅ | [app/mod.rs:606](src/app/mod.rs:606) `view_angle`, désactive règles/pot de peinture hors 0° |

### 3. Calques — 10 ✅ / 1 🟡 / 1 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 19 | Calques multiples empilables | ✅ | `Document::layers: Vec<Layer>` |
| 20 | Groupes de calques | 🟡 | Tag de regroupement visuel (`Layer::group`), pas une hiérarchie de dossiers avec masque/opacité de groupe propre |
| 21 | Modes de fusion | ✅ | **12 modes** ([model/document.rs:13-43](src/model/document.rs:13)) |
| 22 | Masque de calque | ✅ | `Layer::mask: Option<RasterLayer>` |
| 23 | Masque d'écrêtage | ✅ | `Layer::clip`, [render/compositor.rs:191-265](src/render/compositor.rs:191) |
| 24 | Calques de réglage non destructifs | ✅ | `Layer::adjustment: Option<Adjustment>` |
| 25 | Objets dynamiques (Smart Objects) | ❌ | Introuvable |
| 26 | Styles de calque (ombre, contour, lueur) | ✅ | `enum LayerStyle { DropShadow, Stroke, Glow }` |
| 27 | Verrouillage granulaire | ✅ | `locked`/`lock_position`/`lock_alpha` indépendants |
| 28 | Recherche/filtre de calques | ✅ | [ui/layers.rs:124-167](src/ui/layers.rs:124), au-delà de 8 calques |
| 29 | Alignement/répartition de calques | ✅ | [app/layers_ops.rs:20,116](src/app/layers_ops.rs:20) |
| 30 | Fusion/aplatissement de calques | ✅ | `flatten()`/`merge_down()`, annulables |

### 4. Sélection — 6 ✅ / 1 🟡 / 1 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 31 | Sélection rectangle/ellipse/lasso | ✅ | `SelectMode::{Rect,Ellipse,Lasso}` |
| 32 | Lasso polygonal | ✅ | `SelectMode::PolyLasso` |
| 33 | Baguette magique | ✅ | [app/selection.rs:167](src/app/selection.rs:167), tolérance réglable |
| 34 | Sélection par plage de couleurs (image entière) | ✅ *(corrigé le 1er septembre 2026)* | Entrée « Plage de couleurs… » dans le menu Édition (active baguette + portée Global explicitement) |
| 35 | Opérations d'ensemble (add/subtract/intersect) | ✅ | `enum SelectionCombine` |
| 36 | Masque de sélection en pixels | ✅ | `feather`/`dilate`/`erode`, [tools/selection_mask.rs:116-154](src/tools/selection_mask.rs:116) |
| 37 | Détourage automatique par IA | ❌ | Le détourage existant est un flood-fill classique, pas un modèle IA |
| 38 | Amélioration des bords (Refine Edge) | ✅ *(corrigé le 1er septembre 2026)* | `bucket::refine_edges` généralisé via `PaintApp::refine_selection_edges`, menu Édition ▸ Masque de sélection ▸ Améliorer les bords… |

### 5. Peinture & Dessin — 9 ✅ / 1 🟡 / 0 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 39 | Pinceau, pression simulée/réelle | ✅ | `PressureModel`, [input/pressure.rs](src/input/pressure.rs) |
| 40 | Crayon | ✅ | `ActiveTool::Pencil` |
| 41 | Gomme (vectorielle et pixel) | ✅ | `ActiveTool::Eraser`/`PixelEraser` |
| 42 | Aérographe | ✅ | `ActiveTool::Airbrush` |
| 43 | Pot de peinture | ✅ | [app/bucket_cutout.rs:13-40](src/app/bucket_cutout.rs:13) |
| 44 | Tampon de clonage | ✅ | `RasterOp::Clone` |
| 45 | Correcteur / Healing brush | 🟡 | Plus-proche-voisin + diffusion laplacienne, documenté "pas un vrai PatchMatch" |
| 46 | Import d'image comme brosse | ✅ | [app/io.rs](src/app/io.rs) `import_brush_stamp` |
| 47 | Symétrie / miroir temps réel | ✅ | `SymmetryMode::{Radial,MirrorH,MirrorV,MirrorBoth}` |
| 48 | Stabilisation du trait | ✅ | EMA, [input/smoothing.rs](src/input/smoothing.rs) |

### 6. Vectoriel & Plume — 6 ✅ / 0 🟡 / 4 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 49 | Outil Plume (Bézier éditable) | ✅ | [tools/pen.rs](src/tools/pen.rs) |
| 50 | Formes géométriques | ✅ | `enum Shape {Rectangle,Ellipse,Polygon,Star}` |
| 51 | Édition de nœuds après coup | ✅ | [app/pen_edit.rs:126](src/app/pen_edit.rs:126) |
| 52 | Opérations booléennes de formes | ✅ | Clipper, [tools/boolean.rs:15-52](src/tools/boolean.rs:15) |
| 53 | Dégradés sur formes | ✅ | `GradientKind::{Linear,Radial,Conic}` |
| 54 | Motifs de remplissage (patterns) | ❌ | Introuvable |
| 55 | Trait personnalisable (pointillés) | ✅ *(corrigé le 1er septembre 2026)* | `Stroke::dash`, géré au niveau du ruban (`render::ribbon::build_dashed`) — rendu identique écran/export. Case « Pointillés » pour les outils Forme. Épaisseur variable le long du tracé : toujours absent (portée non couverte) |
| 56 | Gradient mesh | ❌ | Introuvable |
| 57 | Déformation de formes (warp) | ✅ | `Adjustment::{ArcWarp,Wave,Sphere,Vortex}` |
| 58 | Cisaillement (skew) | ✅ | `Command::Shear`, [app/transform.rs:20-155](src/app/transform.rs:20) |

### 7. Texte & Typographie — 5 ✅ / 0 🟡 / 3 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 59 | Outil Texte, polices système | ✅ | [fonts.rs:24-113](src/fonts.rs:24) via `fontdb` |
| 60 | Texte sur courbe | ✅ | `TextArc` + `arc_chars` |
| 61 | Gras/italique/soulignement | ✅ *(corrigé le 30 août 2026)* | Gras (faux, décalage), italique (vraie fonte) et **soulignement** tous les trois présents |
| 62 | Interlignage et crénage | ✅ | `line_height`/`letter_spacing` |
| 63 | Contour de texte | ✅ | `outline_w`/`outline_color`, 8 passes |
| 64 | Texte → tracés vectoriels | ✅ *(corrigé le 1er septembre 2026)* | `tools/text_outline.rs` via `ttf-parser` (nouvelle dépendance directe) — contour non rempli par défaut, limite documentée sur les lettres à trou (« O ») si remplies manuellement |
| 65 | Polices variables / Google Fonts | ❌ *(vérifié infaisable le 1er septembre 2026)* | Le rendu de texte d'egui repose sur `ab_glyph`, qui ne supporte pas l'interpolation d'axes fvar/gvar — un slider de graisse continue n'affecterait pas le texte réellement rendu sans réécrire tout le pipeline de police |
| 66 | Vérification orthographique | ❌ | Introuvable |

### 8. Filtres & Effets — 8 ✅ / 1 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 67 | Flou (gaussien/mouvement/radial/bokeh) | ✅ | **5 types distincts** dans [tools/filter.rs](src/tools/filter.rs) |
| 68 | Netteté (Unsharp Mask) | ✅ *(corrigé le 30 août 2026)* | `Adjustment::UnsharpMask { radius, amount, threshold }` — vrai Unsharp Mask (flou gaussien de référence + différence amplifiée + seuil), en plus du `Filter::Sharpen` à noyau fixe existant |
| 69 | Détection de contours (Sobel/Canny) | ✅ | `sobel_magnitude`/`canny_edges` |
| 70 | Pixelisation / Halftone | ✅ | `Adjustment::Pixelate`/`Halftone` |
| 71 | Distorsions | ✅ | `Wave`/`Sphere`/`Vortex`/`ArcWarp`/`Distortion` |
| 72 | Effets créatifs | ✅ | `ChromaticAberration`, `FilmGrain`, `Vintage`, `Sketch`, `Comic`, `OilPainting`, `Watercolor` |
| 73 | Réduction de bruit | ✅ | `Adjustment::Denoise` |
| 74 | Content-Aware Fill | 🟡 | [tools/inpaint.rs](src/tools/inpaint.rs), explicitement "pas un vrai PatchMatch" |
| 75 | Extension IA du cadre (Generative Expand) | ❌ | Introuvable |
| 76 | Suppression d'arrière-plan par IA | ❌ | Seule la sélection par seuil de couleur existe, pas un réseau de neurones |

### 9. Couleur & Réglages — 7 ✅ / 0 🟡 / 1 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 77 | Niveaux / Courbes par canal | ✅ | `CurvesFree{master,r,g,b}`, spline monotone |
| 78 | Teinte/Saturation/Luminosité | ✅ | `HueSaturation` |
| 79 | Balance des blancs | ✅ | `WhiteBalance{temp,tint}` |
| 80 | Vibrance | ✅ | `vibrance()`, épargne les couleurs déjà saturées |
| 81 | Duotone / Mixeur N&B | ✅ | `Duotone`/`ChannelMixerBw` |
| 82 | Extraction de palette dominante | ✅ | [tools/palette.rs:20](src/tools/palette.rs:20) |
| 83 | Correction automatique | ✅ | `Filter::AutoLevels` |
| 84 | Gestion de la couleur (ICC/CMJN) | ❌ | Introuvable |

### 10. Transformation — 4 ✅ / 0 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 85 | Déplacer/redimensionner/pivoter une sélection | ✅ | [app/transform.rs:34-120](src/app/transform.rs:34) |
| 86 | Retourner horizontal/vertical | ✅ | `flip_document()`, annulable |
| 87 | Perspective / distorsion libre | ✅ *(corrigé le 1er septembre 2026)* | Les 4 coins se glissent directement sur le canevas (poignées orange, aperçu du quadrilatère en direct), sliders retirés — `PaintApp::perspective_handles`/`start_perspective_drag_if_handle`/`update_perspective_drag` |
| 88 | Redressement d'horizon | ✅ *(corrigé le 1er septembre 2026)* | Bouton « 📐 Tracer l'horizon » : glisser une ligne cyan sur le canevas calcule l'angle de redressement (`commit_straighten_line`), en plus du curseur d'angle conservé |
| 89 | Alignement automatique / panorama | ❌ | Introuvable |
| 90 | Recadrage avec grille des tiers | ✅ *(corrigé le 1er septembre 2026)* | Grille superposée sur le rectangle de recadrage non tourné, `canvas_overlay.rs::paint_crop` |

### 11. Collaboration, Cloud & Templates — 0 ✅ / 1 🟡 / 4 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 91 | Bibliothèque de modèles par catégorie | 🟡 | Galerie de formats/tailles vierges, pas de designs pré-remplis (texte/formes/mise en page) |
| 92 | Kit de marque | ✅ *(corrigé le 1er septembre 2026)* | `model::BrandKit` (couleurs + polices + logo PNG base64), extension du mécanisme de presets existant (`StylePreset`/`BrushPreset`) |
| 93 | Édition collaborative temps réel | ❌ | Confirmé absent par design (README) |
| 94 | Historique cloud + partage par lien | ❌ | Undo/redo local uniquement, confirmé absent par design |
| 95 | Redimensionnement magique vers un autre format | ❌ | Redimensionne le canevas, ne recompose pas la mise en page |

### 12. Automatisation & Extensibilité — 0 ✅ / 0 🟡 / 5 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 96 | Macros (enregistrement/rejeu) | ❌ | Introuvable |
| 97 | Scripting intégré | ❌ | Introuvable |
| 98 | Plugins tiers | ❌ | Introuvable |
| 99 | Traitement par lots sur un dossier | ❌ | Le "batch export" traite un seul document en plusieurs formats, pas un dossier de fichiers |
| 100 | API/intégrations externes | ❌ | Aucune dépendance réseau, confirmé absent par design |

---

### Analyse

**QuickPaint couvre solidement le cœur "éditeur d'image classique"** (calques,
sélection, peinture, filtres, couleur, vectoriel de base) — les catégories
2 à 9 totalisent 56 ✅ / 9 🟡 / 15 ❌, un score honorable face à des
logiciels professionnels matures. Le pinceau/plume/calques/filtres n'ont
quasiment rien à envier à un GIMP ou un Photoshop de milieu de gamme sur
ces fondamentaux.

**Trois zones concentrent presque tous les ❌, et ce n'est pas un hasard :**

1. **IA générative** (items 37, 75, 76 — détourage/extension de cadre/
   suppression d'arrière-plan par réseau de neurones) : absent partout,
   cohérent avec le choix documenté du projet (README) de préférer des
   heuristiques locales à un modèle ML embarqué. Le "Content-Aware Fill"
   (74) et le "Healing brush" (45) existent mais sont explicitement
   documentés comme *ne synthétisant pas de texture* — l'écart est assumé,
   pas un oubli.
2. **Cloud/collaboration/marque** (catégorie 11, 0 ✅/1 🟡/4 ❌) : c'est
   tout l'ADN "Canva" qui manque — logique, QuickPaint est un outil 100 %
   local par conception (non-goal explicite du README : pas de cloud, pas
   de compte, pas de télémétrie). Comparer QuickPaint à Canva sur cette
   catégorie revient à comparer un couteau à un abonnement SaaS ; le score
   nul ici ne remet rien en cause.
3. **Automatisation/extensibilité** (catégorie 12, 0 ✅/0 🟡/5 ❌) : zéro
   partout — pas de macros, pas de scripting, pas de plugins, pas de
   traitement par lots multi-fichiers. Contrairement aux deux points
   précédents, **celui-ci n'est pas documenté comme un choix assumé** dans
   le README ou les audits précédents — c'est un vrai angle mort, pas une
   limite revendiquée. Si des utilisateurs avancés ou un usage
   professionnel répétitif (traiter 200 photos identiquement) sont visés,
   c'est le manque le plus significatif de tout cet audit.

**Vectoriel avancé (catégorie 6)** : bonnes fondations (plume, formes,
booléennes, dégradés, warp) mais 4 absences typiquement "Illustrator"
(motifs de remplissage, trait à épaisseur variable, gradient mesh, export
en tracés du texte) — cohérent avec un outil qui se positionne dessin/
retouche plutôt que PAO vectorielle avancée.

**Verdict** : le score de 62/12/26 ne doit pas se lire comme "38 % de
retard" — il reflète un choix de positionnement clair (outil de dessin/
retouche local, tactile, sans IA lourde ni cloud) plutôt qu'un projet
inachevé sur son cœur de cible. Le seul manque qui mériterait une vraie
réflexion produit est l'automatisation (catégorie 12), qui n'est adossée
à aucune décision explicite contrairement au cloud et à l'IA générative.

---

### Analyse produit — validité, intégration, finalisation, propriété intellectuelle (30 août 2026)

Passe supplémentaire sur les 38 items non ✅ (26 ❌ + 12 🟡) : lequel est
**non pertinent** pour QuickPaint (contredit une décision déjà prise ou le
positionnement du produit), lequel est **à finaliser** (déjà commencé,
gain à faible effort), lequel serait **pertinent à intégrer** neuf, et
lequel **touche à de la propriété intellectuelle** (brevet, licence,
marque) et mérite une prudence particulière avant tout développement.

#### 🚫 Non pertinent — contredit une décision déjà prise ou le non-goal du produit

| # | Fonctionnalité | Pourquoi |
|---|---|---|
| 37 | Détourage automatique par IA | Non-goal déjà écrit dans le README : modèle ML embarqué explicitement refusé (heuristiques préférées) |
| 75 | Extension IA du cadre (Generative Expand) | Même non-goal ML que #37 |
| 76 | Suppression d'arrière-plan par IA | Même non-goal ML que #37 |
| 93 | Édition collaborative temps réel | Non-goal explicite README : « no real-time collaboration » |
| 94 | Historique cloud + partage par lien | Non-goal explicite README : « 100% local, no cloud » |
| 100 | API/intégrations externes (réseaux sociaux, cloud) | Non-goal explicite README : « no third-party API calls » |
| 5 | Import RAW appareil photo | Déjà tranché (README) : seules libs Rust dispo sont AGPL/LGPL, incompatibles avec la distribution actuelle — pas un manque technique, un refus de licence |
| 3 | Import AI/EPS | Format propriétaire Adobe fermé, aucune lib Rust mature — même famille de refus que PSD export (déjà documenté) |
| 65 (partie réseau) | « Google Fonts intégrées » au sens *téléchargement à la demande* | Contredit le non-goal « 100 % offline » — un appel réseau pour récupérer une police romprait le fonctionnement hors-ligne garanti par le produit |

#### ⚠️ Touche à la propriété intellectuelle — prudence avant tout développement

| # | Fonctionnalité | Le risque concret |
|---|---|---|
| 45, 74 | Healing brush / Content-Aware Fill « avec une vraie synthèse de texture » | **PatchMatch** (Barnes/Goldman et al., SIGGRAPH 2009), l'algorithme derrière le Content-Aware Fill d'Adobe, est couvert par des brevets américains d'Adobe. C'est précisément pour ça que des outils libres (GIMP/Resynthesizer, etc.) évitent l'algorithme littéral et utilisent des variantes (diffusion, exemplar-based non brevetées). **L'implémentation actuelle de QuickPaint (plus-proche-voisin + diffusion laplacienne) évite déjà ce risque** — c'est un bon signe, mais toute amélioration future vers « une vraie synthèse de texture » doit explicitement éviter de réimplémenter PatchMatch tel quel, pas juste viser un meilleur résultat visuel. |
| 91 | Bibliothèque de modèles pré-remplis (pas juste des tailles vierges) | **Précédent réel dans ce dépôt** : le commit `fbe21fe` (« Retire le gabarit Flyer A6 Barbato et son logo embarqué ») montre qu'un gabarit avec un logo tiers a déjà été livré par erreur puis retiré. Toute extension de la galerie vers de vrais designs pré-remplis doit garantir un contenu 100 % original (pas de logo, pas de photo stock, pas de police non redistribuable), avec une revue explicite avant merge — ce n'est pas hypothétique, c'est déjà arrivé une fois. |
| 95 | Redimensionnement magique (recompose automatique vers un autre ratio) | Pas de brevet identifié avec certitude, mais l'algorithme précis de Canva (Magic Resize) est un produit commercial différenciant — le reproduire à l'identique (même heuristique de repositionnement) serait plus risqué qu'une recomposition « maison » avec une logique différente. Je n'ai pas de certitude juridique ici (pas un juriste) — à vérifier avant d'investir dessus si le sujet devient sérieux. |
| 66 | Vérification orthographique | Pas de risque de brevet, mais un risque de licence si on embarque un dictionnaire/moteur type Hunspell (LGPL) — le contournement le plus sûr est l'API native macOS `NSSpellChecker` (déjà le réflexe du projet pour le menu ⌘ natif via `muda`/`objc`), pas une lib tierce embarquée. |

#### 🔧 À finaliser — déjà commencé, gain à effort raisonnable

| # | Fonctionnalité | Ce qu'il reste à faire |
|---|---|---|
| 1 | ✅ Import BMP — corrigé (30 août 2026) | Feature `bmp` activée dans `Cargo.toml`, test de régression `opens_a_bmp_image` (`project.rs`). |
| 61 | ✅ Soulignement de texte — corrigé (30 août 2026) | `TextItem::underline` + rendu (egui `Stroke` sur le remplissage central en live, bandeau blitté manuellement dans le compositeur CPU — tracé une seule fois, pas par passe), bouton dans la barre (icône Phosphor `TEXT_A_UNDERLINE`), presse-papiers de style mis à jour. **Bonus trouvé en implémentant** : le hash d'invalidation du cache de composition (`layer_hash`) oubliait déjà italique/interligne/espacement/police système/ombre depuis le Sprint Q — corrigé au passage (même catégorie de bug, même fonction). 3 tests ajoutés (2 unitaires + 1 bout-en-bout prouvant le blit réel). |
| 87, 88 | Perspective / redressement d'horizon par manipulation directe | Le calcul (homographie 4 points, redressement) existe déjà, seule l'UI est indirecte (sliders au lieu de poignées glissables sur l'image) — amélioration UI pure, pas un nouvel algorithme |
| 34 | Sélection par plage de couleurs | Le mode « Global » de la baguette magique fait déjà l'essentiel — l'exposer/le renommer explicitement comme fonctionnalité dédiée serait presque gratuit |
| 38 | ✅ Amélioration des bords de sélection — corrigé (1er septembre 2026) | Nouvelle méthode `PaintApp::refine_selection_edges` (rend le document composé, applique `bucket::refine_edges` au masque de sélection), intégrée au dialogue partagé feather/dilater/contracter (4e action `SelectionMaskAction::RefineEdges`) avec une portée de rayon dédiée (fenêtre de texture, pas un nombre de pixels). 2 tests (durcit plus le bord côté texturé que côté plat ; no-op sans sélection par région). |
| 68 | ✅ Netteté réglable (Unsharp Mask) — corrigé (30 août 2026) | Nouvel `Adjustment::UnsharpMask` en complément de `Filter::Sharpen` (conservé, noyau fixe rapide toujours utile). 4 tests (identité à quantité nulle, no-op sur zone plate, seuil qui épargne les faibles écarts, overshoot caractéristique sur un bord net). |
| 2 | Import PSD (groupes, masques, texte éditable) | L'essentiel (calques/opacité/blend mode) fonctionne déjà ; étendre à ce qui manque est un prolongement de code existant, pas un nouveau module |

#### ✅ Pertinent à intégrer — nouveau, cohérent avec l'identité du produit, sans souci de PI

| # | Fonctionnalité | Pourquoi ça correspond |
|---|---|---|
| 17 | Mode plein écran / sans distraction | Gagne de l'espace canevas — particulièrement utile en usage tactile, cohérent avec le nom du projet ; effort faible |
| 90 | Grille des tiers pendant le recadrage | Overlay visuel pur, aucune nouvelle logique métier, cohérent avec les règles/guides déjà présents |
| 54 | Motifs de remplissage (patterns) | Extension naturelle du système de remplissage déjà là (uni/dégradé → + motif), aucun souci de PI |
| 55 (partie pointillés) | Trait en pointillés | `tiny-skia` supporte déjà le « dash » nativement — probablement une exposition UI d'une capacité déjà là dans le moteur de rendu |
| 64 | Texte → tracés vectoriels | Le pipeline vectoriel (export SVG/PDF) existe déjà ; extraire les contours de glyphes via `ttf-parser`/`rustybuzz` (déjà des dépendances) est cohérent avec l'architecture actuelle |
| 65 (partie polices variables) | Support des axes de police variable (poids/graisse continue) pour les polices système déjà installées | Aucun appel réseau, aucune police à embarquer — juste exploiter une capacité de `fontdb`/`rustybuzz` si elle existe déjà pour ces polices |
| 96, 99 | Macros + traitement par lots sur un dossier | Déjà identifié dans l'audit précédent comme le vrai angle mort non assumé (contrairement au cloud/IA) — une infrastructure de macros ouvrirait aussi le traitement par lots multi-fichiers presque gratuitement une fois construite |
| 92 | Kit de marque (couleurs/polices réutilisables) | Le projet a déjà des presets réutilisables (styles, brosses, export) persistés dans `settings.json` — un « kit » serait une extension du même mécanisme existant, pas un nouveau système |

#### ❓ À trancher — j'ai besoin de ton avis avant d'aller plus loin

Ces items changeraient le positionnement du produit ou touchent à une
décision déjà en suspens (distribution App Store) — je ne les classe pas
sans ton arbitrage :

- **Plans de travail multiples (artboards, #12)** et **Objets dynamiques /
  Smart Objects (#25)** : features "outil de PAO professionnel", pas
  "dessin/retouche rapide" — voir [explication_artboards_smart_objects.md](explication_artboards_smart_objects.md)
  pour ce que ça impliquerait concrètement. Direction pas encore tranchée.
- **Gestion de la couleur ICC/CMJN (#84)** : pertinent seulement si un
  usage impression professionnelle est visé (au-delà du PDF déjà supporté).
- **Alignement automatique de calques / panorama (#89)** : gros effort de
  vision par ordinateur pour un bénéfice de niche (assemblage photo) —
  aligné avec la cible du produit ?
- ✅ **Tranché (30 août 2026)** : scripting intégré (#97) et plugins tiers
  (#98) — l'App Store n'est plus visé (distribution Developer ID/DMG
  uniquement, voir `previous_audit.md` §2), donc plus de contrainte de sandbox
  App Store bloquant cette voie. Reste un chantier volontaire à scoper si
  jugé prioritaire, pas un point technique en suspens.


---
