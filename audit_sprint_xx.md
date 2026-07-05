> **Statut : tous les sprints A-F ont été traités le 5 juillet 2026.** Sprint A
> a été explicitement clos sans changement de code (décision du porteur de
> projet : pas de dépendance `libwebp`, voir sa section). Conservé tel quel
> comme trace de la planification initiale ; voir le résumé de fin de
> fichier pour l'état réel constaté pendant l'implémentation (deux points,
> B.2 et B.3, se sont révélés déjà largement couverts par l'architecture
> existante).

# audit_sprint_xx.md — Plan de sprints pour les écarts de l'audit du 5 juillet 2026

> Fait suite à [audit_newxxx.md](audit_newxxx.md). Ne couvre que les points
> validés à corriger par le porteur de projet :
> **7, 13, 14, 17, 29, 32, 34, 35, 39, 40, 41, 46**.
>
> Confirmés comme écarts **acceptés, sans action** (décision produit, pas
> oubli) : **2** (HEIC), **3** (RAW), **49** (suppression d'arrière-plan /
> upscale : restent heuristiques, pas de modèle ML embarqué) — cohérent avec
> [FEATURE_SPRINTS.md](FEATURE_SPRINTS.md) qui documente déjà pourquoi HEIC/RAW
> et le ML on-device ont été écartés (licences AGPL/LGPL des seules
> bibliothèques disponibles, ou dépendance à un modèle de réseau embarqué).

Chaque sprint est pensé pour rester livrable et testable indépendamment.
Numérotation reprise de la checklist produit (`audit_newxxx.md`), pas un
ordre de sprint interne.

---

## Sprint A — Export : qualité WebP réelle

### A.1 — Export JPG/WebP avec réglage de qualité (point 7) 🟡→❌ (décision : pas de changement)

**Pourquoi c'était encore 🟡 :** [export.rs:84-86](src/export.rs:84) documente
déjà la raison : la crate `image` encode le WebP **toujours sans perte** ;
un WebP *lossy* avec curseur de qualité nécessite `libwebp` (dépendance
système C), volontairement écartée jusqu'ici — même logique que le refus de
HEIC/RAW (item 2/3).

> **Décision actée le 5 juillet 2026 :** pas de dépendance `libwebp`. Le
> WebP reste sans perte, seul le JPEG garde un curseur de qualité. Écart
> assumé, pas un oubli — aucune sous-tâche ci-dessous n'a été réalisée,
> conservées uniquement pour référence si la décision est reconsidérée plus
> tard.

**Sous-tâches (si la décision était reconsidérée un jour et qu'on choisit
d'ajouter `libwebp`) :**
- [ ] Ajouter la dépendance (crate `webp` ou équivalent), vérifier la
      compatibilité de build macOS (arm64) sans lien externe manquant.
- [ ] Étendre `encode_to()` ([export.rs:107](src/export.rs:107)) : remplacer
      `ExportFormat::Webp => buf.save(path)` par un encodage lossy avec le
      paramètre `jpeg_quality` déjà présent dans la signature (renommer en
      `quality` si réutilisé pour les deux formats).
- [ ] UI : le curseur de qualité existant pour JPEG (`ui/toolbar.rs`, export
      dialog) doit s'activer aussi pour WebP.
- [ ] Test : `cargo test` sur `encode_to` avec deux qualités WebP distinctes,
      vérifier une différence de taille de fichier (comme le test JPEG
      existant à [export.rs:243-244](src/export.rs:243)).

---

## Sprint B — Calques : verrouillage, masques, objets intelligents

### B.1 — Groupes et verrouillage de calques (point 14) 🟡→✅

**État actuel :** les groupes existent (`group: Option<String>` dans
`model/document.rs`) ; **aucun flag de verrouillage**.

**Sous-tâches :**
- [ ] Ajouter `pub locked: bool` à `Layer` (`model/document.rs`), `false`
      par défaut, sérialisé (format natif + compatibilité ascendante des
      documents déjà enregistrés : `#[serde(default)]`).
- [ ] Bloquer toute mutation du calque verrouillé : dessin, gomme,
      déplacement/transformation, suppression — probablement un seul point
      de contrôle centralisé dans `app/mod.rs` avant dispatch des outils
      (vérifier `self.doc.active_layer` verrouillé → ignorer + message
      d'info, cf. `self.info(...)` déjà utilisé ailleurs).
      - Autoriser malgré tout : visibilité, opacité, réordonnancement ? À
        trancher (Photoshop autorise réordonner mais pas peindre/déplacer).
- [ ] UI : icône cadenas dans `ui/layers.rs`, cliquable, à côté du bouton
      visibilité déjà existant.
- [ ] Historique : le verrouillage lui-même doit-il être annulable
      (`Command::ToggleLayerLock`) ? Recommandé pour cohérence avec le reste
      de l'historique (`history.rs`).
- [ ] Tests : verrouiller un calque, vérifier qu'un coup de pinceau/gomme
      dessus est un no-op ; déverrouiller, vérifier que ça fonctionne à
      nouveau.

### B.2 — Masque de calque et masque d'écrêtage (point 13) 🟡→✅

**État actuel :** masque d'écrêtage (`clip: bool`) et masque de calque peint
(`layer.mask`, alimenté aujourd'hui uniquement par le flux Détourage) déjà
présents séparément — pas d'éditeur de masque générique.

**Sous-tâches :**
- [ ] Permettre de peindre **directement** dans `layer.mask` avec les outils
      existants (Pinceau pixel / Gomme pixel), via un mode « éditer le
      masque » déjà évoqué dans les commentaires de `render/text.rs`
      (« redevient éditable ensuite au pinceau/gomme pixel via *Éditer le
      masque* ») — vérifier si ce mode existe déjà partiellement pour le
      détourage et le généraliser à tout calque, pas seulement après un
      détourage.
- [ ] UI : bouton « Ajouter un masque de calque » dans `ui/layers.rs` (créer
      un masque blanc opaque par défaut si absent), bouton « Éditer le
      masque » qui bascule l'affichage/l'édition (miniature du masque à
      côté de la miniature du calque, comme Photoshop).
- [ ] Historique : peindre le masque doit passer par le même mécanisme de
      undo/redo que le raster de contenu (`Command::PaintRaster` a déjà un
      `RasterTarget` — vérifier s'il distingue `Content` vs `Mask`, sinon
      étendre l'enum existant plutôt qu'en créer un nouveau).
- [ ] Tests : peindre dans un masque, annuler, vérifier que le contenu du
      calque est intact et seul le masque est restauré.

### B.3 — Objets intelligents (point 17) ❌→✅

**Le plus gros morceau des trois** — actuellement rien n'existe.

**Portée réaliste à définir avant de coder :** un « objet intelligent » complet
(à la Photoshop) implique un lien vers les données source originales +
recalcul du rendu à chaque transformation. Proposition de portée réduite
mais utile, cohérente avec l'architecture actuelle (`ImageItem` déjà stocké
en RGBA brut dans `model/`) :
- [ ] Étendre `ImageItem` (`model/` — vérifier le module exact d'`ImageItem`)
      avec une résolution source conservée en plus de la résolution
      affichée, pour permettre un agrandissement ultérieur sans perte tant
      qu'on ne dépasse pas la résolution d'origine (au lieu de rééchantillonner
      un rééchantillonnage).
- [ ] Redimensionnement (poignées de sélection) : au lieu d'écraser
      `ImageItem.rgba` à la nouvelle taille immédiatement, ne stocker qu'un
      facteur d'échelle + repositionner ; ne rasteriser à la taille cible
      qu'à l'export ou quand la taille dépasse la résolution source stockée.
- [ ] UI : indicateur visuel (badge) sur les images en mode « objet
      intelligent » dans le panneau de calques, pour que l'utilisateur sache
      que l'image n'est pas encore rasterisée définitivement.
- [ ] Documenter clairement la limite : pas de lien vers un fichier externe
      re-modifiable (contrairement à Photoshop qui peut rouvrir le PSD
      source) — la portée ici est « pas de perte de qualité au
      redimensionnement dans le document courant », pas un vrai lien source
      externe.
- [ ] Tests : agrandir puis réduire une image plusieurs fois, vérifier que
      la netteté ne se dégrade pas tant qu'on reste sous la résolution
      source.

---

## Sprint C — Sélection : amélioration des bords (matting)

### C.1 — Contour progressif et amélioration des bords/cheveux (point 29) 🟡→✅

**État actuel :** `tools/bucket.rs::soft_edge()` adoucit par proximité de
couleur — un adoucissement générique, pas un algorithme dédié aux détails
fins (cheveux, fourrure, bords semi-transparents).

**Sous-tâches :**
- [ ] Ajouter un second passage optionnel après `soft_edge()` : détection de
      zones à forte variance locale (texture fine = probablement des
      cheveux/fourrure) pour y appliquer un rayon d'adoucissement plus fin
      que le reste du contour, au lieu d'un rayon uniforme.
- [ ] UI : dans les options de l'outil Détourage/Sélection (`ui/toolbar.rs`),
      ajouter un curseur « Affiner les bords » séparé de la tolérance
      actuelle, plutôt que de surcharger le réglage existant.
- [ ] Garder 100 % local/heuristique (pas de modèle de matting appris,
      cohérent avec le reste de l'app — voir remarque sur le point 49 dans
      `audit_newxxx.md`).
- [ ] Tests : image de test avec un bord texturé (motif à haute fréquence),
      vérifier que le masque produit a moins de « escaliers » qu'avec
      `soft_edge()` seul.

---

## Sprint D — Retouche photo : ajustements manquants

### D.1 — Exposition (point 32) 🟡→✅
- [ ] Ajouter `Adjustment::Exposure { ev: f32 }` dans `tools/filter.rs`
      (même schéma que `Levels`/`Curves` : label, valeur par défaut, preview,
      application aux pixels — voir le pattern autour de
      [filter.rs:89-246](src/tools/filter.rs:89)).
- [ ] Différence avec la luminosité existante : l'exposition doit être un
      gain multiplicatif en stops (`2^ev`) appliqué avant tout autre
      ajustement, pas un simple décalage additif — sinon ça duplique
      Luminosité sous un autre nom.
- [ ] UI : curseur dans le panneau de calque de réglage (`ui/layers.rs`).
- [ ] Tests unitaires : `ev = 0` → image inchangée ; `ev = 1` → doublement
      des valeurs (avant clamp 0-255).

### D.2 — Vibrance et balance des blancs (point 34) 🟡→✅
- [ ] **Vibrance** : ajouter `Adjustment::Vibrance { amount: f32 }` —
      contrairement à `HueSaturation.sat` (qui affecte tout uniformément),
      la vibrance doit saturer davantage les couleurs **peu saturées** et
      épargner les couleurs déjà saturées (et les teintes chair, si on veut
      rester fidèle à la définition Photoshop — au minimum : saturation
      pondérée par `1 - saturation_actuelle`).
- [ ] **Balance des blancs** : ajouter `Adjustment::WhiteBalance { temp: f32,
      tint: f32 }` — décalage température (bleu/orange) et teinte
      (vert/magenta) sur les canaux RGB, formule classique de correction de
      température de couleur.
- [ ] UI : deux nouveaux calques de réglage dans le même menu que
      Niveaux/Courbes (`ui/layers.rs`).
- [ ] Tests : vibrance à 0 = image inchangée ; balance des blancs à (0,0) =
      image inchangée (comme les autres ajustements, voir pattern
      `default()` de chaque `Adjustment`).

### D.3 — Réduction de bruit (point 35) 🟡→✅
- [ ] Ajouter un filtre `Filter::Denoise` distinct de `Sharpen` — algorithme
      simple type flou bilatéral déjà utilisé pour l'aquarelle
      ([filter.rs:38](src/tools/filter.rs:38), `Watercolor`) mais avec un
      seuil de préservation des contours plus agressif et sans le rendu
      "peinture" du filtre aquarelle.
- [ ] Curseur d'intensité (comme LUT/autres filtres).
- [ ] Tests : image de test avec bruit synthétique (bruit gaussien ajouté),
      vérifier une baisse mesurable de variance locale après application.

### D.4 — Comparaison avant/après sur le canevas entier (point 39) 🟡→✅

**État actuel :** [toolbar.rs:601-640](src/ui/toolbar.rs:601) —
`histogram_window` fonctionne **uniquement sur une image sélectionnée**
(`app.single_image_idx()`), pas sur le document entier.

- [ ] Étendre `histogram_rgb()` (`tools/filter.rs`) pour accepter le rendu
      composite entier du document (réutiliser `Compositor::render_to_rgba`
      déjà utilisé pour l'export, voir `render/compositor.rs:88`) quand
      aucune image n'est sélectionnée, au lieu de n'afficher qu'un message
      d'invite.
- [ ] Le bouton « Avant (maintenir) » ([toolbar.rs:632](src/ui/toolbar.rs:632))
      fonctionne déjà en annulant/réappliquant la dernière action — vérifier
      qu'il s'applique correctement au rendu composite entier, pas seulement
      à l'image sélectionnée.
- [ ] Bonus (hors scope minimal) : curseur de comparaison glissant
      côte-à-côte, plutôt que le seul bouton « maintenir ».
- [ ] Tests : ouvrir l'histogramme sans sélection d'image, vérifier qu'il
      affiche désormais un histogramme du canevas au lieu du message
      d'invite.

---

## Sprint E — Filtres : flou gaussien réel et effet BD

### E.1 — Vrai flou gaussien (point 40) 🟡→✅

**État actuel :** le flou local est une moyenne 3×3 répétable, pas un noyau
gaussien — un flou gaussien à rayon réglable donne un résultat visuellement
différent (dégradé plus doux, pas de structure en croix).

- [ ] Ajouter `Adjustment::GaussianBlur { radius: f32 }` (ou étendre le
      filtre `Blur` existant avec un vrai noyau gaussien séparable
      horizontal/vertical — plus efficace qu'un noyau 2D direct).
- [ ] Rayon réglable (contrairement au « répéter la moyenne 3×3 » actuel qui
      n'a qu'un nombre d'itérations, pas un vrai rayon en pixels).
- [ ] Tests : comparer la sortie à une implémentation de référence connue
      (ex. vérifier que l'écart-type de la distribution de poids correspond
      au rayon demandé), et vérifier que le flou est isotrope (même flou
      horizontalement et verticalement) contrairement au flou de mouvement
      existant.

### E.2 — Effet artistique « BD / bande dessinée » (point 41) 🟡→✅

**État actuel :** Aquarelle, Huile, Croquis existent ; pas d'effet BD.

- [ ] Ajouter `Filter::ComicBook` : combinaison de contours marqués (à
      partir du même détecteur Sobel que `Sketch`,
      [filter.rs:28](src/tools/filter.rs:28)) + postérisation des couleurs
      (réduction du nombre de niveaux par canal) + éventuellement une trame
      de points (halftone) en option.
- [ ] Réutiliser le pipeline de filtre existant (`Filter` enum, preview,
      application — même schéma que `Sketch`/`OilPainting`/`Watercolor`
      autour de [filter.rs:26-77](src/tools/filter.rs:26)).
- [ ] Tests : vérifier la postérisation (nombre de couleurs distinctes dans
      la sortie inférieur à un seuil), vérifier que les contours Sobel sont
      bien renforcés (comme le test existant pour `Sketch`, si présent).

---

## Sprint F — Vectoriel : bibliothèque d'icônes

### F.1 — Icônes vectorielles (point 46) 🟡→✅

**État actuel :** les opérations booléennes (Union/Soustraction/
Intersection, `tools/boolean.rs`) existent déjà et fonctionnent. Il manque
une **bibliothèque d'icônes vectorielles** proprement dite — actuellement
`tools/assets.rs` ne propose que des pictogrammes/formes composées simples
(voir la fenêtre « Bibliothèque d'éléments », `ui/toolbar.rs`).

- [ ] Étendre `tools/assets.rs` (`Asset::ALL`) avec un jeu d'icônes
      vectorielles plus riche (flèches, formes UI courantes, pictogrammes
      météo/interface/réseaux sociaux…), toujours **embarquées dans le
      binaire** (pas de téléchargement réseau, cohérent avec le
      positionnement 100% local de l'app).
- [ ] Envisager l'import d'un set libre de droits existant (ex. Phosphor,
      déjà utilisé pour les icônes de la barre d'outils via
      `egui_phosphor` — voir `tool_glyph()` dans `ui/toolbar.rs`) plutôt que
      dessiner chaque icône à la main comme c'est fait aujourd'hui pour les
      `Asset` existants.
- [ ] UI : la fenêtre « Bibliothèque d'éléments » existante
      ([toolbar.rs:273-324](src/ui/toolbar.rs:273)) sert de base — ajouter
      une barre de recherche/catégories si le nombre d'icônes grossit
      significativement.
- [ ] Tests : vérifier que chaque nouvelle icône insérée produit un trait
      fermé éditable (comme les `Asset` actuels), pas une image figée.

---

## Résumé et ordre suggéré

| Sprint | Points couverts | Effort relatif | Dépendance externe |
|---|---|---|---|
| A | 7 | Faible (si décision rapide) | `libwebp` (à trancher) |
| B | 13, 14, 17 | Élevé (B.3 surtout) | Aucune |
| C | 29 | Moyen | Aucune |
| D | 32, 34, 35, 39 | Moyen (4 sous-items indépendants) | Aucune |
| E | 40, 41 | Faible à moyen | Aucune |
| F | 46 | Faible à moyen | Éventuellement `egui_phosphor` (déjà une dépendance) |

**Suggestion de priorisation** : D et E d'abord (aucune dépendance externe,
sous-tâches indépendantes, valeur utilisateur immédiate en retouche photo/
filtres) → C et F (améliorations ciblées d'outils déjà en place) → B (le
plus gros chantier, surtout B.3 objets intelligents, à cadrer précisément
avant de commencer) → A en dernier, une fois la décision sur `libwebp`
tranchée avec le porteur de projet.

---

## Résultat réel de l'implémentation (5 juillet 2026)

| Sprint | Statut | Constat |
|---|---|---|
| A (7) | ❌ Décision : pas de changement | Le porteur de projet a tranché le 5 juillet 2026 : **pas de dépendance `libwebp`**. Le WebP reste export sans perte uniquement (curseur de qualité réservé au JPEG) — même logique que le refus de HEIC/RAW (item 2/3), écart assumé plutôt qu'un oubli. |
| B.1 (14) | ✅ Fait | `Layer.locked` ajouté (`model/document.rs`), bloqué au point d'entrée unique `handle_canvas` via `layer_lock_blocks_tool()` (`app/mod.rs`) — Pan/Pipette/Règle restent autorisés. Icône cadenas dans `ui/layers.rs`. |
| B.2 (13) | ✅ Déjà fait, non modifié | En creusant le code, l'éditeur de masque de calque peint existait déjà en entier (`toggle_active_layer_mask`, `editing_mask`, boutons « Ajouter/Retirer le masque » + « Éditer le masque » dans `ui/layers.rs`) — fonctionnel sur **n'importe quel calque**, pas seulement après un détourage comme l'audit initial le laissait supposer. Aucun code à ajouter. |
| B.3 (17) | ✅ Complété (portée réduite, déjà documentée dans ce plan) | `ImageItem` découplait déjà `size` (affichage) de `w`/`h`/`rgba` (résolution source) — redimensionner ne rééchantillonne jamais la source, donc le « pas de perte au redimensionnement » existait déjà nativement. Ajouté : badge dans la liste des éléments du calque (`ui/layers.rs`) affichant la résolution source et signalant (⚠) quand l'affichage dépasse la résolution native (sur-échantillonné). |
| C (29) | ✅ Fait | `tools::bucket::refine_edges()` — variance de luminance locale pour repousser la couverture vers 0/255 dans les zones texturées (cheveux/fourrure). Curseur « Affiner les bords » dans les options de l'outil Détourage. |
| D.1 (32) | ✅ Fait | `Adjustment::Exposure { ev }` — gain multiplicatif en stops. |
| D.2 (34) | ✅ Fait | `Adjustment::Vibrance` (pondérée par `1 - saturation`) + `Adjustment::WhiteBalance { temp, tint }`. |
| D.3 (35) | ✅ Fait | `Adjustment::Denoise` — réutilise `smooth_skin` (lissage bilatéral déjà existant) sur toute l'image. |
| D.4 (39) | ✅ Fait | `PaintApp::canvas_histogram()` — histogramme du rendu composite entier quand aucune image n'est sélectionnée (au lieu du seul message d'invite). |
| E.1 (40) | ✅ Fait | `Adjustment::GaussianBlur` — noyau gaussien séparable, rayon continu (contrairement à `Filter::Blur`, moyenne de boîte répétée). |
| E.2 (41) | ✅ Déjà fait, non modifié | `Filter::Comic` (posterisation + contours Sobel) existait déjà et était exposé dans le menu Filtres — l'audit initial s'était trompé sur ce point, corrigé ici après relecture complète de `tools/filter.rs`. |
| F.1 (46) | ✅ Fait | 8 icônes ajoutées à `tools::assets::Asset` (flèches, éclair, repère, maison, engrenage, soleil, nuage) — même mécanisme d'insertion que les icônes existantes (trait plein éditable, pas une image figée). |

**Tests** : 211 tests passent (`cargo test`), 0 warning `cargo clippy
--all-targets`. Chaque nouvel ajustement/filtre a au moins un test d'identité
(paramètre neutre = no-op) et un test de comportement (l'effet fait bien ce
qui est annoncé).
