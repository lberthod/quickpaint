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
}

fn encode_png_b64(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(STANDARD.encode(buf))
}

fn decode_png_b64(b64: &str) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = STANDARD.decode(b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Some((w, h, img.into_raw()))
}
