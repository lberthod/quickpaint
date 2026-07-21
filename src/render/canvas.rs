//! Pont entre le modèle et le `Painter` egui (section 2 : chemin le plus court).
//!
//! On convertit le ruban (coords document) en `egui::Mesh` (coords écran) via
//! une transformation document → écran (pan/zoom, section 10).
//!
//! Optimisation (section 6) : la **géométrie** du ruban (coûteuse :
//! Catmull-Rom + triangulation) est mise en cache par trait validé, en
//! coordonnées document (donc indépendante du pan/zoom). À chaque frame on ne
//! refait que la projection document→écran, bon marché. Seul le trait *en
//! cours* est reconstruit en continu.

use crate::model::{Document, Stroke, Tool};
use crate::render::ribbon;
use egui::{Color32, Mesh, Painter, Pos2, Vec2};
use std::collections::HashMap;

/// Transformation document → écran : origine + échelle + rotation de la vue
/// (Sprint T, point 93 — l'angle ne touche que l'affichage, jamais le
/// document). La rotation pivote autour de `origin` (le point écran du coin
/// (0,0) du document) ; le pivot « visuel » au centre du document est obtenu
/// côté app en compensant le pan au changement d'angle.
#[derive(Clone, Copy)]
pub struct ViewTransform {
    pub origin: Pos2, // coin haut-gauche du canvas à l'écran
    pub scale: f32,
    /// Rotation de la vue en radians (0 = comportement historique exact).
    pub angle: f32,
}

impl ViewTransform {
    pub fn doc_to_screen(&self, p: (f32, f32)) -> Pos2 {
        let (x, y) = (p.0 * self.scale, p.1 * self.scale);
        if self.angle == 0.0 {
            return self.origin + Vec2::new(x, y);
        }
        let (co, si) = (self.angle.cos(), self.angle.sin());
        self.origin + Vec2::new(x * co - y * si, x * si + y * co)
    }

    pub fn screen_to_doc(&self, p: Pos2) -> (f32, f32) {
        let v = p - self.origin;
        if self.angle == 0.0 {
            return (v.x / self.scale, v.y / self.scale);
        }
        let (co, si) = (self.angle.cos(), self.angle.sin());
        // Rotation inverse (transposée) avant la mise à l'échelle inverse.
        ((v.x * co + v.y * si) / self.scale, (-v.x * si + v.y * co) / self.scale)
    }
}

fn color_of(stroke: &Stroke, bg: Color32) -> Color32 {
    match stroke.tool {
        // La gomme « peint » avec la couleur de fond (MVP raster-like).
        Tool::Eraser => bg,
        Tool::Brush => {
            let [r, g, b, a] = stroke.color;
            Color32::from_rgba_unmultiplied(r, g, b, a)
        }
    }
}

/// Projette une géométrie document → `egui::Mesh` écran et la dessine.
fn paint_geometry(painter: &Painter, rib: &ribbon::Mesh, color: Color32, view: &ViewTransform) {
    if rib.indices.is_empty() {
        return;
    }
    let mut mesh = Mesh::default();
    for v in &rib.vertices {
        mesh.colored_vertex(view.doc_to_screen(v.pos), color);
    }
    mesh.indices.clone_from(&rib.indices);
    painter.add(mesh);
}

/// Dessine un trait en (re)construisant sa géométrie immédiatement.
/// À réserver aux aperçus courts (formes) ou aux traits translucides.
pub fn paint_stroke(painter: &Painter, stroke: &Stroke, view: &ViewTransform, bg: Color32) {
    let rib = ribbon::build(stroke);
    paint_geometry(painter, &rib, color_of(stroke, bg), view);
}

/// Taille des paquets « cuits » du trait en cours.
const FREEZE_CHUNK: usize = 64;

/// Rendu **incrémental** du trait en cours de tracé (perf, section 6 / idée
/// fluidité). La géométrie des points anciens est cuite une fois par paquets ;
/// chaque frame on ne reconstruit que la courte queue. Coût par frame borné →
/// le trait ne « traîne » plus derrière le doigt même après des milliers de
/// points (sinon O(n²) cumulé).
#[derive(Default)]
pub struct ActiveStroke {
    frozen: ribbon::Mesh,
    frozen_upto: usize, // nombre de points déjà cuits dans `frozen`
}

impl ActiveStroke {
    /// À appeler au début de chaque nouveau trait.
    pub fn reset(&mut self) {
        self.frozen = ribbon::Mesh::default();
        self.frozen_upto = 0;
    }

    pub fn paint(&mut self, painter: &Painter, stroke: &Stroke, view: &ViewTransform, bg: Color32) {
        let pts = &stroke.points;
        let n = pts.len();
        let color = color_of(stroke, bg);

        // Cuire les segments anciens par paquets (en gardant 1 point de
        // recouvrement pour relier proprement au paquet précédent).
        while n.saturating_sub(self.frozen_upto) > FREEZE_CHUNK {
            let start = self.frozen_upto.saturating_sub(1);
            let end = self.frozen_upto + FREEZE_CHUNK;
            let chunk = ribbon::build_slice(&pts[start..end]);
            ribbon::append(&mut self.frozen, &chunk);
            self.frozen_upto = end;
        }

        paint_geometry(painter, &self.frozen, color, view);

        // Reconstruire seulement la queue (taille bornée par FREEZE_CHUNK).
        let start = self.frozen_upto.saturating_sub(1);
        let tail = ribbon::build_slice(&pts[start..n]);
        paint_geometry(painter, &tail, color, view);
    }
}

/// Cache de géométrie des traits validés, clé = `Stroke::id`.
#[derive(Default)]
pub struct StrokeCache {
    meshes: HashMap<u64, ribbon::Mesh>,
}

impl StrokeCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Vide tout le cache (ex. après ouverture d'un projet).
    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    /// Invalide les traits dont la géométrie a changé (ex. déplacement).
    pub fn invalidate<'a>(&mut self, ids: impl IntoIterator<Item = &'a u64>) {
        for id in ids {
            self.meshes.remove(id);
        }
    }

    /// Supprime les entrées dont le trait n'existe plus (après undo/clear).
    ///
    /// On n'insère un mesh que pour un trait existant ; le cache ne peut donc
    /// devenir « trop grand » qu'après une suppression. On évite alors de
    /// reconstruire un `HashSet` à chaque frame : court-circuit O(1) tant que
    /// rien n'a disparu.
    pub fn prune(&mut self, doc: &Document) {
        let total: usize = doc.layers.iter().map(|l| l.strokes.len()).sum();
        if self.meshes.len() <= total {
            return;
        }
        let live: std::collections::HashSet<u64> = doc
            .layers
            .iter()
            .flat_map(|l| l.strokes.iter().map(|s| s.id))
            .collect();
        self.meshes.retain(|id, _| live.contains(id));
    }

    /// Dessine un seul trait via sa géométrie cachée (rendu en z-order).
    pub fn paint_one(&mut self, painter: &Painter, stroke: &Stroke, view: &ViewTransform, bg: Color32, opacity: f32) {
        let rib = self.meshes.entry(stroke.id).or_insert_with(|| ribbon::build(stroke));
        let color = color_of(stroke, bg).gamma_multiply(opacity.clamp(0.0, 1.0));
        paint_geometry(painter, rib, color, view);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rotation de la vue (Sprint T, point 93) : aller-retour
    /// doc→écran→doc exact quel que soit l'angle, et angle 0 = chemin
    /// historique inchangé.
    #[test]
    fn view_transform_round_trips_with_rotation() {
        for angle in [0.0, 0.3, -1.2, std::f32::consts::FRAC_PI_2] {
            let v = ViewTransform { origin: Pos2::new(100.0, 50.0), scale: 2.0, angle };
            for p in [(0.0, 0.0), (10.0, 20.0), (-5.0, 33.3)] {
                let back = v.screen_to_doc(v.doc_to_screen(p));
                assert!((back.0 - p.0).abs() < 1e-3 && (back.1 - p.1).abs() < 1e-3, "angle {angle}: {p:?} → {back:?}");
            }
        }
    }

    /// À 90°, l'axe X du document part vers le bas de l'écran (sens horaire).
    #[test]
    fn view_transform_rotates_clockwise() {
        let v = ViewTransform { origin: Pos2::ZERO, scale: 1.0, angle: std::f32::consts::FRAC_PI_2 };
        let p = v.doc_to_screen((10.0, 0.0));
        assert!(p.x.abs() < 1e-4 && (p.y - 10.0).abs() < 1e-4, "{p:?}");
    }
}
