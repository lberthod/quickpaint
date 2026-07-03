//! Outil plume (roadmap #9) : chemin de Bézier cubique éditable pendant le
//! tracé. À la validation, on échantillonne en polyligne → `Stroke` standard
//! (donc rendu, sélection, déplacement, remplissage, export gratuits).
//!
//! Roadmap P2 #12 (F2, « rien n'est jamais figé ») : les ancres sont
//! conservées sur le `Stroke` (`PenPath`) au lieu d'être jetées après
//! l'échantillonnage, pour qu'un double-clic ultérieur les fasse réapparaître
//! et les rende à nouveau déplaçables.

use serde::{Deserialize, Serialize};

/// Un sommet d'ancrage et ses deux poignées (coords document).
/// Sommet anguleux : `h_in == h_out == pos`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: (f32, f32),
    pub h_in: (f32, f32),
    pub h_out: (f32, f32),
}

/// Chemin de plume persisté sur un `Stroke` (roadmap P2 #12). `points` reste
/// la source de vérité pour le rendu/hit-test/export ; `PenPath` n'est utile
/// que pour ré-ouvrir l'édition des poignées.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PenPath {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}

impl PenPath {
    /// Ré-échantillonne en points document (met à jour `Stroke::points` après
    /// une édition de nœud).
    pub fn sample(&self) -> Vec<(f32, f32)> {
        sample(&self.anchors, self.closed)
    }
}

impl Anchor {
    pub fn corner(pos: (f32, f32)) -> Self {
        Self { pos, h_in: pos, h_out: pos }
    }

    /// Définit une poignée symétrique à partir du vecteur de glissé.
    pub fn set_symmetric(&mut self, drag_to: (f32, f32)) {
        let v = (drag_to.0 - self.pos.0, drag_to.1 - self.pos.1);
        self.h_out = (self.pos.0 + v.0, self.pos.1 + v.1);
        self.h_in = (self.pos.0 - v.0, self.pos.1 - v.1);
    }
}

/// Échantillonne le chemin (densité ∝ longueur) en points document.
pub fn sample(anchors: &[Anchor], closed: bool) -> Vec<(f32, f32)> {
    let n = anchors.len();
    if n < 2 {
        return anchors.iter().map(|a| a.pos).collect();
    }
    let mut out = Vec::new();
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let a = anchors[i];
        let b = anchors[(i + 1) % n];
        let len = dist(a.pos, b.pos).max(1.0);
        let steps = ((len / 4.0).ceil() as usize).clamp(2, 160);
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            out.push(cubic(a.pos, a.h_out, b.h_in, b.pos, t));
        }
    }
    out.push(if closed { anchors[0].pos } else { anchors[n - 1].pos });
    out
}

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_anchors_make_straight_line() {
        let anchors = [Anchor::corner((0.0, 0.0)), Anchor::corner((10.0, 0.0))];
        let pts = sample(&anchors, false);
        assert!(pts.iter().all(|p| p.1.abs() < 1e-4)); // reste sur l'axe y=0
    }

    #[test]
    fn closed_path_returns_to_start() {
        let anchors = [
            Anchor::corner((0.0, 0.0)),
            Anchor::corner((10.0, 0.0)),
            Anchor::corner((10.0, 10.0)),
        ];
        let pts = sample(&anchors, true);
        assert_eq!(*pts.last().unwrap(), (0.0, 0.0));
    }
}
