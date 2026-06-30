//! Un trait (`Stroke`) = liste de points + style. Modèle vectoriel (section 3b).

use serde::{Deserialize, Serialize};

/// Outil ayant produit le trait. Détermine la couleur effective au rendu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Brush,
    Eraser,
}

/// Un point d'un trait. `width` est calculée à la capture (vitesse → pression
/// simulée, section 4.3).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct StrokePoint {
    pub pos: (f32, f32),
    pub width: f32,
}

/// Un trait complet, posé dans une couche.
///
/// `id` est attribué à la validation (drag terminé) ; `0` = trait non encore
/// validé (en cours). Il sert de clé au cache de maillage (`render::canvas`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub id: u64,
    pub points: Vec<StrokePoint>,
    pub color: [u8; 4],
    pub base_width: f32,
    pub tool: Tool,
    /// `true` = forme pleine (intérieur rempli) au lieu d'un contour.
    #[serde(default)]
    pub fill: bool,
    /// Profondeur de superposition (plus grand = au-dessus).
    #[serde(default)]
    pub z: f64,
}

impl Stroke {
    pub fn new(color: [u8; 4], base_width: f32, tool: Tool) -> Self {
        Self { id: 0, points: Vec::new(), color, base_width, tool, fill: false, z: 0.0 }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}
