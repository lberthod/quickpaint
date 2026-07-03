//! Outils « formes » : ligne, flèche, rectangle, ellipse, polygone, étoile.
//! Tracé à deux points (départ → position courante), largeur constante. Génère
//! un `Stroke` du modèle, donc rendu et undo passent par le même chemin que le
//! pinceau.

use crate::model::{Stroke, StrokePoint, Tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Polygon,
    Star,
}

impl Shape {
    /// Formes fermées (remplissables).
    pub fn closed(self) -> bool {
        matches!(self, Shape::Rectangle | Shape::Ellipse | Shape::Polygon | Shape::Star)
    }
}

/// Nombre de segments pour approximer une ellipse.
const ELLIPSE_SIDES: usize = 64;

/// Construit le trait d'une forme entre `start` et `end`. `sides` sert aux
/// polygones / étoiles. `fill` ignoré pour les formes ouvertes.
pub fn build(
    shape: Shape,
    start: (f32, f32),
    end: (f32, f32),
    color: [u8; 4],
    width: f32,
    fill: bool,
    sides: usize,
) -> Stroke {
    let mut stroke = Stroke::new(color, width, Tool::Brush);
    stroke.fill = fill && shape.closed();
    // Angles nets : pas de lissage Catmull-Rom sur une géométrie déjà exacte.
    stroke.smooth = false;
    let cx = (start.0 + end.0) * 0.5;
    let cy = (start.1 + end.1) * 0.5;
    let rx = (end.0 - start.0).abs() * 0.5;
    let ry = (end.1 - start.1).abs() * 0.5;
    let n = sides.clamp(3, 16);

    let pts: Vec<(f32, f32)> = match shape {
        Shape::Line => vec![start, end],
        Shape::Arrow => arrow(start, end, width),
        Shape::Rectangle => {
            let (x0, y0) = start;
            let (x1, y1) = end;
            vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
        }
        Shape::Ellipse => (0..=ELLIPSE_SIDES)
            .map(|i| {
                let a = i as f32 / ELLIPSE_SIDES as f32 * std::f32::consts::TAU;
                (cx + a.cos() * rx, cy + a.sin() * ry)
            })
            .collect(),
        Shape::Polygon => (0..=n)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
                (cx + a.cos() * rx, cy + a.sin() * ry)
            })
            .collect(),
        Shape::Star => (0..=2 * n)
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2
                    + i as f32 / (2 * n) as f32 * std::f32::consts::TAU;
                let r = if i % 2 == 0 { 1.0 } else { 0.42 };
                (cx + a.cos() * rx * r, cy + a.sin() * ry * r)
            })
            .collect(),
    };
    for pos in pts {
        stroke.points.push(StrokePoint { pos, width });
    }
    stroke
}

/// Hampe + deux barbes à l'extrémité (une seule polyligne).
fn arrow(start: (f32, f32), end: (f32, f32), width: f32) -> Vec<(f32, f32)> {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / len, dy / len); // direction
    let barb = (len * 0.28).clamp(8.0, 40.0).max(width * 2.5);
    let ang = 0.45_f32; // ~26°
    let (c, s) = (ang.cos(), ang.sin());
    // back = -direction, tournée de ±ang
    let b1 = (-ux * c + uy * s, -uy * c - ux * s);
    let b2 = (-ux * c - uy * s, -uy * c + ux * s);
    let p1 = (end.0 + b1.0 * barb, end.1 + b1.1 * barb);
    let p2 = (end.0 + b2.0 * barb, end.1 + b2.1 * barb);
    vec![start, end, p1, end, p2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_is_closed() {
        let s = build(Shape::Rectangle, (0.0, 0.0), (10.0, 6.0), [0; 4], 2.0, false, 6);
        assert_eq!(s.points.len(), 5);
        assert_eq!(s.points.first().unwrap().pos, s.points.last().unwrap().pos);
    }

    #[test]
    fn line_has_two_points() {
        let s = build(Shape::Line, (0.0, 0.0), (10.0, 10.0), [0; 4], 2.0, false, 6);
        assert_eq!(s.points.len(), 2);
    }

    #[test]
    fn arrow_is_never_filled_and_has_barbs() {
        let s = build(Shape::Arrow, (0.0, 0.0), (20.0, 0.0), [0; 4], 2.0, true, 6);
        assert!(!s.fill);
        assert_eq!(s.points.len(), 5);
    }

    #[test]
    fn star_has_expected_vertices() {
        let s = build(Shape::Star, (0.0, 0.0), (10.0, 10.0), [0; 4], 2.0, true, 5);
        assert_eq!(s.points.len(), 11); // 2*5 + fermeture
        assert!(s.fill);
    }
}
