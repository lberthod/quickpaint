# Sprints 7+ — Fermer les écarts produit (100 % local)

Suite de [ROADMAP.md](ROADMAP.md) (P0/P1/P2 livrés — moteur raster, ajustements,
polices, guides, plume, booléens…). Ce document planifie la suite, à partir de
l'audit des « 20 fonctionnalités attendues » d'un éditeur type Canva/PhotoFiltre.

## Contraintes non négociables

- **100 % local** : aucune fonctionnalité ne doit dépendre d'un serveur pour
  fonctionner. Le document reste utilisable hors-ligne, avion, sans compte.
- **Sans collaboration** : pas de partage temps réel, pas de lien de partage,
  pas de commentaires multi-utilisateurs. Un seul utilisateur, un seul poste.
- **Sans API externe** : aucun appel réseau vers un service tiers (pas de
  suppression d'arrière-plan « cloud », pas de banque d'images en ligne, pas de
  télémétrie). Si une fonctionnalité a besoin d'un modèle (ex. segmentation),
  il est **embarqué dans l'app** et exécuté **en local, hors ligne**.

Conséquence directe sur l'audit précédent : **sync cloud**, **historique de
versions distant** et **collaboration** sont explicitement **hors périmètre,
par choix produit** — pas un manque à combler mais une non-cible. Ils sont
donc retirés de la feuille de route (voir §4 « Explicitement écarté »).

---

## Sprint 7 — Confort & personnalisation

Objectif : combler les écarts « petit effort, gros confort quotidien »
identifiés dans l'audit (§ Confort & écosystème).

- [x] **7.1 Palette de couleurs personnalisable** — S/M, ⭐⭐⭐
      `custom_palette: Vec<[u8; 3]>` sur `PaintApp` ([app.rs](src/app.rs)),
      persistée dans `settings.json` (même fichier que la langue,
      [i18n.rs](src/i18n.rs) `load_custom_palette`/`save_custom_palette`) —
      local, aucun compte. UI : section **Palette** dans la barre Pinceau
      ([toolbar.rs](src/ui/toolbar.rs)), bouton **+** ajoute la couleur
      courante, clic droit sur une pastille la retire. Nommage de jeux de
      couleurs laissé au backlog (une seule palette plate pour l'instant).
      ✅
- [x] **7.2 Raccourcis clavier personnalisables** — M, ⭐⭐
      Nouveau module [keybindings.rs](src/keybindings.rs) : `ShortcutAction`
      (12 outils) + `KeyBindings` (table action → `egui::Key`, sérialisée par
      nom via `Key::from_name`/`Key::name()`), persistée dans `settings.json`
      ([i18n.rs](src/i18n.rs) `load_shortcuts`/`save_shortcuts`). Le bloc de
      touches câblées en dur dans `handle_shortcuts()`
      ([app.rs:2561](src/app.rs:2561)) est remplacé par une boucle sur
      `ShortcutAction::ALL`. Panneau **Préférences › ⌨ Raccourcis clavier…**
      ([toolbar.rs](src/ui/toolbar.rs) `shortcuts_prefs_window`) : clic sur
      « Changer » puis appui sur la touche voulue (capture prioritaire dans
      `handle_shortcuts`, `Échap` annule) ; échange automatique si la touche
      est déjà prise par un autre outil ; bouton de réinitialisation. Seuls
      les raccourcis « une touche, un outil » sont personnalisables — les
      combinaisons ⌘ (fichier/édition/zoom) restent les conventions macOS
      fixes. ✅
- [x] **7.3 Export par lots / tailles multiples** — M, ⭐⭐
      `export::save_batch()` ([export.rs](src/export.rs)) : redimensionne
      (Lanczos3, crate `image` déjà présente) la capture recadrée vers
      plusieurs tailles et les écrit dans **un seul dossier** choisi une
      fois. Panneau **Fichier › Exporter sous… › 📐 Exporter en plusieurs
      tailles…** ([toolbar.rs](src/ui/toolbar.rs) `batch_export_window`) :
      cases 0.5×/1×/2×/3× (multiples de `Document::size`, aperçu des
      dimensions en direct) + largeur personnalisée optionnelle (hauteur
      déduite du ratio). Réutilise le mécanisme de capture d'écran différée
      existant (`handle_screenshot`), juste un flag `batch_export_requested`
      en plus de `export_requested`. ✅

**Jalon 7** : un utilisateur retrouve son confort (raccourcis à sa main,
palette de marque, export web+print en un clic) sans quitter l'app.

---

## Sprint 8 — Retouche photo avancée

Objectif : finir le bloc « logique PhotoFiltre » resté en 🟡 dans la matrice
de [ROADMAP.md](ROADMAP.md) (niveaux/courbes continus, correcteur).

- [x] **8.1 Réglages continus : niveaux & teinte/saturation** — M, ⭐⭐⭐
      `Adjustment` ([tools/filter.rs](src/tools/filter.rs)) remplace le
      `Filter` discret des calques de réglage : `Preset(Filter)` (les 9
      d'origine, inchangés) + `Levels { black, white, gamma }` (point
      noir/blanc + gamma, formule Photoshop) + `HueSaturation { hue, sat,
      light }` (aller-retour RVB↔HSL par pixel). Sliders dans le panneau de
      calques ([layers.rs](src/ui/layers.rs)) ; le compositeur applique en
      direct via `apply_adjustment` ([compositor.rs](src/render/compositor.rs)),
      même mécanisme F3 qu'avant. Signature de cache dédiée (`hash_key`, FNV)
      car `Adjustment` porte des `f32` (pas de `Hash` dérivable). ✅
- [x] **8.2 Courbes** — L→S (réduit), ⭐⭐
      Plutôt qu'un éditeur à points libres (glisser-déposer, complexe et
      fragile), **courbe à 3 points ancrés** (ombres/tons moyens/hautes
      lumières, x = 0/128/255) interpolée linéairement — `Adjustment::Curves`.
      Couvre le besoin réel (éclaircir les ombres, assombrir les hautes
      lumières, courbe en S) avec 3 sliders au lieu d'un widget de dessin.
      Un éditeur à points libres façon Photoshop reste possible en
      itération future si ce compromis se révèle trop limité à l'usage. ✅
- [x] **8.3 Correcteur (healing brush)** — M, ⭐⭐
      `RasterLayer::heal_stamp`/`heal_stamp_segment`
      ([model/raster.rs](src/model/raster.rs)) : même géométrie que le tampon
      de clonage, mais calcule la moyenne de couleur source vs. destination
      (pondérée par couverture × alpha des deux côtés) et décale chaque pixel
      recopié de cet écart avant de peindre — un mélange de Poisson simplifié
      (décalage de couleur constant, pas de résolution d'équation complète)
      qui garde la texture de la source sans coller un patch qui détonne.
      Nouvel outil `ActiveTool::Healing`, geste partagé avec le tampon de
      clonage (`handle_clone_stamp(.., heal: bool)`), `RasterOp::Heal` pour
      l'étiquette d'undo. ✅

**Jalon 8** : retouche photo à niveau PhotoFiltre/Photoshop Elements sur les
réglages tonals, toujours non destructif.

---

## Sprint 9 — Détourage local (sans API)

Objectif : la fonctionnalité la plus demandée du marché (suppression
d'arrière-plan) — mais **embarquée et exécutée en local**, aucun appel réseau.

- [x] **9.1 Détourage assisté (algorithmique, sans modèle)** — M, ⭐⭐
      Nouvel outil **Détourage** (`ActiveTool::Cutout`) : clic sur le fond à
      retirer → `bucket::flood` (le même flood-fill que le pot de peinture,
      tolérance réglable par slider) sur la composition affichée, puis
      `bucket::feather` (flou boîte 1 canal, nouveau) adoucit la frontière
      binaire en un dégradé de couverture. Le résultat est écrit directement
      dans le **masque de calque peint** existant (P2 #14) plutôt que dans un
      mécanisme séparé — `RasterOp::Cutout` pour l'étiquette d'undo. 0
      dépendance ; couvre le cas fréquent (fond uni/simple). ✅
- [ ] **9.2 Segmentation par modèle embarqué, 100 % local** — L, ⭐⭐⭐
      Pour les fonds complexes : embarquer un modèle léger de segmentation de
      sujet (type U²-Net-portrait ou MODNet quantifié, quelques Mo) directement
      dans le bundle `.app`, exécuté via **`tract`** (runtime ONNX en Rust
      pur — pas de binding Python, pas de dépendance réseau, se notarise
      normalement). Le modèle tourne sur l'image en mémoire, aucune donnée ne
      quitte la machine. Fallback sur 9.1 si l'utilisateur préfère l'algo
      rapide ou si le modèle échoue.
      *Point de vigilance build* : le poids du modèle augmente la taille du
      DMG — à valider avec le pipeline de signature/notarisation existant.
- [x] **9.3 Édition du masque de détourage** — S, ⭐⭐ (dépend de 9.1/9.2)
      Obtenu gratuitement par construction : 9.1 écrit directement dans
      `Layer.mask` (masque de calque peint, [ROADMAP #14](ROADMAP.md)), donc
      le mécanisme d'édition déjà existant (bouton **Éditer le masque** +
      pinceau/gomme pixel) fonctionne immédiatement sur un résultat de
      détourage sans code supplémentaire. ✅

**Jalon 9** : détourage disponible et crédible sans jamais quitter la machine
— ce qui manquait le plus dans l'audit précédent.

---

## Sprint 10 — Bibliothèque d'assets embarquée & templates riches

Objectif : combler « éléments réutisables » et « templates » de l'audit, en
restant 100 % embarqué (aucune banque d'images en ligne).

- [ ] **10.1 Bibliothèque de pictos/formes composées** — M, ⭐⭐ (déjà noté
      backlog ROADMAP #9b) : jeu de pictos SVG basiques (flèches, formes
      composées, icônes UI courantes) **embarqués dans le binaire**
      (`include_bytes!`/`rust-embed`), insérables comme groupe de formes
      éditables (pas une image bitmap figée). Panneau latéral avec recherche
      texte locale (pas d'indexation réseau).
- [ ] **10.2 Templates riches (contenu pré-rempli)** — M, ⭐⭐
      La galerie actuelle ([toolbar.rs](src/ui/toolbar.rs) `templates()`) ne
      fixe que la taille du document. Étape suivante : quelques modèles avec
      contenu de départ (placeholders texte + formes déjà composées, ex. « post
      Instagram promo » avec zone titre/sous-titre/CTA) sérialisés en `.json`
      natif et embarqués dans le binaire — pas de téléchargement, pas de
      catalogue en ligne.
- [ ] **10.3 Presets de dégradés/styles nommés** — S, ⭐
      Dégradés et styles de texte réutilisables mais non enregistrables
      aujourd'hui (ROADMAP #11/#10 notent le manque). Ajouter une liste de
      presets nommés (dégradés, styles de texte) sauvegardés localement,
      appliqués en un clic — complète 7.1 côté palette de couleurs.

**Jalon 10** : composition rapide façon Canva, contenu et assets 100 %
embarqués dans l'app, sans jamais interroger un service distant.

---

## Ordre d'attaque conseillé

**7.1 → 7.2 → 7.3** (confort, rapide, aucune dépendance) puis
**8.1 → 8.2 → 9.1** (retouche + détourage rapide, cœur PhotoFiltre) puis
**9.2** (détourage IA local, le plus gros chantier technique) puis
**10.1 → 10.2 → 10.3 → 9.3 → 8.3** (finitions Canva + polish).

- **Jalon A — « Confort d'abord »** : Sprint 7 complet.
- **Jalon B — « PhotoFiltre+ »** : Sprint 8 + 9.1.
- **Jalon C — « Détourage local »** : 9.2 + 9.3 livrés et validés (qualité vs.
  poids du binaire).
- **Jalon D — « Canva hors-ligne »** : Sprint 10 complet.

---

## 4. Explicitement écarté (choix produit, pas un manque)

| Fonctionnalité de l'audit initial | Statut ici | Pourquoi |
|---|---|---|
| Synchronisation cloud + auto-save distant | ❌ Hors périmètre | Contraint « 100 % local » : pas de compte, pas de serveur à opérer/sécuriser |
| Historique de versions distant | ❌ Hors périmètre | Idem — l'historique non-linéaire local ([ROADMAP.md](ROADMAP.md)) couvre déjà le besoin dans une session |
| Collaboration / partage par lien | ❌ Hors périmètre | Contrainte explicite « sans collaboration » |
| Export direct vers réseaux sociaux | ❌ Hors périmètre | Nécessiterait une API externe (OAuth réseau social) |
| Suppression d'arrière-plan « IA cloud » | 🔁 Reformulé en 9.2 | Faisable **en local** avec un modèle embarqué — pas besoin d'API pour ça |

Ces cinq points restent notés ici pour mémoire (traçabilité de l'audit), mais
ne doivent pas revenir en tête de backlog sans une décision produit explicite
qui lèverait la contrainte « 100 % local / sans collaboration / sans API ».
