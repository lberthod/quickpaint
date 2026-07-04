# Sprint 12 — Plan d'exécution de l'audit (ANALYSE.md)

> Dérivé directement des 5 recommandations de clôture d'[ANALYSE.md](ANALYSE.md)
> (§10, P1/P2). Contrairement à [SPRINTS.md](SPRINTS.md) (fonctionnalités),
> ce sprint traite uniquement la **qualité du pipeline existant** : fluidité,
> fidélité d'export, robustesse aux entrées, distribution, maintenabilité.
> Aucune nouvelle fonctionnalité utilisateur.

Légende de statut : ✅ fait · 🟡 en cours / partiel · ⛔ bloqué (dépend d'une
action hors du dépôt) · ⬜ pas commencé.

---

## 12.1 — Fluidité : mesurer puis supprimer la recomposition plein cadre

**Statut : ✅ mesuré et corrigé (v1).**

### Diagnostic (avant correctif)

Le compositeur ([render/compositor.rs](src/render/compositor.rs)) cache déjà
un `Pixmap` par calque, invalidé par un hash de contenu — un calque **inchangé**
n'est pas retouché. Le problème ne se voit qu'à l'intérieur d'un calque en
train d'être peint :

- `layer_hash()` mélange `raster.content_hash()`, qui boucle sur **toutes**
  les tuiles du calque à chaque frame pour détecter un changement — coût
  proportionnel à la surface déjà peinte, pas au dab courant.
- Quand le calque est détecté « sale », `raster_content()` appelle
  `raster.flatten()` : alloue et recopie la **boîte englobante complète** de
  toutes les tuiles peintes, convertit chaque pixel en RGBA prémultiplié, puis
  blitte le tout — à **chaque frame** d'un coup de pinceau, même si le dab ne
  touche qu'une ou deux tuiles de 256×256.

Mesure (test `bench_full_layer_repaint_cost`, ignoré par défaut — lancer avec
`cargo test --release bench_full_layer_repaint_cost -- --ignored --nocapture`) :
sur un calque raster de 4096×4096 entièrement peint (256 tuiles), 50 dabs
single-tile via l'ancien chemin (`flatten` + reconversion complète à chaque
dab) contre le nouveau chemin (blit incrémental des tuiles sales
uniquement) — **mesuré sur la machine de dev : 5,27 s vs 51 ms pour 50
appels, soit ≈103× plus rapide** sur un calque déjà bien rempli (le gain
croît avec la quantité déjà peinte, ce qui est exactement le cas qui
dégradait le plus l'expérience).

### Correctif livré

`Compositor` garde maintenant, par calque, un **cache de hash par tuile**
(`HashMap<TileKey, u64>`) en plus du `Pixmap` de contenu peint. À chaque
rebuild d'un calque sale :

1. Diff tuile par tuile entre l'état courant et le cache précédent (coût
   `O(nombre de tuiles)`, déjà présent avant, inévitable pour détecter un
   changement sans instrumenter le modèle).
2. Seules les tuiles **effectivement différentes** sont converties et
   blittées dans le `Pixmap` de contenu peint persistant (`O(tuiles sales)`
   au lieu de `O(surface totale peinte)`).
3. Les éléments vectoriels (traits/images/texte) sont toujours redessinés
   par-dessus depuis zéro sur une copie de ce pixmap — coût `O(w×h)` par
   calque sale, memcpy pur, déjà présent avant et nettement moins coûteux
   que la conversion pixel par pixel qui vient d'être supprimée.

### Restant au backlog (noté honnêtement, pas résolu ici)

- La copie finale `O(w×h)` (memcpy raster → calque composé) subsiste : pour
  aller plus loin il faudrait propager un **rectangle sale** jusqu'au
  compositing multi-calques (fusion/écrêtage séquentiels), ce qui suppose de
  changer la signature de `rebuild()` pour accepter une région au lieu de
  toujours produire l'image entière. Reporté : le gain (v1) traite déjà le
  goulot dominant identifié par la mesure.
- Pas de bascule GPU (wgpu) engagée — la mesure ne la justifie pas : le CPU
  n'est plus le facteur limitant après ce correctif pour les tailles de
  document visées (jusqu'à 4K).

---

## 12.2 — Qualité perçue : export à la résolution native du document

**Statut : ✅ fait.**

### Diagnostic

`export::save_dialog`/`save_batch` recadraient une **capture d'écran du
viewport** (`Event::Screenshot`) — la résolution exportée dépendait donc de
la taille de la fenêtre et du facteur d'échelle de l'écran, pas de la taille
réelle du document. Un document 4000×3000 affiché dans une fenêtre de
1100 pt exportait un bitmap sous-échantillonné ; l'export « par lots »
2×/3× ré-agrandissait ensuite ce qui était déjà dégradé (Lanczos sur une
perte déjà actée).

### Correctif

L'export rend maintenant le document **directement via le compositeur
tiny-skia**, à sa taille native (`doc.size`), indépendamment du zoom/de la
fenêtre à l'écran :

- Nouvelle méthode `Compositor::render_to_rgba(ctx, doc, bg) ->
  (u32, u32, Vec<u8>)` qui réutilise le même chemin de rendu que l'affichage
  (calques, modes de fusion, masques, calques d'ajustement, dégradés) sans
  dépendre de la capture d'écran. `bg` est peint en fond opaque avant la
  composition, comme le fait le canevas à l'écran — un export garde le même
  rendu visuel qu'avant (pas de zones transparentes imprévues).
- `export::save_dialog`/`save_batch` prennent désormais `(w, h, &[u8])`
  directement au lieu de `(ColorImage, Crop)` : le type `Crop` (recadrage sur
  la zone visible du **viewport**) n'a plus de sens une fois la sortie
  toujours au format natif du document — supprimé plutôt que gardé mort.
  `app::request_export`/`request_batch_export` appellent ce rendu
  synchrone et n'ont donc plus besoin de `ctx.send_viewport_cmd(Screenshot)`
  ni de l'aller-retour d'une frame (la capture d'écran différée reste
  utilisée par ailleurs pour le pot de peinture et le détourage, qui
  échantillonnent légitimement les pixels *affichés* sous le clic).
- Bug latent trouvé et corrigé en écrivant le test d'export du texte : le
  compositeur capturait un instantané de l'atlas de glyphes egui **avant**
  que la mise en page du texte n'y insère les glyphes nécessaires. En usage
  normal (plusieurs frames), l'atlas se trouvait déjà « réchauffé » par une
  frame précédente et le bug passait inaperçu ; en rendu ponctuel (l'export,
  justement, ou un calque qui vient de passer en mode composite), le texte
  aurait pu manquer à l'appel. Corrigé par une pré-passe qui force la mise en
  page de tous les textes d'un calque **avant** de capturer l'instantané de
  l'atlas. Vérifié par test (`compositor::tests::render_to_rgba_includes_text`).

### Effet mesurable

Un document A4 300 dpi (2480×3508) exporte désormais exactement à cette
résolution quelle que soit la taille de la fenêtre à l'écran — avant, il
était plafonné à la résolution du viewport (souvent < 1200 px de large sur
un MacBook non-Retina, ou fixé par la fenêtre côté Retina).

---

## 12.3 — Robustesse : bornes d'entrée, version de format, erreurs explicites

**Statut : ✅ fait.**

### Ce qui existait déjà (retrouvé non commité en cours d'audit)

Un plafond `MAX_IMAGE_SIDE = 16_384` px existait déjà côté décodage
(`model::image::decode_png_b64`, `model::raster::decode`,
`project::import_image_dialog`) — bon réflexe, complété ici.

### Complété dans ce sprint

- **Version de format projet** : `Document::format_version` (défaut `1` pour
  les anciens projets sans le champ), stampée à `CURRENT_FORMAT_VERSION` à
  chaque sauvegarde. À l'ouverture, un projet dont la version dépasse celle
  supportée par le binaire est **refusé avec un message explicite** (« ce
  projet a été créé par une version plus récente de QuickPaint ») plutôt que
  silencieusement mal interprété.
- **Erreurs de chargement explicites** : `project::open_dialog()` et
  `project::import_image_dialog()` renvoient désormais
  `Option<Result<T, String>>` (`None` = dialogue annulé, `Some(Err(msg))` =
  fichier invalide/corrompu/trop grand, `Some(Ok(_))` = succès). L'UI
  affiche le message dans la barre de statut au lieu d'échouer en silence.
- **Bornage du collage presse-papiers** (`paste_image`, seul chemin qui ne
  passait pas encore par un plafond) et des **dialogues de redimensionnement**
  (`resize_document`, `resize_canvas`, `new_document_sized`) : les dimensions
  saisies sont maintenant bornées à `MAX_IMAGE_SIDE` avec message si l'entrée
  est hors bornes, au lieu d'allouer sans limite.
- **Bornage de la surface totale** (`MAX_IMAGE_PIXELS`) en plus du côté max,
  pour bloquer un ratio d'aspect extrême (ex. 16000×2 déclaré comme
  légitime par le seul plafond de côté).

---

## 12.4 — Distribution : tag, release, App Store

**Statut : ⛔ partiellement bloqué — nécessite une action locale du mainteneur.**

Ce que je **ne peux pas faire depuis cet environnement** : signer et notariser
le `.dmg` (nécessite le trousseau macOS avec le certificat Developer ID du
compte Apple de Loïc Berthod, et une session `xcrun notarytool` authentifiée
— identifiants qui n'existent pas dans ce dépôt ni dans cet environnement
d'exécution). Publier un DMG non signé sous un tag `v0.12.2` serait pire que
ne rien publier : Gatekeeper bloquerait les utilisateurs et le tag laisserait
croire qu'une release existe.

**Ce qui a été préparé** pour que l'étape manuelle soit la plus courte
possible : version Cargo alignée (`0.12.2`), CHANGELOG à jour, README pointant
déjà vers `github.com/lberthod/quickpaint/releases`, CI verte sur `main`.

**Reste à faire, par toi, en local (macOS avec ton certificat Developer ID)** :

```bash
cargo build --release
cargo bundle --release                      # → QuickPaint.app
codesign --deep --options runtime -s "Developer ID Application: Loïc Berthod (TEAMID)" QuickPaint.app
create-dmg QuickPaint.app                   # ou hdiutil create
xcrun notarytool submit QuickPaint.dmg --keychain-profile "AC_PASSWORD" --wait
xcrun stapler staple QuickPaint.dmg
git tag v0.12.2 && git push origin v0.12.2
gh release create v0.12.2 QuickPaint.dmg --title "QuickPaint 0.12.2" \
  --notes-file CHANGELOG.md
```

Note : cette commande (distribution **Developer ID hors App Store**) ne pose
**pas** d'App Sandbox — le sandbox est spécifique au canal App Store, pas une
exigence de la notarisation classique.

Le chantier **Mac App Store** a démarré en parallèle (Sprint 13.9, avant
même la première release signée) — voir [packaging/SANDBOX_NOTES.md](packaging/SANDBOX_NOTES.md) :
entitlements minimaux définis et validés (sans compte développeur, via
signature ad-hoc + inspection du journal système), diagnostic embarqué
(`quickpaint --sandbox-selftest`). Pour une **soumission App Store** (compte
déjà disponible), la commande de signature devient :

```bash
codesign --deep --options runtime \
  --entitlements packaging/QuickPaint.entitlements \
  -s "3rd Party Mac Developer Application: Loïc Berthod (TEAMID)" QuickPaint.app
xcrun productbuild --component QuickPaint.app /Applications \
  --sign "3rd Party Mac Developer Installer: Loïc Berthod (TEAMID)" QuickPaint.pkg
# puis validation/soumission via Transporter ou `xcrun altool`/App Store Connect.
```

Reste (voir SANDBOX_NOTES.md « reste à faire ») : test interactif des
dialogues `rfd`/presse-papiers sous sandbox, provisioning profile réel, fiche
App Store Connect.

---

## 12.5 — Maintenabilité : démembrer `app.rs`

**Statut : ✅ deux extractions faites (édition de plume, puis transformation
de sélection — cette seconde sous SPRINTS.md 13.8).**

`app.rs` faisait 4 617 lignes (+1000/sprint). Extrait :

- **`app/pen_edit.rs`** (232 lignes, ce sprint) : la machine à états de
  réédition des nœuds de plume après coup (`try_start_pen_edit`,
  `hit_test_pen_node`, `apply_pen_drag`, `commit_pen_edit`,
  `handle_pen_node_edit`, `paint_pen_edit`, le type `PenNodeTarget`) — un
  sous-système autonome (état + geste + rendu) qui ne partage que
  `Document`/`Stroke` avec le reste de `app`. Méthodes `pub(super)` (visibles
  du module parent `app` uniquement, comme avant l'extraction — pas un
  élargissement de l'API publique). Devient testable indépendamment : 2 tests
  unitaires ajoutés sur le hit-test de nœud (choix du nœud le plus proche
  sous le seuil, rejet hors seuil), qui n'existaient qu'indirectement avant.
- **`app/transform.rs`** (211 lignes, sprint suivant, SPRINTS.md 13.8) : la
  machine à états de transformation interactive de la sélection (poignées
  d'échelle/rotation de la boîte englobante, glissé, aperçu, undo dédié —
  `transform_handles`, `start_transform_if_handle`, `update_transform`,
  `commit_transform`, `xform_preview`, les types `XformKind`/`TransformDrag`),
  même schéma `pub(super)` que `pen_edit`. 1 test unitaire ajouté (un
  scale/rotate sous le seuil de bruit — clic sans glissé réel — ne pousse pas
  de commande d'undo).

Au total, `app.rs` → `app/mod.rs` (4 297 lignes) + 2 sous-modules
(443 lignes) — une réduction réelle mais modeste de l'orchestrateur central ;
la partie qui grossit encore le plus vite (nouveaux outils Sprint 11/13) n'est
pas dans ce périmètre.

**Non fait ici, noté pour la suite** (SPRINTS.md 13.8, mis à jour) : la
sélection proprement dite (marquee/lasso/baguette magique, déplacement,
aligner/répartir, copier/coller de style) est un sous-système encore plus
large et plus transverse que la transformation — touche `history`, `guides`
et le rendu des poignées en même temps. Un futur sprint dédié plutôt qu'une
extraction rapide.

---

## Résumé exécutable

| # | Sujet | Effort réel | Risque | Statut |
|---|---|---|---|---|
| 12.1 | Dirty-rects raster (v1, par tuile) | M | Moyen (chemin de rendu chaud) | ✅ |
| 12.2 | Export natif via compositeur | M | Moyen (chemin d'export) | ✅ |
| 12.3 | Bornes + version + erreurs explicites | S | Faible | ✅ |
| 12.4 | Tag + release signée | — | — | ⛔ action manuelle requise |
| 12.5 | Extraction `app/pen_edit.rs` + `app/transform.rs` | S+S | Faible | ✅ (sélection proprement dite restante, notée 13.8) |

Validation : `cargo test --release` (voir le journal git pour le compte de
tests avant/après) et `cargo clippy --release -- -D warnings` doivent rester
verts après chaque étape — vérifié à chaque commit de ce sprint.
