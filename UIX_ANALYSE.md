# UIX_ANALYSE.md — Dossier de conception & plan d'exécution UX/ergonomie

Projet : **QuickPaint** — éditeur de dessin/retouche tactile natif macOS, Rust + egui/eframe.
Version analysée : **0.13.1** (juillet 2026).
Statut : app fonctionnelle, 118 tests, 0 warning clippy, distribuée en `.dmg` signé/notarisé.
Portée de ce document : **uniquement l'UX/ergonomie** (ce que ça fait *ressentir*), pas les
fonctionnalités manquantes (→ [FEATURE_SPRINTS.md](FEATURE_SPRINTS.md)) ni l'architecture
technique (→ [ARCHITECTURE.md](ARCHITECTURE.md)).

> Ce document reprend la trame d'un dossier de conception classique (résumé exécutif,
> architecture condensée, cible UX, épics, cadre agile, backlog en sprints, jalons, risques)
> mais **son contenu part de l'état réel du code**, pas d'un projet hypothétique. Un premier
> audit de ce type (`UX_SPRINTS.md`, 5 sprints, constats C1-C8) a déjà été mené et **entièrement
> implémenté** en v0.13.0/0.13.1 (footer, barre d'outils, panneau calques, menu contextuel, zoom
> persistant — voir [CHANGELOG.md](CHANGELOG.md)) ; ce document repart de zéro sur ce qui reste
> après ce premier passage.

---

## 1. Résumé exécutif

QuickPaint est **déjà un produit démontrable** : dessin vectoriel + raster tuilé, calques
complets, retouche photo locale, texte, formes, export multi-format — tout **100 % local**
(pas de cloud, pas de compte, pas de télémétrie). Le premier audit UX (barre d'outils,
footer, calques, menu contextuel) a corrigé les défauts les plus visibles.

Ce qui reste n'est plus de la « casse visible » mais des écarts au **standard macOS natif** et à
la promesse « tactile » du nom du projet :

| Sujet | État constaté | Écart |
|---|---|---|
| Menu du haut | `egui::menu_button` custom dans un `TopBottomPanel` | Pas de vraie `NSMenu` système : rien dans le menu  ⌘ à gauche du Dock, pas de raccourcis visibles nativement, pas de menu Services/Édition standard macOS |
| Accessibilité | Aucune trace `accesskit`/VoiceOver dans le code | Un utilisateur VoiceOver ne peut pas piloter l'app |
| Gestes tactiles/trackpad | Zoom pincement + pan existent (`app/mod.rs`) | Pas de rotation à deux doigts, pas de retour haptique (Force Touch) |
| Densité d'interaction | 27+ outils, menus multi-niveaux, panneaux redimensionnables | Bon regroupement (Sprint UX précédent) mais aucune palette de commandes (⌘K) pour les utilisateurs avancés |
| Onboarding | Ouvre la galerie de modèles au premier lancement (UX-5) | Pas de tour guidé, pas de tooltips progressifs pour les 27 outils |

**Recommandation de portée** : traiter en priorité l'écart le plus structurel et le plus
coûteux à rattraper plus tard — l'absence de vraie barre de menu macOS et d'accessibilité —
avant d'investir dans du confort avancé (palette de commandes, onboarding).

---

## 2. Constats détaillés (preuves par fichier)

### C1 — Pas de barre de menu système (`NSMenu`)

[src/main.rs](src/main.rs) configure `eframe::NativeOptions` sans intégration de menu natif ;
les menus « Fichier / Édition / Calque / Sélection / Vue / Aide » sont des
`ui.menu_button(...)` dessinés à l'intérieur du `TopBottomPanel::top("toolbar")`
([src/ui/toolbar.rs:781-1024](src/ui/toolbar.rs)), lui-même monté dans
[src/app/mod.rs:4747](src/app/mod.rs). Conséquence concrète : le menu ⌘ en haut à gauche de
l'écran (celui de la barre de menu macOS, toujours visible même fenêtre réduite) reste vide
ou générique — aucune app egui/eframe sans binding natif ne peuple `NSApplication.mainMenu`
par défaut. Un utilisateur macOS qui cherche « Quitter QuickPaint », « À propos », ou un
raccourci appris d'une autre app ne le trouve pas au même endroit que partout ailleurs sur le
système.

### C2 — Aucune intégration accessibilité (VoiceOver)

Recherche exhaustive : aucune occurrence de `accesskit`, `AccessKit`, ni de builder
d'arbre d'accessibilité dans `src/`. `egui` expose un support `accesskit` optionnel côté
`eframe` (feature cargo), non activé dans [Cargo.toml](Cargo.toml). Un utilisateur
malvoyant utilisant VoiceOver ne reçoit aucune lecture d'écran de l'interface — ni les noms
d'outils, ni l'état des calques, ni les valeurs de curseurs.

### C3 — Gestes tactiles limités au zoom/pan

[src/app/mod.rs](src/app/mod.rs) traite le pincement (zoom) et le glissé à deux doigts (pan)
mais aucune rotation de canevas à deux doigts, pourtant standard sur trackpad macOS et
cohérent avec la promesse « tactile » du projet (dessiner en faisant pivoter la feuille comme
sur une vraie table à dessin). Pas de retour haptique Force Touch (ex. accroche sur une
règle/un guide).

### C4 — Pas de palette de commandes

27 outils + menus profonds (jusqu'à 3 niveaux, ex. Édition › Dégradé › …) sans raccourci de
recherche textuelle. `src/keybindings.rs` gère déjà le rebind par outil, mais rien
n'expose une recherche globale « taper pour trouver une action » façon ⌘⇧P — pattern devenu
un standard des apps productivité macOS (Xcode, VS Code, Notion, Linear...).

### C5 — Onboarding minimal

`i18n::read_settings` + logique de première ouverture (galerie de modèles, cf. CHANGELOG
0.13.0) couvre le tout premier lancement, mais aucun mécanisme de découverte progressive des
27 outils au-delà des tooltips statiques déjà en place dans `toolbar.rs`. Un nouvel
utilisateur voit la totalité de la densité fonctionnelle d'un coup.

### C6 — Raccourcis non visibles au niveau système

Puisque les menus sont custom (C1), les raccourcis clavier affichés dans
`ui.menu_button` ne s'enregistrent pas auprès de macOS : ils ne remontent pas dans les
préférences système « Raccourcis clavier » ni dans la recherche Spotlight/Siri de commandes
d'app, contrairement à une app AppKit standard.

---

## 3. Cible UX — MoSCoW

| Priorité | Item |
|---|---|
| **Must** | Barre de menu macOS native (au moins Quitter, À propos, Préférences, Annuler/Rétablir, Copier/Coller reconnus par le système) |
| **Must** | Accessibilité de base (activer la feature `accesskit` d'eframe, vérifier au moins la navigation clavier + lecture des noms d'outils) |
| **Should** | Palette de commandes (⌘⇧P ou équivalent) pour les 27 outils et menus profonds |
| **Should** | Rotation de canevas à deux doigts (trackpad) |
| **Could** | Onboarding progressif (tooltips contextuels au premier usage de chaque outil) |
| **Could** | Retour haptique Force Touch sur accroche de guides/règles |
| **Won't** | Support Touch Bar (matériel abandonné par Apple) |
| **Won't** | Collaboration/cloud (contraire aux contraintes produit, cf. ARCHITECTURE.md §9) |

---

## 4. Épics

| # | Épic | Valeur | Dépend de |
|---|---|---|---|
| U1 | Intégration menu macOS natif | Conformité aux attentes système de base | — |
| U2 | Accessibilité (VoiceOver, navigation clavier) | Rend l'app utilisable par plus d'utilisateurs, conformité Mac App Store | — |
| U3 | Palette de commandes | Confort pour utilisateurs avancés, réduit la profondeur de menu | U1 (réutilise le même registre d'actions) |
| U4 | Gestes trackpad avancés | Renforce la promesse « tactile » | — |
| U5 | Onboarding progressif | Réduit la charge cognitive au premier lancement | — |

---

## 5. Cadre agile & re-planification

Reprend le cadre déjà en usage sur le projet (sprints de 2 semaines, Definition of Ready/Done
avec `cargo fmt` + `cargo clippy -D warnings` + tests + capture d'écran réelle avant de clore un
sprint UX — leçon du sprint précédent, où une capture d'écran a révélé un bug de defaults non
détecté par les tests). Règles de re-planification :

- **Preuve avant clôture** : un item UX n'est « fait » que vérifié par capture d'écran réelle de
  l'app lancée (`cargo bundle --release` ou `cargo run --release`), pas seulement par lecture de
  code — répète la méthode qui a déjà trouvé un bug réel lors du sprint précédent.
- **Spike obligatoire avant U1/U2** : l'intégration `NSMenu` et `accesskit` sous egui/eframe a un
  coût d'investigation inconnu (dépend du support natif d'`eframe` 0.29 sur macOS) — commencer
  chacun par un spike time-boxé (1-2 jours) qui produit une décision écrite, pas du code
  définitif.
- **Aucun nouvel épic tant que U1/U2 (Must) ne sont pas clos** — cohérent avec la contrainte
  produit « app native macOS » du projet.

---

## 6. Backlog par sprints

### Sprint A — Menu macOS natif (Épic U1)
- Spike : évaluer le support `NSMenu` via `eframe`/`winit` sur macOS (menu natif partiel possible
  depuis `winit` 0.29+ ? sinon évaluer `objc2`/`cacao` en complément ciblé, sans réécrire tout le
  chrome — cf. précédent similaire dans le dossier PDF de référence, §2 frontière maison/crates).
- Implémenter au minimum : menu ⌘ (À propos, Préférences, Quitter), Édition (Annuler/Rétablir,
  Copier/Coller reconnus par macOS Services).
- Garder les menus egui existants (Fichier/Calque/Sélection/Vue) tels quels si la bascule
  complète vers `NSMenu` s'avère disproportionnée — objectif : couvrir les attentes système
  minimales, pas une réécriture totale du chrome.
- **Critère de sortie** : `Cmd+Q` fonctionne depuis le menu système, l'app apparaît proprement
  dans le menu ⌘ avec son nom, aucune régression sur les menus existants (tests + capture
  d'écran).

### Sprint B — Accessibilité de base (Épic U2)
- Activer la feature `accesskit` d'`eframe` dans [Cargo.toml](Cargo.toml).
- Vérifier à la main (VoiceOver activé) : navigation au clavier entre outils, lecture du nom de
  l'outil survolé/focus, lecture de l'état des calques (visible/masqué, nom).
- **Critère de sortie** : un parcours minimal (ouvrir un projet, changer d'outil, changer de
  calque) est pilotable et audible avec VoiceOver seul, vérifié en conditions réelles.

### Sprint C — Palette de commandes (Épic U3)
- Construire un registre plat des actions déjà existantes (le code source de chaque
  `ui.menu_button` dans `toolbar.rs` définit déjà ces actions individuellement — les
  centraliser dans une table `(nom, raccourci, closure)` réutilisable à la fois par les menus et
  par la palette).
- UI : overlay de recherche floue (⌘⇧P ou raccourci configurable), navigation clavier
  haut/bas/Entrée.
- **Critère de sortie** : les 27 outils + actions de menu principales sont trouvables et
  exécutables depuis la palette sans souris.

### Sprint D — Gestes trackpad avancés (Épic U4)
- Ajouter la rotation de canevas à deux doigts en s'appuyant sur les évènements de geste déjà
  captés pour le pincement/pan (`app/mod.rs`).
- **Critère de sortie** : rotation fluide, réinitialisable (bouton/raccourci « remettre à
  plat », cohérent avec `reset_view()` déjà existant pour le zoom).

### Sprint E — Onboarding progressif (Épic U5)
- Étendre les tooltips déjà en place (`toolbar.rs`) : un indicateur discret (point/badge) sur les
  outils jamais utilisés par l'utilisateur, disparaissant après premier usage (état à persister
  dans `settings.json`, même mécanisme que les préférences existantes en `i18n.rs`).
- **Critère de sortie** : un nouvel utilisateur voit une hiérarchie visuelle entre outils
  « découverts » et « à découvrir », sans ajouter de flux modal supplémentaire (cohérent avec le
  choix déjà fait d'ouvrir directement la galerie de modèles plutôt qu'un tutoriel).

---

## 7. Jalons

| Jalon | À la fin de | Ce qu'on peut montrer |
|---|---|---|
| J1 — Citoyen macOS de base | Sprint A | Menu ⌘, Quitter, Services reconnus par le système |
| J2 — Accessible | Sprint B | Parcours minimal pilotable au VoiceOver |
| J3 — Confort avancé | Sprint C+D | Palette de commandes, rotation de canevas |
| J4 — Onboarding | Sprint E | Découverte progressive des outils |

---

## 8. Risques

| Risque | Impact | Parade |
|---|---|---|
| Intégration `NSMenu` plus coûteuse que prévu sous egui/eframe | Élevé | Spike time-boxé en tête de Sprint A ; se limiter au strict Must (menu ⌘, Quitter) si le coût dépasse l'estimation de 150 % (règle déjà en usage sur le projet) |
| Support `accesskit` incomplet pour les widgets custom (canevas peint à la main) | Moyen | Prioriser l'accessibilité des menus/panneaux standard d'abord ; traiter le canevas peint comme hors-scope explicite si trop coûteux, documenter la limite plutôt que de bloquer le sprint |
| Régression sur les menus egui existants en modifiant le chrome | Moyen | Garder les menus actuels en fallback, ne remplacer que le strict nécessaire (cf. Sprint A) |

---

## 9. Décisions à prendre avant le Sprint A

- Confirmer si la cible Mac App Store (démarrée, cf. `packaging/SANDBOX_NOTES.md`) impose un
  niveau d'accessibilité minimal à la soumission — orienterait la priorité U1 vs U2.
- Décider du raccourci par défaut de la palette de commandes (⌘⇧P le rendrait cohérent avec les
  standards de l'écosystème dev/productivité, mais QuickPaint cible un public dessin/photo —
  vérifier qu'il n'entre pas en conflit avec un raccourci existant dans `keybindings.rs`).
