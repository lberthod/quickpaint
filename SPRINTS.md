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

- [ ] **7.1 Palette de couleurs personnalisable** — S/M, ⭐⭐⭐
      Remplacer/étendre `recent_colors` ([app.rs:131](src/app.rs:131)) par une
      vraie palette éditable : ajouter/retirer une nuance, nommer un jeu de
      couleurs, persister dans `settings.json` (même fichier que la
      préférence de langue, [i18n.rs](src/i18n.rs)) — donc toujours local,
      aucun compte requis. UI : petit panneau à côté du sélecteur HSV, glisser
      une pastille vers une poubelle pour la retirer.
- [ ] **7.2 Raccourcis clavier personnalisables** — M, ⭐⭐
      `handle_shortcuts()` ([app.rs:2539](src/app.rs:2539)) câble les touches
      en dur. Introduire une table `KeyBindings` (action → touche+modificateurs)
      chargée/sauvée dans `settings.json`, avec un panneau de préférences
      simple (liste d'actions, clic → capture la prochaine touche pressée).
      Défauts actuels conservés si l'utilisateur ne personnalise rien.
- [ ] **7.3 Export par lots / tailles multiples** — M, ⭐⭐
      `export.rs` exporte une image à la fois. Ajouter un dialogue **Fichier ›
      Exporter en plusieurs tailles…** : cocher plusieurs presets (ex. 1×, 2×,
      largeurs cibles en px, ou les formats de la galerie de modèles) →
      ré-échantillonnage + écriture successive dans un dossier choisi, un seul
      clic. Réutilise `crop_rgba`/l'encodage existant, pas de nouvelle
      dépendance.

**Jalon 7** : un utilisateur retrouve son confort (raccourcis à sa main,
palette de marque, export web+print en un clic) sans quitter l'app.

---

## Sprint 8 — Retouche photo avancée

Objectif : finir le bloc « logique PhotoFiltre » resté en 🟡 dans la matrice
de [ROADMAP.md](ROADMAP.md) (niveaux/courbes continus, correcteur).

- [ ] **8.1 Réglages continus : niveaux & teinte/saturation** — M, ⭐⭐⭐
      Les calques d'ajustement actuels ([tools/filter.rs](src/tools/filter.rs))
      sont des presets discrets. Ajouter deux variantes d'`Adjustment` avec
      paramètres continus : `Levels { black, gamma, white }` et
      `HueSaturation { hue, sat, light }`, réglées par sliders dans le panneau
      de calques, recalculées à chaque frame par le compositeur (déjà prévu
      pour une passe de plus, voir F3 dans ROADMAP §2). Aucune dépendance
      externe — arithmétique pixel pure.
- [ ] **8.2 Courbes (RVB + par canal)** — L, ⭐⭐
      Widget courbe (points de contrôle + interpolation spline, `egui::Painter`
      pur) générant une LUT 256 valeurs appliquée par le compositeur, même
      mécanisme que 8.1. Peut réutiliser la table de correspondance pour les 3
      canaux séparés ou en tons.
- [ ] **8.3 Correcteur (healing brush)** — M, ⭐⭐
      Le tampon de clonage existe (décalage figé, [raster.rs](src/model/raster.rs)
      `clone_stamp_segment`). Le correcteur diffère par le **mélange** :
      copier la texture source mais reprojeter la luminance locale de la zone
      cible (mélange de Poisson simplifié ou moyenne glissante) pour effacer
      un défaut sans coller un patch visible. Backlog technique déjà noté
      dans ROADMAP #5.

**Jalon 8** : retouche photo à niveau PhotoFiltre/Photoshop Elements sur les
réglages tonals, toujours non destructif.

---

## Sprint 9 — Détourage local (sans API)

Objectif : la fonctionnalité la plus demandée du marché (suppression
d'arrière-plan) — mais **embarquée et exécutée en local**, aucun appel réseau.

- [ ] **9.1 Détourage assisté (algorithmique, sans modèle)** — M, ⭐⭐
      Première étape sans ML : améliorer la **baguette magique** existante en
      un « détourage en un clic » — flood-fill tolérant sur les bords +
      **feathering** (anti-aliasing du masque résultant, flou gaussien léger
      sur l'alpha) pour un contour propre sur fond uni/simple. Rapide à livrer,
      0 dépendance, couvre le cas fréquent (photo produit sur fond blanc/uni).
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
- [ ] **9.3 Édition du masque de détourage** — S, ⭐⭐ (dépend de 9.1/9.2)
      Le résultat (9.1 ou 9.2) devient un **masque de calque peint**
      ([ROADMAP #14](ROADMAP.md)) déjà existant : l'utilisateur peut retoucher
      au pinceau/gomme pixel les zones mal détourées. Pas de nouveau
      mécanisme, juste brancher la sortie du détourage sur `Layer.mask`.

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
