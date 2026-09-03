# sprint.md — Dette technique & hygiène du dépôt

> Détail complet de chaque sprint : le journal git (messages de commit,
> branche `egui-upgrade` pour T4). Ce document ne garde que le statut et
> les décisions/points ouverts qui ne sont pas ailleurs.

Fait suite à l'audit technique du 12 juillet 2026 (v0.19.0).

| Sprint | Contenu | Statut |
|---|---|---|
| T1 | DMG hors suivi git, dernier warning clippy, profil release (strip/codegen-units) | ✅ Fait |
| T2 | `cargo update`, `.cargo/audit.toml` (advisories Linux-only ignorées avec justification) | ✅ Fait |
| T3 | Découpage de `app/mod.rs` (T3.1-T3.10 : `selection.rs`/`layers_ops.rs`/`io.rs`/`shortcuts.rs`/`raster_paint.rs`/`export_ops.rs`/`canvas_overlay.rs`/`bucket_cutout.rs`/`canvas_input.rs`) + passe `unwrap` | ✅ Fait — 6278 → 4531 lignes. Le seuil initial de < 3000 lignes n'est pas atteint, mais tous les domaines extractibles sans fragmentation artificielle de l'état central (`PaintApp`) l'ont été ; le reste (struct/`Default`/`update()`/`on_exit()`) est le cœur de l'app, pas de la dette. |
| T4 | Migration egui/eframe 0.29 → 0.34 (branche `egui-upgrade`, non fusionnée) | ◐ Voir ci-dessous |
| T5 | Livrabilité (audit_septembre.md / plan_implementation.md) | ✅ Fait — voir ci-dessous |

## T5 — état détaillé (audit du 3 septembre 2026)

CI rouge depuis le 20 juillet (38 lints clippy sur rustc 1.98), sauvegarde
qui perdait le travail non enregistré en silence, bundle encore sandboxé
malgré la décision du 30 août : détail complet et pointeurs de code dans
[audit_septembre.md](audit_septembre.md) et
[plan_implementation.md](plan_implementation.md) (fusionnés dans
[previous_audit.md](previous_audit.md) une fois la release publiée — voir
la section « Ce qui n'a PAS besoin d'action » de ce dernier).

- ✅ CI verte (34 `chunks_exact`→`as_chunks`, 2 alias de type, 1 bloc `?`,
  1 `while let` — mécanique, 0 régression).
- ✅ Écriture atomique (projet/autosave/settings) + erreur d'enregistrement
  affichée au lieu d'être confondue avec une annulation.
- ✅ ⌘S écrit dans le fichier courant, ⌘⇧S = Enregistrer sous, titre de
  fenêtre reflète le document et son état modifié.
- ✅ Modale de confirmation avant de perdre un document modifié (fermeture,
  ⌘Q, menu ⌘, Nouveau, Ouvrir, import PSD/SVG, glisser un fichier).
- ✅ App Sandbox retirée du bundle Developer ID (cassait *Ouvrir récent*).
- ✅ Historique plafonné (768 Mo / 500 commandes).
- ✅ `cargo audit` : 1 vraie vulnérabilité corrigée (`webbrowser`), job CI
  ajouté (`continue-on-error`, `ttf-parser`/`rustybuzz` restent visibles).

**Non fait, reste ouvert** (voir « Décisions à prendre par le porteur de
projet » dans plan_implementation.md) :
- Rebaser/fermer `egui-upgrade` — à faire *après* T5, `app/mod.rs` ayant
  encore bougé.
- Test VoiceOver réel (toujours non fait, nécessite un lecteur d'écran actif).
- Décision produit sur l'écran tactile natif.
- Décision de visibilité sur `ttf-parser`/`rustybuzz` (`.cargo/audit.toml`).
- `Command::SwitchFrame` dédié : `switch_animation_frame` reste le
  `SetDoc` le plus fréquent (à chaque clic sur une frame), clonant tout le
  document deux fois à chaque fois — viable aujourd'hui (plafond
  mémoire T5), mais un vrai delta serait plus propre sur un gros document.

## T4 — état détaillé (branche `egui-upgrade`)

- **T4.0-T4.1** ✅ Faits : cible 0.34 (pas 0.35 — `egui-phosphor` ne suit pas
  encore), `glow` forcé explicitement (évite la bascule silencieuse vers le
  backend `wgpu` devenu défaut depuis eframe 0.30+ — cf. décision Sprint N,
  volontairement non engagé, dans [previous_audit.md](previous_audit.md) §8).
  `cargo clippy --all-targets` zéro
  warning, 299 tests verts.
- **T4.2** ◐ Partiel : menu ⌘ natif macOS, icônes Phosphor, menus
  `toolbar.rs` confirmés visuellement à l'écran. Stylet, presse-papiers,
  DPI vérifiés statiquement (chemins inchangés/indépendants d'egui).
  **VoiceOver non testé** — nécessite un lecteur d'écran actif, à faire par
  le porteur de projet avant tout merge.
- **T4.3** ✅ Vérifié, rien à faire : `winit` reste en 0.30 (inchangé), le
  hack `with_default_menu(false)` ([main.rs](src/main.rs)) reste la seule
  voie (`eframe` 0.34 n'expose toujours pas d'alternative).
- **T4.4** ❌ Bloqué : `usvg`/`fontdb` sont déjà sur leurs dernières
  versions publiées — aucun bump ne réglerait `ttf-parser` (non maintenu,
  RUSTSEC-2026-0192). `cargo audit` sur la branche remonte 11 advisories
  (9 confirmées Linux-only via `muda`, absentes du binaire macOS ; 2
  réelles et non corrigibles — `ttf-parser`/`rustybuzz`, compilées sur
  macOS). **`.cargo/audit.toml` volontairement pas modifié** : décision de
  visibilité sécurité à trancher explicitement par le porteur de projet,
  pas par l'agent.

**Avant de fusionner `egui-upgrade` vers `main`** : test VoiceOver +
décision `audit.toml`, puis reconstruire/notariser le DMG depuis `main`
après fusion.
