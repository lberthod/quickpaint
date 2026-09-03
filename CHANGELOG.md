# Changelog — QuickPaint

Versions alignées sur les sprints. Détail complet : le journal git.

## 0.20.0 — juillet 2026 (Sprints O à U — 18 des 19 fonctionnalités restantes)

Version jamais taguée au moment de sa livraison (20 juillet 2026) —
régularisée ici a posteriori ; le contenu vivait jusque-là dans
`sprint_fonctionnalites.md`, retiré une fois migré (même traitement que
les versions 0.16.0-0.18.0, voir plus bas dans ce fichier).

- **Sprint O** (transformations/sélection) : retourner horizontal/vertical
  (`Document::flip_content`, document entier plutôt que sélection —
  glyphes de texte non inversés, limite documentée), lasso polygonal,
  symétrie miroir axial (radial/horizontal/vertical/les deux), fourmis en
  marche animées sur le contour de sélection (marching squares).
- **Sprint P** (calques/compositing) : fusion de calques qui compose
  vraiment le raster peint (opacité/masque du calque source cuits dans le
  résultat — il était silencieusement perdu avant), modes de fusion 6 → 12
  (Lumière tamisée/crue, Différence, Exclusion, Densité couleur ± ),
  mapping PSD étendu en conséquence.
- **Sprint Q** (texte) : italique (vraie fonte système si disponible, sinon
  repli romain), interlignage et crénage réglables (mêmes valeurs dans le
  rendu live et le compositeur).
- **Sprint R** (UI/navigation macOS) : mode sombre (suit la préférence
  système via `ThemePreference`), guides manuels tirés depuis les règles,
  rotation du canevas (affichage seulement — désactive règles/pot de
  peinture/détourage hors 0°), raccourcis ⌘ personnalisables. Gestes
  multi-touch : trackpad déjà couvert (pinch/pan) ; écran tactile natif
  non engagé, décision produit en attente.
- **Sprint S** (ajustements) : courbes libres par canal (spline monotone
  Fritsch–Carlson, jusqu'à 16 points), l'ancienne courbe à 3 points reste
  lisible sur les projets existants.
- **Sprint T** (import/export/impression) : impression ⌘P via PDF
  vectoriel ouvert dans Aperçu (vrai dialogue macOS) ; export vidéo en
  APNG plutôt que MP4 (aucune dépendance système, décision produit).
  Non-support documenté et assumé : export PSD, import `.abr`, WebP
  lossy (dépendance `libwebp` refusée) — voir le README.
- **Sprint U** (animation) : pelure d'oignon (onion skin) sur les frames
  voisines, teintes rouge/verte à l'origine (passées à orange/bleu plus
  tard pour l'accessibilité daltonisme, voir `audit_uix_expert.md`).

325 tests au total, 0 warning clippy. Bilan : 18 des 19 items visés
livrés — le seul non-engagement (gestes écran tactile) est une décision
produit, pas un reste à faire.

## 0.19.0 — juillet 2026 (les 4 derniers points optionnels de l'audit)

299 tests au total, 0 warning clippy.

- **Distribution multi-calque** (point 36) : sélection multi-calque dans le
  panneau (⇧/⌘+clic sur un nom de calque) + répartition à espacement égal
  (horizontal/vertical), les deux calques extrêmes restent fixes.
- **Détection de contours Canny** (point 87) : nouveau preset « Contours
  (Canny) », en plus de Sobel (toujours utilisé par Croquis/BD) — lissage,
  gradients avec direction, suppression non maximale, double seuil +
  hystérésis.
- **Outil Crayon dédié** (point 40) : bouton dans la barre d'outils, dessine
  comme le Pinceau, applique automatiquement le préréglage « Crayon fin ».
- **Verrouillage granulaire de calque** (point 28) : `lock_position`
  (bloque le glisser-déplacer, pas la peinture) et `lock_alpha` (peindre/
  gommer ne peut plus rendre un pixel opaque/transparent), indépendants du
  verrou global existant et cumulables avec lui.
- Sélection multiple d'éléments dans la liste « Éléments du calque » (⇧/⌘+
  clic), avec deux nouvelles actions : fusionner en image, nouveau calque
  à partir de la sélection.
- Fix : le glisser-déposer pour réordonner les calques capturait le clic
  avant les boutons œil/cadenas/etc. (seule la poignée déclenche le glisser
  désormais).

## 0.18.0 — juillet 2026 (Sprint H : masque de sélection en pixels)

- **Masque de sélection en pixels** (`PaintApp::selection_mask`, en plus du
  jeu d'ID d'éléments existant) : nécessaire pour les cas où un pixel peut
  être « à moitié sélectionné », impossible avec le modèle d'ID seul.
  Nouveau module `tools/selection_mask.rs` — peuplé depuis la géométrie du
  geste (rectangle/ellipse/lasso pixel-précis, baguette magique par union
  des boîtes englobantes) ; combine Add/Subtract/Intersect/Replace comme
  la sélection par ID.
- Contour progressif (feather), dilater/contracter (filtre morphologique
  max/min sur un disque) via le menu Édition.
- Pinceau pixel, Gomme pixel et Aérographe respectent désormais le masque
  de sélection (pas le pot de peinture, le tampon de clonage ni les autres
  outils raster — périmètre volontairement limité).
- 91 ✅ / 10 🟡 / 1 ❌ sur les 102 items de `audit_next.md` — seul le rendu
  GPU `wgpu` (Sprint N) reste comme décision d'architecture non tranchée.

## 0.17.0 — juillet 2026 (Sprint L.6 : export GIF statique et animé)

- **Export GIF statique** (`ExportFormat::Gif`). Corrige au passage une
  régression latente : la feature `gif` de la crate `image` n'était pas
  activée dans `Cargo.toml`, donc l'import GIF (déjà annoncé fonctionnel
  dans un audit précédent) ne décodait en réalité aucun fichier.
- **Animation** : nouveau modèle `Document::frames: Vec<AnimationFrame>`,
  chaque frame un instantané complet de la pile de calques (le choix le
  plus simple face à une timeline de keyframes par calque). Vide par
  défaut = comportement historique inchangé.
- Nouveau sous-module `app/animation.rs` (ajout/suppression/
  réordonnancement de frames, délai par frame, tout annulable via
  `Command::SetDoc`) et panneau « Animation » (`ui/toolbar.rs`) avec
  export en GIF animé (`image::codecs::gif::GifEncoder`).
- Correctif trouvé en cours de route : la sauvegarde/l'ouverture de projet
  ne parcourait que la frame active, pas les autres — un projet animé
  rouvert aurait silencieusement perdu le raster/masque des frames
  inactives.
- 89 ✅ / 10 🟡 / 3 ❌ sur 102 items — Sprint L intégralement traité.

## 0.16.0 — juillet 2026 (Sprints G, K, I, J, M, L)

- **Sprint G** (sélection) : soustraction/intersection (Alt/Maj+Alt),
  inversion (⌘⇧I), rognage des bords vides.
- **Sprint K** (filtres) : pixelisation, halftone, distorsions vague/
  sphère/tourbillon, flou radial, vignette autonome, mixeur de canaux N&B,
  auto-correction (auto-niveaux).
- **Sprint I** (calques) : calque de remplissage (uni/dégradé), code
  couleur, vignettes de prévisualisation, recherche/filtre, alignement du
  contenu d'un calque par rapport au document.
- **Sprint J** (dessin) : aérographe, import de brosse depuis une image,
  aperçu de contour étendu au pinceau/gomme pixel.
- **Sprint M** (couleur/transformations) : extraction de palette
  dominante depuis une image, cisaillement (skew) via poignées dédiées.
- **Sprint L** (export/import) : export d'une zone sélectionnée, aperçu +
  poids estimé, profils d'export nommés, glisser-déposer de fichiers,
  import SVG vectoriel éditable (nouveau module `svg_import.rs` via
  `usvg`), export PDF vectoriel (nouveau module `pdf_vector.rs`), export
  GIF statique — corrige au passage la même régression `gif`/`image`
  mentionnée en 0.17.0. GIF animé volontairement pas traité ici (nécessite
  d'abord un modèle de frames, livré séparément — voir 0.17.0).
- 88 ✅ / 11 🟡 / 3 ❌ sur 102 items.

## 0.15.0 — juillet 2026 (audit fonctionnel de suivi — Sprints A à F)

Audit et plan de sprints (`audit_newxxx.md`/`audit_sprint_xx.md`) retirés une
fois acté — voir [audit_next.md](audit_next.md) pour l'audit suivant.
211 tests au total (+15), 0 warning clippy.

- **Sprint D — Retouche photo** : calques de réglage **Exposition** (gain en
  stops), **Vibrance** (pondérée par la saturation existante, épargne les
  couleurs déjà vives) et **Balance des blancs** (température/teinte) ;
  **réduction de bruit** (réutilise le lissage bilatéral existant) ;
  **histogramme étendu au canevas entier** quand aucune image n'est
  sélectionnée (auparavant limité à une image du calque actif).
- **Sprint E — Filtres** : vrai **flou gaussien** (noyau séparable, rayon
  continu) en complément du flou de boîte existant. L'effet « Bande
  dessinée » s'est révélé déjà présent à l'audit (posterisation + contours
  Sobel) — aucun changement nécessaire.
- **Sprint C — Détourage** : option **« Affiner les bords »** — repousse la
  couverture du masque vers 0/255 dans les zones à forte variance de
  luminance locale (mèches de cheveux, fourrure), au lieu du dégradé uniforme
  générique de `soft_edge` seul.
- **Sprint F — Bibliothèque d'éléments** : 8 icônes vectorielles
  supplémentaires (flèches, éclair, repère, maison, engrenage, soleil,
  nuage), éditables comme les icônes existantes (trait plein, pas une image
  figée).
- **Sprint B — Calques** : **verrouillage de calque** (icône cadenas, bloque
  peinture/édition tout en gardant visibilité/opacité/réordonnancement
  possibles). Le masque de calque peint et les « objets intelligents »
  (redimensionnement sans perte) se sont révélés déjà entièrement couverts
  par l'architecture existante à l'audit — seul un badge de résolution
  native a été ajouté au panneau de calques pour rendre ce dernier point
  visible.
- **Documentation intégrée** : nouvelle fenêtre (À propos ▸ Documentation)
  expliquant chaque outil (icône, nom, description) ainsi que la philosophie
  du projet (pourquoi QuickPaint, pourquoi tactile, pourquoi Rust).
- **Décision actée, pas un oubli** : qualité WebP réglable à l'export
  toujours écartée (nécessiterait `libwebp`, une dépendance système C) —
  même logique que le refus de HEIC/RAW.

## 0.14.0 — juillet 2026 (Sprints 1 à 9 — audit fonctionnel complet)

Plan de sprints (`FEATURE_SPRINTS.md`) retiré une fois acté — voir le journal
git pour l'historique complet. Neuf sprints d'affilée à partir d'un audit
fonctionnel produit/concurrence ; 196 tests au total (+81), 0 warning clippy.

- **Sprint 1 — Fiabilité** : récupération automatique après crash (autosave
  périodique + détection au démarrage) ; sélections nommées (enregistrer/
  recharger/supprimer), persistées avec le projet.
- **Sprint 2 — Sélection & recadrage** : sélection ellipse ; vraie baguette
  magique avec bascule Contigu/Global ; redressement d'horizon dans le
  recadrage (rééchantillonnage par rotation inverse).
- **Sprint 3 — Dessin** : pression réelle de stylet/tablette (événements
  `Touch` d'egui, repli sur la simulation vitesse existante) ; stabilisation
  du tracé réglable ; dégradé conique ; bibliothèque de préréglages de
  pinceau (fournis + import/export `.json`).
- **Sprint 4 — Retouche photo** : comparaison avant/après (maintenir pour
  voir l'état précédent) + histogramme RGB ; correction de distorsion
  (barrel/pincushion) et d'aberration chromatique ; suppression d'objets par
  diffusion (inpainting) ; correction des yeux rouges et retouche peau
  (lissage bilatéral préservant les contours).
- **Sprint 5 — Filtres créatifs** : flou de mouvement et bokeh (avec
  accentuation des hautes lumières) ; grain argentique, vintage, duotone ;
  import de LUT `.cube` (interpolation trilinéaire, intensité réglable) ;
  effets artistiques croquis / bande dessinée / peinture à l'huile (filtre
  de Kuwahara) / aquarelle.
- **Sprint 6 — Calques avancés** : styles de calque non destructifs (ombre
  portée, contour, lueur externe/interne, dérivés de l'alpha du calque).
  Les « objets intelligents » (redimensionnement sans perte) se sont révélés
  déjà acquis à l'audit — les images sont toujours ré-échantillonnées depuis
  leurs pixels natifs, jamais depuis une version déjà réduite.
- **Sprint 7 — Texte & transformation** : ombre portée et texte sur courbe
  (arc de cercle, un galley par caractère) ; transformation en perspective à
  4 coins (homographie) ; warp « Arc » en calque de réglage.
- **Sprint 8 — Formats** : ouverture TIFF ; qualité JPEG réglable à l'export
  (simple, par lots, et PDF) ; import PSD multi-calques (via la crate `psd`,
  modes de fusion mappés). HEIC et RAW volontairement écartés (licences
  AGPL/LGPL des seules bibliothèques disponibles, incompatibles avec une
  distribution simple).
- **Sprint 9 — IA locale → version heuristique** : bords de détourage
  affinés (dégradé continu par proximité de couleur plutôt qu'un flou
  uniforme) ; suréchantillonnage 2×/3×/4× (Lanczos3). Remplace la
  segmentation/super-résolution par réseau de neurones initialement prévue —
  choix assumé pour éviter un modèle ML embarqué et ses dépendances.

## 0.13.1 — juillet 2026 (UX-2.3 + correctif defaults settings)

- **UX-2.3** (reporté dans la 0.13.0) : le survol d'un outil personnalisable
  affiche maintenant sa touche *effective* (`app.keybindings`), pas
  seulement la lettre par défaut figée dans son nom — utile après un rebind
  (Sprint 7.2).
- **Correctif** : `i18n::read_settings()` retombait sur `Settings::default()`
  (dérivé) quand `settings.json` n'existe pas encore, ce qui ignorait les
  `#[serde(default = "fn")]` par champ. Conséquence concrète au tout premier
  lancement : aucun groupe d'outil replié par défaut (UX-2.1), largeur du
  panneau des calques incohérente (UX-3.2) — trouvé en tentant de vérifier
  ces deux items à l'écran. Corrigé (le cas « fichier absent » passe par le
  même chemin de désérialisation qu'un fichier existant) + test de
  régression dédié.
- 118 tests (+3), 0 warning clippy.

## 0.13.0 — juillet 2026 (UX-1 à UX-5 — optimisation UI/UX & fonctionnalité)

Détail complet, constats et critères d'acceptation : `UX_SPRINTS.md` (retiré
une fois acté — voir le journal git pour l'historique complet).

- **Bugs visibles corrigés** : chevauchement de texte dans le footer,
  messages de statut désormais colorés par sévérité (rouge = échec, vert =
  succès — avant toujours vert), icônes undo/redo alignées sur le reste de
  l'interface (Phosphor).
- **Barre d'outils regroupée** : 7 catégories repliables (état persisté),
  14 icônes visibles par défaut au lieu de 29 ; la famille Formes devient un
  sélecteur secondaire à un seul bouton + popup.
- **Panneau des calques modernisé** : glisser-déposer pour réordonner
  (identifié par id de calque, pas par index), largeur ajustable et
  persistée, renommage inline par double-clic. Les actions de sélection
  (aligner/rogner/ordre) migrent vers la barre d'options de l'outil
  Sélection — le panneau ne porte plus que des actions sur des calques.
- **Menu contextuel** : clic droit sur un élément sélectionné du canevas
  (dupliquer, supprimer, copier/coller le style, ordre) — première apparition
  de ce pattern dans l'app.
- **Zoom et navigation** : contrôles persistants dans le footer (avant :
  enterrés dans le menu Vue) ; couleur de fond du document déplacée vers le
  menu Vue (propriété de document, pas d'outil) ; liste de fichiers récents
  dans le menu Fichier.
- **Premier lancement** : ouvre la galerie de modèles plutôt qu'un canevas
  vide. Fenêtres modales harmonisées (croix + bouton Fermer partout). Menu
  Aligner fusionné dans Édition (9 → 8 menus de premier niveau).
- 4 nouveaux tests unitaires (arithmétique du glisser-déposer de calques) ;
  115 tests au total, 0 warning clippy.

## 0.12.2 — juillet 2026 (Sprint 13.9 — démarrage Mac App Store)

- [packaging/QuickPaint.entitlements](packaging/QuickPaint.entitlements) :
  jeu minimal d'entitlements App Sandbox (accès fichiers via les panneaux
  natifs uniquement, aucune entitlement réseau).
- Diagnostic embarqué `quickpaint --sandbox-selftest` : vérifie sans
  interface que polices système, sous-processus de détection de langue et
  écriture/lecture disque fonctionnent sous sandbox.
- Validé par signature ad-hoc + inspection du journal système : tous les
  sous-systèmes non interactifs testés fonctionnent **sans entitlement
  supplémentaire**. Détail et reste à faire : [packaging/SANDBOX_NOTES.md](packaging/SANDBOX_NOTES.md).

## 0.12.1 — juillet 2026 (Sprint 13.8 — suite du découpage de `app.rs`)

- `app/mod.rs` → `app/mod.rs` + `app/transform.rs` : la machine à états de
  transformation interactive de la sélection (poignées d'échelle/rotation,
  glissé, aperçu, undo dédié) extraite en sous-module, même schéma que
  `app/pen_edit.rs` (Sprint 12). `app/mod.rs` passe de 4 444 à 4 297 lignes.
  1 nouveau test unitaire (un scale/rotate en dessous du seuil de bruit ne
  pousse pas de commande d'undo).

## 0.12.0 — juillet 2026 (Sprint 12 — qualité, à partir d'un audit technique)

- **Fluidité** : le compositeur ne re-rastérise plus la surface peinte
  entière à chaque coup de pinceau pixel — cache incrémental par tuile
  (≈103× plus rapide sur un calque 4096×4096 déjà bien rempli, mesuré).
- **Export fidèle** : PNG/JPEG/WebP/PDF (simple et par lots) rendent le
  document à sa résolution native via le compositeur, au lieu de recadrer
  une capture d'écran du viewport — la résolution exportée ne dépend plus
  du zoom ni de la taille de la fenêtre.
- **Robustesse** : version de format projet, erreurs de chargement
  explicites (fichier corrompu, dimensions hors bornes) au lieu d'un échec
  silencieux, bornage du collage presse-papiers et des redimensionnements.
- **Maintenabilité** : `app.rs` → `app/mod.rs` + `app/pen_edit.rs` (édition
  de nœuds Bézier après coup extraite en sous-module, testable seule).

## 0.11.0 — juillet 2026 (Sprint 11)

- 10 nouveaux outils de retouche & composition : densité -/+ (dodge/burn),
  éponge (saturer/désaturer), flou & netteté localisés, estompe (smudge),
  règle/mesure, dessin en miroir rotatif, dégradé interactif au glisser.
- Icônes vectorielles Phosphor (nettes à toute taille) en remplacement des
  emojis système.
- Docs : ARCHITECTURE.md réécrit (état réel), LICENSE (MIT), CHANGELOG,
  CI GitHub Actions.

## 0.10.0 (Sprint 10)

- Bibliothèque d'assets embarquée, templates riches, presets de style nommés.

## 0.9.x (Sprint 9)

- Détourage en un clic (flood-fill + adoucissement), sélection globale
  (non contiguë) à la baguette, restauration.

## 0.8.0 (Sprint 8)

- Réglages continus non destructifs : niveaux, teinte/saturation, courbes ;
  outil correcteur (guérison).

## 0.7.0 (Sprint 7)

- Palette de couleurs personnalisable, raccourcis clavier configurables,
  export par lots multi-tailles.

## 0.6.0 (Sprint 6 + i18n)

- Moteur raster tuilé (fondation F1) : pinceau/gomme pixel, undo par tuile.
- Masques de calque peints, dégradés de remplissage, édition de nœuds Bézier
  après coup, booléens de chemins.
- i18n FR/EN complet, détection de la langue système.

## 0.1.0 → 0.5.0 (Sprints 1–5)

- MVP : trait lissé (Catmull-Rom), pression simulée par la vitesse, calques,
  undo/redo, export PNG.
- Sélection rectangle/lasso/baguette, texte riche, export multi-format
  (JPEG/WebP/PDF/SVG), règles, recadrage à ratio, masque d'écrêtage,
  filtres photo, modes de fusion, plume, grille/magnétisme.
- Release signée Developer ID + notarisée (DMG).
