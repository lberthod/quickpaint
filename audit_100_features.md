# audit_100_features.md — QuickPaint vs. Canva / Photoshop / Illustrator / GIMP (29 août 2026)

Nouvel audit, indépendant des précédents (`audit_next.md`,
`sprint_fonctionnalites.md`) : 100 fonctionnalités représentatives du
superset Canva + Photoshop + Illustrator + GIMP, **choisies avant toute
lecture de code**, puis confrontées au code réel de QuickPaint par lecture
directe (grep + lecture de fichiers, pas d'estimation). Statuts :
**✅ Implémenté** · **🟡 Partiel** · **❌ Absent**.

**Score global : 65 ✅ / 9 🟡 / 26 ❌ sur 100 — mis à jour le 30 août 2026 (BMP #1, soulignement #61, Unsharp Mask #68 corrigés ; initialement 62/12/26).**

---

## 1. Fichiers & Formats — 7 ✅ / 1 🟡 / 2 ❌

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

## 2. Document & Canevas — 6 ✅ / 0 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 11 | Nouveau document, tailles prédéfinies | ✅ | [ui/toolbar.rs:19-49](src/ui/toolbar.rs:19) `templates()` |
| 12 | Plans de travail multiples (artboards) | ❌ | Le "plan de travail" trouvé n'est qu'un pasteboard visuel autour d'un canevas unique, pas des zones indépendantes multiples |
| 13 | Redimensionnement de canevas non destructif | ✅ | [app/mod.rs:2822](src/app/mod.rs:2822) `resize_canvas` avec ancrage, annulable |
| 14 | Règles et guides | ✅ | Guides manuels persistés, [model/document.rs:451](src/model/document.rs:451) |
| 15 | Grille + magnétisme | ✅ | `snap()`, [tools/guides.rs:24](src/tools/guides.rs:24) |
| 16 | Zoom fluide + pan | ✅ | [app/mod.rs:2719](src/app/mod.rs:2719) |
| 17 | Mode plein écran / sans distraction | ❌ | Introuvable |
| 18 | Rotation du canevas (affichage) | ✅ | [app/mod.rs:606](src/app/mod.rs:606) `view_angle`, désactive règles/pot de peinture hors 0° |

## 3. Calques — 10 ✅ / 1 🟡 / 1 ❌

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

## 4. Sélection — 5 ✅ / 2 🟡 / 1 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 31 | Sélection rectangle/ellipse/lasso | ✅ | `SelectMode::{Rect,Ellipse,Lasso}` |
| 32 | Lasso polygonal | ✅ | `SelectMode::PolyLasso` |
| 33 | Baguette magique | ✅ | [app/selection.rs:167](src/app/selection.rs:167), tolérance réglable |
| 34 | Sélection par plage de couleurs (image entière) | 🟡 | Le mode « Global » de la baguette s'en approche, pas de fonctionnalité "Color Range" dédiée |
| 35 | Opérations d'ensemble (add/subtract/intersect) | ✅ | `enum SelectionCombine` |
| 36 | Masque de sélection en pixels | ✅ | `feather`/`dilate`/`erode`, [tools/selection_mask.rs:116-154](src/tools/selection_mask.rs:116) |
| 37 | Détourage automatique par IA | ❌ | Le détourage existant est un flood-fill classique, pas un modèle IA |
| 38 | Amélioration des bords (Refine Edge) | 🟡 | `refine_edges` câblé seulement au détourage du pot de peinture, pas un panneau générique |

## 5. Peinture & Dessin — 9 ✅ / 1 🟡 / 0 ❌

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

## 6. Vectoriel & Plume — 6 ✅ / 0 🟡 / 4 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 49 | Outil Plume (Bézier éditable) | ✅ | [tools/pen.rs](src/tools/pen.rs) |
| 50 | Formes géométriques | ✅ | `enum Shape {Rectangle,Ellipse,Polygon,Star}` |
| 51 | Édition de nœuds après coup | ✅ | [app/pen_edit.rs:126](src/app/pen_edit.rs:126) |
| 52 | Opérations booléennes de formes | ✅ | Clipper, [tools/boolean.rs:15-52](src/tools/boolean.rs:15) |
| 53 | Dégradés sur formes | ✅ | `GradientKind::{Linear,Radial,Conic}` |
| 54 | Motifs de remplissage (patterns) | ❌ | Introuvable |
| 55 | Trait personnalisable (pointillés, épaisseur variable) | ❌ | Introuvable (hors animation de sélection) |
| 56 | Gradient mesh | ❌ | Introuvable |
| 57 | Déformation de formes (warp) | ✅ | `Adjustment::{ArcWarp,Wave,Sphere,Vortex}` |
| 58 | Cisaillement (skew) | ✅ | `Command::Shear`, [app/transform.rs:20-155](src/app/transform.rs:20) |

## 7. Texte & Typographie — 5 ✅ / 0 🟡 / 3 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 59 | Outil Texte, polices système | ✅ | [fonts.rs:24-113](src/fonts.rs:24) via `fontdb` |
| 60 | Texte sur courbe | ✅ | `TextArc` + `arc_chars` |
| 61 | Gras/italique/soulignement | ✅ *(corrigé le 30 août 2026)* | Gras (faux, décalage), italique (vraie fonte) et **soulignement** tous les trois présents |
| 62 | Interlignage et crénage | ✅ | `line_height`/`letter_spacing` |
| 63 | Contour de texte | ✅ | `outline_w`/`outline_color`, 8 passes |
| 64 | Texte → tracés vectoriels | ❌ | Introuvable |
| 65 | Polices variables / Google Fonts | ❌ | Scan des polices système uniquement, aucune police embarquée |
| 66 | Vérification orthographique | ❌ | Introuvable |

## 8. Filtres & Effets — 8 ✅ / 1 🟡 / 2 ❌

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

## 9. Couleur & Réglages — 7 ✅ / 0 🟡 / 1 ❌

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

## 10. Transformation — 2 ✅ / 2 🟡 / 2 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 85 | Déplacer/redimensionner/pivoter une sélection | ✅ | [app/transform.rs:34-120](src/app/transform.rs:34) |
| 86 | Retourner horizontal/vertical | ✅ | `flip_document()`, annulable |
| 87 | Perspective / distorsion libre | 🟡 | Homographie 4 points réglée par sliders, pas de poignées de coin glissables directement |
| 88 | Redressement d'horizon | 🟡 | `straighten_and_crop` réglé par slider d'angle, pas en traçant une ligne |
| 89 | Alignement automatique / panorama | ❌ | Introuvable |
| 90 | Recadrage avec grille des tiers | ❌ | Ratios/redressement présents, pas de grille des tiers superposée |

## 11. Collaboration, Cloud & Templates — 0 ✅ / 1 🟡 / 4 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 91 | Bibliothèque de modèles par catégorie | 🟡 | Galerie de formats/tailles vierges, pas de designs pré-remplis (texte/formes/mise en page) |
| 92 | Kit de marque | ❌ | Introuvable |
| 93 | Édition collaborative temps réel | ❌ | Confirmé absent par design (README) |
| 94 | Historique cloud + partage par lien | ❌ | Undo/redo local uniquement, confirmé absent par design |
| 95 | Redimensionnement magique vers un autre format | ❌ | Redimensionne le canevas, ne recompose pas la mise en page |

## 12. Automatisation & Extensibilité — 0 ✅ / 0 🟡 / 5 ❌

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 96 | Macros (enregistrement/rejeu) | ❌ | Introuvable |
| 97 | Scripting intégré | ❌ | Introuvable |
| 98 | Plugins tiers | ❌ | Introuvable |
| 99 | Traitement par lots sur un dossier | ❌ | Le "batch export" traite un seul document en plusieurs formats, pas un dossier de fichiers |
| 100 | API/intégrations externes | ❌ | Aucune dépendance réseau, confirmé absent par design |

---

## Analyse

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

## Analyse produit — validité, intégration, finalisation, propriété intellectuelle (30 août 2026)

Passe supplémentaire sur les 38 items non ✅ (26 ❌ + 12 🟡) : lequel est
**non pertinent** pour QuickPaint (contredit une décision déjà prise ou le
positionnement du produit), lequel est **à finaliser** (déjà commencé,
gain à faible effort), lequel serait **pertinent à intégrer** neuf, et
lequel **touche à de la propriété intellectuelle** (brevet, licence,
marque) et mérite une prudence particulière avant tout développement.

### 🚫 Non pertinent — contredit une décision déjà prise ou le non-goal du produit

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

### ⚠️ Touche à la propriété intellectuelle — prudence avant tout développement

| # | Fonctionnalité | Le risque concret |
|---|---|---|
| 45, 74 | Healing brush / Content-Aware Fill « avec une vraie synthèse de texture » | **PatchMatch** (Barnes/Goldman et al., SIGGRAPH 2009), l'algorithme derrière le Content-Aware Fill d'Adobe, est couvert par des brevets américains d'Adobe. C'est précisément pour ça que des outils libres (GIMP/Resynthesizer, etc.) évitent l'algorithme littéral et utilisent des variantes (diffusion, exemplar-based non brevetées). **L'implémentation actuelle de QuickPaint (plus-proche-voisin + diffusion laplacienne) évite déjà ce risque** — c'est un bon signe, mais toute amélioration future vers « une vraie synthèse de texture » doit explicitement éviter de réimplémenter PatchMatch tel quel, pas juste viser un meilleur résultat visuel. |
| 91 | Bibliothèque de modèles pré-remplis (pas juste des tailles vierges) | **Précédent réel dans ce dépôt** : le commit `fbe21fe` (« Retire le gabarit Flyer A6 Barbato et son logo embarqué ») montre qu'un gabarit avec un logo tiers a déjà été livré par erreur puis retiré. Toute extension de la galerie vers de vrais designs pré-remplis doit garantir un contenu 100 % original (pas de logo, pas de photo stock, pas de police non redistribuable), avec une revue explicite avant merge — ce n'est pas hypothétique, c'est déjà arrivé une fois. |
| 95 | Redimensionnement magique (recompose automatique vers un autre ratio) | Pas de brevet identifié avec certitude, mais l'algorithme précis de Canva (Magic Resize) est un produit commercial différenciant — le reproduire à l'identique (même heuristique de repositionnement) serait plus risqué qu'une recomposition « maison » avec une logique différente. Je n'ai pas de certitude juridique ici (pas un juriste) — à vérifier avant d'investir dessus si le sujet devient sérieux. |
| 66 | Vérification orthographique | Pas de risque de brevet, mais un risque de licence si on embarque un dictionnaire/moteur type Hunspell (LGPL) — le contournement le plus sûr est l'API native macOS `NSSpellChecker` (déjà le réflexe du projet pour le menu ⌘ natif via `muda`/`objc`), pas une lib tierce embarquée. |

### 🔧 À finaliser — déjà commencé, gain à effort raisonnable

| # | Fonctionnalité | Ce qu'il reste à faire |
|---|---|---|
| 1 | ✅ Import BMP — corrigé (30 août 2026) | Feature `bmp` activée dans `Cargo.toml`, test de régression `opens_a_bmp_image` (`project.rs`). |
| 61 | ✅ Soulignement de texte — corrigé (30 août 2026) | `TextItem::underline` + rendu (egui `Stroke` sur le remplissage central en live, bandeau blitté manuellement dans le compositeur CPU — tracé une seule fois, pas par passe), bouton dans la barre (icône Phosphor `TEXT_A_UNDERLINE`), presse-papiers de style mis à jour. **Bonus trouvé en implémentant** : le hash d'invalidation du cache de composition (`layer_hash`) oubliait déjà italique/interligne/espacement/police système/ombre depuis le Sprint Q — corrigé au passage (même catégorie de bug, même fonction). 3 tests ajoutés (2 unitaires + 1 bout-en-bout prouvant le blit réel). |
| 87, 88 | Perspective / redressement d'horizon par manipulation directe | Le calcul (homographie 4 points, redressement) existe déjà, seule l'UI est indirecte (sliders au lieu de poignées glissables sur l'image) — amélioration UI pure, pas un nouvel algorithme |
| 34 | Sélection par plage de couleurs | Le mode « Global » de la baguette magique fait déjà l'essentiel — l'exposer/le renommer explicitement comme fonctionnalité dédiée serait presque gratuit |
| 38 | Amélioration des bords de sélection | `refine_edges` existe déjà mais seulement câblé au détourage du pot de peinture — le généraliser à un panneau de sélection réutiliserait le code existant |
| 68 | ✅ Netteté réglable (Unsharp Mask) — corrigé (30 août 2026) | Nouvel `Adjustment::UnsharpMask` en complément de `Filter::Sharpen` (conservé, noyau fixe rapide toujours utile). 4 tests (identité à quantité nulle, no-op sur zone plate, seuil qui épargne les faibles écarts, overshoot caractéristique sur un bord net). |
| 2 | Import PSD (groupes, masques, texte éditable) | L'essentiel (calques/opacité/blend mode) fonctionne déjà ; étendre à ce qui manque est un prolongement de code existant, pas un nouveau module |

### ✅ Pertinent à intégrer — nouveau, cohérent avec l'identité du produit, sans souci de PI

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

### ❓ À trancher — j'ai besoin de ton avis avant d'aller plus loin

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
  uniquement, voir `audit_aout.md` §2), donc plus de contrainte de sandbox
  App Store bloquant cette voie. Reste un chantier volontaire à scoper si
  jugé prioritaire, pas un point technique en suspens.
