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

    /// Règle la stabilisation du tracé (Sprint 3.2), au-delà du lissage fixe
    /// déjà en place : 0 = tracé brut (alpha=1, aucun lissage), 1 = très
    /// stabilisé (alpha proche de 0, le tracé « traîne » derrière le
    /// pointeur). `alpha` du filtre EMA décroît quand `stabilization` monte.
    pub fn set_stabilization(&mut self, stabilization: f32) {
        let s = stabilization.clamp(0.0, 1.0);
        self.ema.set_alpha(lerp(1.0, 0.05, s));
    }

    /// Trait en cours (pour le rendu temps réel), s'il existe.
    pub fn current(&self) -> Option<&Stroke> {
        self.stroke.as_ref()
    }

    /// Début d'un trait. `now` = temps absolu en secondes (ex. egui `input.time`).
    /// `pressure` : pression réelle 0..=1 si un stylet/tablette compatible est
    /// détecté ce geste (Sprint 3.1, via les événements `Touch` d'egui) ;
    /// `None` retombe sur l'épaisseur de base pour ce premier point, comme
    /// avant que la pression réelle n'existe.
    pub fn begin(&mut self, pos: (f32, f32), color: [u8; 4], base_width: f32, tool: Tool, now: f64, pressure: Option<f32>) {
        self.ema.reset();
        self.pressure.reset();
        self.start_t = now;
        self.last_t = 0.0;
        let mut stroke = Stroke::new(color, base_width, tool);
        let sp = self.ema.filter(pos);
        let w0 = match pressure {
            Some(p) => self.pressure.width_for_pressure(base_width, p),
            // Premier point sans pression connue : pas de vitesse encore → la base.
            None => base_width,
        };
        stroke.points.push(StrokePoint { pos: sp, width: w0 });
        self.last_pos = Some(sp);
        self.stroke = Some(stroke);
    }

    /// Ajoute un point au trait en cours. `pressure` : voir [`Self::begin`] —
    /// pilote directement l'épaisseur quand connue, sinon retombe sur la
    /// simulation vitesse → épaisseur existante.
    pub fn extend(&mut self, pos: (f32, f32), now: f64, pressure: Option<f32>) {
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
        let width = match pressure {
            Some(p) => self.pressure.width_for_pressure(stroke.base_width, p),
            None => self.pressure.width_for(stroke.base_width, dist, dt),
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_stabilization_lags_more_behind_the_raw_input_than_low() {
        let mut low = GestureCapture::new();
        low.set_stabilization(0.0);
        low.begin((0.0, 0.0), [0, 0, 0, 255], 4.0, Tool::Brush, 0.0, None);
        low.extend((100.0, 0.0), 0.1, None);
        let low_x = low.stroke.as_ref().unwrap().points.last().unwrap().pos.0;

        let mut high = GestureCapture::new();
        high.set_stabilization(1.0);
        high.begin((0.0, 0.0), [0, 0, 0, 255], 4.0, Tool::Brush, 0.0, None);
        high.extend((100.0, 0.0), 0.1, None);
        let high_x = high.stroke.as_ref().unwrap().points.last().unwrap().pos.0;

        assert!(high_x < low_x, "stabilisation haute ({high_x}) devrait traîner plus que basse ({low_x})");
    }

    #[test]
    fn real_pressure_overrides_the_speed_simulation() {
        // Deux gestes à la même vitesse (même distance, même dt) mais des
        // pressions réelles opposées : la largeur doit suivre la pression,
        // pas la vitesse simulée (qui serait identique dans les deux cas).
        let mut light = GestureCapture::new();
        light.begin((0.0, 0.0), [0, 0, 0, 255], 10.0, Tool::Brush, 0.0, Some(0.0));
        light.extend((100.0, 0.0), 0.1, Some(0.0));
        let w_light = light.stroke.as_ref().unwrap().points.last().unwrap().width;

        let mut heavy = GestureCapture::new();
        heavy.begin((0.0, 0.0), [0, 0, 0, 255], 10.0, Tool::Brush, 0.0, Some(1.0));
        heavy.extend((100.0, 0.0), 0.1, Some(1.0));
        let w_heavy = heavy.stroke.as_ref().unwrap().points.last().unwrap().width;

        assert!(w_heavy > w_light, "pression forte ({w_heavy}) devrait être plus épaisse que légère ({w_light})");
    }
}
