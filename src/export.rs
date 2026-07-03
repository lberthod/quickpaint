//! Export bitmap (Sprint 2). On réutilise la capture d'écran du viewport fournie
//! par egui/eframe (`Event::Screenshot`), recadrée sur la zone du document, puis
//! on encode au format choisi : PNG, JPG, WebP ou PDF (mono-page).

use crate::i18n::t;
use egui::ColorImage;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Région de recadrage en pixels physiques : (x, y, largeur, hauteur).
pub type Crop = (usize, usize, usize, usize);

/// Formats d'export bitmap proposés dans le menu Fichier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpg,
    Webp,
    Pdf,
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpg => "JPEG",
            ExportFormat::Webp => "WebP",
            ExportFormat::Pdf => "PDF",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpg => "jpg",
            ExportFormat::Webp => "webp",
            ExportFormat::Pdf => "pdf",
        }
    }
}

/// Recadre la capture sur la zone canvas et renvoie `(largeur, hauteur, RGBA)`.
fn crop_rgba(image: &ColorImage, crop: Crop) -> (u32, u32, Vec<u8>) {
    let (cx, cy, cw, ch) = crop;
    let [iw, ih] = image.size;
    let x0 = cx.min(iw);
    let y0 = cy.min(ih);
    let w = cw.min(iw.saturating_sub(x0));
    let h = ch.min(ih.saturating_sub(y0));
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            rgba.extend_from_slice(&image[(x, y)].to_srgba_unmultiplied());
        }
    }
    (w as u32, h as u32, rgba)
}

/// Exporte simultanément plusieurs tailles dans un dossier choisi une seule
/// fois (Sprint 7.3) — un clic pour couvrir web + print plutôt qu'un export
/// par taille. Renvoie le nombre de fichiers écrits.
pub fn save_batch(image: &ColorImage, crop: Crop, format: ExportFormat, sizes: &[(u32, u32)]) -> std::io::Result<usize> {
    let (w, h, rgba) = crop_rgba(image, crop);
    if w == 0 || h == 0 || sizes.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            t("zone d'export vide", "empty export area"),
        ));
    }
    let base = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "buffer invalide"))?;
    let Some(dir) = rfd::FileDialog::new().pick_folder() else {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, t("annulé", "cancelled")));
    };
    for &(tw, th) in sizes {
        let tw = tw.max(1);
        let th = th.max(1);
        let resized = if (tw, th) == (w, h) {
            base.clone()
        } else {
            image::imageops::resize(&base, tw, th, image::imageops::FilterType::Lanczos3)
        };
        let path = dir.join(format!("QuickPaint-{tw}x{th}.{}", format.ext()));
        encode_to(&path, tw, th, resized.as_raw(), format)?;
    }
    Ok(sizes.len())
}

/// Ouvre un sélecteur « Enregistrer » et écrit l'export au format demandé.
/// Renvoie le chemin écrit, ou `None` si annulé / erreur.
pub fn save_dialog(image: &ColorImage, crop: Crop, format: ExportFormat) -> std::io::Result<PathBuf> {
    let (w, h, rgba) = crop_rgba(image, crop);
    if w == 0 || h == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            t("zone d'export vide", "empty export area"),
        ));
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(path) = rfd::FileDialog::new()
        .add_filter(format.label(), &[format.ext()])
        .set_file_name(format!("QuickPaint-{stamp}.{}", format.ext()))
        .save_file()
    else {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, t("annulé", "cancelled")));
    };
    encode_to(&path, w, h, &rgba, format)?;
    Ok(path)
}

/// Encode et écrit le buffer RGBA dans `path` selon le format.
fn encode_to(path: &PathBuf, w: u32, h: u32, rgba: &[u8], format: ExportFormat) -> std::io::Result<()> {
    let buf = image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "buffer invalide"))?;
    let to_io = |e: image::ImageError| std::io::Error::other(e.to_string());
    match format {
        ExportFormat::Png => buf.save(path).map_err(to_io),
        ExportFormat::Webp => buf.save(path).map_err(to_io),
        ExportFormat::Jpg => {
            // JPEG est opaque : on aplatit l'alpha sur blanc.
            image::DynamicImage::ImageRgba8(buf).to_rgb8().save(path).map_err(to_io)
        }
        ExportFormat::Pdf => write_pdf(path, w, h, &buf),
    }
}

/// Écrit un PDF mono-page embarquant l'image en JPEG (filtre DCTDecode). Format
/// minimal mais valide ; évite une dépendance PDF lourde et changeante.
fn write_pdf(path: &PathBuf, w: u32, h: u32, buf: &image::RgbaImage) -> std::io::Result<()> {
    // Encode l'image en JPEG en mémoire (flux DCTDecode du PDF).
    let mut jpeg = Vec::new();
    let rgb = image::DynamicImage::ImageRgba8(buf.clone()).to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Page à 72 dpi : 1 px = 1 pt.
    let (pw, ph) = (w as f32, h as f32);
    let mut pdf: Vec<u8> = Vec::new();
    let mut offsets = vec![0usize; 6]; // objets 1..=5 (index 0 inutilisé)

    pdf.extend_from_slice(b"%PDF-1.4\n");
    let obj = |pdf: &mut Vec<u8>, offsets: &mut [usize], n: usize, body: &[u8]| {
        offsets[n] = pdf.len();
        pdf.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };

    obj(&mut pdf, &mut offsets, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut pdf, &mut offsets, 2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut pdf,
        &mut offsets,
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        )
        .as_bytes(),
    );

    // Objet image (XObject) : en-tête de dictionnaire + flux JPEG brut.
    offsets[4] = pdf.len();
    pdf.extend_from_slice(b"4 0 obj\n");
    pdf.extend_from_slice(
        format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode \
             /Length {} >>\nstream\n",
            jpeg.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&jpeg);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");

    // Flux de contenu : place l'image à l'échelle de la page.
    let content = format!("q {pw} 0 0 {ph} 0 0 cm /Im0 Do Q");
    obj(
        &mut pdf,
        &mut offsets,
        5,
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes(),
    );

    // Table xref + trailer.
    let xref_pos = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF").as_bytes(),
    );

    let mut f = std::fs::File::create(path)?;
    f.write_all(&pdf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("quickpaint-test-{name}"));
        d
    }

    fn sample() -> image::RgbaImage {
        image::RgbaImage::from_fn(4, 3, |x, _| image::Rgba([x as u8 * 60, 100, 150, 255]))
    }

    #[test]
    fn encodes_each_raster_format() {
        let img = sample();
        let (w, h) = (img.width(), img.height());
        for (fmt, name) in [
            (ExportFormat::Png, "p.png"),
            (ExportFormat::Jpg, "p.jpg"),
            (ExportFormat::Webp, "p.webp"),
        ] {
            let path = tmp(name);
            encode_to(&path, w, h, img.as_raw(), fmt).expect("encode");
            // Relecture : l'image doit se redécoder aux bonnes dimensions.
            let back = image::open(&path).expect("reopen");
            assert_eq!((back.width(), back.height()), (w, h), "{name}");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn pdf_is_well_formed() {
        let path = tmp("p.pdf");
        write_pdf(&path, 4, 3, &sample()).expect("pdf");
        let bytes = std::fs::read(&path).expect("read");
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF"));
        // Doit contenir l'objet image et la table xref.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Image"));
        assert!(s.contains("/DCTDecode"));
        assert!(s.contains("startxref"));
        let _ = std::fs::remove_file(&path);
    }
}
