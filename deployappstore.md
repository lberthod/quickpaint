# Déployer QuickPaint sur le Mac App Store — analyse

État constaté (2026-07-06) : le projet a déjà démarré ce chantier (Sprint 13.9,
voir [CHANGELOG.md](CHANGELOG.md) et [packaging/SANDBOX_NOTES.md](packaging/SANDBOX_NOTES.md)).
Ce document consolide ce qui est fait, ce qui manque, et l'ordre des étapes
restantes.

## 1. Ce qui est déjà en place

- **Entitlements App Sandbox** minimaux dans
  [packaging/QuickPaint.entitlements](packaging/QuickPaint.entitlements) :
  - `com.apple.security.app-sandbox` (obligatoire)
  - `com.apple.security.files.user-selected.read-write` (dialogues `rfd`)
  - Volontairement absente : toute entitlement réseau (l'app est 100 % locale).
- **Validation locale du sandbox** par signature ad-hoc + diagnostic embarqué
  (`quickpaint --sandbox-selftest`) : polices système, sous-processus
  `defaults`, lecture/écriture disque — tout fonctionne sans entitlement
  supplémentaire (voir tableau dans SANDBOX_NOTES.md).
- **Bundle id** fixé : `com.lberthod.quickpaint` (Cargo.toml, Info.plist).
- **Pipeline de build** existant (`make-app.sh`, `make-dmg.sh`) pour la
  distribution Developer ID hors App Store — réutilisable comme base.

## 2. Ce qui manque encore

### 2.1 Compte et certificats (bloquant, hors code)
- [ ] Compte **Apple Developer Program** (99 $/an) au nom de Loïc Berthod ou
      d'une entité.
- [ ] Certificat **"Apple Distribution"** (remplace le "Developer ID
      Application" utilisé pour le DMG — ce sont deux certificats différents).
- [ ] **Profil de provisioning Mac App Store** lié au bundle id
      `com.lberthod.quickpaint`, créé dans App Store Connect / developer.apple.com.
- [ ] Créer l'app dans **App Store Connect** (nom, SKU, bundle id).

### 2.2 Build & signature spécifiques App Store
Le pipeline actuel (`make-app.sh`) signe pour Developer ID + notarisation
(distribution directe). L'App Store demande un chemin différent :

1. `cargo build --release`
2. Bundle `.app` (comme aujourd'hui, via `make-app.sh` ou `cargo bundle`)
3. **Signer avec le certificat "Apple Distribution"**, pas "Developer ID
   Application" :
   ```
   codesign --force --options runtime \
     --entitlements packaging/QuickPaint.entitlements \
     --sign "Apple Distribution: ..." QuickPaint.app
   ```
4. **Créer le `.pkg` d'installation** (l'App Store n'accepte pas de `.app`
   nu ni de `.dmg`) :
   ```
   productbuild --component QuickPaint.app /Applications \
     --sign "3rd Party Mac Developer Installer: ..." QuickPaint.pkg
   ```
   → nécessite un **second certificat** ("3rd Party Mac Developer
   Installer" / "Mac Installer Distribution").
5. **Pas de notarisation manuelle** : App Store Connect notarise/valide
   lui-même à la soumission (contrairement au DMG où c'est fait à la main
   avec `notarytool`).
6. Upload via `xcrun altool` (déprécié) ou plutôt **Transporter.app** /
   `xcrun notarytool` moderne, ou directement Xcode si le projet est ouvert
   dans Xcode (peu probable ici, projet Rust pur → passer par la ligne de
   commande / Transporter).

→ Il manque un script dédié, par ex. `make-appstore-pkg.sh`, distinct de
`make-app.sh`/`make-dmg.sh` (qui restent pour la distribution DMG
directe/GitHub Releases).

### 2.3 Tests interactifs du sandbox (reste à faire selon SANDBOX_NOTES.md)
- [ ] Dialogues `rfd` (ouvrir/enregistrer projet, importer image, exporter,
      export par lots) sous sandbox réel — nécessite un clic manuel.
- [ ] Collage presse-papiers (`arboard`, ⌘V) sous sandbox réel.

Ces deux points sont probablement OK (entitlement déjà couvert) mais pas
encore vérifiés avec un vrai certificat/profil de provisioning — à faire dès
que le compte développeur est actif, avant la soumission.

### 2.4 Fiche App Store Connect (métadonnées produit)
- [ ] Captures d'écran (toutes les tailles requises pour Mac, résolution
      Retina).
- [ ] Description, mots-clés, catégorie (déjà prévue : « Graphisme et
      design », cohérent avec `Cargo.toml` → `public.app-category.graphics-design`).
- [ ] Icône 1024×1024 (déjà générée via `--dump-icon`, à vérifier format
      App Store Connect exact).
- [ ] Questionnaire de confidentialité : réponse simple attendue vu le
      principe « 100 % local, aucune télémétrie, aucune dépendance réseau »
      — cohérent avec l'absence d'entitlement réseau, donc facile à défendre
      en cas de revue.
- [ ] Politique de confidentialité (URL requise même si "aucune collecte de
      données").
- [ ] Prix (gratuit / payant / avec achats intégrés).
- [ ] Version minimale de macOS supportée — actuellement `LSMinimumSystemVersion`
      = 10.15 dans Info.plist ; à confirmer que c'est toujours réaliste vu les
      dépendances (accesskit, muda, winit 0.30) et à tester réellement sur une
      version ancienne si possible, sinon relever la valeur.

### 2.5 Revue Apple — points d'attention probables
- App **non-sandboxée par défaut avant ce sprint** → déjà traité.
- Pas de réseau → pas de risque lié à la confidentialité des données réseau.
- Vérifier qu'aucune API privée n'est utilisée (peu probable en Rust/egui,
  mais `muda` et `winit` touchent à AppKit — à valider par un build signé
  Apple Distribution + un `codesign --verify --strict` avant soumission).
- Fournir un **compte de démonstration** : non applicable (pas de compte
  utilisateur dans l'app).

## 3. Ordre recommandé des étapes

1. Souscrire au compte Apple Developer Program (si pas déjà fait).
2. Créer les certificats Apple Distribution + Mac Installer Distribution,
   et le profil de provisioning pour `com.lberthod.quickpaint`.
3. Créer la fiche app dans App Store Connect.
4. Écrire `make-appstore-pkg.sh` (signature Apple Distribution + `productbuild`).
5. Tester interactivement les dialogues `rfd` et le presse-papiers sous
   sandbox avec la vraie signature (lever les derniers doutes de
   SANDBOX_NOTES.md).
6. Préparer captures d'écran, description, politique de confidentialité.
7. Soumettre via Transporter / `xcrun notarytool` + App Store Connect.
8. Répondre au questionnaire de confidentialité et attendre la revue Apple.

## 4. Effort restant estimé

- **Code/scripts** : faible — l'essentiel (entitlements, sandbox, bundle)
  est déjà fait. Le principal ajout est le script de packaging `.pkg` et
  quelques tests interactifs.
- **Administratif** : le plus gros du reste — compte développeur,
  certificats/profils, fiche App Store Connect, politique de confidentialité,
  captures d'écran. Rien de technique, mais incompressible en délai (revue
  Apple, génération de certificats).
