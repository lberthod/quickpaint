# UX_SPRINTS.md — Optimisation UI/UX & fonctionnalité

> Audit ciblé UI/UX (pas un audit technique général — voir `ARCHITECTURE.md`
> pour ça) mené le 4 juillet 2026 : lecture du code de `src/ui/` et
> `src/app/mod.rs`, **et** capture d'écran réelle de l'app lancée
> (`cargo bundle --release`, fenêtre 1400×950) pour vérifier ce qui se
> passe vraiment à l'écran plutôt que de deviner depuis le code seul.
> Chaque constat ci-dessous est référencé par fichier/ligne ou par
> observation directe sur la capture — aucun n'est spéculatif.
>
> **Statut : les 5 sprints sont implémentés** (même session). Vérifié par
> `cargo build`/`cargo test` (115 tests, dont 4 nouveaux ciblant
> spécifiquement l'arithmétique délicate du glisser-déposer de calques) et
> `cargo clippy -- -D warnings` (0 warning). UX-1 a en plus été vérifié par
> capture d'écran réelle (chevauchement du footer corrigé, icônes
> cohérentes). UX-2 à UX-5 n'ont **pas** pu être revérifiés par capture
> d'écran dans cette session — la fenêtre de l'app se retrouvait derrière
> d'autres fenêtres actives de l'utilisateur, et prendre des captures
> d'écran répétées du bureau complet aurait exposé du contenu sans rapport
> avec cette tâche. À vérifier visuellement à l'occasion.

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

- [x] **UX-1.1 Éliminer le chevauchement du footer** (C1) — S, ⭐⭐⭐. ✅
      `footer.rs::show` mesure `ui.available_width()` avant la zone de
      droite et replie le pavé d'aide raccourcis en icône ⓘ (tooltip) sous
      le seuil calculé depuis la longueur du texte. Vérifié par capture
      d'écran : plus de chevauchement à 1100 px de large (fenêtre de
      lancement par défaut).
- [x] **UX-1.2 Coder la sévérité du statut par couleur** (C2) — S, ⭐⭐⭐. ✅
      Implémenté différemment du plan initial, plus simple : `status_error:
      bool` à côté de `status: Option<String>`, plus deux méthodes
      `PaintApp::info()`/`fail()` qui écrivent toujours les deux ensemble
      (pas de `StatusKind`/tuple — un simple booléen suffisait, et évite de
      changer le type du champ). Les ~10 sites qui construisaient un message
      d'erreur (échecs d'export, projet/image refusés) appellent `fail()` ;
      les ~55 autres (confirmations) appellent `info()`. `footer.rs` colore
      rouge/vert selon `status_error`.
- [x] **UX-1.3 Undo/redo en icônes Phosphor** (C9) — S, ⭐. ✅
      `egui_phosphor::regular::ARROW_U_UP_LEFT`/`ARROW_U_UP_RIGHT` (glyphes
      de flèche courbe, plus proches visuellement d'undo/redo que
      clockwise/counter-clockwise), même style que le reste de la barre.

### Sprint UX-2 — Regrouper la barre d'outils (M, ~1 semaine)

Objectif : que la barre d'outils tienne sur une seule rangée à une largeur
de fenêtre raisonnable (1100 px, taille de lancement par défaut), et reste
lisible même quand un 28ᵉ outil s'ajoute plus tard.

- [x] **UX-2.1 Grouper par catégorie avec repli** — M, ⭐⭐⭐. ✅
      7 groupes nommés (`tool_group_key`/`tool_group_label`,
      [toolbar.rs](src/ui/toolbar.rs)) : Navigation, Dessin, Retouche photo,
      Formes (traité en sélecteur secondaire, UX-2.2), Plume & Texte, Effets
      locaux, Composition. Chevron ▸/▾ par groupe, état replié persisté par
      clé stable (`collapsed_toolbar_groups`, `settings.json`). Retouche
      photo/Effets locaux/Composition démarrent repliés par défaut
      (14 icônes visibles au lancement sur 29) ; Navigation/Dessin/Plume &
      Texte restent toujours dépliés. Un outil actif dans un groupe replié
      reste visible malgré tout (jamais d'outil « invisible » en cours
      d'usage).
- [x] **UX-2.2 Sélecteur d'outil secondaire (fly-out)** — M, ⭐⭐. ✅
      `shape_family_selector` : un seul bouton (icône de l'outil Forme actif,
      ou Rectangle par défaut) avec un petit chevron en coin ; clic = popup
      (`egui::popup_below_widget`) listant les 6 formes, se referme dès
      qu'on en choisit une (`PopupCloseBehavior::CloseOnClick`).
- [x] **UX-2.3 Raccourci clavier affiché au survol** — S, ⭐. **Reporté** —
      non fait dans cette passe (le repli par groupe couvrait déjà l'essentiel
      du gain de lisibilité visé par ce sprint) ; reste un bon candidat
      « quick win » pour une prochaine itération.

### Sprint UX-3 — Panneau des calques et menu contextuel (M/L, ~1-2 semaines)

Le plus gros morceau fonctionnel : aligner le panneau de calques sur les
attentes standard, et combler l'absence totale de clic droit.

- [x] **UX-3.1 Glisser-déposer pour réordonner les calques** — M, ⭐⭐⭐
      (C5). ✅ `dnd_drag_source`/`dnd_drop_zone` sur chaque ligne
      ([layers.rs](src/ui/layers.rs)), payload = **id du calque** (pas son
      index — robuste à un index d'UI qui daterait d'une frame précédente,
      cohérent avec le reste de l'historique qui référence toujours par id).
      Nouvelle méthode `PaintApp::reorder_layer(from_id, to_id)`
      ([app/mod.rs](src/app/mod.rs)), même mécanisme que `move_active_layer`
      (`Command::SetLayers`, undo/redo gratuits). 4 tests unitaires dédiés à
      l'arithmétique de recalcul du calque actif après déplacement (avant/
      arrière dans la pile, calque déplacé = calque actif, id inconnu =
      no-op). Boutons ▲▼ conservés en secours.
- [x] **UX-3.2 Panneau redimensionnable** — S, ⭐⭐ (C5). ✅
      `resizable(true)`, `width_range(140.0..=320.0)`, largeur persistée
      (`layers_panel_width`, `settings.json`) — écrite seulement une fois le
      glissé terminé (`!ctx.input(|i| i.pointer.any_down())`), pas à chaque
      frame pendant le drag.
- [x] **UX-3.3 Renommage inline** — S, ⭐⭐. ✅ Double-clic sur le nom d'un
      calque ouvre un `TextEdit` en place (`app.layer_rename: Option<(u64,
      String)>`) ; perte de focus valide le nouveau nom (ignoré si vide/blanc).
      Le champ « Calque actif » plus bas dans le panneau reste disponible
      (pas retiré, juste plus obligatoire pour un renommage simple).
- [x] **UX-3.4 Sortir « Éléments/Aligner/Rogner/Ordre » du panneau Calques**
      — M, ⭐⭐ (C6). ✅ Nouvelle fonction `selection_actions`
      ([toolbar.rs](src/ui/toolbar.rs)), appelée depuis la barre d'options
      de l'outil Sélection : Ordre (Devant/Avancer/Reculer/Fond), Aligner
      (images côte à côte), Rogner + sélecteur de ratio. Le panneau Calques
      ne contient plus que des actions sur des calques.
- [x] **UX-3.5 Menu contextuel (clic droit)** — M, ⭐⭐⭐ (C7). ✅
      `PaintApp::canvas_context_menu` ([app/mod.rs](src/app/mod.rs)) : clic
      droit sur un élément non sélectionné le sélectionne d'abord, puis
      `egui::Response::context_menu` propose Dupliquer/Supprimer, Copier/
      Coller le style, et les 4 actions d'ordre — toutes réutilisent les
      fonctions existantes (`duplicate_selection`, `delete_selection`,
      `copy_style`, `paste_style`, `reorder`), aucune duplication de logique.

### Sprint UX-4 — Zoom, navigation et réglages de document toujours visibles (S/M, ~3-5 jours)

- [x] **UX-4.1 Contrôles de zoom persistants** — S, ⭐⭐⭐ (C4). ✅
      `−` / `100 %` (cliquable, reset) / `+` / Ajuster dans le footer
      ([footer.rs](src/ui/footer.rs) `zoom_controls`), appelant directement
      `zoom_out`/`reset_view`/`zoom_in`/`fit_view`. `footer::show` prend
      maintenant `&mut PaintApp` (avant `&PaintApp`, lecture seule). Menu Vue
      inchangé (mêmes actions, pas de régression).
- [x] **UX-4.2 Déplacer le réglage de fond de canevas** — S, ⭐⭐ (C8). ✅
      Sorti de `options_row` vers le menu **Vue** (à côté de la grille/des
      règles/du magnétisme) — emplacement stable, visible quel que soit
      l'outil actif.
- [x] **UX-4.3 Fichiers récents** — M, ⭐⭐ (C10). ✅ `recent_projects:
      Vec<String>` (bornée à `MAX_RECENT_PROJECTS = 8`, plus récent en tête)
      dans `settings.json`, alimentée après chaque `save_project`/
      `open_project` réussi. `project::open_dialog` renvoie maintenant aussi
      le chemin choisi (avant : seulement le `Document`) pour pouvoir
      l'enregistrer ; nouvelle fonction `project::open_path` pour rouvrir
      sans dialogue. Sous-menu **Fichier › Ouvrir récent**
      ([toolbar.rs](src/ui/toolbar.rs)), grisé si vide.

### Sprint UX-5 — Premier lancement et cohérence des fenêtres modales (M, ~1 semaine)

- [x] **UX-5.1 Écran d'accueil léger** — M, ⭐⭐. ✅
      `i18n::is_first_launch()` (`true` tant qu'aucun `settings.json`
      n'existe, vérifié avant toute écriture de préférence dans la session)
      pilote `show_template_gallery` à la construction de `PaintApp`. Un
      premier lancement ouvre directement la galerie de modèles ; les
      lancements suivants (fichier déjà présent) gardent le canevas par
      défaut.
- [x] **UX-5.2 Comportement de fermeture uniforme des fenêtres modales** —
      S, ⭐. ✅ Audit des 5 fenêtres : bibliothèque d'éléments et export par
      lots avaient déjà croix + bouton ; galerie de modèles avait croix +
      « Annuler » (fait déjà office de fermeture) ; presets de style et
      raccourcis clavier n'avaient que la croix — un bouton « Fermer »
      explicite leur a été ajouté ([toolbar.rs](src/ui/toolbar.rs)). Les 5
      fenêtres ont maintenant systématiquement les deux.
- [x] **UX-5.3 Décision d'architecture de menu** — S, ⭐. ✅ **Décision
      retenue : fusion dans Édition** (plutôt que garder « Aligner » séparé
      ou créer un menu « Objet »). Justification : Édition contenait déjà un
      sous-menu « Disposition » (z-order) portant exactement sur le même
      type d'objet (la sélection) — Aligner rejoint ce sous-menu comme
      voisin plutôt que de rester un menu racine isolé. Effet : 9 → 8 menus
      de premier niveau. Appliqué dans la même passe (pas de changement de
      comportement pour l'utilisateur au-delà de l'emplacement du menu).

---

## Ordre d'attaque conseillé (historique)

**UX-1** (bugs visibles, quasi gratuit) → **UX-4.1** (zoom persistant,
petit effort/impact fort, complète directement C4) → **UX-2** (barre
d'outils, gros gain de lisibilité quotidienne) → **UX-3** (calques + clic
droit, le plus gros chantier fonctionnel) → **UX-5** (polish, premier
lancement).

Chaque sprint est indépendant des autres (pas de dépendance dure entre eux)
— l'ordre ci-dessus optimise juste le rapport effort/visibilité, pas une
contrainte technique. Tous les items ont finalement été réalisés dans une
seule passe (voir statut en tête de document).

## Reste ouvert

- **UX-2.3** (raccourci clavier affiché au survol) : seul item non fait —
  reporté, bon candidat pour une prochaine itération courte.
- **Vérification visuelle interactive de UX-2 à UX-5** : seul UX-1 a été
  confirmé par capture d'écran dans cette session (voir note de statut en
  tête de document).
