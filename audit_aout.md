# audit_aout.md — Audit technique & plan d'optimisation (29 août 2026)

Consolide un audit technique frais (build/tests/clippy/robustesse, ce jour)
avec l'état déjà connu et documenté ailleurs (`sprint.md`, CHANGELOG.md,
`deployappstore.md`) pour produire **une seule liste d'actions
priorisée**, orientée « rendre l'app plus rapide/robuste et livrable à de
vrais utilisateurs ».

Ne re-décrit pas ce qui est déjà fait — voir [audit_next.md](audit_next.md)
et [CHANGELOG.md](CHANGELOG.md) (0.20.0) pour le détail fonctionnel
(score global : quasi 100 % des ~120 items produit couverts).

---

## Constat du jour (29 août 2026)

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

## P0 — Bloquant pour une vraie mise à disposition utilisateurs

### 1. Fusionner ou abandonner la branche `egui-upgrade`
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

### 2. ✅ Tranché (30 août 2026) — Pas d'App Store, Developer ID/DMG uniquement
Décision explicite du porteur de projet : distribution **Developer ID +
DMG uniquement**, l'App Store n'est plus visé. `appstore_setup.md` et
`deployappstore.md` retirés du dépôt en conséquence (git history les
conserve si le sujet revient un jour).

**Conséquence directe pour la suite** : sans contrainte App Sandbox, la
voie plugins natifs tiers / scripting (voir `audit_100_features.md`,
items #97-98) n'est plus bloquée par une incompatibilité de distribution —
c'était la seule réserve technique identifiée sur ce point.

### 3. Distribution Developer ID (hors App Store) — vérifier que c'est à jour
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

## P1 — Optimisation & robustesse (à faire, faible risque)

### 4. ✅ Fait (29 août 2026) — Nettoyage des 43 warnings `f32`/`f64`
`cargo fix --bin quickpaint --allow-dirty` a suffixé les littéraux
concernés (`1.0_f32` au lieu de `1.0`) dans `canvas_overlay.rs` (28),
`app/mod.rs` (4), `pen_edit.rs` (4), `ui/layers.rs` (3), `ui/toolbar.rs`
(4). `cargo check` est désormais **0 warning**. Diff purement mécanique
(aucun changement de comportement), 325/325 tests toujours verts après.

### 5. Clippy : 38 warnings restants, volontairement non touchés
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

### 6. ~~Vérifier le cas `Pixmap::new(0, 0)` dans le compositeur~~ — vérifié, non-problème
`src/render/compositor.rs:641` (`tint_from_alpha`) et ses appelants
(`apply_drop_shadow`/`apply_layer_stroke`/`apply_glow`, via
`apply_layer_styles`) reçoivent `w`/`h` provenant de la même fonction
`compose()`, qui fait déjà `Pixmap::new(w, h)?` à la ligne 199 — un
document de largeur/hauteur nulle fait déjà échouer (`?`) toute la
composition bien avant d'atteindre `apply_layer_styles`. Le `.unwrap()` de
`tint_from_alpha` est donc protégé par un garde-fou antérieur dans le même
appelant, pas un risque de panic réel. Aucune action nécessaire.

### 7. Refactor différé : `app/mod.rs` (5043 lignes) et `ui/toolbar.rs` (2995 lignes)
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

### 8. ✅ Fait (29 août 2026) — Mesures de perf, plus d'optimisation à l'aveugle

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

## P2 — Usabilité (utilisateur final, pas développeur)

### 9. Accessibilité VoiceOver — jamais validée
Mentionné en P0 côté branche `egui-upgrade`, mais reste vrai même sur
`main` : l'arbre `accesskit` est censé être construit automatiquement par
egui, mais rien dans les tests ne le vérifie (impossible à tester
unitairement — nécessite un lecteur d'écran actif). Pour une app qui vise
un usage tactile/accessible (cf. nom du dépôt), c'est le point d'usabilité
le plus significatif non couvert.

### 10. Premier lancement — pas de tutoriel/onboarding constaté
`i18n.rs`/`app/mod.rs` gèrent les réglages persistés, mais rien dans le
code parcouru n'indique un onboarding pour un nouvel utilisateur (galerie
de modèles au démarrage : oui, via `templates()` — mais pas d'aide
contextuelle sur les outils). À vérifier avec un test manuel « premier
lancement sur un Mac vierge, sans lire le README » : est-ce que les
raccourcis (`keybindings.rs`), la barre d'outils, les calques sont
découvrables sans documentation externe ?

### 11. Limites connues à ré-exposer clairement dans l'UI, pas seulement le code
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

### 12. Gestes multi-touch écran tactile — décision produit à formaliser
CHANGELOG.md (0.20.0, Sprint R) note que le trackpad est couvert (pinch/pan)
mais qu'un **écran tactile** natif (NSEvent hors winit) reste un chantier
non engagé, faute de décision produit. Si le nom du projet
(« paint_tactile ») implique un usage sur écran tactile réel (type
tablette graphique tactile ou écran tactile de bureau), c'est
potentiellement la fonctionnalité manquante la plus alignée avec l'intention
du produit — à re-questionner explicitement plutôt qu'à laisser en
sommeil.

---

## Ce qui n'a PAS besoin d'action (déjà solide, à ne pas re-auditer)

- Couverture fonctionnelle : ~120 items produit, quasi 100 % ✅ (voir
  `audit_next.md`, CHANGELOG.md 0.20.0).
- Format natif, import/export multi-formats, calques, historique,
  filtres, texte, animation : tous couverts avec tests dédiés.
- Hygiène dépôt : `.gitignore` correct, pas de secret, pas de dépendance
  réseau, pas de télémétrie (non-goal assumé et respecté).
- Décisions produit déjà tranchées et closes (à ne pas rouvrir sans
  nouvelle info) : WebP lossy refusé, export PSD non supporté, import
  `.abr` non supporté, MP4 remplacé par APNG — toutes documentées dans le
  README avec justification technique.

---

## Ordre d'attaque recommandé

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
