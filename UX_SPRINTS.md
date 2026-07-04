# UX_SPRINTS.md — Optimisation UI/UX & fonctionnalité

> Audit ciblé UI/UX (pas un audit technique général — voir `ARCHITECTURE.md`
> pour ça) mené le 4 juillet 2026 : lecture du code de `src/ui/` et
> `src/app/mod.rs`, **et** capture d'écran réelle de l'app lancée
> (`cargo bundle --release`, fenêtre 1400×950) pour vérifier ce qui se
> passe vraiment à l'écran plutôt que de deviner depuis le code seul.
> Chaque constat ci-dessous est référencé par fichier/ligne ou par
> observation directe sur la capture — aucun n'est spéculatif.

---

## Constats (preuves, pas d'impression)

### C1 — Le footer superpose deux blocs de texte

[src/ui/footer.rs](src/ui/footer.rs) place à gauche `outil · taille ·
traits · calque · zoom` et à droite le message de statut (ou, à défaut, un
pavé d'aide raccourcis : *« V/B/E/L/R/O/T/I/H · Suppr efface · ⌘D
duplique… »*). Sur la capture d'écran à 1400 px de large, ces deux blocs se
chevauchent et le texte devient illisible (« Zoom▓▓▓▓/R/O/T/I/H » observé à
l'écran). Aucun mécanisme de troncature/priorité entre les deux groupes.

### C2 — Le statut n'a qu'une seule couleur, succès et erreur confondus

`footer.rs::show` : `ui.colored_label(egui::Color32::from_rgb(40, 130, 60), msg)`
— **toujours vert**, quel que soit le message. Depuis le Sprint 12
(ANALYSE.md, robustesse), l'app affiche désormais de vrais messages d'erreur
(« Image refusée : dimensions trop grandes », « Impossible d'ouvrir le
projet : JSON invalide ») dans cette même case verte — visuellement
indiscernables d'un succès. Un utilisateur peut manquer un échec.

### C3 — La barre d'outils déborde sur une deuxième rangée quasi vide

`src/ui/toolbar.rs::tools_row` : 27 outils rendus à plat dans un seul
`horizontal_wrapped`, aucun regroupement visuel replié. Sur la capture à
1400 px, 25 icônes tiennent sur la rangée 1, et la rangée 2 ne contient que
2-3 icônes (Miroir, Dégradé) — une bande quasi vide qui pousse tout le reste
de l'interface vers le bas. Le problème s'aggrave à chaque nouvel outil
(11 outils ajoutés rien qu'au Sprint 11).

### C4 — Le zoom n'a pas de contrôle persistant

`−`/`+`/`100 %`/`Ajuster` n'existent que **dans le menu Vue**
([toolbar.rs:852-866](src/ui/toolbar.rs)) — 2 clics (ouvrir le menu, choisir
l'action) pour un geste censé être immédiat. Le footer n'affiche le zoom
qu'en texte, non cliquable. Incohérent avec l'origine tactile du projet
(architecture-paint-macos-rust, TD1655) où *chaque* interaction gagnante
compte : zoomer/dézoomer est une des actions les plus fréquentes d'une
session de dessin.

### C5 — Le panneau des calques n'a pas de glisser-déposer

`src/ui/layers.rs` : réorganiser un calque passe par les boutons **▲
Monter** / **▼ Descendre** ([layers.rs:53-68](src/ui/layers.rs)) — un clic
par position à franchir. Tous les éditeurs de référence (Photoshop, GIMP,
Affinity, Krita) permettent de glisser une pastille de calque directement
dans la pile. Le panneau est en plus à largeur fixe
(`default_width(170.0).resizable(false)`, [app/mod.rs](src/app/mod.rs)) :
un nom de calque long est tronqué sans recours.

### C6 — Le panneau Calques mélange deux responsabilités

Toujours `layers.rs` : sous la liste des calques vivent **Éléments (0) :
Aligner · Rogner** et **Ordre : Devant · Avancer · Reculer · Fond** — ce
sont des actions sur la **sélection d'objets**, pas sur les **calques**.
Visible sur la capture : la section « Éléments » reste grisée/vide tant
qu'aucun élément n'est sélectionné, alors que le panneau s'appelle
« Calques ». Un utilisateur qui cherche à aligner deux formes ne pense pas
à regarder dans le panneau des calques.

### C7 — Aucun menu contextuel (clic droit) sur le canevas

Recherche exhaustive dans `app/mod.rs` : aucun usage de
`secondary_clicked()` ni de menu contextuel. Dupliquer, supprimer, copier/
coller le style, changer l'ordre — toutes ces actions existent déjà comme
fonctions (`duplicate_selection`, `copy_style`, `set_z_order`…) mais ne sont
accessibles que par le menu du haut ou un raccourci clavier à mémoriser.
Le clic droit sur un élément sélectionné est l'endroit où un utilisateur
cherche spontanément ces actions.

### C8 — La couleur de fond du document est cachée dans les options d'outil

`options_row` ([toolbar.rs:1320-1325](src/ui/toolbar.rs)) : le sélecteur
**Fond :** (couleur du pasteboard, utilisée à l'export JPEG notamment)
n'apparaît que dans la barre d'options des outils de dessin — invisible si
l'outil actif est Texte, Sélection, Règle, etc. C'est un réglage de
**document**, pas d'outil ; sa visibilité ne devrait pas dépendre de
l'outil sélectionné.

### C9 — Incohérence d'icônes : undo/redo en glyphes Unicode, le reste en Phosphor

`toolbar.rs:910-917` : `egui::Button::new("↷")` / `egui::Button::new("↶")` —
glyphes Unicode bruts, alors que 100 % du reste de l'interface (outils,
calques, menus) utilise la police vectorielle Phosphor
(`egui-phosphor`, Sprint 11). Rendu visuellement plus petit/moins net sur la
capture d'écran, détail qui casse la cohérence visuelle par ailleurs
soignée.

### C10 — Pas de liste de fichiers récents

`toolbar.rs:557-605` (menu **Fichier**) : `Nouveau`, `Nouveau depuis un
modèle…`, `Ouvrir…` (dialogue plein à chaque fois), `Enregistrer` — aucune
entrée « Ouvrir récent ». Rouvrir le projet d'hier repart d'un dialogue de
fichiers vide à chaque fois, alors que `settings.json` persiste déjà palette
et raccourcis (le mécanisme de persistance existe, juste pas utilisé pour
ça).

---

## Les 5 sprints

Légende inchangée : Effort S/M/L, Impact ⭐ à ⭐⭐⭐. Chaque item a un critère
d'acceptation vérifiable (pas juste « amélioré »).

### Sprint UX-1 — Corriger les défauts visibles (S, ~1-2 jours)

Bugs constatés à l'écran, corrections chirurgicales, aucun risque
architectural.

- [ ] **UX-1.1 Éliminer le chevauchement du footer** (C1) — S, ⭐⭐⭐.
      `footer.rs::show` : mesurer la largeur du bloc de gauche
      (`ui.available_width()`) et masquer le pavé d'aide raccourcis en
      dessous d'un seuil (ex. 900 px), ou le réduire à une icône `?`
      cliquable qui affiche l'aide en tooltip. **Critère d'acceptation** :
      à aucune largeur de fenêtre entre 900 px et 2000 px les deux blocs de
      texte ne se chevauchent (vérifié par capture d'écran à 3 largeurs).
- [ ] **UX-1.2 Coder la sévérité du statut par couleur** (C2) — S, ⭐⭐⭐.
      Remplacer `status: Option<String>` par
      `status: Option<(StatusKind, String)>` avec
      `enum StatusKind { Success, Error, Info }` ; tous les sites d'appel
      (`self.status = Some(...)`) choisissent explicitement le niveau.
      `footer.rs` colore : vert (Success), rouge (Error), bleu/gris (Info).
      **Critère d'acceptation** : un message d'erreur (ex. import d'image
      refusé) s'affiche en rouge, un succès en vert — testable visuellement
      en déclenchant les deux cas.
- [ ] **UX-1.3 Undo/redo en icônes Phosphor** (C9) — S, ⭐. Remplacer `"↷"`/
      `"↶"` par `egui_phosphor::regular::ARROW_CLOCKWISE`/
      `ARROW_COUNTER_CLOCKWISE` (ou équivalent disponible dans le jeu
      Phosphor déjà embarqué), même style que les boutons d'outils.
      **Critère d'acceptation** : capture d'écran avant/après montrant les
      deux icônes dans le même style que le reste de la barre.

### Sprint UX-2 — Regrouper la barre d'outils (M, ~1 semaine)

Objectif : que la barre d'outils tienne sur une seule rangée à une largeur
de fenêtre raisonnable (1100 px, taille de lancement par défaut), et reste
lisible même quand un 28ᵉ outil s'ajoute plus tard.

- [ ] **UX-2.1 Grouper par catégorie avec repli** — M, ⭐⭐⭐. Remplacer le
      `horizontal_wrapped` plat de `tools_row` par des groupes explicites
      (Navigation, Dessin, Formes, Retouche locale, Texte/Mesure — les
      catégories existent déjà informellement via `tool_groups()` et
      `tool_accent()`) rendus comme des blocs séparés par des séparateurs
      plus marqués, chaque groupe repliable individuellement (mémorisé dans
      `settings.json`, comme la palette). **Critère d'acceptation** : à
      1100 px de large, les groupes actifs par défaut (Navigation, Dessin,
      Formes, Texte) tiennent sur une seule rangée ; les groupes moins
      utilisés (Retouche locale : 7 outils) démarrent repliés.
- [ ] **UX-2.2 Sélecteur d'outil secondaire (fly-out)** — M, ⭐⭐. Pour les
      familles à outils multiples rarement changés en cours de geste
      (Formes : ligne/flèche/rect/ellipse/polygone/étoile — 6 boutons),
      un seul bouton visible affichant l'outil actif de la famille, un
      clic long ou une flèche ouvre les 5 autres — inspiré du sélecteur de
      forme de Procreate/Affinity, cohérent avec l'origine tactile.
      **Critère d'acceptation** : la rangée d'outils compte au plus 15
      boutons visibles simultanément (contre 27 aujourd'hui) sans perdre
      l'accès à un seul outil existant.
- [ ] **UX-2.3 Raccourci clavier affiché au survol** — S, ⭐. Chaque
      `tool_button` affiche déjà `name — hint` au survol
      ([toolbar.rs:1022](src/ui/toolbar.rs)) ; y ajouter le raccourci
      clavier effectif (`app.keybindings`, déjà personnalisable depuis le
      Sprint 7.2) pour renforcer l'apprentissage des raccourcis pendant
      l'usage normal. **Critère d'acceptation** : survoler l'outil Pinceau
      affiche sa lettre de raccourci actuelle (par défaut B), pas un texte
      générique.

### Sprint UX-3 — Panneau des calques et menu contextuel (M/L, ~1-2 semaines)

Le plus gros morceau fonctionnel : aligner le panneau de calques sur les
attentes standard, et combler l'absence totale de clic droit.

- [ ] **UX-3.1 Glisser-déposer pour réordonner les calques** — M, ⭐⭐⭐
      (C5). `egui::Ui` supporte le drag natif via `dnd_drag_source`/
      `dnd_drop_zone` (disponible en 0.29) — chaque ligne de calque devient
      une source *et* une zone de dépôt, réordonnancement en direct avec
      aperçu, undo via `Command::SetLayers` (déjà utilisé par
      fusion/aplatissement). Les boutons ▲▼ restent en secours (accessibilité
      clavier). **Critère d'acceptation** : glisser le calque du bas vers le
      haut dans une pile de 4 calques le replace au sommet, annulable par
      ⌘Z.
- [ ] **UX-3.2 Panneau redimensionnable** — S, ⭐⭐ (C5). Retirer
      `resizable(false)` sur le `SidePanel` calques
      ([app/mod.rs](src/app/mod.rs)), fixer une largeur mini/maxi
      raisonnable (140–320 px), persister la largeur choisie dans
      `settings.json`. **Critère d'acceptation** : le panneau se redimensionne
      à la souris et retrouve sa largeur au relancement de l'app.
- [ ] **UX-3.3 Renommage inline** — S, ⭐⭐. Double-clic sur le nom d'un
      calque → `TextEdit` en place (le champ « Calque actif : » existe déjà
      plus bas dans le panneau, [layers.rs](src/ui/layers.rs) — le
      remplacer par un renommage direct sur la ligne, pattern plus habituel).
      **Critère d'acceptation** : double-cliquer un nom de calque permet de
      le retaper sans passer par un champ séparé.
- [ ] **UX-3.4 Sortir « Éléments/Aligner/Rogner/Ordre » du panneau Calques**
      — M, ⭐⭐ (C6). Déplacer ce bloc vers la barre d'options de l'outil
      Sélection (déjà contextuelle, `options_row` bascule dessus quand
      `ActiveTool::Select`) ou un panneau « Sélection » séparé qui n'apparaît
      que quand des éléments sont sélectionnés. **Critère d'acceptation** :
      le panneau intitulé « Calques » ne contient plus que des actions sur
      des calques.
- [ ] **UX-3.5 Menu contextuel (clic droit)** — M, ⭐⭐⭐ (C7). Sur un
      élément sélectionné du canevas : Dupliquer (⌘D), Supprimer, Copier le
      style / Coller le style, Devant/Derrière/Avancer/Reculer, Grouper
      dans un calque — toutes des fonctions déjà écrites, seulement
      exposées via un `egui::Response::context_menu()` sur la zone du
      canevas. **Critère d'acceptation** : clic droit sur une forme
      sélectionnée ouvre un menu avec au moins Dupliquer/Supprimer/Ordre,
      chaque action fonctionnellement identique à son équivalent menu/clavier
      existant.

### Sprint UX-4 — Zoom, navigation et réglages de document toujours visibles (S/M, ~3-5 jours)

- [ ] **UX-4.1 Contrôles de zoom persistants** — S, ⭐⭐⭐ (C4). Ajouter
      dans le footer (à droite, à la place du texte statique actuel) trois
      boutons `−` / `100 %` (cliquable, reset) / `+`, plus `Ajuster` en
      icône séparée — appelant les méthodes déjà existantes `zoom_out`/
      `reset_view`/`zoom_in`/`fit_view`. Le menu Vue garde les mêmes actions
      (pas de régression). **Critère d'acceptation** : zoomer avant/arrière
      se fait en un clic depuis le footer, sans ouvrir de menu.
- [ ] **UX-4.2 Déplacer le réglage de fond de canevas** — S, ⭐⭐ (C8).
      Sortir **Fond :** de `options_row` vers le menu **Image** (à côté de
      « Taille du document ») ou **Vue** — un emplacement stable, visible
      quel que soit l'outil actif. **Critère d'acceptation** : changer la
      couleur de fond est possible sans sélectionner un outil de dessin au
      préalable.
- [ ] **UX-4.3 Fichiers récents** — M, ⭐⭐ (C10). `recent_projects:
      Vec<PathBuf>` (borné à 8-10 entrées, le plus récent en tête) persisté
      dans `settings.json` comme `custom_palette`/`shortcuts`, mis à jour à
      chaque `save_project`/`open_project` réussi. Nouvelle entrée
      **Fichier › Ouvrir récent** (sous-menu listant les chemins, ou vide
      avec un message si aucun). **Critère d'acceptation** : après avoir
      enregistré un projet, le rouvrir ne demande plus de naviguer dans un
      dialogue de fichiers.

### Sprint UX-5 — Premier lancement et cohérence des fenêtres modales (M, ~1 semaine)

- [ ] **UX-5.1 Écran d'accueil léger** — M, ⭐⭐. Au tout premier lancement
      (détecté par l'absence de `settings.json`, mécanisme déjà en place
      pour la détection de langue), proposer un état initial autre qu'un
      canevas 1280×800 vide et muet : ouvrir directement la galerie de
      modèles déjà existante (`template_gallery`, Sprint 9a) au lieu du
      document par défaut. **Critère d'acceptation** : un premier lancement
      sans `settings.json` affiche la galerie de modèles avant tout canevas
      vide ; les lancements suivants gardent le comportement actuel
      (dernier document ou nouveau document vierge, au choix produit).
- [ ] **UX-5.2 Comportement de fermeture uniforme des fenêtres modales** —
      S, ⭐. Les fenêtres `egui::Window` du projet (bibliothèque d'éléments,
      presets de style, export par lots, raccourcis, galerie de modèles)
      mélangent `.open(&mut open)` (croix native) et un bouton « Fermer »
      explicite selon les cas — vérifier chacune des 5 fenêtres
      ([toolbar.rs](src/ui/toolbar.rs)) et leur donner systématiquement
      les deux (croix **et** bouton Fermer, jamais l'un sans l'autre).
      **Critère d'acceptation** : les 5 fenêtres modales du projet se
      ferment de manière identique (Échap, croix, bouton).
- [ ] **UX-5.3 Décision d'architecture de menu — documentée, pas juste
      corrigée** — S, ⭐. Le menu top-level **Aligner** est isolé alors que
      ces actions concernent la sélection comme celles proposées en
      UX-3.4 ; documenter dans ce fichier (mise à jour après discussion)
      la décision retenue — fusionner dans **Édition**, garder séparé, ou
      renommer en **Objet** (convention Illustrator/Affinity) — puis
      l'appliquer. Pas de changement de code avant la décision, pour éviter
      un aller-retour.

---

## Ordre d'attaque conseillé

**UX-1** (bugs visibles, quasi gratuit) → **UX-4.1** (zoom persistant,
petit effort/impact fort, complète directement C4) → **UX-2** (barre
d'outils, gros gain de lisibilité quotidienne) → **UX-3** (calques + clic
droit, le plus gros chantier fonctionnel) → **UX-5** (polish, premier
lancement).

Chaque sprint est indépendant des autres (pas de dépendance dure entre eux)
— l'ordre ci-dessus optimise juste le rapport effort/visibilité, pas une
contrainte technique.
