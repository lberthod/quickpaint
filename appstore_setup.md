# Configuration App Store Connect / certificats — guide pas-à-pas

Prérequis confirmé : compte **Apple Developer Program** actif (99 $/an).
Ce document couvre les 4 étapes administratives bloquantes identifiées dans
[deployappstore.md](deployappstore.md) §2.1, dans l'ordre à suivre.

## 1. Certificat "Apple Distribution"

C'est le certificat de **signature de code** pour l'App Store (différent du
"Developer ID Application" utilisé par `make-app.sh` pour le DMG hors Store).

### Étapes

1. Ouvrir **Keychain Access** (Trousseau d'accès) sur le Mac.
2. Menu **Trousseau d'accès → Assistant certificat → Demander un certificat
   à une autorité de certification** :
   - Adresse e-mail : celle du compte Apple Developer (lberthod@gmail.com
     ou l'adresse associée au compte payant si différente).
   - Nom commun : `Loic Berthod` (ou le nom légal associé au compte).
   - Cocher **"Enregistré sur le disque"** (pas "Envoyé par courrier
     électronique à une AC").
   - Laisser "Laisser-moi spécifier la paire de clés" décoché sauf besoin
     particulier.
   - Enregistrer le fichier `CertificateSigningRequest.certSigningRequest`
     quelque part (ex. `~/Desktop/`).
3. Aller sur **[developer.apple.com/account/resources/certificates/list](https://developer.apple.com/account/resources/certificates/list)**.
4. Cliquer **+** (nouveau certificat).
5. Choisir **"Apple Distribution"** (pas "Mac App Distribution" qui est
   l'ancien nom séparé iOS/Mac — Apple a unifié sous "Apple Distribution"
   pour macOS + iOS ; si l'interface propose encore les deux types
   séparément, prendre celui explicitement listé pour **macOS App Store**).
6. Uploader le `.certSigningRequest` généré à l'étape 2.
7. Télécharger le certificat généré (`distribution.cer`).
8. **Double-cliquer** sur `distribution.cer` pour l'installer dans le
   Trousseau (catégorie "Connexion" ou "System", selon macOS).
9. Vérifier l'installation :
   ```bash
   security find-identity -v -p codesigning
   ```
   → doit lister une ligne du type
   `"Apple Distribution: Loïc Berthod (TEAMID)"`.

### Second certificat requis : "Mac Installer Distribution"

Nécessaire pour signer le `.pkg` (production build, étape `productbuild`
dans le futur script de packaging). Même procédure :

1. Refaire une demande de CSR (étape 2 ci-dessus) — ou réutiliser la même
   si Keychain Access le permet.
2. Sur developer.apple.com, **+ → "Mac Installer Distribution"**.
3. Uploader le CSR, télécharger, double-cliquer pour installer.
4. Vérifier :
   ```bash
   security find-identity -v -p basic
   ```
   → doit lister `"3rd Party Mac Developer Installer: Loïc Berthod (TEAMID)"`
   (le libellé exact varie selon les versions d'Xcode/macOS, mais contient
   toujours "Installer").

**Noter le TEAMID** (visible dans le nom du certificat ou dans
**Membership** sur developer.apple.com) — il sera réutilisé partout
(profil de provisioning, Info.plist si besoin, scripts de build).

## 2. Enregistrer le bundle ID (App ID)

À faire **avant** de créer le profil de provisioning — le profil a besoin
d'un App ID existant.

1. **[developer.apple.com/account/resources/identifiers/list](https://developer.apple.com/account/resources/identifiers/list)**.
2. **+** → **App IDs** → **App**.
3. Description : `QuickPaint`.
4. Bundle ID : **Explicit** (pas Wildcard) → `com.lberthod.quickpaint`
   (doit correspondre exactement à `Cargo.toml` `[package.metadata.bundle]
   identifier` et à `Info.plist` `CFBundleIdentifier` dans `make-app.sh`).
5. Capabilities : cocher **App Sandbox** uniquement (aucune autre capacité
   — cohérent avec les entitlements minimaux du projet, pas de réseau, pas
   d'iCloud, pas de push).
6. Enregistrer.

## 3. Profil de provisioning Mac App Store

1. **[developer.apple.com/account/resources/profiles/list](https://developer.apple.com/account/resources/profiles/list)**.
2. **+** → sous "Distribution", choisir **"Mac App Store Connect"** (le
   libellé exact peut être "App Store" selon la version de l'interface —
   prendre l'option de distribution App Store, pas Ad Hoc ni Development).
3. Sélectionner l'**App ID** créé à l'étape 2 (`com.lberthod.quickpaint`).
4. Sélectionner le certificat **Apple Distribution** créé à l'étape 1.
5. Nom du profil, ex. `QuickPaint Mac App Store`.
6. Générer, puis **télécharger** le fichier `.provisionprofile`.
7. Installer localement (nécessaire pour la signature en ligne de commande) :
   ```bash
   mkdir -p ~/Library/MobileDevice/"Provisioning Profiles"
   cp QuickPaint_Mac_App_Store.provisionprofile \
      ~/Library/MobileDevice/"Provisioning Profiles"/
   ```
   (Xcode le ferait automatiquement en double-cliquant dessus ; en ligne de
   commande pure comme ici, la copie manuelle suffit.)

Ce profil sera embarqué dans le bundle à la signature :
```bash
cp QuickPaint_Mac_App_Store.provisionprofile \
   QuickPaint.app/Contents/embedded.provisionprofile
```
(étape à ajouter dans le futur `make-appstore-pkg.sh`, avant `codesign`).

## 4. Créer l'app dans App Store Connect

1. **[appstoreconnect.apple.com](https://appstoreconnect.apple.com)** →
   **Mes apps** → **+** → **Nouvelle app**.
2. Plateformes : **macOS** (décocher iOS/autres si affichées par défaut).
3. Nom : `QuickPaint` (vérifier qu'il n'est pas déjà pris — sinon variante,
   ex. `QuickPaint Draw`).
4. Langue principale : français (ou anglais selon le marché visé en
   premier — le projet gère déjà les deux via `i18n.rs`).
5. Bundle ID : sélectionner `com.lberthod.quickpaint` dans le menu déroulant
   (il apparaît automatiquement puisqu'il a été enregistré à l'étape 2).
6. SKU : identifiant interne arbitraire et unique, ex. `quickpaint-macos-001`
   (jamais affiché aux utilisateurs, sert juste de clé interne).
7. Accès utilisateur : "Accès complet" par défaut (pas de restriction
   nécessaire pour un compte individuel).
8. Créer.

À ce stade l'app existe en état **"Préparation pour l'envoi"** — les
métadonnées produit (captures, description, prix, confidentialité) restent
à remplir séparément mais ne bloquent pas le travail de build/signature.

## 5. Vérification de bout en bout avant de continuer

```bash
# Certificats présents
security find-identity -v -p codesigning | grep "Apple Distribution"
security find-identity -v -p basic | grep -i installer

# Profil de provisioning présent
ls ~/Library/MobileDevice/"Provisioning Profiles"

# Cohérence bundle id (doit matcher partout)
grep -n identifier Cargo.toml
grep -n CFBundleIdentifier make-app.sh
```

Si les trois vérifications passent, les 4 points bloquants de
[deployappstore.md](deployappstore.md) §2.1 sont levés — l'étape suivante
est l'écriture de `make-appstore-pkg.sh` (signature Apple Distribution +
`productbuild` + intégration de `embedded.provisionprofile`), couverte dans
deployappstore.md §2.2.
