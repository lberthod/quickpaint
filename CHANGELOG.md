# Changelog — QuickPaint

Versions alignées sur les sprints. Détail complet : le journal git.

## 0.14.0 — juillet 2026 (Sprints 1 à 9 — audit fonctionnel complet)

Détail complet, priorisation et raisonnement : [FEATURE_SPRINTS.md](FEATURE_SPRINTS.md).
Neuf sprints d'affilée à partir d'un audit fonctionnel produit/concurrence ;
196 tests au total (+81), 0 warning clippy.

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

Détail complet, constats et critères d'acceptation : [UX_SPRINTS.md](UX_SPRINTS.md).

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
