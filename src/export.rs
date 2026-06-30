//! Export PNG (étape 7). On réutilise la capture d'écran du viewport fournie
//! par egui/eframe (`Event::Screenshot`) et on recadre sur la zone de dessin.

use egui::ColorImage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Région de recadrage en pixels physiques : (x, y, largeur, hauteur).
pub type Crop = (usize, usize, usize, usize);

/// Recadre la capture sur la zone canvas et écrit un PNG sur le Bureau.
/// Renvoie le chemin du fichier créé.
pub fn save_to_desktop(image: &ColorImage, crop: Crop) -> std::io::Result<PathBuf> {
    let (cx, cy, cw, ch) = crop;
    let [iw, ih] = image.size;
    // Sécurise les bornes contre tout dépassement.
    let x0 = cx.min(iw);
    let y0 = cy.min(ih);
    let w = cw.min(iw.saturating_sub(x0));
    let h = ch.min(ih.saturating_sub(y0));

    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let px = image[(x, y)].to_srgba_unmultiplied();
            rgba.extend_from_slice(&px);
        }
    }

    let path = desktop_path();
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "buffer PNG invalide"))?;
    buf.save(&path).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(path)
}

fn desktop_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut dir = PathBuf::from(home);
    dir.push("Desktop");
    if !dir.is_dir() {
        dir = PathBuf::from(".");
    }
    dir.push(format!("QuickPaint-{stamp}.png"));
    dir
}
