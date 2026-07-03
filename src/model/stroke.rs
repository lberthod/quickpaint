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

/// Dégradé de remplissage (roadmap P2 #11, fait partie de F2). Ne s'applique
/// qu'aux formes pleines (`fill = true`) — un trait/contour reste en couleur
/// unie. Rendu **uniquement** par le compositeur CPU (tiny-skia gère déjà les
/// dégradés nativement) ; le chemin vectoriel « live » retombe sur la couleur
/// du premier arrêt tant que le document n'est pas composité.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    Linear,
    Radial,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Linéaire : point de départ/arrivée. Radial : centre / point sur le
    /// bord (sa distance au centre = le rayon).
    pub from: (f32, f32),
    pub to: (f32, f32),
    /// Arrêts (position 0..=1, couleur), triés par position.
    pub stops: Vec<(f32, [u8; 4])>,
}

impl Gradient {
    /// Dégradé à deux arrêts entre `from`/`to` (couleurs), bords de `bounds`.
    pub fn two_stop(kind: GradientKind, bounds: ((f32, f32), (f32, f32)), a: [u8; 4], b: [u8; 4]) -> Self {
        let (mn, mx) = bounds;
        let (from, to) = match kind {
            GradientKind::Linear => ((mn.0, (mn.1 + mx.1) * 0.5), (mx.0, (mn.1 + mx.1) * 0.5)),
            GradientKind::Radial => {
                let c = ((mn.0 + mx.0) * 0.5, (mn.1 + mx.1) * 0.5);
                (c, (mx.0, c.1))
            }
        };
        Self { kind, from, to, stops: vec![(0.0, a), (1.0, b)] }
    }
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
    /// Dégradé de remplissage (roadmap P2 #11) ; ignoré si `fill = false`.
    #[serde(default)]
    pub gradient: Option<Gradient>,
    /// Ancres de plume (roadmap P2 #12) : présentes seulement pour les
    /// traits créés avec l'outil Plume — permet de rouvrir l'édition des
    /// poignées de Bézier après coup (double-clic).
    #[serde(default)]
    pub anchors: Option<crate::tools::pen::PenPath>,
}

impl Stroke {
    pub fn new(color: [u8; 4], base_width: f32, tool: Tool) -> Self {
        Self {
            id: 0,
            points: Vec::new(),
            color,
            base_width,
            tool,
            fill: false,
            z: 0.0,
            gradient: None,
            anchors: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}
