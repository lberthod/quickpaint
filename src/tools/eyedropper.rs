//! Pipette : prélève la couleur du trait le plus haut sous un point (idée 3).
//! Test géométrique direct sur le modèle vectoriel (instantané, sans rendu).

use crate::model::{Document, Tool};
use crate::tools::hit;

/// Renvoie la couleur RGB du trait le plus en avant couvrant `pos` (coords
/// document), ou `None` si on tape le fond.
pub fn pick(doc: &Document, pos: (f32, f32)) -> Option<[u8; 3]> {
    for layer in doc.layers.iter().rev() {
        if !layer.visible {
            continue;
        }
        for stroke in layer.strokes.iter().rev() {
            if stroke.tool == Tool::Eraser {
                continue; // un ancien trait gomme n'a pas de couleur propre
            }
            if hit::point_on_stroke(stroke, pos) {
                let c = stroke.color;
                return Some([c[0], c[1], c[2]]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Stroke, StrokePoint};

    #[test]
    fn picks_color_on_stroke() {
        let mut doc = Document::new((100, 100));
        let mut s = Stroke::new([200, 10, 10, 255], 8.0, Tool::Brush);
        s.points.push(StrokePoint { pos: (0.0, 0.0), width: 8.0 });
        s.points.push(StrokePoint { pos: (20.0, 0.0), width: 8.0 });
        doc.layers[0].strokes.push(s);
        assert_eq!(pick(&doc, (10.0, 1.0)), Some([200, 10, 10]));
        assert_eq!(pick(&doc, (10.0, 50.0)), None);
    }
}
