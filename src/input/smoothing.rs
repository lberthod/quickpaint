//! Lissage des points bruts (section 4.2) : filtre exponentiel (EMA).
//!
//! L'interpolation Catmull-Rom est faite au rendu (`render::ribbon`) pour
//! garder ce module sans dépendance et testable seul.

/// Filtre exponentiel sur une position 2D : `p = p_prev*(1-α) + p_brut*α`.
#[derive(Clone, Debug)]
pub struct Ema {
    alpha: f32,
    state: Option<(f32, f32)>,
}

impl Ema {
    /// `alpha` ∈ ]0,1] : proche de 1 = peu de lissage, proche de 0 = très lisse.
    pub fn new(alpha: f32) -> Self {
        Self { alpha: alpha.clamp(0.01, 1.0), state: None }
    }

    pub fn reset(&mut self) {
        self.state = None;
    }

    /// Applique le filtre et renvoie la position lissée.
    pub fn filter(&mut self, raw: (f32, f32)) -> (f32, f32) {
        let out = match self.state {
            None => raw,
            Some(prev) => (
                prev.0 * (1.0 - self.alpha) + raw.0 * self.alpha,
                prev.1 * (1.0 - self.alpha) + raw.1 * self.alpha,
            ),
        };
        self.state = Some(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_point_is_unchanged() {
        let mut ema = Ema::new(0.5);
        assert_eq!(ema.filter((10.0, 20.0)), (10.0, 20.0));
    }

    #[test]
    fn smoothing_pulls_toward_previous() {
        let mut ema = Ema::new(0.5);
        ema.filter((0.0, 0.0));
        let p = ema.filter((10.0, 0.0));
        assert!((p.0 - 5.0).abs() < 1e-6);
    }
}
