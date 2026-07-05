# sprint_next.md — Plan de sprints pour les écarts de audit_next.md

> Fait suite à [audit_next.md](audit_next.md) (audit du 5 juillet 2026,
> ~102 items, ~63 % implémenté). Ne couvre que les points 🟡/❌ jugés à
> traiter ; les items ✅ ne sont pas repris. Chaque sprint reste livrable et
> testable indépendamment.

Priorisation : (1) incohérences visibles pour l'utilisateur (avoir 4 modes
de sélection sans pouvoir les combiner), (2) effort/valeur, (3) dépendances
techniques ou décisions d'architecture à trancher avant de coder.

---

## Constat préalable important : le modèle de sélection

Avant de planifier les sprints Sélection, une clarification d'architecture
s'impose. `PaintApp::selection` est un `HashSet<u64>`
([app/mod.rs:263](src/app/mod.rs:263)) — la sélection porte sur des **ID
d'éléments** (traits, textes, images), pas sur un **masque de pixels**
façon Photoshop. `select_in_rect`/`select_in_ellipse`/`select_in_lasso`/
`magic_wand` ([app/mod.rs:991-1037](src/app/mod.rs:991)) choisissent quels
éléments sont sélectionnés (pour déplacer/transformer/supprimer), point.

Conséquence directe sur la checklist :
- **Soustraction/intersection/inversion de sélection** s'intègrent
  naturellement à ce modèle (ce sont des opérations d'ensemble sur des ID).
- **Contour progressif (feather) / dilater / contracter** sont des concepts
  de masque de pixels — ils n'ont de sens que si un pixel peut être « à
  moitié sélectionné ». Le modèle actuel n'a pas cette notion en dehors des
  flux ponctuels Détourage/Pot de peinture (masques jetables, pas persistés
  comme sélection réutilisable par tous les outils).

Le Sprint H ci-dessous propose donc de scinder le problème en deux : un
sprint pour les opérations d'ensemble (peu coûteux, cohérent avec
l'existant) et un sprint séparé, plus gros, pour un vrai masque de
sélection en pixels (avec une piste de réutilisation d'infrastructure
déjà existante — voir H.2).

---

## Sprint G — Sélection : opérations d'ensemble (peu coûteux)

### G.1 — Soustraction / intersection de sélection (point 61) ✅ FAIT

**État actuel :** seul l'ajout existe (`additive: bool`, Maj+glisser).

- [ ] Étendre `select_in_rect`/`select_in_ellipse`/`select_in_lasso`/
      `magic_wand` ([app/mod.rs:991-1037](src/app/mod.rs:991)) : remplacer
      le booléen `additive` par une petite enum `SelectionCombine { Replace,
      Add, Subtract, Intersect }`.
- [ ] Mapper les modificateurs clavier classiques : Maj = Add (déjà fait),
      Alt/⌥ = Subtract, Maj+Alt = Intersect — cohérent avec
      Photoshop/GIMP/Krita, donc pas de nouvel apprentissage pour
      l'utilisateur.
- [ ] `Intersect` : ne garder dans `self.selection` que les ID à la fois
      déjà sélectionnés et retenus par le nouveau geste.
- [ ] `Subtract` : retirer de `self.selection` les ID retenus par le nouveau
      geste, sans toucher au reste.
- [ ] Tests : reprendre les tests existants (`select_in_ellipse_keeps_center_
      drops_corner`, [app/mod.rs:5632](src/app/mod.rs:5632)) et ajouter un
      cas par mode (Add/Subtract/Intersect) avec un jeu d'éléments connu.

### G.2 — Inversion de sélection (point 64) ✅ FAIT

- [ ] Nouvelle méthode `invert_selection()` : `self.selection` devient
      l'ensemble complémentaire parmi tous les ID du calque actif
      (`active_elements_geom()` donne déjà la liste complète à parcourir).
- [ ] Raccourci clavier + entrée de menu (Édition, à côté de « Tout
      sélectionner » s'il existe déjà, sinon `ui/toolbar.rs` menu Édition).
- [ ] Test : sélectionner 2 éléments sur 5, inverser, vérifier que les 3
      autres (et seulement eux) sont sélectionnés.

### G.3 — Découpe automatique des bords vides / trim (point 68) ✅ FAIT

- [ ] Fonction pure `fn trim_bounds(rgba: &[u8], w: usize, h: usize) ->
      Option<(u32,u32,u32,u32)>` (rectangle minimal englobant les pixels
      d'alpha > 0), dans un module existant proche du recadrage (`app/mod.rs`
      ou nouveau `tools/trim.rs` si la logique grossit).
- [ ] Action menu « Rogner les bords vides » : calcule `trim_bounds` sur le
      rendu composite du document (réutiliser `render_for_export`/
      `Compositor::render_to_rgba` déjà utilisés pour l'histogramme canevas
      entier, voir [app/mod.rs](src/app/mod.rs) `canvas_histogram`), puis
      applique un recadrage à ce rectangle (réutilise le pipeline de
      recadrage existant).
- [ ] Cas limite : document entièrement transparent → ne rien faire, message
      d'info plutôt qu'un recadrage à 0×0.
- [ ] Test : image de test avec une bordure transparente connue, vérifier
      que `trim_bounds` renvoie exactement le rectangle attendu.

---

## Sprint H — Sélection : masque de pixels — ✅ FAIT (option 2 retenue, confirmée par le porteur de projet)

### H.1 — Décision prise

**Décision du porteur de projet (2026-07-05) : option 2, l'ajouter.**

Un vrai feather/dilater/contracter nécessitait un **masque de sélection en
pixels**, distinct du `HashSet<u64>` d'ID d'éléments. Fait : nouveau champ
`PaintApp::selection_mask: Option<RasterLayer>` (pas sur `Document` comme
suggéré initialement — placé au même endroit que `selection` elle-même,
qui vit déjà sur `PaintApp` et pas `Document` : une sélection est un état
d'édition éphémère, pas un contenu persisté, cohérence conservée).

### H.2 — Ce qui a été fait

- [x] `PaintApp::selection_mask: Option<RasterLayer>`, peuplé **directement
      depuis la géométrie du geste de sélection** (rectangle/ellipse/lasso)
      plutôt que dérivé des éléments déjà sélectionnés après coup — plus
      précis, réutilise `hit::point_in_ellipse`/`point_in_polygon` déjà
      existants. Baguette magique : approximation par union des boîtes
      englobantes des éléments retenus (documentée comme limite connue,
      la baguette n'a pas de silhouette de geste unique à rasteriser).
      Nouveau module [tools/selection_mask.rs](src/tools/selection_mask.rs) :
      `paint_mask_region` (combine Add/Subtract/Intersect/Replace comme
      `SelectionCombine`, même sémantique que pour les ID d'éléments),
      `feather` (flou boîte du canal de couverture), `dilate`/`erode`
      (filtre morphologique max/min sur un disque).
- [x] Les outils raster (`RasterLayer::stamp`/`stamp_custom`/
      `stroke_segment`, [model/raster.rs](src/model/raster.rs)) prennent
      désormais un `mask: Option<&RasterLayer>` qui multiplie leur
      couverture — intégré au Pinceau pixel, à la Gomme pixel et à
      l'Aérographe. **Non intégré** (périmètre volontairement limité) : Pot
      de peinture, tampon de clonage, calques de réglage « dans la
      sélection », densité +/-, éponge — à étendre plus tard si le besoin
      se confirme, chaque outil supplémentaire est un point d'intégration
      isolé (même paramètre `mask` à brancher).
- [x] UI : teinte semi-transparente sur les zones hors sélection (l'option
      « moins coûteuse » recommandée), pas de vraie animation de contour en
      pointillés (« fourmis en mouvement »). Menu Édition ▸ Masque de
      sélection ▸ Contour progressif…/Dilater…/Contracter…, un seul
      dialogue partagé (rayon en pixels, 1 à 60).
- [x] Tests : feather sur un rectangle net (dégradé confirmé sur le bord),
      dilater/contracter (décalage du bord dans le bon sens), les 4 modes
      de combine sur le masque, et un test d'intégration bout en bout
      (peindre au pinceau pixel à l'intérieur vs. à l'extérieur d'une
      sélection).

---

## Sprint I — Calques : remplissage, alignement, confort

### I.1 — Calque de remplissage (point 33) ✅ FAIT

- [ ] Nouveau variant sur le même modèle que les calques d'ajustement
      (`Layer::new_adjustment`, [model/document.rs:237](src/model/document.rs:237)) :
      `Layer::new_fill(id, name, fill: FillKind)` où `FillKind` = `Solid([u8;4])`
      / `Linear(Gradient)` / `Radial(Gradient)` / `Pattern(...)`. Réutilise le
      type `Gradient` déjà existant (`model::Gradient`, utilisé par les
      traits).
- [ ] Rendu : dans `Compositor::rebuild`
      ([render/compositor.rs:102](src/render/compositor.rs:102)), un calque
      de remplissage peint tout son rectangle (borné par le masque/écrêtage
      s'il y en a un) — cas le plus simple du pipeline existant, plus simple
      qu'un calque d'ajustement puisqu'il ne lit pas les calques du dessous.
- [ ] UI : bouton « Nouveau calque de remplissage » dans `ui/layers.rs`,
      à côté de « Nouveau calque de réglage » s'il existe une entrée
      similaire.
- [ ] Test : calque de remplissage uni rouge par-dessus un calque vide,
      vérifier que le composite est entièrement rouge (sous réserve
      d'opacité/masque par défaut).

### I.2 — Alignement et distribution de calques (point 36) ✅ FAIT

Distribution : `layer_multi_select: HashSet<u64>` (⇧/⌘+clic sur un nom de
calque dans `ui/layers.rs`, même pattern que la sélection multiple
d'éléments) + `PaintApp::distribute_layers(horizontal)` — au moins 3 calques
non vides, les deux extrêmes (par centre de boîte englobante du contenu
vectoriel du calque) restent fixes, les autres sont espacés uniformément
entre eux. Un seul pas d'annulation (`Command::SetDoc`, pas `MoveEach` — ce
dernier ne couvre qu'un seul calque à la fois, alors que la distribution
déplace plusieurs calques en une seule action).

<details><summary>Plan initial (pour mémoire)</summary>

- [ ] Différent de l'alignement d'éléments déjà existant (traits/textes/
      images sélectionnés) : ici, aligner le **contenu entier** d'un calque
      par rapport au document ou à un autre calque (centrer horizontalement/
      verticalement, distribuer un espacement égal entre plusieurs calques
      sélectionnés).
- [ ] Réutiliser le calcul de boîte englobante déjà fait pour l'alignement
      d'éléments (`hit::bbox_intersects` et voisins) mais appliqué à
      l'ensemble `layer.strokes ∪ layer.texts ∪ layer.images` plutôt qu'à
      la sélection courante.
- [ ] UI : dans le menu déjà existant « Aligner » (voir `ui/toolbar.rs`,
      corrigé récemment pour les icônes de menu), ajouter un sous-menu
      « calques » à côté de celui des éléments.

</details>

### I.3 — Vignettes de prévisualisation (point 37) ✅ FAIT

- [ ] Miniature basse résolution (ex. 32×32 ou 48×48) par calque, calculée
      à partir du même chemin que le compositeur par-calque
      (`Compositor` a déjà un cache par calque : `layers: HashMap<u64,
      (u64, Pixmap)>`, [render/compositor.rs:50](src/render/compositor.rs:50))
      — redimensionner ce pixmap déjà calculé plutôt que refaire un rendu
      dédié.
- [ ] UI : remplacer/compléter le texte « (N traits) » actuel
      ([ui/layers.rs:159](src/ui/layers.rs:159)) par une petite image à
      gauche du nom du calque.
- [ ] Perf : ne recalculer la miniature que quand le hash du calque change
      (même logique d'invalidation que le compositeur principal), pas à
      chaque frame.

### I.4 — Recherche/filtre dans la liste des calques (point 38) ✅ FAIT

- [ ] Champ de recherche texte au-dessus de la liste de calques
      (`ui/layers.rs`), filtre simple sur `layer.name` (insensible à la
      casse) — n'affiche que les calques dont le nom correspond.
- [ ] Complément naturel du renommage déjà existant ; utile seulement à
      partir d'un nombre significatif de calques (à activer/dévoiler
      seulement au-delà d'un seuil, ex. 8-10 calques, pour ne pas alourdir
      l'UI des petits documents).

### I.5 — Code couleur des calques (point 30) ✅ FAIT

- [ ] `pub color_tag: Option<[u8; 3]>` sur `Layer` (comme `locked` ajouté
      récemment, [model/document.rs](src/model/document.rs)) — étiquette
      visuelle uniquement, aucun effet sur le rendu.
- [ ] UI : petit carré de couleur cliquable dans la ligne du calque
      (`ui/layers.rs`), palette de 6-8 couleurs prédéfinies façon
      Photoshop/Figma plutôt qu'un sélecteur de couleur complet (plus rapide
      à l'usage).

### I.6 — Verrouillage granulaire (point 28) ✅ FAIT

Plutôt que remplacer `layer.locked: bool` (verrou global, inchangé — reste le
gate le plus large, testé par `layer_lock_blocks_tool()`), deux champs
indépendants et cumulables ont été ajoutés à `Layer` :

- `lock_position: bool` — bloque spécifiquement `PaintApp::push_move` (le
  glisser-déplacer d'éléments sélectionnés), sans bloquer la peinture ni
  l'édition de contenu. Périmètre volontairement limité au glisser sur le
  canevas (pas `align()`/`reorder()`/`distribute_layers()`).
- `lock_alpha: bool` — protège la transparence existante du contenu peint :
  dans `commit_raster_stroke` ([app/mod.rs](src/app/mod.rs)), avant de
  calculer l'« après » pour l'historique, l'alpha des tuiles réellement
  touchées par le geste est restauré à sa valeur d'avant-geste (la couleur
  du nouveau tampon est conservée) — peindre ne peut plus rendre opaque un
  pixel transparent, ni la gomme en rendre un transparent, mais la couleur
  des pixels déjà opaques reste modifiable. Ne s'applique qu'au contenu, pas
  au masque de calque peint (pas de notion de « transparence » comparable
  là). Choix délibéré : corriger au point de commit plutôt que threader un
  paramètre `lock_alpha` à travers `RasterLayer::stamp`/`stamp_custom`/
  `stroke_segment` (et casser une quinzaine d'appels de tests existants) —
  le snapshot « avant » déjà capturé pour l'undo par tuile
  (`self.raster_touch`) est exactement l'état à restaurer.

UI : deux cases à cocher dans le panneau « Calque actif »
([ui/layers.rs](src/ui/layers.rs)), plus une icône discrète dans la ligne du
calque quand l'un des deux est actif (pas une 3e icône permanente à côté du
verrou global, cas d'usage plus rare).

---

## Sprint J — Dessin : outils manquants

### J.1 — Aérographe (point 44) ✅ FAIT

- [ ] Nouveau `ActiveTool::Airbrush` (`tools/mod.rs`) : contrairement au
      Pinceau pixel qui dépose une seule fois par mouvement, l'aérographe
      dépose en continu **tant que le clic est maintenu**, même sans
      déplacement — nécessite un minuteur par frame (accumulateur de temps
      écoulé) plutôt qu'un dépôt uniquement sur `drag_started`/`dragged`.
- [ ] Réutilise `RasterLayer::stamp()` ([model/raster.rs:123](src/model/raster.rs:123))
      à intervalles réguliers (ex. toutes les 30ms) tant que le bouton est
      pressé, avec une opacité par dépôt plus faible qu'un pinceau normal
      (l'accumulation progressive est l'effet recherché).
- [ ] Test : simuler un maintien de N frames sans déplacement, vérifier que
      l'alpha du pixel central augmente de façon monotone.

### J.2 — Import de brosse depuis une image (point 50) ✅ FAIT

- [ ] Charger une image en niveaux de gris (luminance = dureté/couverture),
      la stocker comme tampon de brosse personnalisé — étendre
      `BrushPreset` ([model/stroke.rs](src/model/stroke.rs)) avec une
      variante `Stamp { rgba: Vec<u8>, w: u32, h: u32 }` en plus des
      préréglages paramétriques actuels (Feutre, Crayon fin, etc.).
- [ ] Le Pinceau pixel échantillonne alors ce tampon (avec mise à l'échelle
      selon `brush.width`) au lieu de la formule de couverture circulaire
      actuelle (`RasterLayer::stamp`, formule `cov` basée sur la distance).
- [ ] UI : bouton « Importer une image comme brosse » dans la bibliothèque
      de brosses existante.

### J.3 — Prévisualisation du contour de brosse (point 56) ✅ FAIT

- [ ] Dessiner un cercle (rayon = `brush.width`/2, converti en pixels écran
      via `ViewTransform::doc_to_screen`) centré sur le curseur pendant que
      Pinceau/Gomme/Pinceau pixel/Gomme pixel sont actifs — un simple
      `painter.circle_stroke()` dans `handle_canvas`
      ([app/mod.rs:4120](src/app/mod.rs:4120)), aucune nouvelle donnée de
      modèle nécessaire.
- [ ] Couleur du contour contrastée (ex. inversion ou gris 50 % avec léger
      contour blanc) pour rester visible sur fond clair et sombre.

### J.4 — Outil crayon dédié (point 40) ✅ FAIT

`ActiveTool::Pencil` ajouté, mais délibérément **pas** un second moteur de
dessin : `as_shape()` renvoie `None` pour Pencil comme pour Brush, donc son
geste tombe dans la même branche « trait à main levée » de
`PaintApp::handle_draw` sans aucun cas spécial. La seule différence tient à
la sélection du bouton dans la barre d'outils (`ui/toolbar.rs::tool_button`),
qui applique automatiquement le préréglage « Crayon fin » déjà existant. Un
bouton dédié, plus visible qu'un préréglage caché dans un menu, pour un coût
d'implémentation quasi nul (3 matches exhaustifs à compléter : icône,
couleur d'accent, libellé du footer).

---

## Sprint K — Filtres & effets créatifs

### K.1 — Pixelisation / mosaïque (point 86) ✅ FAIT

- [ ] `Filter::Pixelate` (`tools/filter.rs`, même position que les autres
      presets discrets) : moyenne chaque bloc `n×n` et remplace tous ses
      pixels par cette moyenne. Taille de bloc comme paramètre continu →
      plutôt un `Adjustment::Pixelate { block: f32 }` pour bénéficier du
      curseur en direct (cohérent avec le choix fait pour Exposition/
      Vibrance/etc. lors du sprint précédent).
- [ ] Test : bloc de taille 4 sur un damier fin, vérifier que les 16 pixels
      d'un bloc sont bien identiques après application.

### K.2 — Halftone / trame (point 92) ✅ FAIT

- [ ] `Adjustment::Halftone { cell: f32, angle: f32 }` : convertit en
      luminance, puis pour chaque cellule de la grille (taille `cell`,
      tournée de `angle`) dessine un disque dont le rayon est proportionnel
      à l'obscurité de la cellule — algorithme classique, à réaliser en
      niveaux de gris d'abord (extension couleur CMJN séparée = hors
      scope initial).
- [ ] Test : cellule entièrement noire → disque de rayon maximal (couvre
      toute la cellule) ; cellule blanche → rayon nul.

### K.3 — Distorsions vague / sphère / tourbillon (point 93) ✅ FAIT

- [ ] Trois nouveaux `Adjustment` sur le modèle de `Distortion`/`ArcWarp`
      déjà existants (échantillonnage inverse, voir
      [tools/filter.rs:392](src/tools/filter.rs:392) `distort_radial`
      comme référence directe pour Sphère qui n'en est qu'une variante de
      formule) :
  - `Wave { amplitude: f32, wavelength: f32 }` — décale chaque colonne
    verticalement selon un sinus de longueur d'onde réglable (généralise
    `arc_warp`, qui n'a qu'une seule demi-période fixe).
  - `Sphere { amount: f32 }` — même principe que `distort_radial` mais avec
    une formule de projection sphérique (facteur non-linéaire en fonction
    de la distance au centre, plus prononcé que le simple barrel actuel).
  - `Vortex { angle: f32 }` — rotation de l'échantillonnage proportionnelle
    à la distance au centre (angle max au centre, nul en bordure).
- [ ] Tests : chaque distorsion à paramètre neutre = no-op (même pattern que
      tous les `Adjustment` existants), plus un test de déplacement visible
      à paramètre non neutre.

### K.4 — Flou radial / zoom (point 85) ✅ FAIT

- [ ] `Adjustment::RadialBlur { center: (f32,f32), amount: f32 }` — moyenne
      le long de rayons partant du centre (effet vitesse/explosion),
      complète `MotionBlur` (directionnel) déjà existant. Implémentation
      proche de `motion_blur`
      ([tools/filter.rs:532](src/tools/filter.rs:532)) mais la direction
      d'échantillonnage dépend de la position du pixel par rapport au
      centre plutôt que d'être constante partout.

### K.5 — Vignette artistique indépendante (point 90) ✅ FAIT

- [ ] Extraire la logique de vignettage déjà écrite dans `vintage()`
      ([tools/filter.rs:827](src/tools/filter.rs:827)) en une fonction
      séparée `apply_vignette(rgba, w, h, amount)`, exposée comme son
      propre `Adjustment::Vignette { amount: f32 }` — réutilisation directe
      de code déjà testé, pas une nouvelle formule à inventer.

### K.6 — Détection de contours Canny (point 87) ✅ FAIT

Ajouté comme nouveau preset `Filter::Canny` (« Contours (Canny) »), en plus
de Sobel (`sobel_magnitude`, toujours utilisé par Croquis/BD, inchangé) :
lissage boîte 3×3 (réduction de bruit), gradients Sobel avec magnitude **et**
direction (`canny_edges`, [tools/filter.rs](src/tools/filter.rs)),
suppression non maximale le long de la direction du gradient (arrondie à
0°/45°/90°/135°), puis double seuil + hystérésis en 8-connexité (un pixel
« faible » n'est retenu que s'il est connecté à un pixel « fort ») — élimine
les faux positifs de bord épais qu'un simple seuil sur la magnitude Sobel
produirait. Même convention visuelle que Croquis : traits noirs sur fond
blanc.

### K.7 — Mixeur de canaux pour le N&B (point 76) ✅ FAIT

- [ ] `Adjustment::ChannelMixerBw { r: f32, g: f32, b: f32 }` (poids
      normalisés, somme = 1 par défaut comme `Filter::Grayscale` actuel
      0.299/0.587/0.114) — remplace la formule fixe de `luma()`
      ([tools/filter.rs:848](src/tools/filter.rs:848)) par des poids
      réglables par l'utilisateur, façon « N&B personnalisé » Lightroom.

### K.8 — Auto-correction en un clic (point 83) ✅ FAIT

- [ ] `Adjustment::AutoLevels` calculé une fois au clic (pas un ajustement
      à paramètres continus) : étire l'histogramme de chaque canal pour que
      son 1er/99e centile touchent 0/255 — réutilise `histogram_rgb()`
      ([tools/filter.rs:616](src/tools/filter.rs:616)) déjà existant pour
      calculer les bornes, puis applique une fonction `levels()`-like
      calculée automatiquement plutôt que réglée à la main.
- [ ] UI : bouton « Correction automatique », pas un calque de réglage
      interactif — applique directement et pousse une entrée d'historique
      classique (Command existant, pas de nouveau type de calque).

---

## Sprint L — Formats & export

### L.1 — Export d'une zone sélectionnée uniquement (point 14) ✅ FAIT

- [ ] Étendre `render_for_export`
      ([app/mod.rs:3089](src/app/mod.rs:3089)) pour accepter une région
      optionnelle (bornes en pixels document) ; si des éléments sont
      sélectionnés (`self.selection` non vide), proposer d'exporter leur
      boîte englobante combinée au lieu du document entier.
- [ ] UI : case à cocher « Exporter uniquement la sélection » dans le
      dialogue d'export existant, désactivée/grisée si `selection` est vide.

### L.2 — Aperçu et poids estimé avant export (point 15) ✅ FAIT

- [ ] Avant l'écriture sur disque, encoder en mémoire
      (`encode_to`-like mais vers un `Vec<u8>` plutôt qu'un fichier) pour
      afficher la taille résultante estimée — coût : un encodage
      supplémentaire, acceptable pour un export (pas une opération par
      frame).
- [ ] UI : aperçu miniature (le rendu déjà en mémoire, `ColorImage`) +
      libellé « ≈ N Ko/Mo » avant de confirmer l'export.

### L.3 — Suppression optionnelle des métadonnées (point 17) ✅ VÉRIFIÉ (déjà satisfait par construction, rien à coder)

- [ ] Vérifier d'abord si la crate `image` écrit des métadonnées EXIF par
      défaut à l'export (probablement non, à confirmer) — si c'est déjà le
      cas, cet item est déjà satisfait par construction et il suffit de le
      documenter dans `audit_next.md`/README plutôt que de coder quoi que
      ce soit.
- [ ] Si des métadonnées sont bien écrites (ex. import PSD conservant des
      infos), ajouter une case à cocher « Supprimer les métadonnées » qui
      garantit un ré-encodage propre sans les champs optionnels.

### L.4 — Glisser-déposer de fichiers (point 18) ✅ FAIT

- [ ] `egui::Event::Dropped` (déjà géré nativement par `eframe`/`winit`) —
      écouter cet évènement dans la boucle principale (`app/mod.rs`,
      probablement près de `handle_screenshot`/gestion d'évènements
      existante) et appeler `import_image()`/`open_project()` selon
      l'extension du fichier déposé.
- [ ] Étendre à un `.psd` déposé → `import_psd()`, un `.json` du format
      natif → `open_project()`.

### L.5 — Import SVG (point 3) — ✅ FAIT (vectoriel éditable, décision confirmée par le porteur de projet)

**Décision du porteur de projet (2026-07-05) : import vectoriel éditable**
(pas le rasterisé, plus simple). Non traité dans cette session — chantier
prévu à part, voir ci-dessous.

- [ ] Traduire les primitives SVG (path, rect, circle, ellipse, texte,
      groupes/transforms) vers les `Stroke`/`TextItem` internes — un vrai
      travail de conversion géométrique (courbes de Bézier SVG → `Stroke`
      points, `transform="matrix(...)"` à propager aux enfants, `<text>`
      → `TextItem`), pas un simple ajout de dépendance decode comme pour
      BMP/TIFF.
- [ ] Portée à clarifier avant de coder : sous-ensemble SVG couvert (path/
      rect/circle/ellipse/text/g au minimum ; `<use>`, `<clipPath>`,
      filtres SVG hors scope raisonnable) et bibliothèque de parsing SVG à
      utiliser (`usvg` fournit déjà un arbre normalisé avec transforms
      résolus — probablement le point de départ le plus sûr, réutilise
      un parseur mûr plutôt que d'en écrire un).
- [ ] Tests : un SVG de référence simple (quelques path/rect/text connus)
      → vérifier que les `Stroke`/`TextItem` produits ont les bonnes
      coordonnées/couleurs.

### L.6 — Export GIF, statique et animé (point 9) — ✅ FAIT (statique et animé)

**Décision du porteur de projet (2026-07-05) : traiter l'animé, pas
seulement le statique.** Les deux sont faits.

- [x] GIF statique : `ExportFormat::Gif` ajouté (`export.rs`), encodé via
      `image::ImageFormat::Gif`. A révélé et corrigé au passage une
      régression latente : la feature `gif` de la crate `image` n'était
      **pas** activée dans `Cargo.toml` malgré le filtre de fichiers de
      `import_image_dialog` l'annonçant déjà — l'import GIF, précédemment
      marqué ✅ dans l'audit, ne décodait en réalité aucun fichier GIF.
      Corrigé (feature ajoutée) et couvert par un test de régression
      (`project::tests::import_image_from_path_decodes_gif`).
- [x] GIF **animé** : modèle retenu = un clone complet de la pile de
      calques par frame (`model::AnimationFrame { layers: Vec<Layer>,
      delay_ms: u32 }`, champ `Document::frames: Vec<AnimationFrame>` +
      `Document::active_frame: usize`) — le plus simple des deux options
      évoquées, cohérent avec le reste de l'app (pas de nouveau système
      d'undo : chaque opération de frame passe par `push_doc_snapshot` /
      `Command::SetDoc`, donc annulable comme un redimensionnement de
      document). `frames` vide = document statique, comportement
      historique inchangé (aucun effet sur les documents existants).
  - Nouveau sous-module `app/animation.rs` : `add_animation_frame`
    (duplique la frame active), `switch_animation_frame` (sauvegarde la
    frame quittée avant de charger la nouvelle), `remove_animation_frame`
    (refuse de vider la dernière frame), `move_animation_frame` (voisin
    immédiat, pas de glisser-déposer dans cette version), `set_frame_delay`.
  - UI : panneau « Animation » (`ui/toolbar.rs`, `animation_panel_window`)
    — liste des frames avec délai réglable, boutons monter/descendre/
    supprimer, pas de scrubber/onion skinning (suffisant pour l'usage visé,
    petites boucles simples).
  - Export : `PaintApp::export_animated_gif` rend chaque frame séparément
    via le compositeur existant (`render_to_rgba` sur un `Document`
    temporaire par frame), encode avec `image::codecs::gif::GifEncoder`
    (`export::save_animated_gif`/`encode_animated_gif`, boucle infinie par
    défaut). Testé par un vrai aller-retour encodage → décodage vérifiant
    le nombre de frames et les délais.
  - Correctifs trouvés en cours de route : `encode_all_images`/
    `apply_loaded` ne parcouraient que la frame active, pas `doc.frames` —
    sans correction, sauvegarder/rouvrir un projet animé aurait
    silencieusement vidé le raster/masque des frames non actives. Corrigé,
    couvert par un test de régression dédié
    (`saving_and_reloading_a_project_keeps_raster_content_on_inactive_frames`).

### L.7 — Export PDF vectoriel (point 11) ✅ FAIT (décision : oui, l'ajouter — confirmé par le porteur de projet)

- [ ] Le PDF est aujourd'hui toujours rasterisé (JPEG encapsulé,
      [export.rs:122](src/export.rs:122)) — un PDF vectoriel demanderait de
      réutiliser le même pipeline que l'export SVG (`svg.rs`) mais en
      écrivant des opérateurs de dessin PDF plutôt que des balises SVG.
      Valeur surtout pour les documents à dominante vectorielle (formes,
      texte) sans image bitmap — à évaluer si les utilisateurs du projet
      exportent souvent ce type de document en PDF.

### L.8 — Profils d'export nommés (point 16) ✅ FAIT

- [ ] Regrouper format + qualité + tailles du batch export en un preset
      nommé, persisté comme les préréglages de pinceau/style existants
      (même mécanisme que `style_presets`/`brush_presets`,
      [app/mod.rs](src/app/mod.rs)) — pas une nouvelle infrastructure de
      persistance, réutilisation directe du pattern déjà en place trois
      fois dans le code.

---

## Sprint M — Texte, vectoriel, couleur

### M.1 — Extraction de palette depuis l'image (point 98) ✅ FAIT

- [ ] Quantification simple (ex. k-moyennes sur les pixels d'une image
      sélectionnée, ou histogramme 3D grossier + pics les plus fréquents)
      pour extraire 5-8 couleurs dominantes, ajoutées en un clic à la
      palette personnalisée déjà existante (`app.palette`,
      voir `ui/toolbar.rs` swatches).
- [ ] Test : image de test à 2 couleurs dominantes connues (ex. damier
      rouge/bleu), vérifier que l'extraction renvoie bien ces deux couleurs
      en tête.

### M.2 — Inclinaison / cisaillement (skew) (point 99) ✅ FAIT

- [ ] Absent du code (vérifié : aucune fonction `shear`/`skew` trouvée).
      Ajouter une transformation affine de cisaillement sur les éléments
      sélectionnés, sur le même modèle que la rotation existante (poignée
      dédiée sur la boîte de sélection, glisser = angle de cisaillement
      horizontal/vertical).
- [ ] S'applique aux `Stroke`/`ImageItem`/`TextItem` comme les autres
      transformations (rotation, échelle) déjà supportées — vérifier le
      point d'entrée commun (probablement `tools/transform.rs` ou la
      logique de poignées dans `app/mod.rs`) pour brancher au même endroit.
- [ ] Test : cisaillement horizontal d'un rectangle, vérifier que les
      coins hauts et bas se décalent en sens opposé du montant attendu.

---

## Sprint N — Moteur de rendu (décision d'architecture, pas un simple sprint)

### N.1 — Rendu GPU via `wgpu` (point 100) ❌ (décision majeure à prendre)

- [ ] **Ne pas coder sans décision explicite du porteur de projet.** C'est
      un changement de backend eframe (`glow` → `wgpu`), pas une
      fonctionnalité additive : impact sur le packaging, la compatibilité
      des drivers graphiques, et potentiellement des régressions visuelles
      à ré-valider entièrement (comme le mipmapping des textures traité
      dans une session précédente).
- [ ] Le compositeur photo (`tiny-skia`, calques/filtres/texte rastérisé)
      restera **CPU quoi qu'il arrive** — c'est un choix architectural
      documenté (voir ARCHITECTURE.md), pas la partie concernée par ce
      point. Seul le rendu de l'interface egui elle-même basculerait de
      backend.
- [ ] Si le porteur de projet confirme vouloir `wgpu` (ex. pour de
      meilleures performances sur certains Mac, ou uniformité avec d'autres
      projets egui) : changer `eframe = { version = "0.29", features =
      ["accesskit"] }` en ajoutant `features = ["wgpu"]` et retirer/adapter
      le backend par défaut dans `NativeOptions` (`main.rs`) — tester
      d'abord sur une branche séparée, cette bascule peut casser des
      détails de rendu subtils (anti-aliasing, gestion des textures) qui ne
      se voient qu'à l'usage.

---

## Résumé et ordre suggéré

| Sprint | Points couverts | Effort relatif | Statut |
|---|---|---|---|
| G | 61, 64, 68 | Faible | ✅ Fait |
| H | 62, 63 | Élevé | ✅ Fait (décision : option 2, ajouter le masque) |
| I | 28, 30, 33, 36, 37, 38 | Moyen (6 sous-items indépendants) | ✅ Fait intégralement (28/36 complétés dans une session dédiée aux 4 derniers points optionnels) |
| J | 40, 44, 50, 56 | Moyen | ✅ Fait intégralement (40 complété dans la même session) |
| K | 76, 83, 85, 86, 87, 90, 92, 93 | Moyen à élevé (8 sous-items indépendants) | ✅ Fait intégralement (87/K.6 complété dans la même session) |
| L | 3, 9, 11, 14, 15, 16, 17, 18 | Variable | ✅ Fait intégralement, y compris L.5 (SVG), L.6 (GIF animé), L.7 (PDF vectoriel) |
| M | 98, 99 | Faible à moyen | ✅ Fait |
| N | 100 | Élevé, architecture | Non traité — décision majeure à prendre |

**Suggestion de priorisation** :
1. **G** d'abord (aucune dépendance, incohérence UX visible immédiatement
   corrigée — avoir 4 modes de sélection sans pouvoir les combiner est le
   constat le plus net de l'audit).
2. **K** ensuite (valeur utilisateur immédiate en filtres créatifs, aucune
   décision bloquante, beaucoup de réutilisation de code déjà écrit —
   `distort_radial`, `motion_blur`, `vintage` servent de base à plusieurs
   sous-tâches).
3. **I** et **J** en parallèle si deux personnes disponibles (calques vs.
   dessin, aucune dépendance croisée).
4. **M** (petit, isolé).
5. **L** en dernier parmi les sprints « sans grosse décision », en excluant
   L.5/L.6/L.7 tant que non tranchés avec le porteur de projet.
6. **H** et **N** seulement après une clarification produit explicite — ce
   sont les deux seuls sprints qui changent une décision d'architecture
   déjà assumée (sélection = objets, rendu UI = glow), pas juste des
   fonctionnalités qui manquaient.
