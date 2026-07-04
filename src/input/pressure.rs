//! Simulation de pression (section 4.3) : la vitesse du trait → épaisseur.
//!
//! Lent = trait épais, rapide = trait fin (rendu calligraphique). On lisse
//! aussi l'épaisseur (EMA) pour éviter les sauts brusques.

/// Paramètres de la courbe vitesse → épaisseur.
#[derive(Clone, Debug)]
pub struct PressureModel {
    /// Vitesse (px/s) au-delà de laquelle on atteint `width_min`.
    pub v_ref: f32,
    pub width_min_factor: f32, // fraction de base_width quand on va vite
    pub width_max_factor: f32, // fraction de base_width quand on va lentement
    pub width_alpha: f32,      // lissage de l'épaisseur
    smoothed: Option<f32>,
}

impl Default for PressureModel {
    fn default() -> Self {
        Self {
            v_ref: 2500.0,
            width_min_factor: 0.35,
            width_max_factor: 1.6,
            width_alpha: 0.35,
            smoothed: None,
        }
    }
}

impl PressureModel {
    pub fn reset(&mut self) {
        self.smoothed = None;
    }

    /// Calcule l'épaisseur en pixels à partir de la base et de la vitesse.
    ///
    /// `dist` en px, `dt` en secondes (déjà > 0 garanti par l'appelant).
    pub fn width_for(&mut self, base_width: f32, dist: f32, dt: f32) -> f32 {
        let speed = if dt > 1e-6 { dist / dt } else { 0.0 };
        let k = (speed / self.v_ref).clamp(0.0, 1.0);
        // lerp(max, min, k) : k=0 (lent) → max, k=1 (rapide) → min
        let factor = lerp(self.width_max_factor, self.width_min_factor, k);
        let target = base_width * factor;

        let out = match self.smoothed {
            None => target,
            Some(prev) => prev * (1.0 - self.width_alpha) + target * self.width_alpha,
        };
        self.smoothed = Some(out);
        out
    }

    /// Épaisseur à partir d'une pression **réelle** (0..=1, stylet/tablette
    /// détecté — Sprint 3.1), plutôt que simulée depuis la vitesse : mêmes
    /// bornes `width_min_factor`/`width_max_factor`, même lissage, mais la
    /// pression pilote directement le facteur au lieu d'en être déduite.
    pub fn width_for_pressure(&mut self, base_width: f32, pressure: f32) -> f32 {
        let p = pressure.clamp(0.0, 1.0);
        // p=0 (légère) → min, p=1 (forte) → max — inverse de `width_for` où
        // c'est la vitesse (rapide = fin) qui pilote, pas l'appui.
        let factor = lerp(self.width_min_factor, self.width_max_factor, p);
        let target = base_width * factor;
        let out = match self.smoothed {
            None => target,
            Some(prev) => prev * (1.0 - self.width_alpha) + target * self.width_alpha,
        };
        self.smoothed = Some(out);
        out
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_is_thicker_than_fast() {
        let base = 10.0;
        let mut slow = PressureModel::default();
        let mut fast = PressureModel::default();
        let w_slow = slow.width_for(base, 1.0, 0.05); // 20 px/s
        let w_fast = fast.width_for(base, 500.0, 0.05); // 10000 px/s
        assert!(w_slow > w_fast);
    }

    #[test]
    fn real_pressure_scales_width_directly() {
        let base = 10.0;
        let mut light = PressureModel::default();
        let mut heavy = PressureModel::default();
        let w_light = light.width_for_pressure(base, 0.0);
        let w_heavy = heavy.width_for_pressure(base, 1.0);
        assert!(w_heavy > w_light);
    }
}
