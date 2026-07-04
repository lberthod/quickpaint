//! Un trait (`Stroke`) = liste de points + style. Modèle vectoriel (section 3b).

use crate::i18n::t;
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
    /// Dégradé conique (Sprint 3.3) : balayage angulaire autour de `from`,
    /// l'angle 0 pointant vers `to`. Pas de support natif dans tiny-skia (qui
    /// n'expose que Linear/Radial) : rendu à part, pixel par pixel, dans le
    /// compositeur — voir `render::compositor::paint_conic_gradient`.
    Conic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Linéaire : point de départ/arrivée. Radial : centre / point sur le
    /// bord (sa distance au centre = le rayon). Conique : centre / référence
    /// de l'angle 0.
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
            GradientKind::Radial | GradientKind::Conic => {
                let c = ((mn.0 + mx.0) * 0.5, (mn.1 + mx.1) * 0.5);
                (c, (mx.0, c.1))
            }
        };
        Self { kind, from, to, stops: vec![(0.0, a), (1.0, b)] }
    }
}

/// Style nommé et réutilisable (Sprint 10.3) : couleur, épaisseur, remplissage
/// et dégradé optionnel — capturé depuis un élément sélectionné, persisté
/// localement (`settings.json`, comme la palette et les raccourcis) et
/// applicable en un clic à d'autres éléments. Complète la pipette de style
/// (copier/coller ponctuel, roadmap P1 #10) par des presets qui survivent
/// à la fermeture de l'app.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StylePreset {
    pub name: String,
    pub color: [u8; 4],
    pub width: f32,
    pub fill: bool,
    pub gradient: Option<Gradient>,
}

/// Préréglage de pinceau (Sprint 3.4) : regroupe les réglages du geste de
/// dessin (épaisseur, dureté du pinceau pixel, stabilisation du tracé,
/// intensité de la pression simulée) sous un nom, au même titre qu'un
/// `StylePreset` pour l'apparence — persisté localement, importable/
/// exportable en fichier `.json` autonome (pas seulement via `settings.json`)
/// pour pouvoir être partagé.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrushPreset {
    pub name: String,
    pub width: f32,
    /// Dureté du pinceau pixel (0 = dégradé complet, 1 = bord net).
    pub hardness: f32,
    /// Stabilisation du tracé, 0..=1 (voir `input::capture::GestureCapture`).
    pub stabilization: f32,
    /// Intensité de la pression simulée vitesse→épaisseur, 0..=1.
    pub pressure_strength: f32,
}

impl BrushPreset {
    /// Préréglages fournis par défaut (Sprint 3.4) — toujours proposés dans
    /// le panneau, en plus de ceux enregistrés par l'utilisateur.
    pub fn builtins() -> Vec<Self> {
        vec![
            Self {
                name: t("Feutre", "Felt tip").to_string(),
                width: 6.0,
                hardness: 1.0,
                stabilization: 0.3,
                pressure_strength: 0.6,
            },
            Self {
                name: t("Crayon fin", "Fine pencil").to_string(),
                width: 2.0,
                hardness: 0.9,
                stabilization: 0.15,
                pressure_strength: 0.9,
            },
            Self {
                name: t("Aquarelle douce", "Soft watercolor").to_string(),
                width: 16.0,
                hardness: 0.15,
                stabilization: 0.6,
                pressure_strength: 0.4,
            },
            Self {
                name: t("Calligraphie", "Calligraphy").to_string(),
                width: 10.0,
                hardness: 0.6,
                stabilization: 0.7,
                pressure_strength: 1.0,
            },
        ]
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
    /// `true` (défaut) = lissage Catmull-Rom au rendu (pinceau à main levée).
    /// `false` pour les formes géométriques (rectangle/ellipse/polygone/
    /// étoile/ligne/flèche) dont les sommets doivent rester des angles nets —
    /// le lissage les arrondissait sinon (bug constaté : un rectangle tracé
    /// à l'outil Forme ressortait avec des coins arrondis).
    #[serde(default = "default_smooth")]
    pub smooth: bool,
}

fn default_smooth() -> bool {
    true
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
            smooth: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}
