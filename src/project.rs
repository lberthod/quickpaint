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
/// `Some(Err(message))` : fichier illisible, JSON invalide, version de
/// format trop récente, ou dimensions hors bornes — message localisé prêt à
/// afficher (ANALYSE.md §8.2 : plus d'échec silencieux au chargement).
/// `Some(Ok(doc))` : succès.
pub fn open_dialog() -> Option<Result<Document, String>> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Projet QuickPaint", "QuickPaint project"), &["json"])
        .pick_file()?;
    Some(load_from_path(&path))
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
        .add_filter(t("Images", "Images"), &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
        .pick_file()?;
    Some(load_image_from_path(&path))
}

fn load_image_from_path(path: &std::path::Path) -> Result<ImagePixels, String> {
    let img = image::open(path)
        .map_err(|e| format!("{} : {e}", t("image illisible", "unreadable image")))?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    check_dims(w, h)?;
    Ok((w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
