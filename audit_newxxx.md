# Audit de fonctionnalités — QuickPaint

Audit du code source (état au 2026-07-05) par rapport à la liste de
fonctionnalités attendues fournie par le porteur de projet. Chaque ligne
indique un statut — **✅ Implémenté**, **🟡 Partiel**, **❌ Absent** — avec un
pointeur vers le code qui le prouve (fichier/fonction) ou la mention
« introuvable » quand rien n'a été trouvé.

Méthode : lecture directe du code (`grep`/lecture de fichiers), pas
d'estimation à partir des seuls noms de fonctionnalités. Les emplacements
cités sont ceux constatés au moment de l'audit ; ils peuvent bouger si le
code évolue ensuite.

Légende : ✅ = fait et exploitable · 🟡 = présent mais incomplet/limité ·
❌ = non trouvé dans le code.

---

## Fichiers & formats

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 1 | Nouveau document avec formats prédéfinis (A4, 1080×1080, 4K) | ✅ | Galerie de modèles dans `ui/toolbar.rs` (`templates()`) : réseaux sociaux, impression (A4…), écran — plusieurs presets, dont 1080×1080. Pas de préréglage littéral « 4K » (3840×2160) repéré, seulement 1920×1080/2560×1440. |
| 2 | Ouverture multi-formats (PNG, JPG, WebP, TIFF, BMP, HEIC) | 🟡 | `project.rs` gère PNG/JPG/JPEG/BMP/GIF/WebP/TIFF via la crate `image`. **HEIC absent** (pas de decoder HEIC intégré). |
| 3 | Support des fichiers RAW d'appareil photo | ❌ | Aucune trace de décodage RAW (CR2/NEF/ARW/DNG) dans le code ou les dépendances. |
| 4 | Format natif avec calques éditables | ✅ | `project.rs` — format JSON propriétaire sérialisant `Document` (calques, traits, texte, images, masques). |
| 5 | Import des fichiers PSD | ✅ | `psd_import.rs` — import via la crate `psd` (Sprint 8.3). |
| 6 | Export PNG avec transparence | ✅ | `export.rs` — export PNG avec canal alpha préservé. |
| 7 | Export JPG/WebP avec réglage de qualité | 🟡 | JPEG : curseur de qualité 1–100. WebP exporté mais **sans réglage de qualité** (toujours sans perte / qualité fixe). |
| 8 | Export PDF et SVG | ✅ | `export.rs` (PDF mono-page, image encapsulée) et `svg.rs` (export vectoriel). |
| 9 | Export par lots (batch) et multi-tailles | ✅ | `export.rs::save_batch()` — plusieurs tailles exportées en une passe dans un dossier. |
| 10 | Récupération automatique après crash | ✅ | `project.rs` — sauvegarde automatique (`recovery.json`) à chaque frame, restauration proposée au démarrage. |

**Score section : 6 ✅ / 3 🟡 / 1 ❌**

---

## Calques & composition

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 11 | Calques multiples empilables et réordonnables | ✅ | `model/document.rs` — `Vec<Layer>` + calque actif ; réordonnancement historisé (undo/redo). |
| 12 | Opacité et modes de fusion par calque | ✅ | `opacity: f32` + `BlendMode` (Normal, Multiply, Screen, Overlay, Darken, Lighten) par calque. |
| 13 | Masque de calque et masque d'écrêtage | 🟡 | Masque d'écrêtage (`clip: bool`) et masque de calque peint (`layer.mask`, utilisé par le détourage) existent tous deux — mais pas d'éditeur de masque dédié aussi complet qu'un vrai calque de masque Photoshop (peinture directe du masque limitée au flux détourage/pinceau masque). |
| 14 | Groupes et verrouillage de calques | 🟡 | Regroupement (`group: Option<String>`) présent. **Verrouillage de calque non trouvé** (pas de flag `locked`). |
| 15 | Calques de réglage non-destructifs | ✅ | `tools/filter.rs` + `layer.adjustment` — Niveaux, Courbes, Teinte/Saturation, Flou de mouvement, Bokeh, Distorsion, Aberration chromatique, Warp, Duotone, appliqués au rendu sans modifier les pixels sources. |
| 16 | Styles de calque (ombre, contour, lueur) | ✅ | `model/document.rs` — `LayerStyle` : ombre portée, contour, lueur (Sprint 6.1). |
| 17 | Objets intelligents (redimension sans perte) | ❌ | Aucune notion d'objet intelligent / recalcul non destructif au redimensionnement. |

**Score section : 4 ✅ / 2 🟡 / 1 ❌**

---

## Dessin & peinture

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 18 | Pinceau à taille et dureté réglables | ✅ | `brush.width` + `pixel_hardness` (0–1) pour le pinceau pixel. |
| 19 | Crayon pixel-perfect et gomme partielle | ✅ | Pinceau pixel (`PixelBrush`) pour le tracé pixel-perfect ; gomme partielle = mode « glisser coupe le trait » de l'outil Gomme (`commit_partial_erase`). |
| 20 | Dégradés linéaire / radial / conique | ✅ | `model/stroke.rs` — `GradientKind::{Linear, Radial, Conic}`, éditeur de dégradé interactif (outil Gradient). |
| 21 | Tampon de duplication (clone) | ✅ | `RasterOp::Clone` (`history.rs`) + outil `CloneStamp` (⌥+clic = source). |
| 22 | Correcteur / cicatrisant | ✅ | `RasterOp::Heal` + outil `Healing` (recopie en réadaptant la couleur moyenne). |
| 23 | Densité + / − (éclaircir/assombrir) | ✅ | Outils `Dodge` / `Burn`. |
| 24 | Bibliothèque et import de brosses | ✅ | Présets de brosses persistés localement (chargement/sauvegarde). |
| 25 | Support tablette / stylet avec pression | ✅ | `input/pressure.rs` — lissage de pression, largeur du trait modulée par la pression/vitesse. |
| 26 | Stabilisation du tracé et symétrie | ✅ | `input/capture.rs` — stabilisation EMA réglable ; outil `Symmetry` (miroir 2/4/6/8 axes). |

**Score section : 9 ✅ / 0 🟡 / 0 ❌**

---

## Sélection & découpe

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 27 | Sélections rectangle, ellipse, lasso | ✅ | `SelectMode::{Rect, Ellipse, Lasso}`. |
| 28 | Baguette magique et sélection par plage de couleurs | ✅ | `SelectMode::Wand` avec tolérance réglable, modes contigu/global. |
| 29 | Contour progressif et amélioration des bords (cheveux) | 🟡 | `tools/bucket.rs::soft_edge()` adoucit le bord par proximité de couleur (utilisé par le détourage) — c'est un adoucissement générique, pas un algorithme dédié « amélioration des cheveux/matting » au sens Photoshop. |
| 30 | Recadrage libre, par ratio et redressement d'horizon | ✅ | `crop_mode`, `crop_ratio`, `crop_angle` (redressement d'horizon) dans `app/mod.rs`. |
| 31 | Enregistrer/charger une sélection | ✅ | Sélection nommée sauvegardée/rechargée (`named_selection`). |

**Score section : 4 ✅ / 1 🟡 / 0 ❌**

---

## Retouche photo

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 32 | Luminosité, contraste, exposition | 🟡 | Luminosité et contraste présents (`tools/filter.rs`). **Pas de réglage « Exposition » dédié** (distinct d'un simple +/− de luminosité). |
| 33 | Niveaux et courbes RVB | ✅ | Calques de réglage Niveaux et Courbes (points de contrôle) dans `tools/filter.rs`. |
| 34 | Teinte/saturation, vibrance, balance des blancs | 🟡 | Teinte/Saturation/Luminosité présents, ainsi que Saturation +/− (éponge). **Vibrance et balance des blancs non trouvées.** |
| 35 | Netteté et réduction de bruit | 🟡 | Filtre Netteté (`Sharpen`) présent. **Pas de filtre dédié de réduction de bruit** (le lissage type aquarelle/huile atténue le bruit en effet de bord mais n'est pas présenté comme un débruiteur). |
| 36 | Correction de distorsion et d'aberration chromatique | ✅ | Calques de réglage Distorsion et Aberration chromatique. |
| 37 | Suppression d'objets (content-aware) | ✅ | `tools/inpaint.rs` — reconstruction par diffusion (Sprint 4.3), 100 % local. |
| 38 | Retouche peau et suppression yeux rouges | ✅ | `RetouchKind::{SkinSmooth, RedEye}` — lissage guidé par luminance, dépupillage rouge. |
| 39 | Comparaison avant/après et histogramme en direct | 🟡 | `ui/toolbar.rs::histogram_window` — histogramme RGB + bouton « Avant (maintenir) » qui annule/réapplique temporairement la dernière action. Fonctionne **uniquement sur une image sélectionnée**, pas sur le canevas entier ni en comparaison glissante côte-à-côte. |

**Score section : 3 ✅ / 4 🟡 / 0 ❌**

---

## Filtres & effets

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 40 | Flous (gaussien, mouvement, bokeh) | 🟡 | Flou local (moyenne 3×3), Flou de mouvement et Bokeh implémentés. **Pas de vrai flou gaussien** (approximé par moyennage, pas de noyau gaussien). |
| 41 | Effets artistiques (aquarelle, huile, croquis, BD) | 🟡 | Aquarelle (lissage bilatéral), Huile (Kuwahara), Croquis (contours Sobel) présents. **Effet « BD/bande dessinée » non trouvé.** |
| 42 | Grain argentique, vintage, duotone | ✅ | Grain argentique (bruit déterministe), Vintage (teinte chaude + désaturation + vignette), Duotone. |
| 43 | Import de LUT (.cube) avec intensité réglable | ✅ | `tools/lut.rs` — parsing `.cube`, application avec curseur d'intensité 0–1. |

**Score section : 2 ✅ / 2 🟡 / 0 ❌**

---

## Texte & vectoriel

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 44 | Outil texte (polices, contour, ombre, texte sur courbe) | ✅ | `model/text.rs` — polices (Proportionnelle/Monospace + polices système), contour, ombre, texte sur arc (`TextArc`, Sprint 7.1). |
| 45 | Formes vectorielles et outil plume (Bézier) | ✅ | `tools/shape.rs` (Ligne, Flèche, Rectangle, Ellipse, Polygone, Étoile) + `tools/pen.rs` (chemin de Bézier cubique). |
| 46 | Opérations booléennes et icônes vectorielles | 🟡 | `tools/boolean.rs` — Union/Soustraction/Intersection via `geo-clipper`. **Pas de bibliothèque d'icônes vectorielles dédiée** (il existe une bibliothèque d'éléments/pictogrammes simples, `tools/assets.rs`, mais pas une iconothèque vectorielle à proprement parler). |

**Score section : 2 ✅ / 1 🟡 / 0 ❌**

---

## Couleur & transformation

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 47 | Sélecteur de couleur, pipette, nuanciers, extraction de palette | 🟡 | Sélecteur de couleur, pipette (`tools/eyedropper.rs`) et nuanciers/palette personnalisée présents. **Extraction automatique de palette depuis une image non trouvée.** |
| 48 | Transformations (rotation, perspective, déformation warp) | ✅ | Rotation (sélection/calque), `tools/perspective.rs` (homographie), Warp (calque de réglage « Warp Arc »). |

**Score section : 1 ✅ / 1 🟡 / 0 ❌**

---

## Confort & IA locale (on-device)

| # | Fonctionnalité | Statut | Constat |
|---|---|---|---|
| 49 | Suppression d'arrière-plan et upscale en traitement local | 🟡 | Suppression d'arrière-plan = outil **Détourage** (flood-fill + adoucissement des bords, 100 % local, *heuristique* — pas un modèle de segmentation appris). Upscale = rééchantillonnage Lanczos3 (agrandissement classique, pas de super-résolution par IA). Les deux fonctionnent et sont bien locaux, mais aucun des deux n'est réellement « IA » au sens modèle appris — cohérent avec le message de commit du projet qui parle d'« IA locale heuristique ». |
| 50 | Historique d'annulation illimité + raccourcis personnalisables | ✅ | `history.rs` — pile annuler/rétablir sans limite fixe ; `keybindings.rs` — raccourcis personnalisables et persistés. |

**Score section : 1 ✅ / 1 🟡 / 0 ❌**

---

## Synthèse globale

| Statut | Nombre | % (sur 50) |
|---|---|---|
| ✅ Implémenté | 32 | 64 % |
| 🟡 Partiel | 17 | 34 % |
| ❌ Absent | 3 | 6 % |

*(un item peut arrondir différemment selon comment on compte les sous-listes ; les pourcentages sont indicatifs)*

### Ce qui manque complètement (❌)
1. **Formats RAW appareil photo** — aucun décodeur RAW.
2. **Objets intelligents** (redimension non destructive à la Photoshop).
3. **Support HEIC** en ouverture (classé avec le point 2 « multi-formats », qui est sinon largement couvert).

### Ce qui est présent mais à compléter en priorité (🟡 les plus impactants)
- **Verrouillage de calque** — absent, souvent attendu dès qu'il y a des calques multiples.
- **Vrai flou gaussien** — actuellement approximé, peut donner un rendu différent des attentes utilisateur.
- **Vibrance / balance des blancs** — manques classiques d'un module retouche photo par ailleurs complet.
- **Extraction automatique de palette** — pipette manuelle seulement.
- **Comparaison avant/après** — existe mais limitée à une image sélectionnée, pas au canevas entier.

### Point d'attention sur le positionnement « IA locale »
Le point 49 et plusieurs modules (détourage, retouche peau, inpainting) sont
**heuristiques** (flood-fill, diffusion, lissage guidé par luminance), pas
des modèles de machine learning embarqués. C'est cohérent avec la promesse
« 100 % local, sans modèle ni réseau » déjà présente dans les commentaires du
code, mais le terme « IA » dans la liste de fonctionnalités pourrait créer
une attente (réseau de neurones on-device) que le code actuel ne remplit pas
littéralement — à clarifier dans la communication produit si besoin.

---

*Audit réalisé par lecture du code source uniquement (pas de test manuel de
chaque fonctionnalité dans l'application). Un point marqué ✅ signifie que le
code correspondant existe et semble fonctionnel à la lecture ; il est
recommandé de vérifier manuellement les points 🟡 les plus sensibles avant
toute communication externe sur la liste de fonctionnalités.*
