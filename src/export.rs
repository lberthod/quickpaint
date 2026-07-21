//! Export bitmap. Le document est rendu à sa résolution **native** via le
//! compositeur tiny-skia ([`render::compositor::Compositor::render_to_rgba`],
//! roadmap ANALYSE.md §12.2) — plus de dépendance à une capture d'écran du
//! viewport, donc plus de perte de résolution liée au zoom ou à la taille de
//! la fenêtre. Ce module se contente d'encoder le buffer RGBA reçu au format
//! choisi : PNG, JPG, WebP, GIF (statique) ou PDF (mono-page).
//!
//! GIF (Sprint L.6) : export **statique** uniquement — une palette 256
//! couleurs (quantification de la crate `image`), pas d'animation. Le GIF
//! animé demande d'abord une notion de frames/timeline absente du modèle de
//! document actuel (le document est une image fixe) ; voir `sprint_next.md`
//! L.6 pour ce que ça impliquerait de concevoir avant de coder l'export
//! animé lui-même — hors de portée de ce module.
//!
//! Métadonnées (Sprint L.3, point 17 de l'audit) : aucune n'est jamais
//! écrite. L'export part toujours d'un buffer RGBA fraîchement rendu par le
//! compositeur (`render_to_rgba`), jamais des octets d'un fichier source —
//! il n'y a donc aucun EXIF/IPTC à faire transiter, même quand l'image
//! d'origine (import PNG/JPEG/PSD…) en portait. Les encodeurs de la crate
//! `image` (PNG/WebP/JPEG) et le PDF construit à la main n'ajoutent rien
//! non plus de leur propre chef. Vérifié par lecture de code plutôt que par
//! un ajout de case à cocher qui n'aurait rien à faire.

use crate::i18n::t;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Formats d'export bitmap proposés dans le menu Fichier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Png,
    Jpg,
    Webp,
    /// GIF statique (Sprint L.6) — voir la doc de module pour l'animé.
    Gif,
    Pdf,
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpg => "JPEG",
            ExportFormat::Webp => "WebP",
            ExportFormat::Gif => "GIF",
            ExportFormat::Pdf => "PDF",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpg => "jpg",
            ExportFormat::Webp => "webp",
            ExportFormat::Gif => "gif",
            ExportFormat::Pdf => "pdf",
        }
    }
}

/// Profil d'export nommé (Sprint L.8) : regroupe format + qualité JPEG +
/// tailles du batch export en un préréglage réutilisable en un clic — même
/// mécanisme de persistance que `style_presets`/`brush_presets`
/// (`i18n::{load,save}_export_profiles`), pas une nouvelle infrastructure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportProfile {
    pub name: String,
    pub format: ExportFormat,
    pub jpeg_quality: u8,
    pub scale_half: bool,
    pub scale_1: bool,
    pub scale_2: bool,
    pub scale_3: bool,
    pub custom_enabled: bool,
    pub custom_width: String,
}

/// Exporte simultanément plusieurs tailles dans un dossier choisi une seule
/// fois (Sprint 7.3) — un clic pour couvrir web + print plutôt qu'un export
/// par taille. Renvoie le nombre de fichiers écrits. `rgba` doit faire
/// exactement `w * h * 4` octets (rendu natif du document, roadmap §12.2).
pub fn save_batch(
    w: u32,
    h: u32,
    rgba: &[u8],
    format: ExportFormat,
    sizes: &[(u32, u32)],
    jpeg_quality: u8,
) -> std::io::Result<usize> {
    if w == 0 || h == 0 || sizes.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            t("zone d'export vide", "empty export area"),
        ));
    }
    let base = image::RgbaImage::from_raw(w, h, rgba.to_vec())
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
        encode_to(&path, tw, th, resized.as_raw(), format, jpeg_quality)?;
    }
    Ok(sizes.len())
}

/// Ouvre un sélecteur « Enregistrer » et écrit l'export au format demandé.
/// Renvoie le chemin écrit, ou `None` si annulé / erreur. `rgba` doit faire
/// exactement `w * h * 4` octets (rendu natif du document, roadmap §12.2).
/// `jpeg_quality` (1..=100) : ignoré pour les formats autres que JPEG/PDF (le
/// PDF embarque son image en JPEG, voir `write_pdf`). Le PNG reste sans perte
/// et le WebP de la crate `image` est **toujours** sans perte — encoder du
/// WebP *lossy* nécessiterait `libwebp` (dépendance système), volontairement
/// évitée ici (même arbitrage que pour l'ouverture de HEIC).
pub fn save_dialog(w: u32, h: u32, rgba: &[u8], format: ExportFormat, jpeg_quality: u8) -> std::io::Result<PathBuf> {
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
    encode_to(&path, w, h, rgba, format, jpeg_quality)?;
    Ok(path)
}

/// Encode et écrit le buffer RGBA dans `path` selon le format.
fn encode_to(path: &PathBuf, w: u32, h: u32, rgba: &[u8], format: ExportFormat, jpeg_quality: u8) -> std::io::Result<()> {
    let bytes = encode_to_bytes(w, h, rgba, format, jpeg_quality)?;
    std::fs::write(path, bytes)
}

/// Encode le buffer RGBA en mémoire selon le format, sans écrire sur disque
/// (Sprint L.2) — sert à l'aperçu/poids estimé avant export, et réutilisé
/// tel quel par `encode_to` pour ne coder la logique d'encodage qu'une fois.
pub fn encode_to_bytes(w: u32, h: u32, rgba: &[u8], format: ExportFormat, jpeg_quality: u8) -> std::io::Result<Vec<u8>> {
    let buf = image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "buffer invalide"))?;
    let to_io = |e: image::ImageError| std::io::Error::other(e.to_string());
    let mut out = Vec::new();
    match format {
        ExportFormat::Png => {
            image::DynamicImage::ImageRgba8(buf)
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(to_io)?;
        }
        ExportFormat::Webp => {
            image::DynamicImage::ImageRgba8(buf)
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::WebP)
                .map_err(to_io)?;
        }
        ExportFormat::Gif => {
            // GIF statique (Sprint L.6) : une seule image, quantifiée par
            // l'encodeur GIF de la crate `image` (palette 256 couleurs).
            image::DynamicImage::ImageRgba8(buf)
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Gif)
                .map_err(to_io)?;
        }
        ExportFormat::Jpg => {
            // JPEG est opaque : on aplatit l'alpha sur blanc.
            let rgb = image::DynamicImage::ImageRgba8(buf).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, jpeg_quality.clamp(1, 100))
                .encode_image(&rgb)
                .map_err(to_io)?;
        }
        ExportFormat::Pdf => out = build_pdf_bytes(w, h, &buf, jpeg_quality)?,
    }
    Ok(out)
}

/// Ouvre un sélecteur « Enregistrer » et écrit `bytes` déjà encodés
/// (Sprint L.2) — évite un second encodage entre l'aperçu et l'écriture
/// finale (notamment pour le JPEG, dont l'encodage n'est pas gratuit).
pub fn save_dialog_bytes(format: ExportFormat, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(path) = rfd::FileDialog::new()
        .add_filter(format.label(), &[format.ext()])
        .set_file_name(format!("QuickPaint-{stamp}.{}", format.ext()))
        .save_file()
    else {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, t("annulé", "cancelled")));
    };
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Ouvre un sélecteur « Enregistrer » et écrit un GIF **animé** (Sprint L.6) :
/// `frames` doit déjà porter son délai par frame (`image::Delay`) — voir
/// `PaintApp::export_animated_gif`, qui rend chaque frame séparément via le
/// compositeur avant d'appeler cette fonction. Boucle infinie par défaut
/// (`Repeat::Infinite`), comme la quasi-totalité des GIF animés du web.
pub fn save_animated_gif(frames: Vec<image::Frame>) -> std::io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(path) = rfd::FileDialog::new()
        .add_filter("GIF animé", &["gif"])
        .set_file_name(format!("QuickPaint-{stamp}-anime.gif"))
        .save_file()
    else {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, t("annulé", "cancelled")));
    };
    let file = std::fs::File::create(&path)?;
    encode_animated_gif(file, frames)?;
    Ok(path)
}

/// Ouvre un sélecteur « Enregistrer » et écrit un **APNG animé** (Sprint T,
/// point 100a — décision produit du 20 juillet 2026 : APNG plutôt que MP4,
/// aucune dépendance système, couleurs 24 bits + alpha là où le GIF est
/// limité à 256 couleurs). `frames` = (délai ms, pixels RGBA pleine taille).
pub fn save_animated_apng(frames: &[(u32, Vec<u8>)], w: u32, h: u32) -> std::io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let Some(path) = rfd::FileDialog::new()
        .add_filter("PNG animé (APNG)", &["png"])
        .set_file_name(format!("QuickPaint-{stamp}-anime.png"))
        .save_file()
    else {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, t("annulé", "cancelled")));
    };
    let file = std::fs::File::create(&path)?;
    encode_animated_apng(file, frames, w, h)?;
    Ok(path)
}

/// Encodage APNG proprement dit, séparé de la sélection de fichier pour
/// rester testable sans dialogue natif. Boucle infinie (num_plays = 0).
fn encode_animated_apng<W: std::io::Write>(out: W, frames: &[(u32, Vec<u8>)], w: u32, h: u32) -> std::io::Result<()> {
    if frames.len() < 2 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, t("au moins 2 frames requises", "at least 2 frames required")));
    }
    let to_io = |e: png::EncodingError| std::io::Error::other(e.to_string());
    let mut enc = png::Encoder::new(out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_animated(frames.len() as u32, 0).map_err(to_io)?;
    let mut writer = enc.write_header().map_err(to_io)?;
    for (delay_ms, rgba) in frames {
        // Délai en fraction de seconde (ms/1000), plafonné au u16 du format.
        writer.set_frame_delay((*delay_ms).min(u16::MAX as u32) as u16, 1000).map_err(to_io)?;
        writer.write_image_data(rgba).map_err(to_io)?;
    }
    writer.finish().map_err(to_io)?;
    Ok(())
}

/// Encodage GIF animé proprement dit (Sprint L.6), séparé de la sélection de
/// fichier pour rester testable sans dialogue natif. `frames` doit porter au
/// moins 2 éléments (sinon ce n'est pas une animation) et déjà son délai par
/// frame (`image::Delay`). Boucle infinie par défaut (`Repeat::Infinite`),
/// comme la quasi-totalité des GIF animés du web.
fn encode_animated_gif<W: std::io::Write>(w: W, frames: Vec<image::Frame>) -> std::io::Result<()> {
    if frames.len() < 2 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, t("au moins 2 frames requises", "at least 2 frames required")));
    }
    let mut encoder = image::codecs::gif::GifEncoder::new(w);
    let to_io = |e: image::ImageError| std::io::Error::other(e.to_string());
    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(to_io)?;
    encoder.encode_frames(frames).map_err(to_io)?;
    Ok(())
}

/// Construit un PDF mono-page embarquant l'image en JPEG (filtre DCTDecode).
/// Format minimal mais valide ; évite une dépendance PDF lourde et changeante.
fn build_pdf_bytes(w: u32, h: u32, buf: &image::RgbaImage, jpeg_quality: u8) -> std::io::Result<Vec<u8>> {
    // Encode l'image en JPEG en mémoire (flux DCTDecode du PDF).
    let mut jpeg = Vec::new();
    let rgb = image::DynamicImage::ImageRgba8(buf.clone()).to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, jpeg_quality.clamp(1, 100))
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

    Ok(pdf)
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
            (ExportFormat::Gif, "p.gif"),
        ] {
            let path = tmp(name);
            encode_to(&path, w, h, img.as_raw(), fmt, 90).expect("encode");
            // Relecture : l'image doit se redécoder aux bonnes dimensions.
            let back = image::open(&path).expect("reopen");
            assert_eq!((back.width(), back.height()), (w, h), "{name}");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn jpeg_quality_affects_file_size() {
        // Image avec assez de détail (pas un aplat) pour que la compression
        // JPEG varie réellement avec la qualité.
        let img = image::RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([((x * 7 + y * 3) % 256) as u8, ((x * 13) % 256) as u8, ((y * 17) % 256) as u8, 255])
        });
        let low = tmp("quality-low.jpg");
        let high = tmp("quality-high.jpg");
        encode_to(&low, 64, 64, img.as_raw(), ExportFormat::Jpg, 10).expect("encode low");
        encode_to(&high, 64, 64, img.as_raw(), ExportFormat::Jpg, 95).expect("encode high");
        let low_size = std::fs::metadata(&low).unwrap().len();
        let high_size = std::fs::metadata(&high).unwrap().len();
        assert!(low_size < high_size, "qualité basse ({low_size}o) devrait être plus légère que qualité haute ({high_size}o)");
        let _ = std::fs::remove_file(&low);
        let _ = std::fs::remove_file(&high);
    }

    #[test]
    fn encode_to_bytes_matches_encode_to_file_size() {
        // L'aperçu en mémoire (Sprint L.2) doit produire exactement les mêmes
        // octets que l'écriture sur disque — même chemin d'encodage, pas une
        // approximation.
        let img = sample();
        let path = tmp("bytes-vs-file.png");
        encode_to(&path, img.width(), img.height(), img.as_raw(), ExportFormat::Png, 90).expect("encode");
        let from_file = std::fs::read(&path).expect("read");
        let from_bytes = encode_to_bytes(img.width(), img.height(), img.as_raw(), ExportFormat::Png, 90).expect("encode bytes");
        assert_eq!(from_file, from_bytes);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn encode_animated_gif_round_trips_frame_count_and_delays() {
        let mut buf = Vec::new();
        let frames = vec![
            image::Frame::from_parts(sample(), 0, 0, image::Delay::from_numer_denom_ms(100, 1)),
            image::Frame::from_parts(sample(), 0, 0, image::Delay::from_numer_denom_ms(250, 1)),
        ];
        encode_animated_gif(&mut buf, frames).expect("encode");
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&buf)).expect("decode");
        use image::AnimationDecoder;
        let decoded: Vec<_> = decoder.into_frames().collect_frames().expect("collect frames");
        assert_eq!(decoded.len(), 2);
        let (num, den) = decoded[0].delay().numer_denom_ms();
        assert!((num as f32 / den as f32 - 100.0).abs() < 15.0, "délai frame 0 ≈ 100ms, got {num}/{den}");
        let (num, den) = decoded[1].delay().numer_denom_ms();
        assert!((num as f32 / den as f32 - 250.0).abs() < 15.0, "délai frame 1 ≈ 250ms, got {num}/{den}");
    }

    /// APNG (Sprint T, point 100a) : le fichier produit est un PNG valide
    /// portant le chunk d'animation `acTL` et un `fcTL` par frame.
    #[test]
    fn encode_animated_apng_writes_animation_chunks() {
        let mut buf = Vec::new();
        let px = vec![255u8; 2 * 2 * 4];
        let frames = vec![(100u32, px.clone()), (200u32, px)];
        encode_animated_apng(&mut buf, &frames, 2, 2).expect("apng");
        assert!(buf.starts_with(&[0x89, b'P', b'N', b'G']));
        let has = |tag: &[u8]| buf.windows(tag.len()).any(|w| w == tag);
        assert!(has(b"acTL"), "chunk de contrôle d'animation manquant");
        assert!(has(b"fcTL"), "chunk de contrôle de frame manquant");
    }

    #[test]
    fn encode_animated_apng_rejects_fewer_than_two_frames() {
        let mut buf = Vec::new();
        let frames = vec![(100u32, vec![0u8; 4])];
        assert!(encode_animated_apng(&mut buf, &frames, 1, 1).is_err());
    }

    #[test]
    fn encode_animated_gif_rejects_fewer_than_two_frames() {
        let mut buf = Vec::new();
        let frames = vec![image::Frame::new(sample())];
        assert!(encode_animated_gif(&mut buf, frames).is_err());
    }

    #[test]
    fn pdf_is_well_formed() {
        let bytes = build_pdf_bytes(4, 3, &sample(), 90).expect("pdf");
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF"));
        // Doit contenir l'objet image et la table xref.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Subtype /Image"));
        assert!(s.contains("/DCTDecode"));
        assert!(s.contains("startxref"));
    }
}
