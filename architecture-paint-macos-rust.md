# Paint tactile macOS en Rust — Architecture & logique

Cible matérielle : ViewSonic TD1655 (tactile capacitif 10 points, stylet **passif**, USB-C).
Contrainte clé : pas de pression ni d'inclinaison matérielle → la « pression » est **simulée** (vitesse du trait).

---

## 1. Vue d'ensemble en couches

```
┌──────────────────────────────────────────────┐
│  UI / Outils        (palette, boutons, menus) │  egui
├──────────────────────────────────────────────┤
│  Logique d'application  (état, commandes)     │  Rust pur
│   - outil actif, couleur, taille             │
│   - undo / redo (pile de commandes)          │
├──────────────────────────────────────────────┤
│  Modèle de document  (les données du dessin)  │  Rust pur
│   - couches (layers), traits (strokes)       │
├──────────────────────────────────────────────┤
│  Pipeline d'entrée   (capture du geste)       │  winit / egui events
│   - points bruts → lissage → trait           │
├──────────────────────────────────────────────┤
│  Rendu               (pixels à l'écran)       │  wgpu / tiny-skia
├──────────────────────────────────────────────┤
│  Plateforme / packaging                       │  cargo-bundle + dmg
└──────────────────────────────────────────────┘
```

Principe directeur : **séparer le modèle (données) du rendu (pixels) et de l'UI**.
Le modèle ne sait pas qu'il est affiché ; on peut le tester sans fenêtre.

---

## 2. Choix de la pile technique

| Besoin | Option simple (recommandée) | Option avancée |
|---|---|---|
| Fenêtre + boucle d'évènements | `eframe`/`egui` (gère winit) | `winit` seul |
| UI (palette, boutons) | `egui` | UI custom en wgpu |
| Rendu du canvas | `tiny-skia` (raster CPU) | `wgpu` (GPU) |
| Maths géométriques | `glam` ou types maison | — |
| Packaging .app/.dmg | `cargo-bundle` + `create-dmg` | `cargo-packager` |

**Recommandation pour démarrer** : `egui` pour l'UI + un canvas dessiné soit directement avec le `Painter` d'egui, soit sur une texture rendue par `tiny-skia`. C'est le chemin le plus court vers quelque chose de fonctionnel ; on migre vers `wgpu` seulement si la performance l'exige.

---

## 3. Le modèle de document (cœur du Paint)

C'est la partie la plus importante à bien concevoir. Deux familles d'approches :

### a) Bitmap (raster) — « vrai Paint »
Le dessin = un tableau de pixels (`Vec<u8>` RGBA). On peint directement dedans.
- ✅ Simple, comportement « peinture » classique, taille mémoire fixe
- ❌ Pas d'édition après coup (un trait posé est « cuit »), zoom = pixelisé

### b) Vectoriel (liste de traits) — « strokes »
Le dessin = une liste de traits, chaque trait = liste de points + style.
- ✅ Undo/redo trivial, lissage, rendu net à tout zoom, modifiable
- ❌ Rendu à recalculer, plus de logique

```rust
struct Document {
    layers: Vec<Layer>,
    active_layer: usize,
    size: (u32, u32),
}

struct Layer {
    name: String,
    visible: bool,
    strokes: Vec<Stroke>,
}

struct Stroke {
    points: Vec<StrokePoint>,
    color: [u8; 4],
    base_width: f32,
    tool: Tool,            // Pinceau, Gomme, ...
}

struct StrokePoint {
    pos: (f32, f32),
    width: f32,            // largeur calculée (vitesse → simulation pression)
    t: f32,                // horodatage relatif, sert au calcul de vitesse
}
```

**Approche hybride conseillée** : modèle **vectoriel** pour le trait en cours + l'historique, et on « aplatit » (rasterise) sur un bitmap par couche pour la performance d'affichage. On garde le meilleur des deux : undo facile + rendu rapide.

---

## 4. Pipeline d'entrée tactile (la logique délicate)

Le geste passe par 4 étapes entre le doigt et le pixel :

```
Évènement OS          Lissage             Largeur            Rendu
(x,y,temps)  ──►  filtre + courbe  ──►  vitesse→épaisseur ──► triangles/pixels
```

### Étape 1 — Capture
Sur macOS, le tactile arrive comme évènements **pointeur/souris** (`PointerButton`, `PointerMoved`) via winit/egui. On récupère position + horodatage à chaque mouvement.
- `pressed` → début d'un nouveau `Stroke`
- `moved` (tant que pressé) → on ajoute des points
- `released` → on clôt le trait, on le pousse dans la couche + dans la pile undo

> Limite matérielle : le TD1655 (stylet passif) n'envoie **pas** de pression. Donc `force` n'existe pas ; on la fabrique à l'étape 3.

### Étape 2 — Lissage (smoothing)
Les points bruts sont saccadés. Deux techniques cumulables :
- **Filtre exponentiel** (EMA) sur la position : `p = p_prev*(1-α) + p_brut*α` pour réduire le tremblement.
- **Interpolation par courbe** (Catmull-Rom ou Bézier) entre points clés pour un trait fluide même quand on bouge vite (les évènements OS sont espacés).

### Étape 3 — Simulation de la pression (vitesse → épaisseur)
Astuce centrale pour compenser le stylet passif :
```
vitesse = distance(p, p_prev) / (t - t_prev)
épaisseur = lerp(width_max, width_min, clamp(vitesse / v_ref))
```
Lent = trait épais, rapide = trait fin → rendu calligraphique naturel.
On lisse aussi l'épaisseur elle-même (EMA) pour éviter les sauts.

### Étape 4 — Rendu du trait
Un trait à épaisseur variable n'est pas une simple polyligne : on construit un **ruban** (deux bords parallèles décalés de ±épaisseur/2 perpendiculairement à la direction), puis on remplit. Pour les bouts ronds, on ajoute des demi-disques aux extrémités et aux jointures.

---

## 5. Undo / Redo (pattern Command)

Le modèle vectoriel rend ça simple : chaque action est une commande réversible.

```rust
enum Command {
    AddStroke { layer: usize, stroke: Stroke },
    Erase { layer: usize, removed: Vec<Stroke> },
    AddLayer(Layer),
    Clear { layer: usize, previous: Vec<Stroke> },
}

struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}
```
`undo` = dépiler une commande, appliquer son inverse, la pousser sur `redo`. Simple et robuste.

---

## 6. Boucle principale (logique de frame)

```
loop {
    1. Lire évènements (pointeur, clavier, redimension)
    2. Mettre à jour l'état d'entrée → modifier le trait en cours
    3. Mettre à jour l'UI (egui : palette, sélection d'outil)
    4. Rendre :
         - couches déjà aplaties (bitmap mis en cache)
         - + le trait en cours (recalculé chaque frame)
    5. Présenter la frame
}
```
Optimisation clé : **ne pas re-rasteriser tout le document à chaque frame**. On garde un cache bitmap par couche, invalidé seulement quand la couche change. Seul le trait en cours est redessiné en continu.

---

## 7. Modules Rust suggérés (organisation du crate)

```
src/
  main.rs            // init eframe, lance l'app
  app.rs             // struct App, état global, impl eframe::App
  model/
    document.rs      // Document, Layer
    stroke.rs        // Stroke, StrokePoint
  input/
    capture.rs       // évènements → points bruts
    smoothing.rs     // EMA + Catmull-Rom
    pressure.rs      // vitesse → épaisseur
  render/
    canvas.rs        // rasterisation des traits (tiny-skia)
    ribbon.rs        // génération du ruban épaisseur variable
  tools/
    brush.rs
    eraser.rs
  history.rs         // Command, undo/redo
  ui/
    toolbar.rs       // palette egui
```

---

## 8. Packaging .dmg (résumé)

1. `cargo build --release`
2. `cargo bundle --release` → produit `QuickPaint.app`
3. Signature : `codesign --deep --options runtime -s "Developer ID Application: ..."`
4. `.dmg` : `create-dmg QuickPaint.app` (ou `hdiutil create`)
5. Notarisation Apple : `xcrun notarytool submit ... --wait` puis `xcrun stapler staple`

> Sans compte développeur Apple (99 $/an), le `.dmg` marche sur **ton** Mac mais Gatekeeper bloquera les autres machines (l'utilisateur doit faire « clic droit → Ouvrir »).

---

## 9. Étapes de réalisation conseillées (incrémental)

1. **MVP** : fenêtre egui + canvas, trait simple à largeur fixe au doigt. (1 fichier)
2. Couleur + taille + gomme (outils).
3. Lissage du trait (EMA puis Catmull-Rom).
4. Largeur dynamique (vitesse → épaisseur).
5. Undo/redo (pattern Command).
6. Couches + cache bitmap par couche.
7. Sauvegarde/chargement (PNG export + format projet en `.json`/`bincode`).
8. Packaging .app puis .dmg.

Chaque étape est testable seule. Le modèle (model/, input/, history/) se teste **sans interface** avec de simples `#[test]`.

---

## 10. Pièges spécifiques au TD1655 / macOS

- **Pas de multitouch utile par défaut** : winit reçoit un seul pointeur (le tactile est converti en souris). Pour palm-rejection ou pinch-zoom à 2 doigts, il faudrait descendre vers `NSEvent`/`NSTouch` via `objc2` — à éviter pour un premier projet.
- **DPI / écran externe** : gérer le `scale_factor` (Retina vs TD1655) pour que le pinceau ait la bonne taille en pixels physiques.
- **Latence** : viser le rendu du trait en cours en GPU ou un bitmap partiel ; recalculer tout le document à chaque mouvement donnerait un trait « en retard » sur le doigt.
- **Coordonnées** : bien convertir points écran (logiques) → coordonnées document (avec zoom/pan) dès la capture.
```
