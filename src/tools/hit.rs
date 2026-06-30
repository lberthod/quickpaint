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
