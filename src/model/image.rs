//! Élément image bitmap (roadmap #7, première brique raster #5).
//!
//! Posé sur un calque comme un rectangle texturé. Persistance : les pixels sont
//! encodés en **PNG base64** dans le JSON (`png`) ; les pixels RGBA décodés
//! (`rgba`/`w`/`h`) sont reconstruits au chargement et ne sont pas sérialisés.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ImageItem {
    pub id: u64,
    /// Coin haut-gauche, coords document.
    pub pos: (f32, f32),
    /// Taille affichée (coords document), indépendante de la résolution source.
    pub size: (f32, f32),
    /// Rotation (radians) autour du centre.
    #[serde(default)]
    pub rot: f32,
    /// Profondeur de superposition (plus grand = au-dessus).
    #[serde(default)]
    pub z: f64,
    /// Source de vérité persistée : PNG encodé en base64.
    #[serde(rename = "png")]
    pub png_b64: String,
    #[serde(skip)]
    pub rgba: Vec<u8>,
    #[serde(skip)]
    pub w: u32,
    #[serde(skip)]
    pub h: u32,
}

impl std::fmt::Debug for ImageItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageItem")
            .field("id", &self.id)
            .field("pos", &self.pos)
            .field("size", &self.size)
            .field("w", &self.w)
            .field("h", &self.h)
            .finish()
    }
}

impl ImageItem {
    /// Construit depuis des pixels RGBA bruts (au moment de l'import).
    /// L'encodage PNG est **paresseux** (fait à la sauvegarde via
    /// [`ensure_encoded`]) pour ne pas ralentir coller / import / filtres.
    pub fn from_rgba(id: u64, pos: (f32, f32), w: u32, h: u32, rgba: Vec<u8>) -> Self {
        Self { id, pos, size: (w as f32, h as f32), rot: 0.0, z: 0.0, png_b64: String::new(), rgba, w, h }
    }

    /// Encode le PNG base64 si nécessaire (avant sérialisation / export SVG).
    pub fn ensure_encoded(&mut self) {
        if self.png_b64.is_empty() && !self.rgba.is_empty() {
            self.png_b64 = encode_png_b64(self.w, self.h, &self.rgba).unwrap_or_default();
        }
    }

    /// Reconstruit les pixels RGBA depuis le PNG base64 (après chargement).
    pub fn decode(&mut self) {
        if !self.rgba.is_empty() {
            return;
        }
        if let Some((w, h, rgba)) = decode_png_b64(&self.png_b64) {
            self.w = w;
            self.h = h;
            self.rgba = rgba;
        }
    }

    pub fn bounds(&self) -> ((f32, f32), (f32, f32)) {
        (self.pos, (self.pos.0 + self.size.0, self.pos.1 + self.size.1))
    }

    /// Retourne les pixels source en miroir (point 66 de l'audit). Invalide
    /// le PNG persisté, ré-encodé paresseusement via [`Self::ensure_encoded`].
    pub fn flip_pixels(&mut self, horizontal: bool) {
        self.decode();
        let (w, h) = (self.w as usize, self.h as usize);
        if w == 0 || h == 0 || self.rgba.len() < w * h * 4 {
            return;
        }
        let src = self.rgba.clone();
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = if horizontal { (w - 1 - x, y) } else { (x, h - 1 - y) };
                let d = (y * w + x) * 4;
                let s = (sy * w + sx) * 4;
                self.rgba[d..d + 4].copy_from_slice(&src[s..s + 4]);
            }
        }
        self.png_b64.clear();
    }
}

/// Réutilisé par `BrandKit::set_logo` (previous_audit.md #92) : même
/// encodage PNG base64 que le raster/masque de calque, pas de raison d'en
/// avoir un second.
pub(crate) fn encode_png_b64(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(STANDARD.encode(buf))
}

/// Côté maximal accepté pour une image ou un document (projet ouvert, import,
/// collage, redimensionnement) — audit sécurité (ANALYSE.md §8.2) : sans
/// plafond, un fichier corrompu ou malveillant peut déclarer des dimensions
/// énormes (decompression bomb PNG) et faire allouer `w*h*4` octets sans
/// limite, jusqu'à épuiser la mémoire. 16 384 px de côté couvre tout usage
/// réel (bien au-delà d'un scan A0 à 300 dpi).
pub const MAX_IMAGE_SIDE: u32 = 16_384;

/// Plafond de surface totale, en plus du plafond par côté : bloque un ratio
/// d'aspect extrême (ex. 16000×2000 est sous le plafond par côté mais reste
/// 128 Mpx — un cas légitime rare, mais 16000×16000 ne l'est pas et doit
/// être refusé même si chaque côté pris isolément est sous `MAX_IMAGE_SIDE`).
/// 64 Mpx ≈ un scan A2 à 300 dpi, largement au-dessus d'un usage courant.
pub const MAX_IMAGE_PIXELS: u64 = 64_000_000;

/// Valide des dimensions avant toute allocation (`w * h * 4` octets) — import,
/// collage, chargement de projet, ou dialogue de redimensionnement. Renvoie
/// un message localisé prêt à afficher en cas de refus.
pub fn check_dims(w: u32, h: u32) -> Result<(), String> {
    use crate::i18n::t;
    if w == 0 || h == 0 {
        return Err(t("dimensions vides", "empty dimensions").to_string());
    }
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return Err(format!(
            "{} ({w}×{h}, {} {MAX_IMAGE_SIDE}px)",
            t("dimensions trop grandes", "dimensions too large"),
            t("max", "max"),
        ));
    }
    if (w as u64) * (h as u64) > MAX_IMAGE_PIXELS {
        return Err(format!(
            "{} ({w}×{h} = {} Mpx, {} {} Mpx)",
            t("image trop grande", "image too large"),
            (w as u64 * h as u64) / 1_000_000,
            t("max", "max"),
            MAX_IMAGE_PIXELS / 1_000_000,
        ));
    }
    Ok(())
}

fn decode_png_b64(b64: &str) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = STANDARD.decode(b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    if check_dims(w, h).is_err() {
        return None;
    }
    Some((w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Régression sécurité (ANALYSE.md §8.2) : une image dont une dimension
    /// dépasse `MAX_IMAGE_SIDE` doit être rejetée plutôt que d'allouer
    /// `w*h*4` octets sans plafond (protection contre une decompression bomb
    /// PNG dans un fichier projet corrompu ou malveillant).
    #[test]
    fn decode_rejects_dimensions_above_the_cap() {
        // Image volontairement fine (1 px de haut) mais trop large — coûte
        // peu à construire/encoder tout en dépassant le plafond.
        let w = MAX_IMAGE_SIDE + 1;
        let img = image::RgbaImage::from_pixel(w, 1, image::Rgba([1, 2, 3, 255]));
        let b64 = encode_png_b64(w, 1, img.as_raw()).unwrap();
        assert!(decode_png_b64(&b64).is_none());
    }

    #[test]
    fn decode_accepts_dimensions_within_the_cap() {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]));
        let b64 = encode_png_b64(4, 4, img.as_raw()).unwrap();
        let (w, h, rgba) = decode_png_b64(&b64).unwrap();
        assert_eq!((w, h), (4, 4));
        assert_eq!(rgba.len(), 4 * 4 * 4);
    }
}
