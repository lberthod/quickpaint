//! Transformation interactive de la sélection (échelle / rotation par les
//! poignées de la boîte englobante), extraite de `app` en sous-module
//! (journal git (ex-SPRINTS.md) 13.8, suite du découpage commencé en previous_audit.md (ex-ANALYSE) §12.5) : un
//! sous-système autonome (poignées, glissé, aperçu, undo) qui ne partage que
//! `Document`/`Command::Scale`/`Command::Rotate` avec le reste de l'application.
//!
//! Les méthodes sont `pub(super)` : appelées depuis `app` (le module parent),
//! elles restent invisibles hors de l'arbre `app::*`, exactement comme avant
//! l'extraction (pas un élargissement de l'API).

use super::PaintApp;
use crate::history::Command;
use crate::render::canvas::ViewTransform;
use egui::Pos2;

/// Nature de la transformation interactive en cours.
enum XformKind {
    Scale { anchor: (f32, f32) },              // coin opposé fixe
    Rotate { center: (f32, f32), start: f32 }, // pivot + angle initial du pointeur
    /// Cisaillement (Sprint M.2), autour du centre de la boîte de sélection :
    /// `horizontal` (poignée du bord bas, glissé en x) ou vertical (poignée
    /// du bord droit, glissé en y). `start` = position pointeur (doc) au
    /// début du geste, pour mesurer le glissé accumulé.
    Shear { pivot: (f32, f32), horizontal: bool, start: (f32, f32) },
}

/// Transformation en cours sur la sélection active.
pub(super) struct TransformDrag {
    kind: XformKind,
    bbox: ((f32, f32), (f32, f32)), // boîte de sélection au départ (doc)
    sx: f32,
    sy: f32,
    angle: f32,
}

impl PaintApp {
    /// 4 coins + poignée de rotation de la boîte de sélection (écran).
    pub(super) fn transform_handles(&self, view: &ViewTransform) -> Option<([Pos2; 4], Pos2)> {
        let (mn, mx) = self.selection_bounds()?;
        let corners = [
            view.doc_to_screen((mn.0, mn.1)),
            view.doc_to_screen((mx.0, mn.1)),
            view.doc_to_screen((mx.0, mx.1)),
            view.doc_to_screen((mn.0, mx.1)),
        ];
        let top = view.doc_to_screen(((mn.0 + mx.0) * 0.5, mn.1));
        let rot = Pos2::new(top.x, top.y - 22.0);
        Some((corners, rot))
    }

    /// Poignées de cisaillement (Sprint M.2) : milieu du bord bas
    /// (cisaillement horizontal) et milieu du bord droit (cisaillement
    /// vertical), distinctes des coins (échelle) et de la poignée au-dessus
    /// (rotation).
    pub(super) fn shear_handles(&self, view: &ViewTransform) -> Option<(Pos2, Pos2)> {
        let (mn, mx) = self.selection_bounds()?;
        let bottom_mid = view.doc_to_screen(((mn.0 + mx.0) * 0.5, mx.1));
        let right_mid = view.doc_to_screen((mx.0, (mn.1 + mx.1) * 0.5));
        Some((Pos2::new(bottom_mid.x, bottom_mid.y + 18.0), Pos2::new(right_mid.x + 18.0, right_mid.y)))
    }

    /// Démarre une transformation si le clic tombe sur une poignée.
    pub(super) fn start_transform_if_handle(&mut self, p: Pos2, view: &ViewTransform) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let Some((corners, rot_handle)) = self.transform_handles(view) else { return false };
        let Some((mn, mx)) = self.selection_bounds() else { return false };
        let center = ((mn.0 + mx.0) * 0.5, (mn.1 + mx.1) * 0.5);
        // Rotation en priorité.
        if (rot_handle - p).length() <= 10.0 {
            let pc = view.doc_to_screen(center);
            let start = (p.y - pc.y).atan2(p.x - pc.x);
            self.xform = Some(TransformDrag {
                kind: XformKind::Rotate { center, start },
                bbox: (mn, mx),
                sx: 1.0,
                sy: 1.0,
                angle: 0.0,
            });
            return true;
        }
        // Cisaillement (bords bas/droit), avant les coins d'échelle.
        if let Some((bottom_mid, right_mid)) = self.shear_handles(view) {
            if (bottom_mid - p).length() <= 10.0 {
                self.xform = Some(TransformDrag {
                    kind: XformKind::Shear { pivot: center, horizontal: true, start: view.screen_to_doc(p) },
                    bbox: (mn, mx),
                    sx: 0.0,
                    sy: 0.0,
                    angle: 0.0,
                });
                return true;
            }
            if (right_mid - p).length() <= 10.0 {
                self.xform = Some(TransformDrag {
                    kind: XformKind::Shear { pivot: center, horizontal: false, start: view.screen_to_doc(p) },
                    bbox: (mn, mx),
                    sx: 0.0,
                    sy: 0.0,
                    angle: 0.0,
                });
                return true;
            }
        }
        // Coins → échelle.
        let doc_corners = [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)];
        for (ci, cs) in corners.iter().enumerate() {
            if (*cs - p).length() <= 10.0 {
                let anchor = doc_corners[(ci + 2) % 4];
                self.xform = Some(TransformDrag {
                    kind: XformKind::Scale { anchor },
                    bbox: (mn, mx),
                    sx: 1.0,
                    sy: 1.0,
                    angle: 0.0,
                });
                return true;
            }
        }
        false
    }

    /// Met à jour les paramètres de la transformation pendant le glissé.
    pub(super) fn update_transform(&mut self, p: Pos2, view: &ViewTransform, uniform: bool) {
        let Some(x) = &mut self.xform else { return };
        match x.kind {
            XformKind::Scale { anchor } => {
                let d = view.screen_to_doc(p);
                let (w, h) = (x.bbox.1 .0 - anchor.0, x.bbox.1 .1 - anchor.1);
                // Largeur/hauteur signées entre l'ancre et le coin tiré.
                let denom_x = if w.abs() < 1e-3 { x.bbox.0 .0 - anchor.0 } else { w };
                let denom_y = if h.abs() < 1e-3 { x.bbox.0 .1 - anchor.1 } else { h };
                let mut sx = if denom_x.abs() > 1e-3 { (d.0 - anchor.0) / denom_x } else { 1.0 };
                let mut sy = if denom_y.abs() > 1e-3 { (d.1 - anchor.1) / denom_y } else { 1.0 };
                if uniform {
                    let s = sx.abs().max(sy.abs());
                    sx = s * sx.signum();
                    sy = s * sy.signum();
                }
                x.sx = sx.clamp(-20.0, 20.0);
                x.sy = sy.clamp(-20.0, 20.0);
            }
            XformKind::Rotate { center, start } => {
                let pc = view.doc_to_screen(center);
                let a = (p.y - pc.y).atan2(p.x - pc.x);
                let mut da = a - start;
                if uniform {
                    // Maj : par pas de 15°.
                    let step = std::f32::consts::FRAC_PI_8 * 0.5;
                    da = (da / step).round() * step;
                }
                x.angle = da;
            }
            XformKind::Shear { horizontal, start, .. } => {
                let d = view.screen_to_doc(p);
                if horizontal {
                    let half_h = ((x.bbox.1 .1 - x.bbox.0 .1) * 0.5).max(1e-3);
                    x.sx = ((d.0 - start.0) / half_h).clamp(-3.0, 3.0);
                } else {
                    let half_w = ((x.bbox.1 .0 - x.bbox.0 .0) * 0.5).max(1e-3);
                    x.sy = ((d.1 - start.1) / half_w).clamp(-3.0, 3.0);
                }
            }
        }
    }

    /// Valide la transformation en cours (annulable).
    pub(super) fn commit_transform(&mut self) {
        let Some(x) = self.xform.take() else { return };
        let (strokes, texts, images) = self.selection_ids();
        let layer = self.doc.active_id();
        match x.kind {
            XformKind::Scale { anchor } => {
                if (x.sx - 1.0).abs() < 1e-3 && (x.sy - 1.0).abs() < 1e-3 {
                    return;
                }
                if x.sx.abs() < 1e-2 || x.sy.abs() < 1e-2 {
                    return;
                }
                self.history.push(
                    &mut self.doc,
                    Command::Scale { layer, strokes: strokes.clone(), texts, images, pivot: anchor, sx: x.sx, sy: x.sy },
                );
            }
            XformKind::Rotate { center, .. } => {
                if x.angle.abs() < 1e-3 {
                    return;
                }
                self.history.push(
                    &mut self.doc,
                    Command::Rotate { layer, strokes: strokes.clone(), texts, images, pivot: center, angle: x.angle },
                );
            }
            XformKind::Shear { pivot, .. } => {
                if x.sx.abs() < 1e-3 && x.sy.abs() < 1e-3 {
                    return;
                }
                self.history.push(
                    &mut self.doc,
                    Command::Shear { layer, strokes: strokes.clone(), texts, images, pivot, shx: x.sx, shy: x.sy },
                );
            }
        }
        self.cache.invalidate(strokes.iter());
    }

    /// Polygone (coordonnées écran, boucle non refermée) de l'aperçu de la
    /// transformation en cours, ou `None` si aucune transformation n'est
    /// active. L'appelant referme la boucle et choisit le style de trait.
    pub(super) fn xform_preview(&self, view: &ViewTransform) -> Option<Vec<Pos2>> {
        let x = self.xform.as_ref()?;
        let (mn, mx) = x.bbox;
        let corners0 = [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)];
        Some(
            corners0
                .iter()
                .map(|c| {
                    let t = match x.kind {
                        XformKind::Scale { anchor } => (
                            anchor.0 + (c.0 - anchor.0) * x.sx,
                            anchor.1 + (c.1 - anchor.1) * x.sy,
                        ),
                        XformKind::Rotate { center, .. } => {
                            let (co, si) = (x.angle.cos(), x.angle.sin());
                            let (dx, dy) = (c.0 - center.0, c.1 - center.1);
                            (center.0 + dx * co - dy * si, center.1 + dx * si + dy * co)
                        }
                        XformKind::Shear { pivot, .. } => {
                            let (dx, dy) = (c.0 - pivot.0, c.1 - pivot.1);
                            (pivot.0 + dx + x.sx * dy, pivot.1 + dy + x.sy * dx)
                        }
                    };
                    view.doc_to_screen(t)
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une échelle ou rotation quasi nulle (bruit d'un clic sans glissé réel)
    /// ne doit pas pousser de commande d'undo — régression : un simple clic
    /// sur une poignée sans déplacement ne doit rien changer à l'historique.
    #[test]
    fn commit_transform_is_a_noop_below_threshold() {
        let mut app = PaintApp::default();
        let id = app.next_id;
        app.next_id += 1;
        let mut s = crate::model::Stroke::new([255, 0, 0, 255], 2.0, crate::model::Tool::Brush);
        s.id = id;
        s.points = vec![
            crate::model::StrokePoint { pos: (0.0, 0.0), width: 2.0 },
            crate::model::StrokePoint { pos: (10.0, 10.0), width: 2.0 },
        ];
        app.doc.layers[0].strokes.push(s);
        app.selection.insert(id);

        app.xform = Some(TransformDrag {
            kind: XformKind::Scale { anchor: (0.0, 0.0) },
            bbox: ((0.0, 0.0), (10.0, 10.0)),
            sx: 1.0 + 1e-6,
            sy: 1.0,
            angle: 0.0,
        });
        let before = app.history.can_undo();
        app.commit_transform();
        assert_eq!(app.history.can_undo(), before, "pas de commande pour un scale ~identité");
    }
}
