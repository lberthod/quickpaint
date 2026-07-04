//! Géométrie de sélection partagée : test de recouvrement d'un point par un
//! trait (pipette) et de proximité (gomme vectorielle). Travaille sur le
//! modèle, sans rendu.

use crate::model::Stroke;

/// `true` si `p` est sur le trait (dans sa demi-largeur). Pour la pipette.
pub fn point_on_stroke(stroke: &Stroke, p: (f32, f32)) -> bool {
    within(stroke, p, 0.0)
}

/// `true` si le trait passe à moins de `radius` de `p`. Pour la gomme.
pub fn stroke_near(stroke: &Stroke, p: (f32, f32), radius: f32) -> bool {
    within(stroke, p, radius)
}

/// Boîte englobante (min, max) des traits donnés, en coords document.
pub fn bounds_of<'a>(strokes: impl Iterator<Item = &'a Stroke>) -> Option<((f32, f32), (f32, f32))> {
    let mut min = (f32::INFINITY, f32::INFINITY);
    let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for s in strokes {
        for p in &s.points {
            let h = p.width * 0.5;
            min.0 = min.0.min(p.pos.0 - h);
            min.1 = min.1.min(p.pos.1 - h);
            max.0 = max.0.max(p.pos.0 + h);
            max.1 = max.1.max(p.pos.1 + h);
            any = true;
        }
    }
    any.then_some((min, max))
}

/// `true` si les deux boîtes (min, max) se recoupent. Pour le marquee.
pub fn bbox_intersects(a: ((f32, f32), (f32, f32)), b: ((f32, f32), (f32, f32))) -> bool {
    let ((amn, amx), (bmn, bmx)) = (a, b);
    amn.0 <= bmx.0 && amx.0 >= bmn.0 && amn.1 <= bmx.1 && amx.1 >= bmn.1
}

/// Test point-dans-polygone par lancer de rayon (règle pair/impair). Pour le
/// lasso. `poly` est la liste ordonnée des sommets (non refermée).
pub fn point_in_polygon(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        // Le segment (j → i) traverse-t-il la demi-droite horizontale en `p` ?
        if (yi > p.1) != (yj > p.1) {
            let x_cross = xi + (p.1 - yi) / (yj - yi) * (xj - xi);
            if p.0 < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Test point-dans-ellipse : `rect` (min, max) définit la boîte englobante
/// de l'ellipse (mêmes bornes que le marquee rectangulaire, dessinée comme
/// une ellipse inscrite). Pour l'outil Sélection en mode Ellipse.
pub fn point_in_ellipse(rect: ((f32, f32), (f32, f32)), p: (f32, f32)) -> bool {
    let ((x0, y0), (x1, y1)) = rect;
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let (rx, ry) = ((x1 - x0).abs() * 0.5, (y1 - y0).abs() * 0.5);
    if rx <= 0.0 || ry <= 0.0 {
        return false;
    }
    let (dx, dy) = ((p.0 - cx) / rx, (p.1 - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

fn within(stroke: &Stroke, p: (f32, f32), radius: f32) -> bool {
    let pts = &stroke.points;
    match pts.len() {
        0 => false,
        1 => dist(p, pts[0].pos) <= pts[0].width.max(1.0) * 0.5 + radius,
        _ => pts.windows(2).any(|w| {
            let half = w[0].width.max(w[1].width) * 0.5 + radius + 0.5;
            dist_to_segment(p, w[0].pos, w[1].pos) <= half
        }),
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-6 {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
    dist(p, (a.0 + dx * t, a.1 + dy * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_overlap_and_disjoint() {
        let a = ((0.0, 0.0), (10.0, 10.0));
        assert!(bbox_intersects(a, ((5.0, 5.0), (15.0, 15.0))));
        assert!(bbox_intersects(a, ((-5.0, -5.0), (1.0, 1.0))));
        assert!(!bbox_intersects(a, ((11.0, 0.0), (20.0, 10.0))));
        assert!(!bbox_intersects(a, ((0.0, 11.0), (10.0, 20.0))));
    }

    #[test]
    fn point_in_square() {
        let sq = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(point_in_polygon(&sq, (5.0, 5.0)));
        assert!(!point_in_polygon(&sq, (15.0, 5.0)));
        assert!(!point_in_polygon(&sq, (-1.0, 5.0)));
    }

    #[test]
    fn degenerate_polygon_is_empty() {
        assert!(!point_in_polygon(&[(0.0, 0.0), (1.0, 1.0)], (0.0, 0.0)));
    }

    #[test]
    fn point_in_ellipse_center_and_corners() {
        let rect = ((0.0, 0.0), (20.0, 10.0));
        assert!(point_in_ellipse(rect, (10.0, 5.0))); // centre
        assert!(!point_in_ellipse(rect, (0.0, 0.0))); // coin, hors de l'ellipse inscrite
        assert!(point_in_ellipse(rect, (10.0, 0.0))); // sommet haut de l'ellipse
    }

    #[test]
    fn point_in_ellipse_degenerate_rect_is_empty() {
        assert!(!point_in_ellipse(((0.0, 0.0), (0.0, 10.0)), (0.0, 5.0)));
    }
}
