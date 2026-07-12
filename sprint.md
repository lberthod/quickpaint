# sprint.md — Plan de sprints : points d'attention de l'audit technique

> Fait suite à l'audit technique du 12 juillet 2026 (v0.19.0). État des
> lieux : compilation propre, 299 tests verts, zéro `unsafe`, 1 seul warning
> clippy. Ce document ne couvre donc **pas** des fonctionnalités produit
> (voir [sprint_next.md](sprint_next.md) pour ça) mais la **dette
> technique et l'hygiène du dépôt** : 6 points d'attention, regroupés en
> 4 sprints livrables indépendamment.

Priorisation : (1) ce qui dégrade le dépôt à chaque release (DMG versionné),
(2) les gains à une ligne (clippy, profil release), (3) la maintenabilité
long terme (`app/mod.rs`), (4) la migration egui, plus grosse et à isoler.

---

## Sprint T1 — Hygiène du dépôt & corrections à une ligne (½ journée)

### T1.1 — Sortir `QuickPaint.dmg` du suivi git 🔴 priorité haute

**État actuel :** `*.dmg` est bien dans le [.gitignore](.gitignore), mais
`QuickPaint.dmg` (5,6 Mo) a été ajouté en force et 4 commits le touchent.
Le `.git` pèse déjà 24 Mo et grossira d'~5 Mo à chaque release notarisée.

- [ ] `git rm --cached QuickPaint.dmg` + commit — le fichier reste sur le
      disque mais n'est plus suivi (le `.gitignore` existant prend le relais).
- [ ] Décider du canal de distribution de remplacement : **GitHub Releases**
      (recommandé — `gh release create v0.19.0 QuickPaint.dmg`), à
      documenter dans le README section installation.
- [ ] Optionnel (décision à trancher) : purger les 4 blobs de l'historique
      avec `git filter-repo --path QuickPaint.dmg --invert-paths` pour
      récupérer ~20 Mo. ⚠️ Réécrit l'historique : à ne faire que si le
      remote n'est cloné par personne d'autre, et avec une sauvegarde du
      dépôt avant. Sinon, s'abstenir — le simple `rm --cached` suffit à
      arrêter l'hémorragie.
- [ ] Statuer sur `appstore_setup.md` et `deployappstore.md` (non suivis
      actuellement) : les committer s'ils font partie de la doc de
      distribution, sinon les ajouter au `.gitignore`.

### T1.2 — Dernier warning clippy ✅ trivial

- [ ] [filter.rs:1799](src/tools/filter.rs:1799) : remplacer
      `vec![200u8, 200, 200, 255].repeat(w * h)` par
      `[200u8, 200, 200, 255].repeat(w * h)` (le `vec!` est inutile,
      `repeat` existe sur les slices).
- [ ] Vérifier : `cargo clippy --all-targets` doit sortir **zéro** warning.
- [ ] Optionnel : ajouter `-D warnings` au job CI clippy (s'il existe dans
      [.github](.github)) pour que le zéro-warning soit contractuel.

### T1.3 — Profil release : binaire plus petit

**État actuel :** `[profile.release]` a `opt-level = 3` et `lto = true`
([Cargo.toml](Cargo.toml)), mais pas de strip ni de codegen-units réduit.

- [ ] Ajouter au [Cargo.toml](Cargo.toml) :
      ```toml
      [profile.release]
      opt-level = 3
      lto = true
      strip = true          # symboles de debug retirés du binaire distribué
      codegen-units = 1     # meilleure optimisation inter-modules (build plus lent, OK pour release)
      ```
- [ ] **Ne pas** ajouter `panic = "abort"` pour l'instant : avec ~120
      `unwrap`/`expect` encore présents dans les couches UI/outils, un
      unwind propre (fermeture des dialogues, pas de corruption d'état
      affiché) vaut mieux qu'un abort sec. À revisiter après T3.
- [ ] Mesurer avant/après : taille de `target/release/quickpaint` et du
      DMG, noter dans le commit.

---

## Sprint T2 — Dépendances : mises à jour sans risque (½ journée)

**État actuel :** `cargo audit` remonte 4 « vulnérabilités » qui concernent
`quick-xml` via `wayland-scanner` et `zbus_xml` — des dépendances **Linux
uniquement**, jamais compilées dans le binaire macOS. Pas d'urgence
sécurité réelle, mais deux crates « unmaintained » dans l'arbre effectif
(`ttf-parser 0.25` via `fontdb`/`usvg`, `paste`).

- [ ] `cargo update` (mises à jour semver-compatibles du lock uniquement) ;
      relancer `cargo test` (299 tests) et un smoke-test manuel de l'app
      (dessin, export PNG, ouverture d'un .json de projet).
- [ ] Créer `.cargo/audit.toml` avec les advisories ignorées **et la
      justification** en commentaire, pour que `cargo audit` redevienne un
      signal utile (vert = vraiment rien à voir) :
      ```toml
      [advisories]
      ignore = [
          "RUSTSEC-2026-0194", # quick-xml : dépendance Linux-only (wayland/zbus), non compilée sur macOS
          "RUSTSEC-2026-0195", # idem
      ]
      ```
- [ ] `ttf-parser`/`fontdb` : vérifier si `usvg` ≥ 0.48 et `fontdb` récent
      tirent une version maintenue ; si oui, bump mineur ici, sinon laisser
      pour T4 (la montée d'usvg peut interagir avec la migration egui).
- [ ] Optionnel : job CI hebdomadaire `cargo audit` (cron GitHub Actions)
      plutôt qu'un contrôle manuel.

---

## Sprint T3 — Découpage de `app/mod.rs` (2-3 jours, incrémental)

**État actuel :** [app/mod.rs](src/app/mod.rs) fait **7 132 lignes / ~275
fonctions** — le principal risque de maintenabilité du projet. Les
extractions précédentes (`pen_edit.rs`, `transform.rs`, `animation.rs`)
ont montré la méthode : extraire un domaine cohérent + ses tests, sans
changement de comportement. Objectif : passer sous ~3 000 lignes.

Règles du jeu (identiques aux sprints 12.5/13.8) :
- Un domaine extrait = un commit = `cargo test` vert, zéro changement
  fonctionnel.
- Les méthodes restent des `impl PaintApp` dans le nouveau fichier
  (`impl` séparés par module, comme `transform.rs` le fait déjà) — pas de
  refonte d'architecture, juste du déménagement.
- Les tests du domaine déménagent avec lui.

Candidats à extraire, du plus autonome au plus imbriqué (à confirmer en
lisant les blocs — l'ordre peut changer) :

- [ ] **T3.1 — `app/selection.rs`** : `select_in_rect`/`select_in_ellipse`/
      `select_in_lasso`/`magic_wand` ([app/mod.rs:991-1037](src/app/mod.rs:991)),
      opérations d'ensemble (invert, combine), pont vers `selection_mask`
      ([app/mod.rs:1146](src/app/mod.rs:1146)) et les tests associés
      (feather/dilate/contract déjà testés). Probablement le plus gros gain.
- [ ] **T3.2 — `app/layers_ops.rs`** : opérations sur calques et groupes
      (création, duplication, fusion, verrouillage granulaire de v0.19,
      nommage [app/mod.rs:3208](src/app/mod.rs:3208)) — distinct de
      [ui/layers.rs](src/ui/layers.rs) qui reste la vue.
- [ ] **T3.3 — `app/io.rs`** : ouverture/sauvegarde de projet, import
      (PSD/SVG/image, [app/mod.rs:1960](src/app/mod.rs:1960)), dialogues
      `rfd`, chemins récents.
- [ ] **T3.4 — `app/shortcuts.rs`** : dispatch clavier / mapping des
      raccourcis vers les actions (le tableau de bindings vit déjà dans
      [keybindings.rs](src/keybindings.rs) ; ici il s'agit du gros `match`
      de traitement des événements).
- [x] **T3.5 — passe `unwrap`** ✅ FAIT : les 3 `unwrap()` non-test restants
      dans le périmètre `app/` (T3.1-T3.4) traités — `picked.sort_by(...z.
      partial_cmp(...).unwrap())` → `total_cmp` (évite un panic si `z`
      devient `NaN`, ex. donnée corrompue rechargée), et
      `sorted.first()/last().unwrap()` dans `align_layer_to_document`
      (Distribute) → `let ... else { return }` explicite (le garde
      `elems.len() < 3` en amont rendait déjà le panic inatteignable, mais
      le rendre visible au lecteur vaut le coût nul). Aucun autre `unwrap`/
      `expect` non-test trouvé dans `app/mod.rs`, `selection.rs`,
      `layers_ops.rs`, `io.rs`, `shortcuts.rs`, `animation.rs`,
      `pen_edit.rs`, `transform.rs` (les deux `expect` restants dans
      `animation.rs` sont dans un test de round-trip sérialisation).
- [ ] **Critère de sortie** : `app/mod.rs` < 3 000 lignes (actuellement
      ~6 050 lignes après T3.1-T3.5 ; `selection.rs`/`layers_ops.rs`/
      `io.rs`/`shortcuts.rs` extraits, restent `animation.rs`/`pen_edit.rs`/
      `transform.rs` déjà extraits avant ce sprint — candidats suivants pour
      repasser sous la barre : découper le reste du fichier restant, par ex.
      le rendu canevas/UI panels encore en place), `cargo test` vert à
      chaque commit, aucun diff de comportement.

---

## Sprint T4 — Migration egui 0.29 → version courante (3-5 jours, à isoler)

**État actuel :** eframe/egui 0.29 avec `winit` épinglé sur la version
interne d'eframe (fragilité documentée dans [Cargo.toml](Cargo.toml)) et
`egui-phosphor 0.7`. L'écosystème est plusieurs versions plus loin ; chaque
version d'écart ajoute des breaking changes (l'API egui bouge beaucoup).
Plus on attend, plus c'est cher — mais c'est le sprint le plus risqué,
donc **à faire en dernier, sur une branche, et après T3** (moins de code à
migrer dans un `app/mod.rs` réduit... et surtout pas les deux en même temps).

- [ ] **T4.0 — Inventaire préalable** : lister les breaking changes des
      changelogs egui/eframe entre 0.29 et la cible, et vérifier la
      disponibilité des compagnons : `egui-phosphor` compatible,
      comportement `accesskit`, et surtout le hack `muda`/`winit`
      (`with_default_menu`, [native_menu.rs](src/native_menu.rs)) qui
      dépend de la version de winit interne à eframe — c'est le point le
      plus susceptible de casser silencieusement (menu ⌘ écrasé).
- [ ] **T4.1 — Bump sur branche `egui-upgrade`** : monter eframe/egui/
      egui-phosphor/winit d'un bloc, corriger les erreurs de compilation
      module par module (`ui/` d'abord, plus gros consommateur d'API).
- [ ] **T4.2 — Vérifications manuelles ciblées** (ce que les tests ne
      couvrent pas) : menu macOS natif présent après lancement, VoiceOver
      (accesskit), pression du stylet/trackpad ([input/pressure.rs](src/input/pressure.rs)),
      rendu des icônes Phosphor, DPI/Retina, presse-papiers ⌘V.
- [ ] **T4.3 — Retirer l'épinglage winit** si la nouvelle version d'eframe
      expose de quoi désactiver le menu par défaut proprement ; sinon
      re-documenter la contrainte dans Cargo.toml comme aujourd'hui.
- [ ] **T4.4 — Dans la foulée** (même branche) : bump `usvg`/`fontdb` vers
      des versions à `ttf-parser` maintenu (reliquat de T2).
- [ ] **Critère de sortie** : `cargo clippy` zéro warning, 299+ tests
      verts, checklist T4.2 validée, DMG reconstruit et notarisé.

---

## Récapitulatif

| Sprint | Contenu | Effort | Risque |
|--------|---------|--------|--------|
| T1 | DMG hors git, clippy, profil release | ½ j | quasi nul |
| T2 | `cargo update`, audit.toml, CI audit | ½ j | faible |
| T3 | Découpage `app/mod.rs` (< 3 000 l.) + passe unwrap | 2-3 j | faible (mécanique, testé) |
| T4 | Migration egui + usvg/fontdb | 3-5 j | moyen (menu natif, stylet) |

Ordre recommandé : T1 → T2 → T3 → T4. T1 et T2 peuvent se faire dans la
même session ; T4 attend que T3 soit fini et vit sur sa propre branche.
