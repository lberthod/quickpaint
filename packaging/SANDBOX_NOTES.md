# App Sandbox — notes de validation (Sprint 13.9)

> Suite du chantier « Mac App Store » (SPRINTS.md 13+, ROADMAP.md). Objectif :
> lever tôt les mauvaises surprises de sandbox, **avant** la soumission qui
> nécessite le compte développeur. Tout ce qui suit a été vérifié en local,
> par signature ad-hoc, sur cette machine — pas encore avec un vrai
> certificat Developer ID / profil de provisioning.

## Fichier d'entitlements

[QuickPaint.entitlements](QuickPaint.entitlements) — le plus petit jeu qui
couvre les usages réels de l'app :

- `com.apple.security.app-sandbox` (obligatoire pour l'App Store)
- `com.apple.security.files.user-selected.read-write` (dialogues natifs
  `rfd` : ouvrir/enregistrer projet, importer image, exporter, choisir un
  dossier pour l'export par lots)

Volontairement **absente** : toute entitlement réseau — aucune dépendance
réseau dans le projet (ANALYSE.md §8), le sandbox le rend vérifiable par
Apple à la revue, pas seulement affirmé dans le README.

## Méthode de test

Sans certificat Developer ID, on ne peut pas activer le *vrai* App Sandbox
via Xcode/App Store Connect, mais on peut vérifier une bonne partie du
comportement avec une **signature ad-hoc** (`codesign --sign -`), qui active
correctement l'App Sandbox côté noyau du moment que l'exécutable est **dans
un vrai bundle `.app`** (voir « piège » ci-dessous).

```bash
cargo build --release
cargo bundle --release   # → target/release/bundle/osx/QuickPaint.app
codesign --force --deep --sign - \
  --entitlements packaging/QuickPaint.entitlements \
  target/release/bundle/osx/QuickPaint.app

# Vérifier que les entitlements sont bien appliquées :
codesign -d --entitlements - target/release/bundle/osx/QuickPaint.app

# Lancer le diagnostic embarqué (voir plus bas) :
target/release/bundle/osx/QuickPaint.app/Contents/MacOS/quickpaint --sandbox-selftest

# Observer les refus sandbox pendant un usage interactif normal :
/usr/bin/log show --predicate 'process == "quickpaint"' --style compact --last 5m \
  | grep -iE "deny|sandbox"
```

**Piège rencontré en testant** : signer un binaire de test brut (celui produit
par `cargo test --release --no-run`, hors de tout bundle `.app`) avec ces
mêmes entitlements le fait **crasher au tout premier instant**
(`EXC_BREAKPOINT` dans `_libsecinit_appsandbox`, avant qu'une seule ligne de
code Rust ne s'exécute) — l'initialisation de l'App Sandbox par macOS exige
un vrai bundle `.app` (Info.plist, structure `Contents/MacOS/`, etc.), pas
juste un exécutable signé. Le diagnostic doit donc tourner **depuis le
bundle**, jamais sur le binaire de test nu.

## `--sandbox-selftest` : diagnostic embarqué

`quickpaint --sandbox-selftest` ([main.rs](../src/main.rs)) exerce sans
interface les sous-systèmes qui touchent le disque ou un sous-processus,
pour ne pas dépendre de clics manuels à chaque vérification :

- détection de la langue système (sous-processus `defaults read -g AppleLocale`)
- énumération des polices système (`fontdb`, balayage `/System/Library/Fonts`
  etc.)
- **chargement des octets** d'une police (`with_face_data`) — le point qui
  semblait le plus à risque a priori (l'énumération pourrait passer par
  CoreText, sandbox-safe, alors que la lecture réelle du fichier pourrait
  rouvrir le chemin en direct, hors sandbox)
- écriture/lecture disque au même endroit que `settings.json`
  (`~/Library/Application Support/QuickPaint/`) — **sur un fichier dédié**,
  jamais `settings.json` lui-même, pour ne jamais écraser les préférences
  réelles d'un utilisateur qui lancerait ce diagnostic par erreur sur l'app
  installée normalement.

## Résultat des tests (4 juillet 2026, macOS, signature ad-hoc)

```
[sandbox-selftest] langue détectée : En
[sandbox-selftest] polices système énumérées : 379
[sandbox-selftest] octets de police chargés : 10 ok / 0 échec (sur 10 testées)
[sandbox-selftest] écriture/lecture disque (Application Support) : ok
[sandbox-selftest] terminé.
```

Confirmé par lecture du journal système (`log show`) en parallèle :
`AppSandbox request successful` au démarrage, **aucun** message
`Sandbox: ... deny` pendant toute l'exécution, y compris pendant l'appel au
sous-processus `defaults` (connexion `cfprefsd.agent` établie normalement).

| Sous-système | Résultat sous sandbox | Entitlement nécessaire |
|---|---|---|
| Énumération des polices système (`fontdb`) | ✅ fonctionne | aucune |
| Lecture des octets d'une police (`with_face_data`) | ✅ fonctionne (10/10) | aucune |
| Sous-processus `defaults` (détection de langue) | ✅ fonctionne | aucune |
| Lecture/écriture dans `~/Library/Application Support/` | ✅ fonctionne (redirigé vers le conteneur) | aucune |
| Dialogues natifs `rfd` (ouvrir/enregistrer/importer/exporter) | 🟡 non testé (nécessite un clic réel) | `files.user-selected.read-write` (déjà dans le fichier d'entitlements) |
| Presse-papiers `arboard` (coller ⌘V) | 🟡 non testé (nécessite un clic réel) | normalement aucune (pasteboard général) |

**Conclusion** : contrairement à l'hypothèse de départ (fontdb pourrait
scanner le système de fichiers en direct et se heurter au sandbox), **aucun
sous-système non interactif testé ne nécessite d'entitlement au-delà du
strict minimum** déjà dans `QuickPaint.entitlements`. Bonne nouvelle : le
chemin vers l'App Store est plus court que redouté.

## Reste à faire

- [ ] **Test interactif** des dialogues `rfd` et du collage presse-papiers
      sous sandbox (nécessite de cliquer réellement — pas automatisable
      depuis ce diagnostic en ligne de commande). À faire une fois lancée
      une session interactive sur l'app sandboxée.
- [ ] **Signature réelle** : Developer ID / certificat de distribution Mac
      App Store depuis le compte développeur, profil de provisioning associé
      au bundle id `com.lberthod.quickpaint`, puis remplacer la signature
      ad-hoc (`--sign -`) par la vraie identité dans le script de build.
- [ ] **App Store Connect** : fiche produit (captures d'écran, description,
      catégorie « Graphisme et design »), réponse au questionnaire de
      confidentialité (aucune collecte de données — cohérent avec « 100 %
      local », voir ANALYSE.md/SPRINTS.md).
- [ ] **cargo-bundle** ne pose pas les entitlements lui-même : le
      `codesign --entitlements` reste une étape manuelle après
      `cargo bundle --release`, à intégrer dans le script de release final
      (voir SPRINTANALYSIS.md §12.4 pour la commande de signature/notarisation
      complète — à adapter avec `--entitlements packaging/QuickPaint.entitlements`).
