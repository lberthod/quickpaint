//! Booléens de chemins (roadmap P2 #13) : union / soustraction / intersection
//! de deux formes pleines sélectionnées, via [`geo-clipper`] (Clipper 2D).
//!
//! MVP volontairement simple : seul le contour extérieur de chaque polygone
//! résultat est conservé (pas de trous) — cohérent avec `Stroke`, qui ne
//! modélise qu'un seul anneau de points. Un résultat multi-polygones (ex.
//! soustraction qui sépare une forme en deux) produit un trait par polygone.

use geo_clipper::Clipper;
use geo_types::{Coord, LineString, Polygon};

use crate::model::{Stroke, StrokePoint};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanKind {
    Union,
    Subtract,
    Intersect,
}

impl BooleanKind {
    pub fn label(self) -> &'static str {
        match self {
            BooleanKind::Union => "Union",
            BooleanKind::Subtract => "Soustraction",
            BooleanKind::Intersect => "Intersection",
        }
    }
}

/// Précision du scaling entier interne à Clipper (plus grand = plus précis,
/// voir doc de `geo-clipper`). Les coordonnées document sont en pixels, une
/// précision de 2 décimales est largement suffisante.
const CLIPPER_SCALE: f64 = 100.0;

fn stroke_to_polygon(s: &Stroke) -> Polygon<f64> {
    let coords: Vec<Coord<f64>> =
        s.points.iter().map(|p| Coord { x: p.pos.0 as f64, y: p.pos.1 as f64 }).collect();
    Polygon::new(LineString(coords), vec![])
}

/// Calcule `subject OP clip` et renvoie les contours extérieurs des polygones
/// résultat, en points document (dans l'ordre décroissant d'aire). `None` si
/// le résultat est vide (ex. intersection disjointe).
pub fn apply(subject: &Stroke, clip: &Stroke, kind: BooleanKind) -> Option<Vec<Vec<(f32, f32)>>> {
    let a = stroke_to_polygon(subject);
    let b = stroke_to_polygon(clip);
    let result = match kind {
        BooleanKind::Union => a.union(&b, CLIPPER_SCALE),
        BooleanKind::Subtract => a.difference(&b, CLIPPER_SCALE),
        BooleanKind::Intersect => a.intersection(&b, CLIPPER_SCALE),
    };
    if result.0.is_empty() {
        return None;
    }
    let mut polys: Vec<Vec<(f32, f32)>> = result
        .0
        .iter()
        .map(|poly| poly.exterior().coords().map(|c| (c.x as f32, c.y as f32)).collect())
        .collect();
    polys.sort_by(|a, b| polygon_area(b).partial_cmp(&polygon_area(a)).unwrap());
    Some(polys)
}

fn polygon_area(pts: &[(f32, f32)]) -> f32 {
    let mut sum = 0.0;
    for w in pts.windows(2) {
        sum += w[0].0 * w[1].1 - w[1].0 * w[0].1;
    }
    sum.abs() * 0.5
}

/// Construit un nouveau `Stroke` plein à partir d'un contour résultat, en
/// reprenant le style (couleur, épaisseur) du trait `subject` d'origine.
pub fn stroke_from_points(template: &Stroke, points: Vec<(f32, f32)>) -> Stroke {
    let mut s = Stroke::new(template.color, template.base_width, template.tool);
    s.fill = true;
    s.points = points.into_iter().map(|pos| StrokePoint { pos, width: template.base_width }).collect();
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tool;

    fn square(x: f32, y: f32, w: f32) -> Stroke {
        let mut s = Stroke::new([0, 0, 0, 255], 2.0, Tool::Brush);
        s.fill = true;
        s.points = vec![(x, y), (x + w, y), (x + w, y + w), (x, y + w), (x, y)]
            .into_iter()
            .map(|pos| StrokePoint { pos, width: 2.0 })
            .collect();
        s
    }

    #[test]
    fn union_of_overlapping_squares_covers_both() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 5.0, 10.0);
        let result = apply(&a, &b, BooleanKind::Union).unwrap();
        assert_eq!(result.len(), 1);
        assert!(polygon_area(&result[0]) > 100.0);
    }

    #[test]
    fn intersection_of_overlapping_squares_is_the_overlap() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 5.0, 10.0);
        let result = apply(&a, &b, BooleanKind::Intersect).unwrap();
        assert_eq!(result.len(), 1);
        let area = polygon_area(&result[0]);
        assert!((area - 25.0).abs() < 0.5, "aire attendue ~25, obtenue {area}");
    }

    #[test]
    fn intersection_of_disjoint_squares_is_empty() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(100.0, 100.0, 10.0);
        assert!(apply(&a, &b, BooleanKind::Intersect).is_none());
    }

    #[test]
    fn subtract_removes_overlap() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 5.0, 10.0);
        let result = apply(&a, &b, BooleanKind::Subtract).unwrap();
        assert_eq!(result.len(), 1);
        let area = polygon_area(&result[0]);
        assert!((area - 75.0).abs() < 0.5, "aire attendue ~75, obtenue {area}");
    }
}
