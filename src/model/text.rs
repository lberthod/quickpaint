//! Élément texte (roadmap #2). Posé sur un calque, indépendant des traits.
//! Texte riche (Sprint 3) : police, gras, alignement, contour. Modèle pur — le
//! mapping vers egui est dans `render::text`, partagé par les deux chemins de
//! rendu (painter live et compositeur CPU) pour rester cohérent.

use serde::{Deserialize, Serialize};

/// Famille de police. On s'appuie sur les polices intégrées d'egui (aucun
/// fichier à embarquer) : proportionnelle ou monospace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextFont {
    #[default]
    Proportional,
    Monospace,
}

impl TextFont {
    pub const ALL: [TextFont; 2] = [TextFont::Proportional, TextFont::Monospace];
    pub fn label(self) -> &'static str {
        match self {
            TextFont::Proportional => "Sans",
            TextFont::Monospace => "Mono",
        }
    }
}

/// Alignement horizontal des lignes d'un bloc de texte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "⬅",
            TextAlign::Center => "⬌",
            TextAlign::Right => "➡",
        }
    }
}

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
    /// Police (Sprint 3).
    #[serde(default)]
    pub font: TextFont,
    /// Graisse simulée (faux-bold) — double dépôt décalé au rendu.
    #[serde(default)]
    pub bold: bool,
    /// Alignement des lignes (Sprint 3).
    #[serde(default)]
    pub align: TextAlign,
    /// Épaisseur du contour (0 = aucun), en unités document.
    #[serde(default)]
    pub outline_w: f32,
    /// Couleur du contour.
    #[serde(default = "default_outline_color")]
    pub outline_color: [u8; 4],
}

fn default_outline_color() -> [u8; 4] {
    [255, 255, 255, 255]
}

impl TextItem {
    pub fn new(id: u64, pos: (f32, f32), size: f32, color: [u8; 4]) -> Self {
        Self {
            id,
            pos,
            text: String::new(),
            size,
            color,
            rot: 0.0,
            z: 0.0,
            font: TextFont::default(),
            bold: false,
            align: TextAlign::default(),
            outline_w: 0.0,
            outline_color: default_outline_color(),
        }
    }

    /// Boîte englobante approximative (sans mesure de police) — suffit au
    /// test de sélection et au cadre. Tient compte du contour.
    pub fn approx_bounds(&self) -> ((f32, f32), (f32, f32)) {
        let lines = self.text.lines().count().max(1) as f32;
        let cols = self.text.lines().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
        // Le monospace est plus large par caractère.
        let cw = if self.font == TextFont::Monospace { 0.62 } else { 0.55 };
        let w = cols * self.size * cw;
        let h = lines * self.size * 1.25;
        let pad = self.outline_w.max(0.0);
        (
            (self.pos.0 - pad, self.pos.1 - pad),
            (self.pos.0 + w.max(self.size) + pad, self.pos.1 + h + pad),
        )
    }
}
