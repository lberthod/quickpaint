# audit_uix_expert.md — Avis critique UI/UX expert (29 août 2026)

Angle : critique de designer produit / UX senior, pas d'audit de code. Basé
sur la lecture directe de la construction de l'interface (`app/mod.rs`,
`ui/toolbar.rs`, `ui/layers.rs`, `keybindings.rs`, `i18n.rs`) — pas de
capture d'écran (permission Enregistrement d'écran non accordée à ce
jour), donc certains constats sont formulés comme des **hypothèses à
vérifier visuellement**, pas des certitudes. Là où c'est le cas, c'est dit
explicitement.

Verdict global : c'est une interface **conçue par itération réelle sur
des retours d'usage** (le journal de corrections C2-C10 dans le code en
atteste — panneau calques figé corrigé, icônes undo/redo remplacées,
message d'erreur repeint en rouge…), ce qui est rare et sain. Mais cette
démarche reste **ad hoc, pas systématique** : aucune évaluation
heuristique formelle, aucun test utilisateur documenté, et au moins un
choix de design entre en contradiction frontale avec le nom même du
projet (« tactile »). Le détail ci-dessous.

---

## 🔴 Critique n°1 — Les tooltips comme seul mécanisme de découverte des 32 outils est incompatible avec un usage tactile

**Fait** : chaque bouton d'outil est une icône seule (police Phosphor,
`ui/toolbar.rs:2110`/`2238-2245`), sans label texte visible. Le nom de
l'outil et son raccourci n'apparaissent qu'au survol (`on_hover_text`).

**Pourquoi c'est un problème sérieux ici précisément** : un tooltip
déclenché par `hover` **n'existe pas au doigt**. Sur un écran tactile, un
tap est soit un clic, soit rien — il n'y a pas d'état intermédiaire
« survol » qui laisse le temps de lire un texte d'aide avant de
déclencher l'action. Un stylet peut simuler un hover selon le pilote,
mais ce n'est pas garanti pour tous les modèles. Résultat concret : sur
un projet dont le nom promet un usage tactile, **32 outils sur 32** sont
actuellement indiscoverables au doigt sans essai-erreur ou lecture
préalable de documentation externe. C'est le genre de défaut qui ne se
voit jamais en testant à la souris (ce qui explique probablement qu'il
n'a pas été détecté par les corrections C2-C10, toutes formulées comme
des observations à la souris/clavier).

**Test à faire** : poser l'app sur un vrai écran tactile ou une tablette
graphique en mode tactile, chronométrer un utilisateur qui n'a jamais vu
l'interface : combien de temps pour identifier correctement 5 outils sur
32 sans aide externe ?

**Piste de correction** (pas à décider par un agent — implication produit) :
soit un mode « libellés visibles » activable, soit un premier lancement
qui affiche les noms 3 secondes, soit une palette de commandes recherchable
(texte tapé → outil), qui ne dépend d'aucun hover.

---

## 🔴 Critique n°2 — Rouge/vert utilisé comme seul signal dans au moins deux endroits sensibles

**Faits** :
- Onion skin (Sprint U) : frame N-1 en teinte rouge, N+1 en teinte verte
  (`app/mod.rs:3703-3704`).
- Statut de fond (succès/échec d'une action) : vert vs rouge
  (`info()`/`fail()`, `app/mod.rs:2153-2163`).

**Pourquoi c'est un problème** : rouge/vert est la paire de couleurs la
moins distinguable pour ~8% des hommes (deutéranopie/protanopie — la
forme la plus fréquente de daltonisme). Pour l'onion skin, un utilisateur
concerné ne peut simplement pas dire quelle frame est avant/après sans
lire les tooltips à chaque fois — ça casse la fonctionnalité, pas juste
l'esthétique. Pour le statut succès/échec, le texte accompagne
probablement la couleur (donc dégradation gracieuse), mais l'onion skin
n'a a priori pas ce filet de sécurité textuel en continu à l'écran.

**Test à faire** : simuler deutéranopie (Sim Daltonism ou équivalent) sur
le panneau Animation avec onion skin actif — vérifier si les deux frames
restent distinguables sans la couleur seule (forme, opacité différente,
etc.).

**Piste de correction** : différencier aussi par un second canal (motif
en pointillés vs plein, ou orange/bleu plutôt que rouge/vert — paire
beaucoup plus sûre), pas seulement la teinte.

---

## 🟠 Critique n°3 — Densité d'icônes dans le panneau de calques : jusqu'à 7 éléments visuels par ligne, certains apparaissant/disparaissant selon l'état

**Fait** : une ligne de calque peut afficher, dans l'ordre : poignée de
drag, œil, cadenas global, cadenas position (si actif), cadenas alpha (si
actif), pastille de couleur, miniature 20×20, nom éditable +
suffixes (`(N traits)`, `%opacité`, préfixe `[clip]`) — `ui/layers.rs:191-320`.

**Pourquoi c'est un problème** : deux défauts cumulés — (1) c'est déjà
dense pour une ligne de liste probablement haute de 30-40px ; (2) les
icônes de verrou granulaire **n'existent visuellement que si elles sont
déjà activées**. Un nouvel utilisateur qui n'a jamais activé
`lock_position`/`lock_alpha` ne voit jamais leur icône et ne peut donc
pas deviner que la fonctionnalité existe en scannant l'interface — elle
n'est découvrable que via un menu contextuel ou une documentation externe.
C'est l'inverse du principe *visibility of system status* : le système a
un état caché qui n'a pas de représentation visuelle par défaut.

**Test à faire** : demander à quelqu'un qui n'a pas lu ce document de
verrouiller uniquement la position d'un calque (pas les pixels) sans
lui dire où chercher. Mesurer s'il trouve le clic-droit/menu adéquat.

**Piste de correction** : une icône « fantôme » (grisée, faible opacité)
visible en permanence pour signaler l'existence de l'option, qui se
« remplit » une fois activée — plutôt que absente/présente.

---

## 🟠 Critique n°4 — 32 couleurs d'accent codées en dur par outil : signal ou bruit ?

**Fait** : `tool_accent()` (`ui/toolbar.rs:2160-2194`) assigne une couleur
RVB fixe et distincte à chacun des 32 outils, indépendante du thème
clair/sombre.

**Pourquoi c'est ambigu sans capture d'écran** : le code ne dit pas si
cette couleur s'affiche en permanence sur l'icône (dans ce cas, 32
teintes différentes sur une seule barre créent un bruit visuel qui nuit
au regroupement par catégorie déjà mis en place) ou seulement à l'état
actif/survolé (dans ce cas c'est un renfort de mémorisation musculaire
utile — « le pinceau est toujours orange »). C'est le point le plus
important à trancher visuellement en priorité, parce que la réponse
change complètement le verdict (atout vs défaut).

**Test à faire** : capture d'écran de la barre d'outils au repos (aucun
outil sélectionné) — si les 32 couleurs sont déjà visibles, c'est un vrai
problème de hiérarchie visuelle à corriger (les groupes créés pour
lutter contre le désordre de l'ancienne barre plate seraient
partiellement annulés par ce bruit chromatique).

---

## 🟡 Critique n°5 — ~~Manque de labels de catégorie visibles~~ — corrigé, constat partiellement inexact

**Correction du constat initial** : en vérifiant le code pour appliquer un
correctif, `tools_row()` (`ui/toolbar.rs:2080-2086`) montre que le chevron
de repli **a déjà** un `on_hover_text(format!("{label} ..."))` avec le nom
de catégorie — l'info n'est pas aussi absente que le premier passage de
lecture l'a laissé penser (l'agent d'exploration avait trouvé
`TOOL_CATEGORY_TITLES`, utilisé dans la fenêtre d'aide, mais pas relu ce
second site). Reste vrai : cette info n'est visible qu'au survol, pas en
permanence — un utilisateur qui scanne la barre sans survoler chaque
chevron ne voit toujours que des séparateurs anonymes. Non corrigé plus
avant (afficher le nom en permanence demanderait de la place horizontale
supplémentaire dans une barre déjà dense — arbitrage produit, pas un fix
mécanique).

---

## 🟡 Critique n°6 — Sliders par défaut pour un usage annoncé « tactile »

**Fait** : au moins 38 `Slider`/`Checkbox`/`ComboBox` dans la barre
d'options contextuelle (`ui/toolbar.rs`, ex. taille/dureté/intensité de
pinceau lignes 2702-2757). Ce sont des sliders standards egui, dont la
zone de préhension est calibrée pour un curseur de souris (précision au
pixel), pas pour un doigt (zone de contact ~8-10mm, bien plus imprécise).

**Pourquoi c'est pertinent** : sans mesure de la largeur réelle des
sliders à l'écran, impossible de conclure avec certitude, mais c'est un
angle mort classique quand une UI est développée et testée principalement
à la souris (ce qui semble être le cas ici, vu que toutes les corrections
C2-C10 documentées sont des observations d'usage souris/clavier). Un
réglage de dureté de pinceau au 1/100e près est un geste précis — au
doigt, sur un slider de 150px de large, chaque incrément vaut un peu plus
d'1px, ce qui est bien en dessous de la résolution tactile humaine.

**Test à faire** : mesurer la largeur effective des sliders à l'écran (en
points), comparer à la cible Apple HIG pour les contrôles tactiles
(44×44pt minimum pour une cible fiable au doigt) — probablement en
dessous pour la hauteur de la piste du slider, même si sa largeur peut
suffire.

---

## 🟢 Points forts constatés (à ne pas perdre en corrigeant le reste)

- **Thème dynamique complet** (Système/Clair/Sombre, `UiTheme`,
  `app/mod.rs:778`) sans `Visuals::light()` forcé résiduel — propre.
- **Historique de corrections UX documenté dans le code** (C2 à C10) :
  rare et précieux — montre qu'un vrai retour d'usage a déjà été
  intégré (fond figé du panneau calques, icônes undo/redo, couleur
  d'erreur, menu contextuel canvas, fichiers récents…). C'est la base
  d'un vrai processus de design continu, à formaliser plutôt qu'à
  laisser en commentaires épars.
- **Tooltip qui affiche le raccourci effectif**, pas juste le nom
  (`ui/toolbar.rs:2238-2244`) — bon réflexe pour les utilisateurs avancés
  au clavier/souris (ne résout pas la Critique n°1 pour le tactile, mais
  bon point pour ce canal-là).
- **Réordonnancement de calques en glisser-déposer** a remplacé des
  boutons ▲/▼ à clic unique (commentaire `ui/layers.rs:177-182`) —
  décision UX qui va dans le bon sens (geste direct plutôt qu'indirection
  par bouton), cohérente avec un usage tactile, à l'inverse de la
  Critique n°1.
- **i18n minimaliste mais fonctionnelle** (`t("fr", "en")` inline,
  `i18n.rs`) : pas scalable à 10 langues, mais honnête pour un produit à
  2 langues — pas de sur-ingénierie.

---

## Ce qui manque structurellement (process, pas pixels)

1. **Aucune évaluation heuristique formelle** (type Nielsen 10 heuristics)
   n'a jamais été faite — les corrections C2-C10 sont des observations
   ponctuelles, pas une passe systématique. Une évaluation heuristique
   complète prendrait une demi-journée et trouverait probablement
   d'autres points que les six ci-dessus.
2. **Aucun test utilisateur documenté** avec un vrai utilisateur naïf
   (pas le porteur de projet). Toutes les corrections listées semblent
   venir de l'auto-observation du développeur, ce qui a un angle mort
   connu : on ne voit pas ses propres réflexes acquis comme des obstacles.
3. **Aucune vérification d'accessibilité visuelle** (contraste, daltonisme,
   VoiceOver — déjà noté dans `audit_aout.md` P2.9) au-delà du thème
   clair/sombre.
4. **Pas de définition de cible tactile explicite** (44×44pt Apple HIG) —
   le nom du projet suggère un usage tactile mais rien dans le code ne
   référence de contrainte de taille de cible tactile documentée.

---

## Plan de test recommandé (priorisé)

1. **Capture d'écran réelle** de la barre d'outils au repos + un calque
   sélectionné — tranche immédiatement les Critiques n°4 (bruit
   chromatique) et confirme/infirme n°3, n°5 visuellement. *(Nécessite la
   permission Enregistrement d'écran sur ce Mac.)*
2. **Test tactile réel** (tablette graphique tactile ou écran tactile,
   pas souris) : 10 minutes, utilisateur naïf, tâche « trouve l'outil
   Pinceau, puis verrouille juste sa position sur un calque ». Valide
   Critique n°1 et n°3 en conditions réelles.
3. **Simulation daltonisme** sur l'onion skin et les messages de statut
   (Critique n°2).
4. **Mesure de contraste** (WCAG AA, ratio 4.5:1 texte normal) sur les
   tooltips et le texte du footer dans les deux thèmes clair/sombre —
   pas vérifié par cet audit, à faire.
5. **Évaluation heuristique complète** (Nielsen) sur l'ensemble de
   l'app — celle-ci ne couvre que 6 points remarqués depuis le code, une
   passe visuelle systématique en trouvera d'autres.

---

## Note de méthode

Cet avis vient de la lecture du code de construction de l'UI, pas d'un
usage réel de l'app — les critiques n°1, n°2, n°3, n°5 sont des faits de
construction directement vérifiables dans le code (peu de marge d'erreur).
La critique n°4 et n°6 sont des hypothèses raisonnables mais **non
confirmées visuellement** — à vérifier en priorité avant d'investir du
temps de correction dessus, pour ne pas corriger un problème qui n'existe
peut-être pas à l'écran.

---

## Correctifs appliqués (29 août 2026)

| # | Critique | Statut | Ce qui a été fait |
|---|---|---|---|
| 1 | Icônes seules, découverte au survol uniquement | ✅ Corrigé | Réglage persisté « Afficher les noms des outils » (menu Vue, `ui/toolbar.rs`) : quand activé, chaque bouton d'outil (52×44 au lieu de 34×30) affiche son nom court sous l'icône, sans dépendre du survol. Désactivé par défaut pour ne pas alourdir l'usage souris/clavier existant — l'utilisateur (ou un premier lancement détectant un usage tactile, à décider séparément) doit l'activer. |
| 2 | Rouge/vert seul pour l'onion skin et les statuts | ✅ Corrigé | Onion skin : orange/bleu (paire sûre pour le daltonisme rouge-vert) au lieu de rouge/vert, tooltip et tooltip du menu Animation mis à jour en conséquence. Statuts succès/échec : icône (✓/⚠) ajoutée en plus de la couleur dans le footer, pour un second canal non-chromatique. |
| 3 | Icônes de verrou granulaire invisibles tant qu'inactives | ⚠️ Non corrigé, ré-examiné | Vérifié en relisant le code (`ui/layers.rs:224-229`) : c'est un choix documenté et assumé (« pas une 3e icône permanente pour un cas d'usage plus rare »), pas un oubli — et le réglage reste accessible dans le panneau « Calque actif » toujours visible, pas caché derrière un menu obscur. Le problème de découvrabilité pure reste réel mais moins grave que formulé initialement ; corriger demanderait de renverser une décision produit déjà prise consciemment, pas juste un fix mécanique. Laissé tel quel. |
| 4 | 32 couleurs d'accent visibles en permanence (bruit visuel) | ✅ Corrigé, confirmé réel | Le code confirmait bien le problème (icône peinte dans la couleur d'accent même au repos, pas seulement au survol/sélection — `tool_button`/`shape_family_selector`, `ui/toolbar.rs`). Corrigé : couleur neutre du thème (`ui.visuals().text_color()`) au repos, accent réservé au survol et à la sélection, où il sert de confirmation plutôt que de fond permanent. |
| 5 | Pas de label de catégorie visible dans la barre | ↩️ Constat corrigé | Le hover du chevron de groupe portait déjà le nom de catégorie — l'audit initial avait raté ce site. Rien à corriger, le fait a été rectifié dans la section correspondante ci-dessus. |
| 6 | Sliders potentiellement trop fins pour un doigt | ⏸️ Non corrigé, en attente | Nécessite une mesure de la largeur/hauteur réelle des sliders à l'écran (capture requise, permission non accordée) avant de pouvoir dimensionner un correctif sans deviner — redimensionner à l'aveugle risquerait de casser la mise en page dense de la barre d'options pour un problème peut-être déjà acceptable en pratique. |

**Validation** : `cargo check` et `cargo clippy` propres (0 nouveau
warning), 325/325 tests toujours verts après ces changements.
