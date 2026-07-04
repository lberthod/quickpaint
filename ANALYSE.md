# ANALYSE.md — Audit complet du projet QuickPaint

> Audit réalisé le 4 juillet 2026, sur l'état du dépôt au commit `48edec0`
> (Sprint 10) + modifications locales en cours (Sprint 11).
> Périmètre : raison d'être, stack, fonctionnalités, architecture, qualité,
> performance, sécurité, dette technique, recommandations.
>
> **Instantané figé** : ce document décrit l'état du projet à cette date et
> n'est pas remis à jour au fil de l'eau. Les 5 recommandations de clôture
> (§10 P1/P2) ont été traitées dans la foulée sous forme d'un **Sprint 12** —
> voir [SPRINTANALYSIS.md](SPRINTANALYSIS.md) pour le détail, les mesures
> avant/après et ce qui reste ouvert.

---

## 1. Synthèse exécutive

| Axe | Note | Verdict en une ligne |
|---|---|---|
| Raison d'être / positionnement | ★★★★★ | Niche claire et défendable : « PhotoFiltre + cœur de Canva, 100 % local, sur macOS » |
| Pertinence de la stack | ★★★★☆ | Rust + egui/tiny-skia : choix cohérent avec la cible, un point de vigilance sur le rendu CPU |
| Architecture | ★★★★☆ | Couches propres (model/input/render/history/tools/ui), sauf `app.rs` devenu monolithique |
| Fonctionnalités | ★★★★☆ | Couverture remarquable pour 22 commits : raster tuilé, calques d'ajustement, booléens, i18n |
| Qualité du code | ★★★★☆ | 96 tests verts, 0 `unsafe`, 0 `panic!`, 0 warning de build ni de clippy |
| Performance | ★★★☆☆ | Bonnes fondations (tuiles, caches), mais recomposition plein cadre et export par capture d'écran |
| Sécurité | ★★★★☆ | Surface d'attaque minimale (aucun réseau), parsing d'entrées non bornées à durcir |
| Industrialisation | ★★☆☆☆ | Pas de CI, pas de fichier LICENSE, distribution du .dmg incohérente avec le README |

**Conclusion générale** : projet solo d'une maturité inhabituelle. La discipline
produit (ROADMAP/SPRINTS avec non-cibles explicites) et la discipline technique
(tests sur le cœur, undo par tuile, ids stables) sont au-dessus du standard des
projets personnels. Les faiblesses sont concentrées sur trois points :
l'industrialisation (CI/licence/distribution), la taille de `app.rs`, et le
pipeline d'export/recomposition.

---

## 2. Raison d'être et positionnement produit

### 2.1 Le problème visé

QuickPaint vise le créneau entre trois mondes qui se parlent mal sur macOS :

- **Preview/Paintbrush** : trop pauvres (pas de calques, pas de vraie retouche) ;
- **Photoshop/GIMP** : trop lourds, courbe d'apprentissage, abonnement (PS) ;
- **Canva** : rapide mais **en ligne**, avec compte, télémétrie et dépendance cloud.

Le positionnement, formalisé dans [ROADMAP.md](ROADMAP.md) et
[SPRINTS.md](SPRINTS.md), est : *atteindre le niveau d'usage de PhotoFiltre
(retouche légère) et de Canva (composition formes + texte + images), en
empruntant les fondations architecturales des grands (moteur raster tuilé de
GIMP/PS, édition vectorielle après coup d'Illustrator, ajustements non
destructifs de PS)*.

### 2.2 Les contraintes produit — le vrai différenciateur

SPRINTS.md fixe trois contraintes **non négociables** :

1. **100 % local** — aucune fonctionnalité ne dépend d'un serveur ;
2. **Sans collaboration** — un utilisateur, un poste ;
3. **Sans API externe** — pas de cloud, pas de banque d'images en ligne,
   **pas de télémétrie**.

C'est rare et précieux : la plupart des projets listent ce qu'ils feront, ici
sont aussi listées les **non-cibles** (sync cloud, partage, collaboration —
« §4 Explicitement écarté »). Cela protège le projet de la dérive de périmètre
et constitue un argument marketing réel (avion, données sensibles, écoles).

### 2.3 Origine et cible matérielle

Le document de conception d'origine (remplacé depuis par
[ARCHITECTURE.md](ARCHITECTURE.md), à jour du code réel — voir le journal git
pour la version fondatrice) révèle l'origine du projet : un éditeur
**tactile** pour écran ViewSonic TD1655 (tactile
10 points, stylet passif, donc **pas de pression matérielle**). D'où deux choix
structurants toujours visibles dans le code :

- la **pression simulée par la vitesse** du trait ([input/pressure.rs](src/input/pressure.rs)) ;
- le zoom/pan tactile et les cibles de clic généreuses dans l'UI.

**Verdict** : raison d'être claire, différenciée, réaliste (on ne promet pas la
parité Photoshop), et documentée à un niveau professionnel. C'est le point le
plus fort du projet.

---

## 3. Stack technique et pertinence

### 3.1 Inventaire des dépendances directes ([Cargo.toml](Cargo.toml))

| Crate | Rôle | Pertinence |
|---|---|---|
| `eframe`/`egui` 0.29 | Fenêtre + UI immédiate | ✅ Le bon choix pour une app outil solo : itération rapide, pas de XAML/Swift, cross-platform en bonus |
| `egui-phosphor` 0.7 | Icônes vectorielles (police Phosphor) | ✅ Rendu net à toute taille, pas de dépendance aux emojis système |
| `tiny-skia` 0.11 | Compositeur raster CPU (modes de fusion, masques) | ✅ pour la justesse ; ⚠️ CPU-only, voir §6 |
| `image` 0.25 (png/jpeg/webp seulement) | Encodage/décodage images | ✅ default-features off = build léger, réflexe sain |
| `serde`/`serde_json` | Persistance projet `.json` | ✅ simple ; ⚠️ base64 inline, voir §8 |
| `rfd` 0.15 | Dialogues natifs ouvrir/enregistrer | ✅ et déjà compatible sandbox App Store (anticipé dans la roadmap) |
| `arboard` 3 | Presse-papiers (coller ⌘V) | ✅ |
| `fontdb` 0.23 (fs+memmap) | Scan des polices système, chargement paresseux | ✅ exactement l'outil pour ça |
| `geo-clipper`/`geo-types` | Booléens de chemins (union/soustraction/intersection) | ✅ ; ⚠️ binding C++ (Clipper), seule dépendance non-Rust-pur notable |
| `base64` 0.22 | Images embarquées dans le `.json` | ✅ mécaniquement, ⚠️ architecturalement (§8) |

443 crates dans le lock (arbre transitif normal pour egui/winit), **aucune
dépendance réseau** (pas de reqwest/hyper/tokio/rustls dans le lock) — vérifié.

### 3.2 Analyse des choix

**Rust** : pertinent pour un éditeur graphique (pas de GC → latence de trait
prévisible, sécurité mémoire pour du code qui manipule des buffers de pixels
en permanence). Le coût habituel de Rust (temps de compilation, verbosité) est
accepté en connaissance de cause.

**egui (mode immédiat)** plutôt que AppKit/SwiftUI ou Tauri : bon arbitrage
pour un solo dev. On perd le « look natif macOS » et l'accessibilité système
(VoiceOver), on gagne une vitesse d'itération énorme et un seul langage. La
roadmap App Store devra assumer ce non-natif.

**tiny-skia (CPU)** plutôt que wgpu (GPU) : le document d'architecture
recommandait explicitement « on migre vers wgpu seulement si la performance
l'exige » — démarche saine (YAGNI). Le point de bascule approche cependant :
les outils Sprint 11 (flou/netteté/estompe au pinceau) recomposent le cadre à
chaque dab (voir §6).

**Versions** : egui 0.29 date de fin 2024 ; l'écosystème egui casse ses API à
chaque minor. Ce n'est pas urgent, mais chaque sprint qui passe augmente le
coût de la future migration (notamment pour le chantier « pression réelle du
stylet » #15 qui dépend de winit/egui-winit).

**Verdict** : stack minimale, cohérente, chaque dépendance justifiée par un
commentaire dans le Cargo.toml (pratique rare et appréciable). Aucune
dépendance superflue détectée.

---

## 4. Architecture

### 4.1 Découpage en couches (respecté)

```
ui/        toolbar (1453 l.), layers, footer          — egui pur
app.rs     état global, orchestration (4617 l.) ⚠️
tools/     brush, eraser, pen, shape, bucket, boolean,
           guides, filter, assets, eyedropper, hit
history.rs pattern Command, undo/redo non linéaire (862 l.)
model/     document, stroke, text, image, raster (962 l.)
input/     capture, smoothing (EMA+Catmull-Rom), pressure
render/    compositor tiny-skia (675 l.), ribbon, canvas, text
i18n.rs    FR/EN inline, settings.json
export.rs / project.rs / svg.rs / fonts.rs / keybindings.rs
```

Total : **13 260 lignes de Rust** en 39 fichiers, 22 commits.

Le principe fondateur (« le modèle ne sait pas qu'il est affiché ; on peut le
tester sans fenêtre ») est réellement appliqué : `model/`, `tools/`,
`history`, `input/` se testent sans UI, et c'est là que vivent les 96 tests.

### 4.2 Points d'architecture remarquables

- **Moteur raster tuilé** ([model/raster.rs](src/model/raster.rs)) : tuiles
  256×256 allouées à la demande, undo **par tuile** (seules les tuiles
  touchées sont clonées, avant/après) — c'est le mécanisme .xcf/.psd, et il
  est correctement implémenté (`div_euclid`/`rem_euclid` pour les coordonnées
  négatives, 25 tests).
- **Historique par ids stables** ([history.rs](src/history.rs)) : les
  commandes référencent les calques par id, pas par index — suppression et
  réordonnancement ne corrompent pas la pile ; une commande sur un calque
  disparu devient un no-op. C'est le genre de bug que la plupart des éditeurs
  amateurs découvrent en production ; ici il est prévenu par conception.
- **Calques d'ajustement non destructifs** : les 9 filtres réutilisés comme
  passe de compositing plutôt qu'appliqués aux pixels — la fondation F3 de la
  roadmap, réellement livrée.
- **i18n `t(fr, en)`** : les deux langues voyagent côte à côte au site
  d'appel, donc jamais désynchronisées. Pragmatique et malin **pour 2
  langues** ; ne passe pas à l'échelle d'une 3ᵉ (chaque appel devrait changer
  de signature). Arbitrage assumé et documenté.

### 4.3 Le point noir : `app.rs` (4 617 lignes)

Plus du tiers du projet dans un seul fichier : machine à états des outils,
gestion de la sélection, raccourcis, édition de nœuds, dialogues… C'est le
destin classique du « struct App » en egui, mais à ce rythme (Sprint 12+
prévu), le fichier dépassera 6 000 lignes dans l'année. Les candidats à
l'extraction sont visibles : la machine à états de sélection/transformation,
l'édition de plume (`try_start_pen_edit`/`apply_pen_drag`), la gestion des
dialogues modaux. À noter : `ui/toolbar.rs` (1 453 l.) suit la même pente.

---

## 5. Audit fonctionnel

### 5.1 Couverture livrée (vérifiée dans le code, pas seulement déclarée)

- **Dessin** : pinceau vectoriel (lissage Catmull-Rom, largeur par vitesse),
  pinceau/gomme **pixel** sur raster tuilé, pot de peinture flood-fill réel,
  tampon de clonage (⌥+clic), correcteur, détourage flood-fill,
  dodge/burn/éponge/flou/netteté/estompe (Sprint 11).
- **Vectoriel** : ligne/flèche/rect/ellipse/polygone/étoile, dégradés
  linéaires/radiaux, plume Bézier **avec réédition des nœuds après coup**
  (double-clic), booléens de chemins (union/soustraction/intersection).
- **Calques** : visibilité, opacité, 6 modes de fusion, groupes,
  fusion/aplatissement, masques d'écrêtage, **masques peints** (réutilisation
  du moteur raster), calques d'ajustement.
- **Composition** : sélection (clic/rectangle/lasso/baguette), aligner/répartir,
  guides intelligents magnétiques, z-order, copier/coller le style, galerie de
  templates (formats Canva-like), bibliothèque d'assets (Sprint 10).
- **Texte** : polices système en chargement paresseux, faux-gras, alignement,
  contour.
- **Sortie** : PNG/JPEG/WebP/PDF (writer PDF minimal maison, testé), SVG,
  export par lots multi-tailles, projet `.json`.
- **Confort** : i18n FR/EN complet, palette personnalisable, raccourcis
  reconfigurables, historique non linéaire avec panneau, tactile zoom/pan.

### 5.2 Écarts restants vs l'ambition affichée

Tenus à jour honnêtement dans la matrice de ROADMAP.md :

| Manque | Gravité vs le positionnement |
|---|---|
| Pression réelle du stylet (winit/egui-winit ne la transmettent pas ; investigation faite et scopée « L ») | Faible — la cible est le stylet passif |
| Import PSD | Faible — interop d'appel |
| Format projet v2 (zip au lieu de base64 inline) | **Moyenne** — voir §8, ça mord déjà |
| Gestion couleur ICC | Moyenne si l'App Store/print est visé |
| Mac App Store (sandbox + entitlements) | **Élevée** pour la découvrabilité — c'est la roadmap elle-même qui le dit |
| Accessibilité (VoiceOver) | Angle mort non mentionné dans la roadmap |

### 5.3 Cohérence déclaré/livré

Point d'audit important : les cases cochées de ROADMAP.md correspondent à du
code réel et testé (vérifié par sondage : raster tuilé, booléens, guides,
masques, i18n). Les limites sont documentées *dans* les items livrés (ex. :
booléens sans trous — « limite du modèle Stroke, un seul anneau », documenté).
**La documentation ne survend pas** — c'est suffisamment rare pour être noté.

---

## 6. Performance et efficacité

### 6.1 Ce qui est bien conçu

- Tuiles 256×256 éparses : un calque raster vide = 0 octet ; un coup de
  pinceau ne clone que les tuiles traversées (undo compris).
- Cache bitmap par calque, invalidé sélectivement ; le trait en cours est le
  seul élément recalculé chaque frame.
- Polices : métadonnées seules au scan, octets chargés à la première
  utilisation.
- Encodage PNG des images **paresseux** (à la sauvegarde, pas au collage).
- Optimisation déjà réalisée et documentée : l'atlas de glyphes egui
  (plusieurs Mo) n'est plus cloné à chaque `rebuild()` du compositeur.
- `lto = true` + `opt-level = 3` en release ; binaire final ≈ 8,3 Mo, DMG
  ≈ 3,8 Mo — très sobre.

### 6.2 Les deux goulots connus (et un troisième moins visible)

1. **Recomposition plein cadre à chaque dab** : identifié au backlog de
   ROADMAP.md. Pendant une peinture pixel, chaque mouvement recompose tout le
   cadre au lieu du rectangle sale. Les dirty-rects existent côté modèle
   (tuiles), il manque leur propagation à travers le compositing
   (écrêtage/fusion séquentiels). C'est **le** chantier perf prioritaire : les
   outils Sprint 11 (flou/estompe au pinceau) le rendent sensible sur grands
   documents.
2. **Compositing CPU pur** : acceptable aujourd'hui ; à re-mesurer sur un
   document 4K / 10 calques (mesure notée « toujours à faire » dans la
   roadmap — elle devrait être faite avant d'ajouter d'autres outils raster).
3. **Export par capture d'écran du viewport** ([export.rs](src/export.rs)) :
   l'export bitmap recadre `Event::Screenshot` — la résolution exportée est
   donc **bornée par les pixels physiques du viewport**. Un document 4 000 px
   affiché dans une fenêtre de 1 100 pt sur écran non-Retina exporte du
   sous-échantillonné, et l'export par lots « 2×/3× » ré-agrandit ensuite au
   Lanczos ce qui a déjà été perdu. Le compositeur tiny-skia sait pourtant
   rendre le document à sa taille native : l'export devrait passer par lui.
   **C'est la faiblesse fonctionnelle la plus sérieuse relevée par cet audit**
   (non listée dans la roadmap).

---

## 7. Qualité du code et vérification

Mesures effectuées pendant l'audit (build local, macOS) :

| Indicateur | Résultat |
|---|---|
| `cargo test` | **96 tests, 0 échec** (0,19 s) |
| `cargo build --release` | **0 warning** |
| `cargo clippy --release` | 9 warnings cosmétiques au moment de l'audit — **tous corrigés depuis** (0 warning) |
| `unsafe` | **0** dans tout le crate |
| `panic!` | **0** ; `unwrap`/`expect` rares (~34 occurrences, concentrées dans les tests et `main --dump-icon`) |

Les tests couvrent le cœur algorithmique (raster : 25 ; filtres : 12 ;
compositeur : 8 ; historique : 7 ; booléens, guides, bucket, formes : 4
chacun) — c'est-à-dire exactement les endroits où les bugs seraient coûteux.
L'UI (app.rs, toolbar.rs) n'est pas testée, ce qui est normal en egui, mais
c'est aussi là que vivent 6 000 lignes : la logique extraite de `app.rs` (§4.3)
deviendrait testable par la même occasion.

Style : commentaires de haut niveau expliquant le *pourquoi* (y compris les
bugs trouvés en développement, ex. le hit-test des nœuds de plume avec
`press_origin()` — documenté dans la roadmap), nommage cohérent, français
technique homogène. Qualité largement au-dessus de la moyenne.

---

## 8. Sécurité

### 8.1 Posture générale : excellente par construction

- **Aucune dépendance réseau** dans tout l'arbre (vérifié dans Cargo.lock) :
  pas de télémétrie, pas de mise à jour auto, pas d'exfiltration possible.
  La surface d'attaque se réduit aux **fichiers que l'utilisateur ouvre**.
- **Rust sans `unsafe`** : les classes de vulnérabilités mémoire (buffer
  overflow dans les décodeurs, UAF) sont exclues du code du projet ; le
  parsing d'images repose sur la crate `image` (Rust pur pour PNG/JPEG/WebP).
- Écritures disque uniquement via dialogues natifs `rfd` (l'utilisateur
  choisit chaque chemin) + `settings.json` dans
  `~/Library/Application Support/QuickPaint/`. Pas de chemins construits à
  partir de données de fichiers ouverts → pas de traversée de répertoire.
- Le sous-processus `defaults read -g AppleLocale` ([i18n.rs](src/i18n.rs))
  est à arguments fixes — pas d'injection possible.
- App signée et notarisée pour la distribution (Gatekeeper OK).

### 8.2 Points à durcir (par ordre de priorité)

1. **Entrées non bornées** : un `.json` de projet ou une image malveillante
   peut déclarer des dimensions énormes. `decode_png_b64` puis `to_rgba8()`
   alloue `w*h*4` octets sans plafond ; idem `import_image_dialog`
   ([project.rs](src/project.rs)) et le décodage des calques raster aplatis.
   Risque réel : **déni de service par épuisement mémoire** (decompression
   bomb PNG), pas d'exécution de code. Correctif simple : plafonner les
   dimensions acceptées (ex. 16 384 px de côté) et la taille du JSON avant
   parsing.
2. **Erreurs silencieuses au chargement** : `open_dialog` renvoie `None`
   indistinctement pour « annulé », « JSON invalide » et « version
   inconnue » ; `decode()` d'une image qui échoue laisse un item vide sans
   message. Un fichier corrompu devrait le dire à l'utilisateur (intégrité
   plus que sécurité, mais même chantier).
3. **Pas de versionnage du format projet** : aucun champ `version` dans le
   `.json`. Le jour du « format v2 » (déjà en backlog), la migration sera plus
   pénible ; et un fichier v2 ouvert par une vieille version échouera en
   silence (cf. point 2).
4. **Dépendance C++** : `geo-clipper` (Clipper) est la seule brique non-Rust
   notable ; les entrées qu'elle reçoit sont des polygones déjà construits par
   l'app (pas des données de fichier brutes), l'exposition est donc faible.
5. **Hygiène de chaîne d'approvisionnement** : pas de `cargo audit`/`cargo
   deny` en routine (pas de CI du tout). À 443 crates, un contrôle RUSTSEC
   automatisé est le minimum.
6. **Sandbox App Store** : anticipée (rfd compatible) mais pas encore
   activée. L'activer tôt (entitlements minimaux) réduira les surprises.

Aucune vulnérabilité exploitable identifiée ; le profil de risque est celui
d'un lecteur de fichiers local bien écrit.

---

## 9. Industrialisation, distribution, gouvernance

C'est l'axe le plus faible du projet :

- **Pas de CI** (aucun `.github/`) : les 96 tests ne tournent que sur le poste
  du développeur. Une GitHub Action `fmt + clippy + test` coûte 20 lignes.
- **Pas de fichier `LICENSE`** : Cargo.toml et README déclarent MIT, mais le
  texte de la licence n'est nulle part. Juridiquement, MIT sans texte n'est
  pas octroyé proprement.
- **Distribution incohérente** : le README pointe vers `QuickPaint.dmg` *dans
  le dépôt*, mais le `.gitignore` exclut `*.dmg` — le lien est donc **cassé
  sur GitHub**. Le canal propre est une GitHub Release avec le DMG en asset.
- **Artefacts locaux versionnés/présents** : `QuickPaint.app/` et le `.dmg`
  traînent dans l'arbre de travail (ignorés, mais sources de confusion), et
  `.DS_Store` est présent malgré la règle.
- **Pas de CHANGELOG** ni de tags de version (`0.1.0` depuis le début alors
  que 11 sprints sont livrés) : impossible pour un utilisateur de savoir ce
  qui a changé entre deux DMG.
- Commits de qualité (un par sprint, messages descriptifs), mais gros —
  la revue a posteriori d'un sprint entier est difficile.

---

## 10. Recommandations priorisées

### P0 — Vite fait, gros gain (< 1 jour cumulé) — ✅ traité depuis l'audit
1. ~~Ajouter le fichier `LICENSE`~~ ✅ fait (texte MIT).
2. ~~CI GitHub Actions~~ ✅ fait (`clippy -D warnings` + `test` ;
   reste à ajouter : `cargo audit` hebdomadaire).
3. **Corriger la distribution** : 🟡 le README pointe désormais vers les
   GitHub Releases — reste à **publier le DMG en Release** et pousser le tag
   `v0.11.0` (version Cargo déjà alignée).
4. ~~Fixes clippy + alias de types~~ ✅ fait (0 warning).

### P1 — Structurel (à glisser dans les prochains sprints)
5. **Export via le compositeur** plutôt que par capture du viewport :
   résolution native garantie quel que soit l'écran/zoom ; c'est aussi le
   prérequis d'un export « 2×/3× » réellement net.
6. **Borner les entrées** (dimensions d'image max, taille de JSON max) +
   messages d'erreur au chargement + champ `version` dans le format projet.
7. **Dirty-rects à travers le compositing** (déjà au backlog) — à faire
   *avant* d'ajouter de nouveaux outils raster, avec la mesure 4K/10 calques
   comme critère d'acceptation.
8. **Démembrer `app.rs`** : extraire la machine à états de sélection,
   l'édition de plume et les dialogues en modules — testables au passage.

### P2 — Cap produit (inchangé, la roadmap est bonne)
9. **Format projet v2** (zip : JSON + PNG séparés, façon .ora) — réglera
   l'explosion base64 et le versionnage d'un coup.
10. **Mac App Store** (sandbox tôt, entitlements minimaux) — la roadmap a
    raison : la découvrabilité vaut plus qu'une fonctionnalité.
11. Surveiller le point de bascule **CPU → wgpu** avec des mesures, pas à
    l'intuition.

---

## 11. Verdict final

QuickPaint est un projet **atypiquement sain** : un positionnement produit
défendable et écrit noir sur blanc (local-first, non-cibles explicites), une
stack minimale où chaque dépendance est justifiée, des fondations
d'architecture empruntées aux bons modèles (tuiles GIMP/PS, commandes
réversibles, ids stables) et réellement implémentées, 96 tests sur le cœur,
zéro `unsafe`, zéro réseau.

Ses risques ne sont pas là où on les attend d'habitude : le code est bon, la
sécurité est bonne par construction. Les vrais chantiers sont
**l'industrialisation** (CI, licence, releases — trivial à régler),
**deux dettes de pipeline** (export par screenshot, recomposition plein
cadre) et **la croissance de `app.rs`**. Tout est rattrapable, et la roadmap
existante en identifie déjà la moitié elle-même — meilleur signe qui soit sur
la gouvernance du projet.
