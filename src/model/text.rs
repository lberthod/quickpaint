//! Élément texte (roadmap #2). Posé sur un calque, indépendant des traits.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextItem {
    pub id: u64,
    /// Coin haut-gauche, en coordonnées document.
    pub pos: (f32, f32),
    pub text: String,
    pub size: f32,
    pub color: [u8; 4],
    /// Rotation (radians) autour de `pos`.
    #[serde(default)]
    pub rot: f32,
    /// Profondeur de superposition (plus grand = au-dessus).
    #[serde(default)]
    pub z: f64,
}

impl TextItem {
    pub fn new(id: u64, pos: (f32, f32), size: f32, color: [u8; 4]) -> Self {
        Self { id, pos, text: String::new(), size, color, rot: 0.0, z: 0.0 }
    }

    /// Boîte englobante approximative (sans mesure de police) — suffit au
    /// test de sélection et au cadre.
    pub fn approx_bounds(&self) -> ((f32, f32), (f32, f32)) {
        let lines = self.text.lines().count().max(1) as f32;
        let cols = self.text.lines().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
        let w = cols * self.size * 0.55;
        let h = lines * self.size * 1.25;
        (self.pos, (self.pos.0 + w.max(self.size), self.pos.1 + h))
    }
}
