//! Édition de nœuds Bézier après coup (roadmap P2 #12, F2) : double-clic sur
//! un trait de plume déjà posé pour rouvrir ses ancres/poignées, les glisser,
//! puis refermer — avec undo/redo dédié. Extrait de `app` en sous-module
//! (ANALYSE.md §12.5) : un sous-système autonome (état + geste + rendu) qui
//! ne partage que `Document`/`Stroke` avec le reste de l'application.
//!
//! Les méthodes sont `pub(super)` : appelées depuis `app` (le module parent),
//! elles restent invisibles hors de l'arbre `app::*`.

use super::PaintApp;
use crate::history::Command;
use crate::render::canvas::ViewTransform;
use egui::{Color32, Rect, Vec2};

/// Nœud ciblé par un glissé pendant l'édition de plume après coup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PenNodeTarget {
    Anchor(usize),
    HandleIn(usize),
    HandleOut(usize),
}

impl PaintApp {
    /// Double-clic sur un trait de plume : rouvre ses ancres/poignées.
    /// `false` si le trait ciblé n'a pas d'ancres (ex. formes, pinceau libre).
    pub(super) fn try_start_pen_edit(&mut self, id: u64) -> bool {
        let l = &self.doc.layers[self.doc.active_layer];
        let Some(s) = l.strokes.iter().find(|s| s.id == id) else { return false };
        let Some(path) = &s.anchors else { return false };
        self.editing_pen = Some((id, path.clone()));
        self.selection.clear();
        self.selection.insert(id);
        self.status = Some(crate::i18n::t(
            "Édition du chemin : glisse une ancre/poignée ; Échap ou double-clic ailleurs pour terminer.",
            "Editing path: drag an anchor/handle; Esc or double-click elsewhere to finish.",
        ).into());
        true
    }

    pub(super) fn hit_test_pen_node(&self, d: (f32, f32)) -> Option<PenNodeTarget> {
        let (_, path) = self.editing_pen.as_ref()?;
        let thresh = 8.0 / self.zoom.max(0.01);
        let mut best: Option<(f32, PenNodeTarget)> = None;
        let consider = |pos: (f32, f32), target: PenNodeTarget, best: &mut Option<(f32, PenNodeTarget)>| {
            let dd = ((pos.0 - d.0).powi(2) + (pos.1 - d.1).powi(2)).sqrt();
            if dd <= thresh && best.map(|(bd, _)| dd < bd).unwrap_or(true) {
                *best = Some((dd, target));
            }
        };
        for (i, a) in path.anchors.iter().enumerate() {
            consider(a.pos, PenNodeTarget::Anchor(i), &mut best);
            if a.h_in != a.pos {
                consider(a.h_in, PenNodeTarget::HandleIn(i), &mut best);
            }
            if a.h_out != a.pos {
                consider(a.h_out, PenNodeTarget::HandleOut(i), &mut best);
            }
        }
        best.map(|(_, t)| t)
    }

    /// Déplace le nœud ciblé dans la copie de travail, puis ré-échantillonne
    /// immédiatement le trait pour un aperçu live (même geste que la peinture
    /// raster : mutation directe + `history.touch()`, undo à la fin du geste).
    pub(super) fn apply_pen_drag(&mut self, target: PenNodeTarget, d: (f32, f32)) {
        let Some((id, path)) = &mut self.editing_pen else { return };
        match target {
            PenNodeTarget::Anchor(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    let delta = (d.0 - a.pos.0, d.1 - a.pos.1);
                    a.pos = d;
                    a.h_in = (a.h_in.0 + delta.0, a.h_in.1 + delta.1);
                    a.h_out = (a.h_out.0 + delta.0, a.h_out.1 + delta.1);
                }
            }
            PenNodeTarget::HandleIn(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    a.h_in = d;
                }
            }
            PenNodeTarget::HandleOut(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    a.h_out = d;
                }
            }
        }
        let id = *id;
        let pts = path.sample();
        let active = self.doc.active_layer;
        if let Some(s) = self.doc.layers[active].strokes.iter_mut().find(|s| s.id == id) {
            let w = s.base_width;
            s.points = pts.into_iter().map(|pos| crate::model::StrokePoint { pos, width: w }).collect();
        }
        self.cache.invalidate(std::iter::once(&id));
        self.history.touch();
    }

    /// Fin du glissé d'un nœud : pousse une commande d'undo (avant/après).
    pub(super) fn commit_pen_edit(&mut self, id: u64) {
        let Some(before_path) = self.pen_edit_before.take() else { return };
        let Some((_, after_path)) = &self.editing_pen else { return };
        let after_path = after_path.clone();
        let active = self.doc.active_layer;
        let layer = self.doc.active_id();
        let Some(s) = self.doc.layers[active].strokes.iter().find(|s| s.id == id) else { return };
        let width = s.base_width;
        let after_points = s.points.clone();
        let before_points: Vec<_> = before_path
            .sample()
            .into_iter()
            .map(|pos| crate::model::StrokePoint { pos, width })
            .collect();
        if before_points.len() == after_points.len()
            && before_points.iter().zip(&after_points).all(|(a, b)| a.pos == b.pos)
        {
            return; // pas de déplacement net (clic sans glissé réel)
        }
        self.history.push(
            &mut self.doc,
            Command::EditPenPath { layer, id, before_path, before_points, after_path, after_points },
        );
    }

    /// Gère le glissé des ancres/poignées tant qu'un chemin est en édition ;
    /// appelé en priorité par l'outil Sélection.
    pub(super) fn handle_pen_node_edit(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let Some((id, _)) = self.editing_pen else { return };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.editing_pen = None;
            self.pen_drag = None;
            self.status = Some(crate::i18n::t("Édition du chemin terminée.", "Path editing finished.").into());
            return;
        }
        if response.drag_started() {
            // `interact_pointer_pos()` peut déjà refléter une position avancée
            // dans le glissé (plusieurs évènements souris fusionnés avant la
            // première frame où `drag_started()` devient vrai) : on teste donc
            // le nœud visé à partir du point de pression réel, pas de la
            // position courante du pointeur.
            let press = ctx.input(|i| i.pointer.press_origin());
            if let Some(p) = press {
                let d = view.screen_to_doc(p);
                self.pen_drag = self.hit_test_pen_node(d);
                if self.pen_drag.is_some() {
                    self.pen_edit_before = self.editing_pen.as_ref().map(|(_, p)| p.clone());
                }
            }
        }
        if response.dragged() {
            if let (Some(target), Some(p)) = (self.pen_drag, response.interact_pointer_pos()) {
                let d = self.snap(view.screen_to_doc(p));
                self.apply_pen_drag(target, d);
            }
        }
        if response.drag_stopped() && self.pen_drag.take().is_some() {
            self.commit_pen_edit(id);
        }
        // Double-clic hors de tout nœud : referme l'édition.
        if response.double_clicked() && self.pen_drag.is_none() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if self.hit_test_pen_node(d).is_none() {
                    self.editing_pen = None;
                    self.status = Some(crate::i18n::t("Édition du chemin terminée.", "Path editing finished.").into());
                }
            }
        }
    }

    /// Édition de nœuds après coup : ancres/poignées d'un trait de plume déjà
    /// posé, rouvertes par double-clic — même style visuel que le tracé en
    /// cours (`paint_pen`), en orange pour distinguer « en édition » de
    /// « en train de tracer ».
    pub(super) fn paint_pen_edit(&self, painter: &egui::Painter, view: &ViewTransform) {
        let Some((_, path)) = &self.editing_pen else { return };
        let orange = Color32::from_rgb(230, 140, 20);
        let pts = path.sample();
        let screen: Vec<egui::Pos2> = pts.iter().map(|p| view.doc_to_screen(*p)).collect();
        if screen.len() >= 2 {
            painter.add(egui::Shape::line(screen, egui::Stroke::new(1.5_f32, orange)));
        }
        for a in &path.anchors {
            let c = view.doc_to_screen(a.pos);
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(7.0)), 1.0, orange);
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(7.0)), 1.0, egui::Stroke::new(1.0_f32, Color32::WHITE));
            for h in [a.h_in, a.h_out] {
                if h != a.pos {
                    let hp = view.doc_to_screen(h);
                    painter.line_segment([c, hp], egui::Stroke::new(1.0_f32, Color32::from_gray(150)));
                    painter.circle_filled(hp, 3.5, Color32::WHITE);
                    painter.circle_stroke(hp, 3.5, egui::Stroke::new(1.0_f32, orange));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::pen::{Anchor, PenPath};

    fn anchor(pos: (f32, f32)) -> Anchor {
        Anchor { pos, h_in: pos, h_out: pos }
    }

    /// L'ancre la plus proche du point testé doit être choisie, pas
    /// n'importe laquelle sous le seuil (régression : un glissé rapide sur
    /// une petite poignée pouvait en manquer une autre plus loin mais encore
    /// dans le seuil).
    #[test]
    fn hit_test_pen_node_picks_the_closest_anchor() {
        let path = PenPath { anchors: vec![anchor((0.0, 0.0)), anchor((100.0, 0.0))], closed: false };
        let app = PaintApp { editing_pen: Some((1, path)), zoom: 1.0, ..Default::default() };
        // Plus proche de l'ancre à (100, 0) que de celle à (0, 0), mais les
        // deux sont sous le seuil de 8px (converti en unités document par
        // `hit_test_pen_node`, ici zoom = 1 donc seuil = 8).
        let target = app.hit_test_pen_node((97.0, 0.0));
        assert_eq!(target, Some(PenNodeTarget::Anchor(1)));
    }

    #[test]
    fn hit_test_pen_node_none_outside_threshold() {
        let path = PenPath { anchors: vec![anchor((0.0, 0.0))], closed: false };
        let app = PaintApp { editing_pen: Some((1, path)), zoom: 1.0, ..Default::default() };
        assert_eq!(app.hit_test_pen_node((50.0, 50.0)), None);
    }
}
