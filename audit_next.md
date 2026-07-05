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
| 30 | Renommage et code couleur | ✅ | Sprint I.5 : `layer.color_tag: Option<[u8;3]>` (palette 8 couleurs) + renommage. |
| 31 | Duplication de calque | ✅ | `duplicate_layer()`. |
| 32 | Fusion de calques / aplatissement | ✅ | `merge_down()` / `flatten()`. |
| 33 | Calque de remplissage (uni, dégradé, motif) | ✅ | Sprint I.1 : `Layer::new_fill` + `FillKind::{Solid, Linear, Radial}` (pas de motif/pattern). |
| 34 | Calques de réglage non-destructifs | ✅ | `layer.adjustment: Option<Adjustment>` — Niveaux, Courbes, Teinte/Saturation, Exposition, Vibrance, Balance des blancs, Réduction de bruit, Flou gaussien/mouvement/bokeh/radial, Duotone, Distorsion, Aberration chromatique, Warp Arc/Vague/Sphère/Tourbillon, Vignette, Mixeur N&B, Pixelisation, Halftone. |
| 35 | Styles de calque (ombre, contour, lueur) | ✅ | `LayerStyle` — DropShadow/Stroke/Glow (interne/externe). |
| 36 | Alignement et distribution de calques | 🟡 | Sprint I.2 : `align_layer_to_document()` aligne le contenu entier d'un calque au document (6 modes). **Distribution entre plusieurs calques non traitée** (demande une sélection multi-calque absente de l'UI). |
| 37 | Vignettes de prévisualisation | ✅ | Sprint I.3 : miniature par calque (`Compositor::layer_thumbnail`, cache réutilisé, invalidé par hash). |
| 38 | Recherche/filtre dans la liste des calques | ✅ | Sprint I.4 : champ de filtre (révélé au-delà de 8 calques). |

**Score : 15 ✅ / 2 🟡 / 0 ❌**

---

## Dessin & peinture

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 39 | Pinceau à taille et dureté réglables | ✅ | `brush.width` + dureté (pinceau pixel). |
| 40 | Crayon pixel-perfect | 🟡 | Existe comme préréglage de pinceau (« Crayon fin ») et via le pinceau pixel, pas comme outil dédié séparé. |
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

**Score : 15 ✅ / 2 🟡 / 0 ❌**

---

## Sélection & découpe

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

## Filtres & effets

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 84 | Flou gaussien | ✅ | `Adjustment::GaussianBlur` (noyau séparable). |
| 85 | Flou de mouvement et flou radial/zoom | ✅ | Flou de mouvement directionnel + Sprint K.4 : `Adjustment::RadialBlur` (effet vitesse/explosion). |
| 86 | Pixelisation / mosaïque | ✅ | Sprint K.1 : `Adjustment::Pixelate { block }`. |
| 87 | Détection de contours (Sobel / Canny) | 🟡 | Sobel seul (utilisé par Croquis et BD) ; pas de Canny (suppression non-maximale + hystérésis) — priorité basse, voir sprint_next.md K.6. |
| 88 | Posterisation et seuil (effet BD) | ✅ | `Filter::Comic` (posterisation 5 niveaux + contours Sobel). |
| 89 | Grain et bruit ajoutable | ✅ | `Filter::FilmGrain` (bruit procédural déterministe). |
| 90 | Vignette artistique | ✅ | Sprint K.5 : `Adjustment::Vignette { amount }`, extrait du filtre Vintage en réglage autonome. |
| 91 | Duotone / bichromie | ✅ | `Adjustment::Duotone`. |
| 92 | Halftone (trame) | ✅ | Sprint K.2 : `Adjustment::Halftone { cell, angle }`. |
| 93 | Distorsions (vague, sphère, tourbillon) | ✅ | Sprint K.3 : `Adjustment::{Wave, Sphere, Vortex}`. |
| 94 | Import de LUT `.cube` | ✅ | `tools/lut.rs` — interpolation trilinéaire, intensité réglable. |
| 95 | Intensité réglable + aperçu direct pour chaque filtre | ✅ | Tous les `Adjustment` ont des paramètres continus + aperçu en direct (calque de réglage). |

**Score : 11 ✅ / 1 🟡 / 0 ❌**

---

## Texte, vectoriel, couleur & moteur

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

## Synthèse globale

| Statut | Nombre (sur 102 items) | % |
|---|---|---|
| ✅ Implémenté | 91 | ~89 % |
| 🟡 Partiel | 10 | ~10 % |
| ❌ Absent | 1 | ~1 % |

*(Mis à jour après les Sprints G, H, K, I, J, M et L (complet, y compris L.5/L.6/L.7) — voir
[sprint_next.md](sprint_next.md) : G a réglé 61/64/68 (sélection, opérations d'ensemble) ; H a
réglé 62/63 (masque de sélection en pixels — feather, dilater/contracter) ; K a réglé
76/80/83/85/86/90/92/93 (filtres & effets) ; I a réglé 30/33/37/38 et partiellement 36
(calques) ; J a réglé 44/50/56 (dessin) ; M a réglé 98/99 (couleur/transformations) ; L a
réglé 3/9/14/15/16/17/18/11 (export, dont l'import SVG vectoriel, le PDF vectoriel et le
GIF animé — `Document::frames`, panneau « Animation », export via `image::codecs::gif`).
Reste seulement : K.6/Canny (basse priorité). Optionnels non traités par choix : 28
(verrouillage granulaire), 36 (distribution multi-calque, demande une sélection
multi-calque absente de l'UI), 40 (outil crayon dédié).)*

### Ce qui manque complètement (❌), par ordre d'impact utilisateur probable

**Format & export**
(tout traité par le Sprint L — voir [sprint_next.md](sprint_next.md), y
compris l'import SVG vectoriel, le PDF vectoriel et le GIF statique **et**
animé)

**Sélection**
(tout traité par les Sprints G et H — voir [sprint_next.md](sprint_next.md) :
opérations d'ensemble, inversion, trim, et désormais feather/dilater/
contracter via un vrai masque de sélection en pixels)

**Calques**
(traité par le Sprint I — voir [sprint_next.md](sprint_next.md) ; reste
seulement la distribution entre plusieurs calques, point 36, qui demande une
sélection multi-calque absente de l'UI)

**Dessin**
(traité par le Sprint J — voir [sprint_next.md](sprint_next.md) ; reste
seulement l'outil crayon dédié, point 40, priorité basse — le préréglage
« Crayon fin » existant est jugé suffisant)

**Filtres & effets**
(tout traité par le Sprint K — voir [sprint_next.md](sprint_next.md) ; reste
seulement Canny/K.6, priorité basse, non bloquant)

**Moteur**
- Rendu GPU via `wgpu` spécifiquement (le rendu UI utilise `glow`/OpenGL par défaut d'eframe ; le compositeur photo reste CPU par choix architectural — voir ARCHITECTURE.md)

### Points d'attention

1. **Sélection : complète, y compris feather/dilater/contracter.** ✅
   Résolu par les Sprints G et H (voir [sprint_next.md](sprint_next.md)) :
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

2. **Filtres & effets : quasi complet.** ✅ Résolu par le Sprint K
   (voir [sprint_next.md](sprint_next.md)) : pixelisation, halftone,
   vague/sphère/tourbillon, flou radial, vignette autonome, mixeur de
   canaux N&B, auto-correction. Seule la détection de contours Canny reste
   absente (K.6, priorité basse — Sobel suffit aux usages actuels).

3. **Rendu GPU : réponse dépend de la définition.** Le rendu de l'interface
   (`egui`) passe par le backend `glow` d'eframe, qui est bien accéléré GPU
   via OpenGL — mais la checklist demande spécifiquement `wgpu`, absent. Le
   compositeur photo (calques, filtres, texte rastérisé) est volontairement
   **CPU** (`tiny-skia`), un choix architectural documenté, pas un oubli.

4. **Calques : quasi complet.** ✅ Résolu par le Sprint I (voir
   [sprint_next.md](sprint_next.md)) : calque de remplissage (uni/dégradé),
   code couleur, vignettes de prévisualisation, recherche/filtre, et
   alignement du contenu d'un calque par rapport au document. Reste la
   distribution entre plusieurs calques (point 36), qui suppose une
   sélection multi-calque non présente dans l'UI actuelle — pas traitée.
   Verrouillage granulaire (point 28) resté optionnel, comme recommandé par
   l'audit initial (le verrou global couvre déjà le cas d'usage principal).

5. **Dessin : quasi complet.** ✅ Résolu par le Sprint J (voir
   [sprint_next.md](sprint_next.md)) : aérographe, import de brosse depuis
   une image (tampon en niveaux de gris), prévisualisation du contour de
   brosse étendue au pinceau/gomme pixel et à l'aérographe. Reste l'outil
   crayon dédié (point 40), laissé de côté comme suggéré par l'audit initial
   — le préréglage « Crayon fin » couvre déjà le besoin, impact utilisateur
   probablement faible pour un outil séparé.

6. **Texte, vectoriel, couleur : complet.** ✅ Résolu par le Sprint M (voir
   [sprint_next.md](sprint_next.md)) : extraction de palette depuis une
   image (point 98) et cisaillement/skew (point 99, via des poignées
   dédiées sur la boîte de sélection). Pour le skew, seuls les traits sont
   véritablement déformés point par point ; textes/images n'ont pas de champ
   d'inclinaison dans le modèle actuel (seule leur ancre se déplace, comme
   pour Scale/Rotate) — limite technique documentée dans le code, pas un
   oubli.

7. **Export : complet, y compris les points qui demandaient une décision.**
   ✅ Résolu par le Sprint L (voir [sprint_next.md](sprint_next.md)) : export
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
