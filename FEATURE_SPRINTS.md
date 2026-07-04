# FEATURE_SPRINTS.md — Fonctionnalités manquantes/partielles, plan de sprints

> **Statut (0.14.0) : les 9 sprints sont livrés.** HEIC, RAW et le ML
> on-device (Sprint 8.4/9) ont été volontairement écartés en cours de route
> — licences AGPL/LGPL des seules bibliothèques disponibles, ou dépendance à
> un modèle de réseau de neurones embarqué. Voir [CHANGELOG.md](CHANGELOG.md)
> pour le détail par sprint.

> Audit fonctionnel (formats, calques, dessin, retouche photo, filtres,
> texte/vectoriel, couleur, IA locale) mené le 4 juillet 2026 par lecture du
> code (`src/model/`, `src/tools/`, `src/export.rs`, `src/project.rs`,
> `src/render/`). Ne couvre **que** les items 🟡 partiels et ❌ absents de
> la checklist produit ; les fonctionnalités déjà ✅ complètes ne sont pas
> reprises ici. Chaque sprint est dimensionné pour rester livrable et
> testable indépendamment — pas de sprint qui dépend d'un autre sprint non
> encore fait, sauf mention explicite.

Priorisation basée sur : (1) risque utilisateur (perte de travail > reste),
(2) rapport effort/valeur, (3) dépendances techniques (ex. LUT a besoin du
pipeline de filtres déjà en place).

---

## Sprint 1 — Fiabilité : ne pas perdre le travail de l'utilisateur

**Pourquoi en premier :** c'est la seule catégorie où l'absence de la
fonctionnalité peut coûter du travail irrécupérable à l'utilisateur. Tout le
reste est une question de confort ou de richesse fonctionnelle.

### 1.1 — Récupération automatique après crash ❌
- Sauvegarder périodiquement (ex. toutes les 60-120s, ou après N actions)
  une copie du document courant dans un dossier de récupération
  (`~/Library/Application Support/<app>/recovery/`).
- Au démarrage, détecter la présence d'un fichier de récupération non
  nettoyé (= crash précédent) et proposer à l'utilisateur de le restaurer
  ou de l'ignorer.
- Nettoyer le fichier de récupération à la fermeture normale de l'app.
- Fichiers concernés : [project.rs](src/project.rs), [history.rs](src/history.rs), `app/mod.rs`.

### 1.2 — Enregistrer/charger une sélection ❌
- Étendre le modèle de sélection (probablement dans `tools/mod.rs` /
  `tools/bucket.rs`) pour qu'elle soit sérialisable.
- Ajouter une entrée « Sélections nommées » sauvegardée dans le document
  (`project.rs`), au même titre que les calques.
- UI minimale : liste déroulante « Enregistrer sélection… » / « Charger
  sélection ».

---

## Sprint 2 — Compléter la sélection et le recadrage

**Pourquoi ensuite :** ce sont des extensions d'outils déjà partiellement
implémentés (🟡) — effort réduit, gain visible immédiat pour l'usage photo
courant.

### 2.1 — Sélection ellipse explicite 🟡→✅
- Le rectangle et le lasso existent déjà dans l'outil Select
  ([tools/mod.rs](src/tools/mod.rs)) ; ajouter un mode ellipse au même
  endroit (même logique de hit-test, formule elliptique au lieu de
  rectangulaire).

### 2.2 — Vraie baguette magique (magic wand) 🟡→✅
- Aujourd'hui seul le flood-fill du bucket existe
  ([tools/bucket.rs](src/tools/bucket.rs)).
- Réutiliser l'algorithme de flood-fill mais produire une **sélection**
  (pas un remplissage), avec un seuil de tolérance de couleur réglable.
- Ajouter une variante « sélection par plage de couleur » globale à
  l'image (pas seulement une région contiguë).

### 2.3 — Redressement d'horizon dans le recadrage 🟡→✅
- Le crop tool a déjà les ratios prédéfinis ([ui/toolbar.rs](src/ui/toolbar.rs)).
- Ajouter une poignée de rotation sur l'overlay de crop, avec aperçu en
  temps réel de l'angle et rotation de l'image lors de la validation.

---

## Sprint 3 — Dessin : confort et fidélité du trait

**Pourquoi ensuite :** ce sont des attentes de base pour un outil de
dessin sérieux, notamment vu le nom du projet (« tactile ») — la pression
stylet et la stabilisation du tracé sont centrales à l'expérience de
dessin, pas des à-côtés.

### 3.1 — Support tablette/stylet réel ❌
- Actuellement la pression est simulée par la vélocité du curseur
  ([input/pressure.rs](src/input/pressure.rs)).
- Intégrer la lecture de la pression réelle exposée par macOS
  (NSEvent `pressure` / tablet events) quand un périphérique compatible
  est détecté, en gardant le fallback vélocité pour souris/trackpad.

### 3.2 — Stabilisation du tracé ❌
- La symétrie existe déjà (miroir 6/8 axes) mais pas de lissage/stabilisation.
- Ajouter un filtre de lissage du chemin en cours de tracé (ex. moyenne
  glissante pondérée sur les derniers points, ou un « lazy radius » façon
  Krita/Photoshop) dans [input/smoothing.rs](src/input/smoothing.rs) — le
  fichier existe déjà, vérifier s'il ne fait que du lissage post-tracé et
  étendre à un mode « live ».

### 3.3 — Dégradé conique ❌
- Linéaire et radial existent déjà ([model/stroke.rs:26-29](src/model/stroke.rs)).
- Ajouter un troisième mode de calcul de gradient basé sur l'angle
  polaire autour d'un centre.

### 3.4 — Bibliothèque et import de brosses ❌
- Définir un format de brosse (forme, dureté, espacement, texture
  optionnelle) sérialisable.
- UI : panneau de sélection de brosse avec préréglages fournis + bouton
  import (fichier `.brush`/`.json` ou texture PNG en niveaux de gris comme
  masque d'alpha).

---

## Sprint 4 — Retouche photo avancée

**Pourquoi ensuite :** ce sont des fonctionnalités à forte valeur perçue
pour un usage « photo » mais plus coûteuses techniquement (traitement
d'image non trivial) — les placer après les fondations plus simples.

### 4.1 — Comparaison avant/après et histogramme en direct ❌
- Le plus simple à livrer du groupe : un mode « split view » ou bascule
  clavier montrant l'image avant les filtres appliqués.
- Histogramme RGB calculé à partir du buffer courant, affiché en overlay
  dans le panneau de filtres ([tools/filter.rs](src/tools/filter.rs)).

### 4.2 — Correction de distorsion et d'aberration chromatique ❌
- Distorsion : déformation radiale simple (barrel/pincushion) paramétrable.
- Aberration chromatique : décalage indépendant des canaux R/B en périphérie
  de l'image.
- S'insère dans le pipeline de filtres existant.

### 4.3 — Suppression d'objets (content-aware) ❌
- Le plus coûteux du sprint : nécessite un algorithme d'inpainting
  (ex. PatchMatch simplifié, ou a minima un flood-fill directionnel avec
  échantillonnage des bords).
- Dépend d'avoir une sélection (Sprint 2) pour désigner la zone à effacer.

### 4.4 — Retouche peau / suppression yeux rouges ❌
- Yeux rouges : détection de zone rouge saturée dans une sélection
  circulaire + désaturation/assombrissement local — relativement simple.
- Retouche peau : lissage sélectif (flou guidé par la luminance) — réutilise
  la logique de flou déjà présente ([tools/filter.rs](src/tools/filter.rs)).

---

## Sprint 5 — Filtres créatifs et flous avancés

**Pourquoi ensuite :** valeur esthétique plutôt que fonctionnelle,
s'appuie sur le pipeline de filtres déjà solide (niveaux, courbes,
teinte/saturation existent).

### 5.1 — Flou mouvement et bokeh 🟡→✅
- Le flou gaussien approximé existe ([tools/filter.rs:19](src/tools/filter.rs)).
- Flou de mouvement : convolution directionnelle (angle + distance
  réglables).
- Bokeh : flou en disque (kernel circulaire) avec accentuation optionnelle
  des hautes lumières.

### 5.2 — Grain argentique, vintage, duotone ❌
- Trois filtres relativement simples à base de LUT/courbes de tons +
  bruit procédural pour le grain — bon warm-up avant le LUT import (5.3).

### 5.3 — Import de LUT (.cube) ❌
- Parser de fichier `.cube` (format texte standard, table 3D RGB).
- Application via interpolation trilinéaire sur l'image, avec curseur
  d'intensité (mix entre image originale et image LUT-appliquée).

### 5.4 — Effets artistiques (aquarelle, huile, croquis, BD) ❌
- Le plus gros morceau du sprint, à traiter en dernier : chaque effet est
  un algorithme distinct (ex. croquis = détection de contours + niveaux de
  gris ; BD = quantification de couleurs + contours noirs ; huile = filtre
  de Kuwahara ou équivalent).
- Possibilité de livrer un effet à la fois sur plusieurs sprints suivants
  plutôt que tout d'un coup.

---

## Sprint 6 — Calques avancés

**Pourquoi ensuite :** les calques de base (empilement, opacité, blend
modes, masques, groupes, réglages non-destructifs) sont déjà ✅ — ce qui
reste est un raffinement, pas une fondation manquante.

### 6.1 — Styles de calque (ombre, contour, lueur) ❌
- Ajouter au modèle de calque ([model/document.rs](src/model/document.rs))
  une liste de styles non-destructifs appliqués au rendu final du calque
  (ombre portée, contour, lueur externe/interne), à la manière des calques
  de réglage déjà en place.

### 6.2 — Objets intelligents (redimension sans perte) ❌
- Nécessite de conserver une référence à la donnée source (image/vecteur)
  séparément de sa transformation affichée, pour pouvoir redimensionner
  sans dégrader.
- Impact plus large sur le modèle de document — à specifier avant
  implémentation (peut nécessiter son propre mini-sprint de conception).

---

## Sprint 7 — Texte, vectoriel et transformation

**Pourquoi ensuite :** styles de texte et perspective sont des extensions
ponctuelles d'outils déjà fonctionnels (texte de base, plume Bézier,
rotation/échelle existent tous).

### 7.1 — Contour, ombre et texte sur courbe 🟡→✅
- Le texte de base existe ([model/text.rs](src/model/text.rs)).
- Contour et ombre : deux passes de rendu supplémentaires autour du glyphe.
- Texte sur courbe : positionner chaque glyphe le long d'un chemin Bézier
  existant (réutilise [tools/pen.rs](src/tools/pen.rs)).

### 7.2 — Transformation perspective et warp ❌
- Rotation/échelle existent ([app/transform.rs](src/app/transform.rs)).
- Perspective : transformation homographique à 4 points de contrôle.
- Warp : grille de déformation (maillage de points de contrôle, façon
  Photoshop Puppet Warp simplifié) — le plus complexe, à traiter en dernier
  du sprint.

---

## Sprint 8 — Formats de fichiers étendus

**Pourquoi en avant-dernier :** valeur réelle mais niche (utilisateurs
avec du matériel/logiciel spécifique — RAW, PSD) ; sans urgence business
contrairement à la fiabilité (Sprint 1).

### 8.1 — TIFF et HEIC en ouverture 🟡→✅
- PNG/JPG/BMP/GIF/WebP sont déjà supportés ([project.rs:81](src/project.rs)).
- Ajouter les décodeurs correspondants (crates Rust existantes pour les
  deux formats) au même point d'entrée d'import.

### 8.2 — Réglage de qualité à l'export JPG/WebP 🟡→✅
- Les formats sont gérés en export ([export.rs](src/export.rs)) mais sans
  curseur de qualité exposé — ajouter le paramètre à l'encodeur existant et
  un slider dans le dialogue d'export.

### 8.3 — Import PSD ❌
- Nécessite un parseur du format PSD (structure de calques, blend modes,
  masques) — mapper vers le modèle de document interne
  ([model/document.rs](src/model/document.rs)). Effort substantiel, prévoir
  une bibliothèque tierce plutôt qu'un parseur maison.

### 8.4 — Support RAW appareil photo ❌
- Le plus gros morceau du groupe : décodage RAW (démosaïçage, balance des
  blancs capteur, profils couleur constructeur) — s'appuyer sur une
  bibliothèque native existante plutôt que réimplémenter le pipeline RAW.

---

## Sprint 9 — IA locale (on-device)

**Pourquoi en dernier :** techniquement le plus lourd (intégration de
modèles ML), et la valeur ajoutée, bien que forte, n'est pas bloquante —
à traiter une fois les fondations (Sprints 1-8) solides.

### 9.1 — Suppression d'arrière-plan on-device ❌
- Intégrer un modèle de segmentation léger exécuté localement (ex. via
  Core ML sur macOS) pour produire un masque alpha à partir de l'image,
  puis appliquer ce masque comme masque de calque (réutilise le système de
  masques déjà en place, Sprint 6 non requis).

### 9.2 — Upscale on-device ❌
- Modèle de super-résolution léger exécuté localement (Core ML), appliqué
  à la demande sur l'image ou une sélection, avec aperçu avant application
  définitive.

---

## Récapitulatif ordonné

| # | Sprint | Nb items | Risque si non fait |
|---|---|---|---|
| 1 | Fiabilité (autosave, sélections sauvegardées) | 2 | Perte de travail utilisateur |
| 2 | Sélection & recadrage | 3 | Frustration usage courant |
| 3 | Dessin : pression, stabilisation, brosses | 4 | Expérience de dessin en dessous des attentes « tactile » |
| 4 | Retouche photo avancée | 4 | Manque face à la concurrence photo |
| 5 | Filtres créatifs | 4 | Manque esthétique/créatif |
| 6 | Calques avancés | 2 | Raffinement seulement |
| 7 | Texte/vectoriel/transformation | 2 | Raffinement seulement |
| 8 | Formats étendus (TIFF/HEIC/PSD/RAW) | 4 | Niche, dépend du public visé |
| 9 | IA locale | 2 | Différenciateur, non bloquant |
