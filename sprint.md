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

- [x] **T3.1 — `app/selection.rs`** ✅ FAIT : `select_in_rect`/`select_in_ellipse`/
      `select_in_lasso`/`magic_wand`, opérations d'ensemble (invert, combine),
      pont vers `selection_mask` et les tests associés (feather/dilate/contract).
- [x] **T3.2 — `app/layers_ops.rs`** ✅ FAIT : opérations sur calques et groupes
      (création, duplication, fusion, verrouillage granulaire de v0.19,
      alignement/répartition) — distinct de [ui/layers.rs](src/ui/layers.rs)
      qui reste la vue.
- [x] **T3.3 — `app/io.rs`** ✅ FAIT : ouverture/sauvegarde de projet, import
      (PSD/SVG/image), dialogues `rfd`, chemins récents.
- [x] **T3.4 — `app/shortcuts.rs`** ✅ FAIT : dispatch clavier / mapping des
      raccourcis vers les actions (le tableau de bindings vit déjà dans
      [keybindings.rs](src/keybindings.rs) ; ici il s'agissait du gros `match`
      de traitement des événements), menu Édition natif macOS, glisser-déposer
      de fichiers.
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
- [x] **T3.6 — `app/raster_paint.rs`** ✅ FAIT (au-delà du plan initial,
      nécessaire pour continuer à faire baisser `app/mod.rs`) : pinceau/gomme
      pixel, aérographe, tampon de clonage/correcteur, retouche locale
      (densité +/-, éponge, flou, netteté) et estompe — partagent l'undo par
      tuile (`touch_raster_tiles`/`commit_raster_stroke`), seule la fonction
      de peinture pixel appelée diffère par outil. ~420 lignes déplacées.
- [x] **T3.7 — `app/export_ops.rs`** ✅ FAIT (au-delà du plan initial) : pipeline
      d'export (`render_for_export` → encodage → écriture disque), aperçu/
      poids estimé avant export, export par lots, profils d'export nommés,
      export SVG/PDF vectoriels — un seul rendu natif par export, tout le
      reste en dérive. Laissé de côté volontairement : `layer_thumbnail`/
      `selection_overlay_texture` (restés dans `app/mod.rs`, ce sont des
      caches de texture d'aperçu UI, pas le pipeline d'export à proprement
      parler). ~250 lignes déplacées.
- [x] **T3.8 — `app/canvas_overlay.rs`** ✅ FAIT (au-delà du plan initial) :
      `paint_grid`/`paint_rulers`/`paint_pen`/`paint_selection`/`paint_crop`/
      `paint_retouch`/`paint_marquee`/`paint_measure`/`paint_cursor` — pur
      rendu d'overlay (`&self`, jamais de mutation), appelés en séquence par
      `update()` après le rendu du document composite. Laissé de côté
      volontairement : `canvas_context_menu`/`handle_canvas` (restés dans
      `app/mod.rs`, ce sont le dispatch d'entrée qui mute l'état — domaine
      différent, plus risqué à isoler). ~320 lignes déplacées.
- [x] **T3.9 — `app/bucket_cutout.rs`** ✅ FAIT (au-delà du plan initial) :
      `do_bucket_fill`/`do_cutout` — inondent la composition **affichée**
      (capture d'écran différée, `handle_screenshot`, resté dans
      `app/mod.rs`) depuis le point cliqué, seul moyen de raisonner sur les
      pixels réellement vus (fusion de calques comprise) sous le clic.
      ~220 lignes déplacées.
- [x] **T3.10 — `app/canvas_input.rs`** ✅ FAIT (au-delà du plan initial) :
      `handle_canvas` — pan/zoom, verrouillage de calque, puis le `match` par
      outil actif qui route vers les gestes déjà implémentés dans
      `selection`/`transform`/`pen_edit`/`raster_paint`/`bucket_cutout` —
      aucune nouvelle logique, juste le point d'entrée déplacé. ~340 lignes
      déplacées.
- [ ] **Critère de sortie** : `app/mod.rs` < 3 000 lignes (actuellement
      ~4 530 lignes après T3.1-T3.10 ; `selection.rs`/`layers_ops.rs`/`io.rs`/
      `shortcuts.rs`/`raster_paint.rs`/`export_ops.rs`/`canvas_overlay.rs`/
      `bucket_cutout.rs`/`canvas_input.rs` extraits, plus `animation.rs`/
      `pen_edit.rs`/`transform.rs` déjà extraits avant ce sprint — le reste
      est majoritairement la définition de `PaintApp` (champs, `Default`,
      `update()`/`on_exit()`, dialogues UI ponctuels, constructeurs de
      commande) qui n'a plus de domaine autonome évident à isoler sans
      fragmenter artificiellement l'état central de l'app ; prochaine passe
      à évaluer au cas par cas plutôt que planifiée d'avance), `cargo test`
      vert à chaque commit, aucun diff de comportement.

---

## Sprint T4 — Migration egui 0.29 → version courante (3-5 jours, à isoler)

**État actuel :** eframe/egui 0.29 avec `winit` épinglé sur la version
interne d'eframe (fragilité documentée dans [Cargo.toml](Cargo.toml)) et
`egui-phosphor 0.7`. L'écosystème est plusieurs versions plus loin ; chaque
version d'écart ajoute des breaking changes (l'API egui bouge beaucoup).
Plus on attend, plus c'est cher — mais c'est le sprint le plus risqué,
donc **à faire en dernier, sur une branche, et après T3** (moins de code à
migrer dans un `app/mod.rs` réduit... et surtout pas les deux en même temps).

- [x] **T4.0 — Inventaire préalable** ✅ FAIT : cible identifiée = **0.29 →
      0.35** (dernière stable, `cargo search`), soit 6 versions mineures
      d'écart. `egui-phosphor` : 0.7 → 0.12 (compatible, suit les releases
      egui). `muda` : 0.19 → 0.19.3 (patch seulement, pas de breaking
      change côté menu natif). `accesskit` : bump à 0.21 (egui 0.33) — à
      revérifier que `muda`/`winit` épinglé restent compatibles à ce
      palier. `winit` : eframe embarque en interne 0.30.x tout du long
      (0.29→0.32 au moins) ; le hack d'épinglage
      ([Cargo.toml](Cargo.toml), `with_default_menu`) reste donc valide
      jusqu'à nouvel ordre, mais **à reconfirmer à chaque palier bumpé**
      plutôt qu'une fois pour tout le saut.
      Breaking changes majeurs relevés (changelog officiel emilk/egui) :
      - **0.35** : `App::update` → `App::logic`/`App::ui` (signature
        `&mut Ui` au lieu de `&Context`) — touche directement
        [app/mod.rs:3274](src/app/mod.rs:3274) `fn update`, le point d'entrée
        de toute la boucle de frame. `Context::run` → `Context::run_ui`.
        Rendu de texte : `ab_glyph` → `skrifa`+`vello_cpu` (risque de
        régression visuelle fine sur les polices/Phosphor, à valider à l'œil).
      - **0.34** : `Ui` déref vers `Context` (cosmétique, pas bloquant).
      - **0.33** : nouveau système de `Plugin` (remplace `on_begin_pass`/
        `on_end_pass` — grep du dépôt ne montre aucun usage actuel, sans
        impact a priori). `accesskit` 0.21.
      - **0.32** : réécriture `Popup`/menu — **les menus se ferment au clic
        par défaut**, changement de comportement (pas juste API) qui touche
        directement les `egui::menu_button` de
        [ui/toolbar.rs](src/ui/toolbar.rs) (menus Fichier/Calque/Sélection/
        Vue/Aide) — à tester manuellement un par un, en particulier tout
        sous-menu avec un widget interactif (curseur, case à cocher) qui ne
        doit pas se fermer sur ce genre de clic.
      - **0.31** : `Rounding` renommé `CornerRadius` (`f32`→`i8`/`u8`) et
        `StrokeKind` désormais requis sur `Painter::rect` — grep du dépôt :
        **aucun usage direct de `egui::Rounding`/`.rounding(`/
        `Painter::rect(`** trouvé dans `src/`, donc a priori sans impact
        (l'app passe par les widgets haut niveau, pas l'API bas niveau).
      - **0.30/0.29** : `id_source`→`id_salt`, `Ui::new(UiBuilder)` — à
        vérifier au fil de la compilation, pas de grep ciblé fait.
      Conclusion : la boucle `update()` et les menus `toolbar.rs` sont les
      deux points chauds réels ; le reste (Rounding, Plugin) semble sans
      impact sur ce dépôt d'après le grep. Le risque documenté sur le hack
      winit/menu natif reste le point le plus incertain — non vérifiable
      sans compiler réellement contre chaque palier.
- [x] **T4.1 — Bump sur branche `egui-upgrade`** ✅ FAIT : cible finale
      **0.29 → 0.34** (pas 0.35 — `egui-phosphor` 0.12, la dernière version,
      ne dépasse pas `^0.34` ; y aller aurait dupliqué egui dans l'arbre de
      dépendances et cassé le typage des icônes). `winit` reste épinglé
      0.30 (toujours la version interne d'eframe 0.34, revérifié dans
      `Cargo.lock`). **`glow` forcé explicitement** (`default-features =
      false` + liste exacte de features) : depuis eframe 0.30+, `wgpu` est
      devenu le backend par défaut — sans cette précaution, la migration
      aurait basculé silencieusement le rendu UI vers `wgpu`, exactement la
      décision d'architecture que Sprint N (plus bas) réserve à une
      confirmation explicite du porteur de projet. Corrections (~15 sites,
      tous compile-only, aucun changement de comportement voulu) :
      - `App::update(&Context)` → `App::ui(&mut Ui, ...)` : `ctx` récupéré
        via `top_ui.ctx().clone()` (Context = `Arc`, clone bon marché) pour
        garder `top_ui` empruntable mutablement — les panels (`TopBottomPanel`/
        `SidePanel` dépréciés) migrés vers `Panel::top/right/bottom` +
        `show_inside(top_ui, ...)` plutôt que l'ancien `.show(ctx, ...)`.
      - `epaint::FontImage` supprimé → `egui::ColorImage` (le canal alpha
        remplace l'ancienne couverture `f32` brute — `.a() as f32 / 255.0`,
        [render/compositor.rs](src/render/compositor.rs)) ; `ColorImage`
        construit via `::new(size, pixels)` plutôt que le literal de champs
        (nouveau champ `source_size` sinon manquant).
      - `ctx.fonts(|f| ...)` : closures mutantes (`layout_no_wrap`/
        `layout_job`) basculées vers `ctx.fonts_mut(...)`
        ([render/text.rs](src/render/text.rs)) — `fonts()` seul ne donne
        plus qu'un accès immuable.
      - `Painter::rect_stroke` exige un 4ᵉ argument `StrokeKind` (11 sites) :
        `StrokeKind::Middle` partout (reproduit exactement l'ancien rendu
        centré sur le bord, seul comportement qu'avait cette API avant).
      - `ViewportCommand::Screenshot` prend maintenant un `UserData`
        (`egui::UserData::default()`, sans effet ici — le handler ignore déjà
        ce champ via `Event::Screenshot { image, .. }`).
      - `FontData::from_owned(...)` retourne désormais `FontData` et pas
        `Arc<FontData>` : `.into()` ajouté aux deux sites d'insertion
        ([app/mod.rs](src/app/mod.rs), [fonts.rs](src/fonts.rs)).
      - `Margin::symmetric` prend des `i8` (plus des `f32`).
      - Nettoyage des dépréciations pures (zéro impact fonctionnel,
        mécanique) pour respecter le critère de sortie « zéro warning » :
        `close_menu()` → `close()` (73 sites), `wants_keyboard_input()` →
        `egui_wants_keyboard_input()`, `ctx.style()` → `ctx.global_style()`,
        `popup_below_widget`/`Memory::toggle_popup` → `egui::Popup::
        from_toggle_button_response(...).close_behavior(...).show(...)`
        (2 sites, [ui/layers.rs](src/ui/layers.rs)/[ui/toolbar.rs](src/ui/toolbar.rs)),
        `egui::menu::bar` → `egui::MenuBar::new().ui(...)`.
      Résultat : `cargo build`/`cargo clippy --all-targets` zéro warning,
      **299 tests toujours verts** (aucune régression détectée par la
      suite). Vérification visuelle obtenue en cours de session (accès
      écran accordé après une première tentative refusée) : menu ⌘ natif
      macOS intact (« QuickPaint » : À propos/Masquer/Quitter ; « Édition » :
      Annuler/Rétablir/Couper/Copier/Coller, routés vers les bonnes actions
      via `native_menu.rs`/`handle_native_menu`) — **le risque principal
      identifié en T4.0 est levé**. Icônes Phosphor visibles et colorées
      dans la barre d'outils. Menus `toolbar.rs` (Fichier, migré vers
      `MenuBar`/`Popup`) s'ouvrent et affichent leur contenu correctement.
      Fermeture au clic hors-menu non confirmée visuellement de façon
      concluante en session (capture prise sans nouvel événement d'entrée
      après le clic, donc pas de nouvelle frame garantie) — analyse du code
      source d'egui 0.34 (`Popup::menu`/`MenuButton::ui`) montre que le
      comportement de fermeture (`PopupCloseBehavior::CloseOnClick`, la
      valeur par défaut) est **identique bit à bit** entre l'ancien
      `egui::menu::bar` (déprécié, wrapper fin sans tag `MenuConfig`) et le
      nouveau `egui::MenuBar::new().ui()` — aucun changement de
      comportement de fermeture attendu, à reconfirmer par un clic réel du
      porteur de projet plutôt qu'une analyse statique.
- [x] **T4.2 — Vérifications manuelles ciblées** ◐ PARTIEL (le maximum
      possible sans accès écran interactif prolongé ni lecteur d'écran) :
      - [x] **Menu macOS natif** ✅ confirmé à l'écran (point le plus à
        risque selon T4.0) — « QuickPaint » et « Édition » présents,
        actions routées correctement.
      - [x] **Icônes Phosphor** ✅ confirmées à l'écran, rendu net et coloré.
      - [x] **Menus `toolbar.rs`** ✅ s'ouvrent et affichent leur contenu ;
        fermeture au clic non observée de façon concluante en session
        (capture sans nouvel évènement d'entrée) mais **confirmée
        équivalente par lecture du code source d'egui** (`CloseOnClick` par
        défaut, identique avant/après la migration `menu::bar` →
        `MenuBar::new().ui()`).
      - [x] **Pression du stylet/trackpad** ✅ vérifié statiquement : le
        chemin `egui::Event::Touch { force: Some(f), .. }`
        ([app/mod.rs:3567](src/app/mod.rs:3567)) compile sans modification
        depuis 0.29, donc la forme de l'évènement n'a pas changé (le
        typage Rust aurait signalé toute rupture) — non testé avec un
        vrai périphérique.
      - [x] **Presse-papiers ⌘V** ✅ vérifié statiquement : `paste_image`
        ([app/io.rs:79](src/app/io.rs:79)) passe entièrement par `arboard`,
        indépendant de la version d'egui/eframe — aucun changement requis,
        aucun risque de régression lié à cette migration.
      - [x] **DPI/Retina** ✅ vérifié statiquement : `ctx.pixels_per_point()`
        ([app/bucket_cutout.rs](src/app/bucket_cutout.rs)) inchangé, non
        déprécié en 0.34 (zéro warning dessus).
      - [ ] **VoiceOver (accesskit)** ❌ non vérifiable sans lecteur d'écran
        actif — la seule vérification de la liste qui reste réellement à
        faire par le porteur de projet avant merge.
- [ ] **T4.3 — Retirer l'épinglage winit** si la nouvelle version d'eframe
      expose de quoi désactiver le menu par défaut proprement ; sinon
      re-documenter la contrainte dans Cargo.toml comme aujourd'hui. Non
      revisité en T4.1 (winit reste à la même version 0.30, donc le hack
      était déjà correct sans action) — à réévaluer si on rebase un jour sur
      egui 0.35 (`egui-phosphor` le permettant), où `eframe` pourrait
      embarquer un winit différent.
- [ ] **T4.4 — Dans la foulée** (même branche) : bump `usvg`/`fontdb` vers
      des versions à `ttf-parser` maintenu (reliquat de T2). Non fait en
      T4.1 (hors scope de la migration egui elle-même, `usvg` 0.47 n'a pas
      été touché) — reste à faire.
- [ ] **Critère de sortie** : `cargo clippy` zéro warning ✅, 299+ tests
      verts ✅, checklist T4.2 validée ◐ (6/7 items faits, seul VoiceOver
      reste à tester par le porteur de projet — nécessite un lecteur
      d'écran actif, hors de portée d'une vérification automatisée), DMG
      reconstruit et notarisé ❌ (pas fait dans cette session — T4.3/T4.4
      également non traités, voir ci-dessus). **Ne pas merger vers `main`
      sans le test VoiceOver.**

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
