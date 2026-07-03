//! Sauvegarde / ouverture de projet vectoriel en `.json` (étape 7).
//!
//! Format simple et lisible : sérialisation directe du `Document`. Les boîtes
//! de dialogue natives passent par `rfd`.

use crate::i18n::t;
use crate::model::Document;
use std::path::PathBuf;

/// Ouvre un sélecteur « Enregistrer » et écrit le document en JSON.
/// Renvoie le chemin écrit, ou `None` si annulé / erreur.
pub fn save_dialog(doc: &Document) -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Projet QuickPaint", "QuickPaint project"), &["json"])
        .set_file_name(t("dessin.json", "drawing.json"))
        .save_file()?;
    let json = serde_json::to_string_pretty(doc).ok()?;
    std::fs::write(&path, json).ok()?;
    Some(path)
}

/// Ouvre un sélecteur « Ouvrir » et charge un document JSON.
pub fn open_dialog() -> Option<Document> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Projet QuickPaint", "QuickPaint project"), &["json"])
        .pick_file()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Sélecteur d'image ; renvoie `(largeur, hauteur, pixels RGBA)`.
pub fn import_image_dialog() -> Option<(u32, u32, Vec<u8>)> {
    let path = rfd::FileDialog::new()
        .add_filter(t("Images", "Images"), &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
        .pick_file()?;
    let img = image::open(&path).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((w, h, img.into_raw()))
}
