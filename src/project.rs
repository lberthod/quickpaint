//! Sauvegarde / ouverture de projet vectoriel en `.json` (étape 7).
//!
//! Format simple et lisible : sérialisation directe du `Document`. Les boîtes
//! de dialogue natives passent par `rfd`.

use crate::i18n::t;
use crate::model::document::CURRENT_FORMAT_VERSION;
use crate::model::{image::check_dims, Document};
use std::path::PathBuf;

/// Ouvre un sélecteur « Enregistrer » et écrit le document en JSON.
/// Renvoie le chemin écrit, ou `None` si annulé / erreur.
pub fn save_dialog(doc: &Document) -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Projet QuickPaint", "QuickPaint project"), &["json"])
        .set_file_name(t("dessin.json", "drawing.json"))
        .save_file()?;
    // Stampée à la version courante à chaque écriture (pas seulement à la
    // création) : rouvrir puis resauvegarder un vieux projet le met à jour.
    let mut doc = doc.clone();
    doc.format_version = CURRENT_FORMAT_VERSION;
    let json = serde_json::to_string_pretty(&doc).ok()?;
    std::fs::write(&path, json).ok()?;
    Some(path)
}

/// Ouvre un sélecteur « Ouvrir » et charge un document JSON.
///
/// `None` : dialogue annulé par l'utilisateur (pas une erreur).
/// `Some((path, Err(message)))` : fichier illisible, JSON invalide, version
/// de format trop récente, ou dimensions hors bornes — message localisé prêt
/// à afficher (ANALYSE.md §8.2 : plus d'échec silencieux au chargement).
/// `Some((path, Ok(doc)))` : succès. Le chemin est toujours renvoyé (succès
/// ou échec) pour que l'appelant puisse alimenter/nettoyer les fichiers
/// récents (UX-4.3).
pub fn open_dialog() -> Option<(PathBuf, Result<Document, String>)> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Projet QuickPaint", "QuickPaint project"), &["json"])
        .pick_file()?;
    let result = load_from_path(&path);
    Some((path, result))
}

/// Charge un document JSON depuis un chemin déjà connu (UX-4.3, fichiers
/// récents) — pas de dialogue, mêmes règles de validation que [`open_dialog`].
pub fn open_path(path: &std::path::Path) -> Result<Document, String> {
    load_from_path(path)
}

fn load_from_path(path: &std::path::Path) -> Result<Document, String> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("{} : {e}", t("fichier illisible", "unreadable file")))?;
    let doc: Document = serde_json::from_str(&data)
        .map_err(|e| format!("{} : {e}", t("projet JSON invalide", "invalid project JSON")))?;
    if doc.format_version > CURRENT_FORMAT_VERSION {
        return Err(format!(
            "{} (v{} > v{})",
            t(
                "ce projet a été créé par une version plus récente de QuickPaint",
                "this project was created by a newer version of QuickPaint"
            ),
            doc.format_version,
            CURRENT_FORMAT_VERSION,
        ));
    }
    check_dims(doc.size.0, doc.size.1)
        .map_err(|e| format!("{} : {e}", t("taille de document invalide", "invalid document size")))?;
    Ok(doc)
}

/// (largeur, hauteur, pixels RGBA) d'une image décodée.
type ImagePixels = (u32, u32, Vec<u8>);

/// Sélecteur d'image ; renvoie `(largeur, hauteur, pixels RGBA)`.
///
/// Mêmes conventions que [`open_dialog`] : `None` = annulé, `Some(Err(_))` =
/// fichier invalide ou dimensions hors bornes (ANALYSE.md §8.2), `Some(Ok(_))`
/// = succès.
pub fn import_image_dialog() -> Option<Result<ImagePixels, String>> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Images", "Images"), &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff"])
        .pick_file()?;
    Some(load_image_from_path(&path))
}

/// Charge une image depuis un chemin déjà connu (glisser-déposer, Sprint
/// L.4) — pas de dialogue, mêmes règles de validation que
/// [`import_image_dialog`].
pub fn import_image_from_path(path: &std::path::Path) -> Result<ImagePixels, String> {
    load_image_from_path(path)
}

fn load_image_from_path(path: &std::path::Path) -> Result<ImagePixels, String> {
    let img = image::open(path)
        .map_err(|e| format!("{} : {e}", t("image illisible", "unreadable image")))?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    check_dims(w, h)?;
    Ok((w, h, img.into_raw()))
}

// --- Récupération après crash (Sprint 1.1) ------------------------------
//
// Même dossier local que `settings.json` (voir `i18n::settings_path`) :
// un unique fichier `recovery.json`, écrasé à chaque autosave. Sa seule
// présence au démarrage signale une session précédente non terminée
// proprement (crash, kill -9) — une fermeture normale le supprime via
// `clear_recovery`.

fn recovery_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join("Library/Application Support/QuickPaint/recovery.json"))
}

/// `true` si un fichier de récupération existe déjà (session précédente
/// interrompue). À vérifier une seule fois, au démarrage, avant toute
/// écriture de la session courante.
pub fn has_recovery() -> bool {
    recovery_path().map(|p| p.exists()).unwrap_or(false)
}

/// Écrit l'état courant du document dans le fichier de récupération.
/// Best-effort : une erreur d'écriture (disque plein, permissions) ne doit
/// pas interrompre la session, donc pas de valeur de retour exploitée par
/// l'appelant au-delà du logging silencieux.
pub fn autosave(doc: &Document) {
    let Some(path) = recovery_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(doc) {
        let _ = std::fs::write(path, json);
    }
}

/// Charge le document depuis le fichier de récupération, mêmes règles de
/// validation que [`open_dialog`].
pub fn load_recovery() -> Result<Document, String> {
    let path = recovery_path().ok_or_else(|| t("dossier utilisateur introuvable", "couldn't find user home directory"))?;
    load_from_path(&path)
}

/// Supprime le fichier de récupération : appelé après une sauvegarde/
/// fermeture propre, ou une fois la restauration proposée à l'utilisateur
/// (acceptée ou refusée — dans les deux cas la session précédente est actée).
pub fn clear_recovery() {
    if let Some(path) = recovery_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Verrou partagé pour les tests qui redirigent temporairement `$HOME`
/// (`recovery_path`, `i18n::settings_path` en dépendent tous les deux).
/// `$HOME` est un état **global au process** : sans ce verrou, deux tests de
/// modules différents qui le redirigent chacun de leur côté (ce fichier et
/// `app::tests`) peuvent s'entrelacer — l'un restaure `$HOME` pendant que
/// l'autre écrit encore, et le fichier réel du poste se retrouve pollué par
/// des données de test (vu en pratique : un `settings.json` de test a fini
/// dans le vrai dossier utilisateur avant l'ajout de ce verrou).
#[cfg(test)]
pub(crate) fn home_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_image_from_path_decodes_gif() {
        // Régression (Sprint L.6) : la feature `gif` de la crate `image`
        // n'était pas activée dans Cargo.toml malgré le filtre de fichiers
        // l'annonçant déjà — un .gif déposé/importé échouait silencieusement.
        let path = std::env::temp_dir().join("quickpaint-test-import.gif");
        image::RgbaImage::from_fn(3, 2, |x, _| image::Rgba([x as u8 * 60, 100, 150, 255]))
            .save(&path)
            .expect("write gif");
        let (w, h, _) = import_image_from_path(&path).expect("decode gif");
        assert_eq!((w, h), (3, 2));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_a_format_version_from_the_future() {
        let mut doc = Document::new((10, 10));
        doc.format_version = CURRENT_FORMAT_VERSION + 1;
        let json = serde_json::to_string(&doc).unwrap();
        let dir = std::env::temp_dir().join("quickpaint-test-project-future.json");
        std::fs::write(&dir, json).unwrap();
        let err = load_from_path(&dir).unwrap_err();
        assert!(err.contains(&CURRENT_FORMAT_VERSION.to_string()) || !err.is_empty());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn rejects_invalid_json() {
        let dir = std::env::temp_dir().join("quickpaint-test-project-garbage.json");
        std::fs::write(&dir, b"not json at all").unwrap();
        assert!(load_from_path(&dir).is_err());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn accepts_a_well_formed_project() {
        let doc = Document::new((100, 80));
        let json = serde_json::to_string(&doc).unwrap();
        let dir = std::env::temp_dir().join("quickpaint-test-project-ok.json");
        std::fs::write(&dir, json).unwrap();
        let loaded = load_from_path(&dir).expect("should load");
        assert_eq!(loaded.size, (100, 80));
        let _ = std::fs::remove_file(&dir);
    }

    /// TIFF (Sprint 8.1) : format ouvert en plus de PNG/JPG/BMP/GIF/WebP —
    /// encode un petit TIFF via la crate `image` elle-même, puis vérifie que
    /// `load_image_from_path` (utilisé par `import_image_dialog`) le décode.
    #[test]
    fn opens_a_tiff_image() {
        let img = image::RgbaImage::from_fn(4, 3, |x, _y| image::Rgba([x as u8 * 10, 50, 100, 255]));
        let dir = std::env::temp_dir().join("quickpaint-test-import.tiff");
        img.save(&dir).expect("should encode a tiff");
        let (w, h, rgba) = load_image_from_path(&dir).expect("should decode the tiff");
        assert_eq!((w, h), (4, 3));
        assert_eq!(rgba.len(), (4 * 3 * 4) as usize);
        let _ = std::fs::remove_file(&dir);
    }

    /// BMP (audit_100_features.md #1) : listé dans le filtre du dialogue
    /// d'import depuis toujours, mais la feature `bmp` de la crate `image`
    /// n'était pas activée dans `Cargo.toml` — le dialogue promettait un
    /// format qu'il ne savait pas réellement décoder. Même régression que
    /// le GIF au Sprint L.6 (voir `sprint.md`), même correctif : activer la
    /// feature, couvrir par un test de régression.
    #[test]
    fn opens_a_bmp_image() {
        let img = image::RgbaImage::from_fn(4, 3, |x, _y| image::Rgba([x as u8 * 10, 50, 100, 255]));
        let dir = std::env::temp_dir().join("quickpaint-test-import.bmp");
        img.save(&dir).expect("should encode a bmp");
        let (w, h, rgba) = load_image_from_path(&dir).expect("should decode the bmp");
        assert_eq!((w, h), (4, 3));
        assert_eq!(rgba.len(), (4 * 3 * 4) as usize);
        let _ = std::fs::remove_file(&dir);
    }

    /// `recovery_path` dérive de `$HOME`, comme `i18n::settings_path` — donc
    /// testé via une valeur de `$HOME` temporaire plutôt que sur le vrai
    /// dossier du poste, pour rester déterministe et ne rien polluer.
    #[test]
    fn recovery_round_trip_via_temporary_home() {
        let _guard = home_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var("HOME").ok();
        let tmp = std::env::temp_dir().join("quickpaint-test-recovery-home");
        std::env::set_var("HOME", &tmp);

        assert!(!has_recovery());

        let doc = Document::new((64, 48));
        autosave(&doc);
        assert!(has_recovery());

        let loaded = load_recovery().expect("should load recovery file");
        assert_eq!(loaded.size, (64, 48));

        clear_recovery();
        assert!(!has_recovery());

        if let Some(home) = real_home {
            std::env::set_var("HOME", home);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
