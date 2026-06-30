//! Capture du geste (section 4.1) : assemble lissage + pression en un trait.
//!
//! Cycle de vie : `begin` au `pressed`, `extend` à chaque `moved`, `finish`
//! au `released`. La machine produit un `Stroke` du modèle, prêt à être
//! poussé dans une couche + l'historique.

use crate::input::pressure::PressureModel;
use crate::input::smoothing::Ema;
use crate::model::{Stroke, StrokePoint, Tool};

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// État de capture du trait en cours.
pub struct GestureCapture {
    ema: Ema,
    pressure: PressureModel,
    stroke: Option<Stroke>,
    last_pos: Option<(f32, f32)>,
    last_t: f32,
    start_t: f64,
}

impl GestureCapture {
    pub fn new() -> Self {
        Self {
            ema: Ema::new(0.5),
            pressure: PressureModel::default(),
            stroke: None,
            last_pos: None,
            last_t: 0.0,
            start_t: 0.0,
        }
    }

    /// Règle l'intensité de la simulation de pression (0 = épaisseur fixe,
    /// 1 = effet vitesse→épaisseur maximal). Interpole les facteurs vers 1.0.
    pub fn set_pressure_strength(&mut self, s: f32) {
        let s = s.clamp(0.0, 1.0);
        self.pressure.width_min_factor = lerp(1.0, 0.35, s);
        self.pressure.width_max_factor = lerp(1.0, 1.6, s);
    }

    /// Trait en cours (pour le rendu temps réel), s'il existe.
    pub fn current(&self) -> Option<&Stroke> {
        self.stroke.as_ref()
    }

    /// Début d'un trait. `now` = temps absolu en secondes (ex. egui `input.time`).
    pub fn begin(&mut self, pos: (f32, f32), color: [u8; 4], base_width: f32, tool: Tool, now: f64) {
        self.ema.reset();
        self.pressure.reset();
        self.start_t = now;
        self.last_t = 0.0;
        let mut stroke = Stroke::new(color, base_width, tool);
        let sp = self.ema.filter(pos);
        // Premier point : pas de vitesse encore → on prend la base.
        stroke.points.push(StrokePoint { pos: sp, width: base_width });
        self.last_pos = Some(sp);
        self.stroke = Some(stroke);
    }

    /// Ajoute un point au trait en cours.
    pub fn extend(&mut self, pos: (f32, f32), now: f64) {
        let Some(stroke) = self.stroke.as_mut() else { return };
        let t = (now - self.start_t) as f32;
        let dt = (t - self.last_t).max(1e-4);

        let sp = self.ema.filter(pos);
        let dist = match self.last_pos {
            Some(p) => ((sp.0 - p.0).powi(2) + (sp.1 - p.1).powi(2)).sqrt(),
            None => 0.0,
        };
        // Évite d'empiler des points quasi confondus (bruit du capteur).
        if dist < 0.5 {
            return;
        }
        let width = self.pressure.width_for(stroke.base_width, dist, dt);
        stroke.points.push(StrokePoint { pos: sp, width });
        self.last_pos = Some(sp);
        self.last_t = t;
    }

    /// Clôt le trait et le renvoie (ou `None` si trait vide/insignifiant).
    pub fn finish(&mut self) -> Option<Stroke> {
        self.last_pos = None;
        self.stroke.take().filter(|s| !s.is_empty())
    }
}

impl Default for GestureCapture {
    fn default() -> Self {
        Self::new()
    }
}
