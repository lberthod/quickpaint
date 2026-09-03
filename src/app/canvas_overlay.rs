//! Rendu des overlays du canevas : grille, règles graduées, aperçu du
//! chemin de plume, cadre + poignées de sélection, recadrage, retouche par
//! rectangle, marquee/lasso, segment de mesure, anneau de curseur — extrait
//! de `app` en sous-module (sprint.md T3.8, suite de T3.1-T3.7) : fonctions
//! de peinture pures (`&self`, jamais de mutation), appelées en séquence par
//! `update()` après le rendu du document composite.

use super::{ruler_step, ActiveTool, Color32, PaintApp, Pos2, Rect, SelectMode, Vec2, ViewTransform};
use crate::tools::hit;

impl PaintApp {
    /// Cadre bleu autour de la sélection (outil flèche), suit le déplacement.
    /// Grille du document (roadmap #10), rognée au cadre.
    pub(super) fn paint_grid(&self, painter: &egui::Painter, view: &ViewTransform, doc_rect: Rect) {
        let g = self.grid_size;
        if g <= 0.0 {
            return;
        }
        let p = painter.with_clip_rect(doc_rect.intersect(self.last_canvas_rect));
        let stroke = egui::Stroke::new(1.0_f32, Color32::from_black_alpha(28));
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        let mut x = 0.0;
        while x <= w {
            let a = view.doc_to_screen((x, 0.0));
            let b = view.doc_to_screen((x, h));
            p.line_segment([a, b], stroke);
            x += g;
        }
        let mut y = 0.0;
        while y <= h {
            let a = view.doc_to_screen((0.0, y));
            let b = view.doc_to_screen((w, y));
            p.line_segment([a, b], stroke);
            y += g;
        }
    }

    /// Guides manuels (Sprint R, point 95) : lignes cyan sur toute la
    /// hauteur/largeur du document, plus l'aperçu du guide en cours de
    /// glissé (plus opaque). Peints sous les règles, au-dessus du contenu.
    pub(super) fn paint_manual_guides(&self, painter: &egui::Painter, view: &ViewTransform) {
        if self.doc.guides.is_empty() && self.guide_drag_preview().is_none() {
            return;
        }
        let p = painter.with_clip_rect(self.last_canvas_rect);
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        let color = Color32::from_rgba_unmultiplied(0, 170, 230, 170);
        let line = |vertical: bool, pos: f32, stroke: egui::Stroke| {
            let (a, b) = if vertical {
                (view.doc_to_screen((pos, 0.0)), view.doc_to_screen((pos, h)))
            } else {
                (view.doc_to_screen((0.0, pos)), view.doc_to_screen((w, pos)))
            };
            p.line_segment([a, b], stroke);
        };
        for g in &self.doc.guides {
            line(g.vertical, g.pos, egui::Stroke::new(1.0_f32, color));
        }
        if let Some((vertical, pos)) = self.guide_drag_preview() {
            line(vertical, pos, egui::Stroke::new(1.5_f32, Color32::from_rgb(0, 170, 230)));
        }
    }

    /// Règles graduées (haut + gauche) en coordonnées document. Le pas est
    /// choisi pour rester lisible quel que soit le zoom.
    pub(super) fn paint_rulers(&self, painter: &egui::Painter, view: &ViewTransform) {
        const TH: f32 = 18.0; // épaisseur de la règle (px écran)
        let cr = self.last_canvas_rect;
        // Thème (Sprint R, point 96) : les règles suivent le mode clair/sombre.
        let dark = painter.ctx().style().visuals.dark_mode;
        let bg = if dark { Color32::from_gray(38) } else { Color32::from_gray(244) };
        let line = Color32::from_gray(120);
        let text = if dark { Color32::from_gray(170) } else { Color32::from_gray(90) };
        let top = Rect::from_min_max(cr.min, egui::pos2(cr.max.x, cr.min.y + TH));
        let left = Rect::from_min_max(cr.min, egui::pos2(cr.min.x + TH, cr.max.y));
        painter.rect_filled(top, 0.0, bg);
        painter.rect_filled(left, 0.0, bg);

        let step = ruler_step(self.zoom); // pas en unités document
        let font = egui::FontId::proportional(9.0);
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);

        // Graduations horizontales (axe X) sur la règle du haut.
        let mut x = 0.0;
        while x <= w {
            let sx = view.doc_to_screen((x, 0.0)).x;
            if sx >= left.max.x && sx <= cr.max.x {
                painter.line_segment(
                    [egui::pos2(sx, cr.min.y + TH * 0.45), egui::pos2(sx, cr.min.y + TH)],
                    egui::Stroke::new(1.0_f32, line),
                );
                painter.text(
                    egui::pos2(sx + 2.0, cr.min.y + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", x as i32),
                    font.clone(),
                    text,
                );
            }
            x += step;
        }
        // Graduations verticales (axe Y) sur la règle de gauche.
        let mut y = 0.0;
        while y <= h {
            let sy = view.doc_to_screen((0.0, y)).y;
            if sy >= top.max.y && sy <= cr.max.y {
                painter.line_segment(
                    [egui::pos2(cr.min.x + TH * 0.45, sy), egui::pos2(cr.min.x + TH, sy)],
                    egui::Stroke::new(1.0_f32, line),
                );
                painter.text(
                    egui::pos2(cr.min.x + 1.0, sy + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", y as i32),
                    font.clone(),
                    text,
                );
            }
            y += step;
        }
        // Coin + liserés de séparation.
        painter.rect_filled(Rect::from_min_size(cr.min, Vec2::splat(TH)), 0.0, bg);
        painter.line_segment([egui::pos2(cr.min.x, cr.min.y + TH), egui::pos2(cr.max.x, cr.min.y + TH)], egui::Stroke::new(1.0_f32, line));
        painter.line_segment([egui::pos2(cr.min.x + TH, cr.min.y), egui::pos2(cr.min.x + TH, cr.max.y)], egui::Stroke::new(1.0_f32, line));
    }

    /// Aperçu du chemin de plume en cours : courbe + ancres + poignées.
    pub(super) fn paint_pen(&self, painter: &egui::Painter, view: &ViewTransform, response: &egui::Response) {
        if self.active_tool != ActiveTool::Pen || self.pen.is_empty() {
            return;
        }
        let blue = Color32::from_rgb(40, 110, 220);
        // Chemin échantillonné, avec un segment élastique vers le curseur.
        let mut anchors = self.pen.clone();
        if let Some(hp) = response.hover_pos() {
            anchors.push(crate::tools::pen::Anchor::corner(self.snap(view.screen_to_doc(hp))));
        }
        let pts = crate::tools::pen::sample(&anchors, false);
        let screen: Vec<egui::Pos2> = pts.iter().map(|p| view.doc_to_screen(*p)).collect();
        if screen.len() >= 2 {
            painter.add(egui::Shape::line(screen, egui::Stroke::new(1.5_f32, blue)));
        }
        // Ancres (carrés) + poignées (lignes + cercles).
        for a in &self.pen {
            let c = view.doc_to_screen(a.pos);
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(6.0)), 1.0, blue);
            for h in [a.h_in, a.h_out] {
                if h != a.pos {
                    let hp = view.doc_to_screen(h);
                    painter.line_segment([c, hp], egui::Stroke::new(1.0_f32, Color32::from_gray(150)));
                    painter.circle_filled(hp, 3.0, Color32::WHITE);
                    painter.circle_stroke(hp, 3.0, egui::Stroke::new(1.0_f32, blue));
                }
            }
        }
    }

    pub(super) fn paint_selection(&self, painter: &egui::Painter, view: &ViewTransform, moving: bool) {
        if self.active_tool != ActiveTool::Select
            || self.selection.is_empty()
            || self.crop_mode
            || self.retouch_mode.is_some()
            || self.show_perspective_panel
        {
            return;
        }
        let active = self.doc.active_layer;
        let sel = &self.selection;
        let layer = &self.doc.layers[active];
        let mut bounds = hit::bounds_of(layer.strokes.iter().filter(|s| sel.contains(&s.id)));
        // Inclut aussi les textes et images sélectionnés.
        let extra = layer
            .texts
            .iter()
            .filter(|t| sel.contains(&t.id))
            .map(|t| t.approx_bounds())
            .chain(layer.images.iter().filter(|im| sel.contains(&im.id)).map(|im| im.bounds()));
        for (lo, hi) in extra {
            bounds = Some(match bounds {
                Some((mn, mx)) => (
                    (mn.0.min(lo.0), mn.1.min(lo.1)),
                    (mx.0.max(hi.0), mx.1.max(hi.1)),
                ),
                None => (lo, hi),
            });
        }
        let Some((min, max)) = bounds else { return };
        let blue = Color32::from_rgb(40, 110, 220);

        // En cours de transformation : aperçu de la boîte transformée.
        if let Some(mut poly) = self.xform_preview(view) {
            poly.push(poly[0]);
            painter.add(egui::Shape::line(poly, egui::Stroke::new(1.5_f32, blue)));
            return;
        }

        let d = if moving { self.move_delta } else { (0.0, 0.0) };
        // Cadre = polygone des 4 coins projetés : suit la rotation de la vue
        // (Sprint T, point 93) là où un Rect écran resterait axis-aligned.
        let frame: Vec<Pos2> = [
            (min.0 + d.0, min.1 + d.1),
            (max.0 + d.0, min.1 + d.1),
            (max.0 + d.0, max.1 + d.1),
            (min.0 + d.0, max.1 + d.1),
        ]
        .into_iter()
        .map(|c| view.doc_to_screen(c))
        .collect();
        painter.add(egui::Shape::closed_line(frame, egui::Stroke::new(1.5_f32, blue)));

        // Poignées d'échelle (coins) + rotation (au-dessus) — toute sélection.
        if !moving {
            if let Some((corners, rot)) = self.transform_handles(view) {
                let top = view.doc_to_screen(((min.0 + max.0) * 0.5, min.1));
                painter.line_segment([top, rot], egui::Stroke::new(1.0_f32, blue));
                painter.circle_filled(rot, 5.0, Color32::WHITE);
                painter.circle_stroke(rot, 5.0, egui::Stroke::new(1.5_f32, blue));
                for c in corners {
                    let hr = Rect::from_center_size(c, Vec2::splat(9.0));
                    painter.rect_filled(hr, 1.0, Color32::WHITE);
                    painter.rect_stroke(hr, 1.0, egui::Stroke::new(1.5_f32, blue));
                }
            }
            // Poignées de cisaillement (Sprint M.2) : losanges, pour rester
            // visuellement distinctes des carrés (échelle) et du cercle
            // (rotation).
            if let Some((bottom_mid, right_mid)) = self.shear_handles(view) {
                for h in [bottom_mid, right_mid] {
                    let pts: Vec<Pos2> = [(0.0, -6.0), (6.0, 0.0), (0.0, 6.0), (-6.0, 0.0)]
                        .into_iter()
                        .map(|(dx, dy)| Pos2::new(h.x + dx, h.y + dy))
                        .collect();
                    painter.add(egui::Shape::convex_polygon(pts, Color32::WHITE, egui::Stroke::new(1.5_f32, blue)));
                }
            }
        }
    }

    /// Overlay du recadrage : rectangle orange (zone conservée).
    pub(super) fn paint_crop(&self, painter: &egui::Painter, view: &ViewTransform) {
        if !self.crop_mode {
            return;
        }
        // Ligne d'horizon en cours de tracé (previous_audit.md #88) :
        // cyan pour rester distincte de l'orange du rectangle de recadrage —
        // ce n'est pas la même chose qui est en train de se dessiner.
        if let Some((a, b)) = self.straighten_drag {
            let cyan = Color32::from_rgb(40, 210, 210);
            painter.line_segment([view.doc_to_screen(a), view.doc_to_screen(b)], egui::Stroke::new(2.0_f32, cyan));
            return;
        }
        let orange = Color32::from_rgb(255, 170, 0);
        if let Some((a, b)) = self.crop_rect {
            if self.crop_angle.abs() < 1e-4 {
                let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, orange));
                // Grille des tiers (previous_audit.md #90) : aide de
                // composition, aucun effet sur le recadrage réel — seulement
                // dans le cas non tourné, une grille sur un rectangle
                // pivoté (redressement en cours) ajouterait de la
                // complexité pour un cas d'usage marginal.
                let thin = egui::Stroke::new(1.0_f32, Color32::from_white_alpha(140));
                for i in 1..3 {
                    let fx = r.min.x + r.width() * (i as f32 / 3.0);
                    painter.line_segment([Pos2::new(fx, r.min.y), Pos2::new(fx, r.max.y)], thin);
                    let fy = r.min.y + r.height() * (i as f32 / 3.0);
                    painter.line_segment([Pos2::new(r.min.x, fy), Pos2::new(r.max.x, fy)], thin);
                }
            } else {
                // Redressement d'horizon (Sprint 2.3) : montre la région de
                // l'image source (encore affichée droite) qui finira droite
                // dans le résultat — c'est ce rectangle tourné qui est
                // échantillonné, cf. `straighten_and_crop`.
                let (cx0, cy0) = (a.0.min(b.0), a.1.min(b.1));
                let (cx1, cy1) = (a.0.max(b.0), a.1.max(b.1));
                let center = ((cx0 + cx1) * 0.5, (cy0 + cy1) * 0.5);
                let (cos_a, sin_a) = (self.crop_angle.cos(), self.crop_angle.sin());
                let rotate = |p: (f32, f32)| {
                    let (rx, ry) = (p.0 - center.0, p.1 - center.1);
                    view.doc_to_screen((center.0 + rx * cos_a - ry * sin_a, center.1 + rx * sin_a + ry * cos_a))
                };
                let corners = [rotate((cx0, cy0)), rotate((cx1, cy0)), rotate((cx1, cy1)), rotate((cx0, cy1))];
                painter.add(egui::Shape::closed_line(corners.to_vec(), egui::Stroke::new(1.5_f32, orange)));
            }
        } else if let Some((_, corners)) = self.selected_image_corners() {
            // Avant le glissé : souligne l'image à recadrer.
            let r = Rect::from_two_pos(view.doc_to_screen(corners[0]), view.doc_to_screen(corners[2]));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, orange));
        }
    }

    /// Overlay des modes de retouche par rectangle (Sprint 4.3/4.4) — une
    /// couleur par type, pour ne pas prêter à confusion avec le recadrage
    /// (orange, garde le contenu) sur ce qui va se passer.
    pub(super) fn paint_retouch(&self, painter: &egui::Painter, view: &ViewTransform) {
        use crate::tools::RetouchKind;
        let Some(kind) = self.retouch_mode else { return };
        let color = match kind {
            RetouchKind::Remove => Color32::from_rgb(230, 60, 60),
            RetouchKind::RedEye => Color32::from_rgb(230, 60, 230),
            RetouchKind::SkinSmooth => Color32::from_rgb(60, 200, 200),
        };
        if let Some((a, b)) = self.retouch_rect {
            let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, color));
        } else if let Some((_, corners)) = self.selected_image_corners() {
            let r = Rect::from_two_pos(view.doc_to_screen(corners[0]), view.doc_to_screen(corners[2]));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, color));
        }
    }

    /// Quadrilatère + 4 poignées de coin du panneau perspective
    /// (previous_audit.md #87) : aperçu en direct du résultat avant
    /// application, orange pour rester visuellement distinct du cadre de
    /// sélection bleu habituel (panneau modal, pas une sélection normale).
    pub(super) fn paint_perspective_handles(&self, painter: &egui::Painter, view: &ViewTransform) {
        let Some(handles) = self.perspective_handles(view) else { return };
        let orange = Color32::from_rgb(230, 140, 40);
        painter.add(egui::Shape::closed_line(handles.to_vec(), egui::Stroke::new(1.5_f32, orange)));
        for h in handles {
            painter.circle_filled(h, 5.0, Color32::WHITE);
            painter.circle_stroke(h, 5.0, egui::Stroke::new(1.5_f32, orange));
        }
    }

    /// Overlay de la sélection par région : rectangle (marquee) ou tracé lasso,
    /// en bleu translucide tant que le geste est en cours.
    pub(super) fn paint_marquee(&self, painter: &egui::Painter, view: &ViewTransform) {
        let blue = Color32::from_rgb(40, 110, 240);
        let fill = Color32::from_rgba_unmultiplied(40, 110, 240, 28);
        if let Some((a, b)) = self.marquee {
            // Géométrie construite en coordonnées document puis projetée
            // point à point : suit la rotation de la vue (Sprint T, point 93).
            if self.select_mode == SelectMode::Ellipse {
                let (cx, cy) = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
                let (rx, ry) = (((b.0 - a.0) * 0.5).abs(), ((b.1 - a.1) * 0.5).abs());
                let pts: Vec<Pos2> = (0..48)
                    .map(|i| {
                        let t = i as f32 / 48.0 * std::f32::consts::TAU;
                        view.doc_to_screen((cx + rx * t.cos(), cy + ry * t.sin()))
                    })
                    .collect();
                painter.add(egui::Shape::Path(egui::epaint::PathShape::convex_polygon(
                    pts,
                    fill,
                    egui::Stroke::new(1.0_f32, blue),
                )));
            } else {
                let corners: Vec<Pos2> = [(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)]
                    .into_iter()
                    .map(|c| view.doc_to_screen(c))
                    .collect();
                painter.add(egui::Shape::Path(egui::epaint::PathShape::convex_polygon(
                    corners,
                    fill,
                    egui::Stroke::new(1.0_f32, blue),
                )));
            }
        } else if !self.lasso.is_empty() {
            let mut pts: Vec<Pos2> = self.lasso.iter().map(|&d| view.doc_to_screen(d)).collect();
            // Lasso polygonal (point 52) : segment élastique du dernier sommet
            // posé vers le curseur, comme l'aperçu de la plume (`paint_pen`).
            if self.select_mode == SelectMode::PolyLasso {
                if let Some(hp) = painter.ctx().pointer_hover_pos() {
                    pts.push(hp);
                }
                for &p in &pts {
                    painter.rect_filled(Rect::from_center_size(p, Vec2::splat(5.0)), 1.0, blue);
                }
            }
            if pts.len() >= 2 {
                painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, blue)));
                // Trait de fermeture (du dernier point au premier).
                painter.line_segment([pts[pts.len() - 1], pts[0]], egui::Stroke::new(1.0_f32, fill));
            }
        }
    }

    /// Segment de mesure (Sprint 11, outil Règle) : ligne + étiquette
    /// distance/angle, jamais écrit dans le document — cf. `handle_measure`.
    pub(super) fn paint_measure(&self, painter: &egui::Painter, view: &ViewTransform) {
        let Some((a, b)) = self.measure else { return };
        let (sa, sb) = (view.doc_to_screen(a), view.doc_to_screen(b));
        let col = Color32::from_rgb(40, 200, 160);
        painter.line_segment([sa, sb], egui::Stroke::new(1.5_f32, col));
        painter.circle_filled(sa, 3.0, col);
        painter.circle_filled(sb, 3.0, col);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx).to_degrees();
        let mid = sa + (sb - sa) * 0.5;
        painter.text(
            mid + Vec2::new(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.0} px · {:.1}°", dist, angle),
            egui::FontId::proportional(13.0),
            col,
        );
    }

    /// Anneau de prévisualisation de la taille de l'outil sous le curseur
    /// (repère ergonomique « vrai Paint »). Bichromie pour rester visible sur
    /// tout fond.
    pub(super) fn paint_cursor(&self, painter: &egui::Painter, response: &egui::Response) {
        let Some(p) = response.hover_pos() else { return };
        let radius = match self.active_tool {
            ActiveTool::Eraser | ActiveTool::PixelEraser => self.eraser.width * 0.5 * self.zoom,
            ActiveTool::Brush
            | ActiveTool::Pencil
            | ActiveTool::PixelBrush
            | ActiveTool::Airbrush
            | ActiveTool::Line
            | ActiveTool::Rectangle
            | ActiveTool::Ellipse
            | ActiveTool::Dodge
            | ActiveTool::Burn
            | ActiveTool::Saturate
            | ActiveTool::Desaturate
            | ActiveTool::Blur
            | ActiveTool::Sharpen
            | ActiveTool::Smudge => self.brush.width * 0.5 * self.zoom,
            _ => return,
        };
        if radius < 1.0 {
            return;
        }
        painter.circle_stroke(p, radius, egui::Stroke::new(2.0_f32, Color32::from_white_alpha(200)));
        painter.circle_stroke(p, radius, egui::Stroke::new(1.0_f32, Color32::from_black_alpha(160)));
    }
}
