//! Dispatch des évènements du canevas (pan/zoom, verrouillage de calque,
//! puis un `match` par outil actif) vers les gestes déjà implémentés dans
//! les sous-modules dédiés (`selection`, `transform`, `pen_edit`,
//! `raster_paint`, …) — extrait de `app` en sous-module (sprint.md T3.10,
//! suite de T3.1-T3.9) : un seul point d'entrée pour l'interaction souris/
//! trackpad sur le canevas, pas de nouvelle logique.

use super::{
    eyedropper, layer_lock_blocks_tool, t, ActiveTool, PaintApp, RasterOp, SelectMode, SelectionCombine, ViewTransform,
};
use egui::Vec2;

impl PaintApp {
    /// Gère le geste sur le canvas selon l'outil actif.
    pub(super) fn handle_canvas(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let origin_base = response.rect.min;

        // Pan / zoom à la molette ou au pincé (uniquement au-dessus du canvas).
        if response.hovered() {
            let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if scroll != Vec2::ZERO {
                self.pan += scroll;
            }
            if zoom_delta != 1.0 {
                if let Some(ptr) = response.hover_pos() {
                    self.zoom_about(ptr, origin_base, zoom_delta);
                }
            }
        }

        // Espace maintenu = déplacement temporaire (réflexe « main » à la
        // Photoshop), quel que soit l'outil.
        let space_pan = ctx.input(|i| i.key_down(egui::Key::Space)) && !ctx.wants_keyboard_input();
        if space_pan {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if response.dragged() {
                self.pan += response.drag_delta();
            }
            return;
        }

        // Guides manuels (Sprint R, point 95) : tirer depuis une règle crée
        // un guide ; l'outil Sélection peut saisir un guide existant. Le
        // geste, quand il est actif, a priorité sur l'outil courant.
        // (Règles et guides masqués/inactifs quand la vue est tournée —
        // Sprint T, point 93 : leurs graduations sont axis-aligned.)
        if self.show_rulers && self.view_angle == 0.0 && self.handle_guide_gesture(response, view) {
            return;
        }

        // Verrouillage de calque (audit_sprint_xx.md B.1) : bloque tout geste
        // qui modifierait le contenu du calque actif — un seul point de
        // contrôle avant le dispatch par outil, plutôt qu'une vérification
        // dispersée dans chaque branche. Pan/Pipette/Règle restent autorisés
        // (jamais destructifs, cf. `layer_lock_blocks_tool`).
        if self.doc.layers[self.doc.active_layer].locked
            && layer_lock_blocks_tool(self.active_tool)
            && (response.clicked() || response.dragged() || response.drag_started())
        {
            self.info(t("Calque verrouillé : déverrouille-le pour le modifier.", "Layer locked: unlock it to edit."));
            return;
        }

        match self.active_tool {
            ActiveTool::Select => {
                let shift = ctx.input(|i| i.modifiers.shift);
                let alt = ctx.input(|i| i.modifiers.alt);
                let combine = SelectionCombine::from_modifiers(shift, alt);
                // Édition de nœuds (roadmap P2 #12) : prioritaire tant qu'un
                // chemin de plume est rouvert.
                if self.editing_pen.is_some() {
                    self.handle_pen_node_edit(ctx, response, view);
                    return;
                }
                if response.double_clicked() {
                    // Lasso polygonal (point 52) : le double-clic referme le
                    // polygone en cours, prioritaire sur l'édition de plume.
                    if self.select_mode == SelectMode::PolyLasso && !self.lasso.is_empty() {
                        let poly = std::mem::take(&mut self.lasso);
                        if poly.len() >= 3 {
                            self.select_in_lasso(&poly, combine);
                        }
                        return;
                    }
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some(id) = self.topmost_at(d) {
                            if self.try_start_pen_edit(id) {
                                return;
                            }
                        }
                    }
                }
                // Mode recadrage : le glissé définit la zone à conserver.
                if self.crop_mode {
                    // Tracé de la ligne d'horizon (audit_100_features.md #88),
                    // prioritaire sur le glissé normal du rectangle tant que
                    // le geste est armé — un seul glissé, se désactive de
                    // lui-même dans `commit_straighten_line`.
                    if self.straighten_line_mode {
                        if response.drag_started() {
                            if let Some(p) = response.interact_pointer_pos() {
                                self.update_straighten_line(view.screen_to_doc(p));
                            }
                        }
                        if response.dragged() {
                            if let Some(p) = response.interact_pointer_pos() {
                                self.update_straighten_line(view.screen_to_doc(p));
                            }
                        }
                        if response.drag_stopped() {
                            self.commit_straighten_line();
                        }
                        return;
                    }
                    if response.drag_started() {
                        if let Some(p) = response.interact_pointer_pos() {
                            let d = view.screen_to_doc(p);
                            self.crop_rect = Some((d, d));
                        }
                    }
                    if response.dragged() {
                        if let (Some((s, _)), Some(p)) =
                            (self.crop_rect, response.interact_pointer_pos())
                        {
                            let e = self.constrain_crop(s, view.screen_to_doc(p));
                            self.crop_rect = Some((s, e));
                        }
                    }
                    if response.drag_stopped() {
                        self.apply_crop();
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.crop_mode = false;
                        self.crop_rect = None;
                    }
                    return;
                }
                // Modes de retouche par rectangle (Sprint 4.3/4.4) : même
                // geste que le recadrage, sans contrainte de ratio.
                if self.retouch_mode.is_some() {
                    if response.drag_started() {
                        if let Some(p) = response.interact_pointer_pos() {
                            let d = view.screen_to_doc(p);
                            self.retouch_rect = Some((d, d));
                        }
                    }
                    if response.dragged() {
                        if let (Some((s, _)), Some(p)) =
                            (self.retouch_rect, response.interact_pointer_pos())
                        {
                            self.retouch_rect = Some((s, view.screen_to_doc(p)));
                        }
                    }
                    if response.drag_stopped() {
                        self.apply_retouch();
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.retouch_mode = None;
                        self.retouch_rect = None;
                    }
                    return;
                }
                // Panneau perspective ouvert (audit_100_features.md #87) :
                // glisser directement les 4 coins plutôt que des sliders X/Y,
                // même schéma modal que crop_mode/retouch_mode ci-dessus —
                // tant que le panneau est ouvert, le canevas ne sert qu'à
                // manipuler ses poignées.
                if self.show_perspective_panel {
                    if response.drag_started() {
                        if let Some(p) = response.interact_pointer_pos() {
                            self.start_perspective_drag_if_handle(p, view);
                        }
                    }
                    if response.dragged() {
                        if let Some(p) = response.interact_pointer_pos() {
                            self.update_perspective_drag(p, view);
                        }
                    }
                    if response.drag_stopped() {
                        self.perspective_drag = None;
                    }
                    return;
                }
                // Échap annule une sélection par région en cours.
                if ctx.input(|i| i.key_pressed(egui::Key::Escape))
                    && (self.marquee.is_some() || !self.lasso.is_empty())
                {
                    self.marquee = None;
                    self.lasso.clear();
                }
                if response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        // Poignée d'échelle / rotation en priorité.
                        if self.start_transform_if_handle(p, view) {
                            return;
                        }
                        let d = view.screen_to_doc(p);
                        match self.topmost_at(d) {
                            // Sur un élément déjà sélectionné → on garde la sélection.
                            Some(id) if self.selection.contains(&id) => {
                                self.move_origin = Some(d);
                                self.move_delta = (0.0, 0.0);
                            }
                            Some(id) => {
                                if !shift {
                                    self.selection.clear();
                                }
                                self.selection.insert(id);
                                self.move_origin = Some(d);
                                self.move_delta = (0.0, 0.0);
                            }
                            // Glissé sur le vide → sélection par région (marquee/lasso).
                            None => match self.select_mode {
                                SelectMode::Lasso => self.lasso = vec![d],
                                // Lasso polygonal : les sommets se posent au
                                // clic, un glissé sur le vide ne fait rien.
                                SelectMode::PolyLasso => {}
                                _ => self.marquee = Some((d, d)),
                            },
                        }
                    }
                }
                if response.dragged() {
                    if self.xform.is_some() {
                        if let Some(p) = response.interact_pointer_pos() {
                            self.update_transform(p, view, shift);
                        }
                    } else if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some((s, _)) = self.marquee {
                            self.marquee = Some((s, d));
                        } else if !self.lasso.is_empty() && self.select_mode == SelectMode::Lasso {
                            self.lasso.push(d);
                        } else if let Some(o) = self.move_origin {
                            self.apply_move_with_snap(o, d);
                        }
                    }
                }
                if response.drag_stopped() {
                    if self.xform.is_some() {
                        self.commit_transform();
                    } else if let Some((a, b)) = self.marquee.take() {
                        if self.select_mode == SelectMode::Ellipse {
                            self.select_in_ellipse(a, b, combine);
                        } else {
                            self.select_in_rect(a, b, combine);
                        }
                    } else if !self.lasso.is_empty() && self.select_mode != SelectMode::PolyLasso {
                        // (PolyLasso : le tracé en cours survit aux glissés —
                        // il ne se referme qu'au double-clic ou près du départ.)
                        let poly = std::mem::take(&mut self.lasso);
                        self.select_in_lasso(&poly, combine);
                    } else {
                        self.commit_move();
                    }
                }
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        // Baguette magique : sélection par couleur.
                        if self.select_mode == SelectMode::Wand {
                            self.magic_wand(d, combine);
                        } else if self.select_mode == SelectMode::PolyLasso {
                            // Lasso polygonal (point 52) : chaque clic pose un
                            // sommet ; un clic près du premier sommet (≥ 3
                            // sommets) referme, comme la plume (`pen_press`).
                            let near_first = self.lasso.first().is_some_and(|f| {
                                ((d.0 - f.0).powi(2) + (d.1 - f.1).powi(2)).sqrt() < 8.0 / self.zoom.max(0.01)
                            });
                            if near_first && self.lasso.len() >= 3 {
                                let poly = std::mem::take(&mut self.lasso);
                                self.select_in_lasso(&poly, combine);
                            } else {
                                self.lasso.push(d);
                            }
                        } else {
                            match self.topmost_at(d) {
                                Some(id) => {
                                    if shift && self.selection.contains(&id) {
                                        self.selection.remove(&id);
                                    } else {
                                        if !shift {
                                            self.selection.clear();
                                        }
                                        self.selection.insert(id);
                                    }
                                }
                                None => {
                                    if !shift {
                                        self.selection.clear();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ActiveTool::Pan => {
                if response.dragged() {
                    self.pan += response.drag_delta();
                }
            }
            ActiveTool::Text => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        self.create_or_edit_text(d);
                    }
                }
            }
            ActiveTool::Bucket => {
                if response.clicked() {
                    // Le pot échantillonne les pixels *affichés* : sous
                    // rotation de la vue, la capture ne correspond plus au
                    // repère document (Sprint T, point 93 — limite assumée).
                    if self.view_angle != 0.0 {
                        self.info(t(
                            "Remets la rotation de la vue à 0° pour le pot de peinture (menu Vue).",
                            "Reset the view rotation to 0° to use the paint bucket (View menu).",
                        ));
                        return;
                    }
                    if let Some(p) = response.interact_pointer_pos() {
                        // Remplissage différé : on capture la composition affichée.
                        self.bucket_click = Some(p);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    }
                }
            }
            ActiveTool::Cutout => {
                if response.clicked() {
                    if self.view_angle != 0.0 {
                        self.info(t(
                            "Remets la rotation de la vue à 0° pour le détourage (menu Vue).",
                            "Reset the view rotation to 0° to use cutout (View menu).",
                        ));
                        return;
                    }
                    if let Some(p) = response.interact_pointer_pos() {
                        // Détourage différé : même mécanisme que le pot de peinture.
                        // ⌥+clic restaure la visibilité au lieu de la retirer —
                        // corrige une zone trop agressivement détourée sans
                        // repasser par « Éditer le masque ».
                        let restore = ctx.input(|i| i.modifiers.alt);
                        self.cutout_click = Some((p, restore));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    }
                }
            }
            ActiveTool::Pen => {
                // Clic = sommet anguleux ; clic-glissé = sommet lisse (poignées).
                if response.drag_started() || response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        self.pen_press(view.screen_to_doc(p));
                    }
                }
                if response.dragged() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some(last) = self.pen.last_mut() {
                            last.set_symmetric(d);
                        }
                    }
                }
                if response.double_clicked() {
                    self.commit_pen(false);
                }
            }
            ActiveTool::Eyedropper => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        match eyedropper::pick(&self.doc, d) {
                            Some(rgb) => {
                                self.brush.color = [rgb[0], rgb[1], rgb[2], self.brush.color[3]];
                                self.info(t("Couleur prélevée.", "Color picked."));
                            }
                            None => self.info(t("Pas de trait ici (fond).", "No stroke here (background).")),
                        }
                    }
                }
            }
            ActiveTool::Eraser => {
                if response.drag_started() {
                    self.erase_pending.clear();
                    self.erase_path.clear();
                }
                if response.dragged() || response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        self.erase_at(view.screen_to_doc(p));
                    }
                }
                if response.drag_stopped() {
                    if self.eraser_partial {
                        self.commit_partial_erase();
                    } else {
                        self.commit_erase();
                    }
                }
            }
            ActiveTool::PixelBrush | ActiveTool::PixelEraser => {
                let erase = self.active_tool == ActiveTool::PixelEraser;
                if response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        self.raster_touch.clear();
                        self.paint_raster_point(d, erase);
                        self.raster_stroke_last = Some(d);
                    }
                }
                if response.dragged() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        match self.raster_stroke_last {
                            Some(last) => self.paint_raster_segment(last, d, erase),
                            None => self.paint_raster_point(d, erase),
                        }
                        self.raster_stroke_last = Some(d);
                    }
                } else if self.raster_stroke_last.is_some() {
                    // Le geste s'est arrêté sans `drag_stopped` propre (perte
                    // de focus de la fenêtre, clic intercepté par l'OS…) :
                    // on referme quand même le trait, sinon la peinture déjà
                    // appliquée au document resterait sans entrée d'annulation.
                    self.raster_stroke_last = None;
                    self.commit_raster_stroke(if erase { RasterOp::Eraser } else { RasterOp::Brush }, self.editing_mask);
                }
                if response.drag_stopped() {
                    self.raster_stroke_last = None;
                    self.commit_raster_stroke(if erase { RasterOp::Eraser } else { RasterOp::Brush }, self.editing_mask);
                }
            }
            ActiveTool::Airbrush => self.handle_airbrush(ctx, response, view),
            ActiveTool::CloneStamp => self.handle_clone_stamp(ctx, response, view, false),
            ActiveTool::Healing => self.handle_clone_stamp(ctx, response, view, true),
            ActiveTool::Dodge => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Lighten, RasterOp::Dodge),
            ActiveTool::Burn => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Darken, RasterOp::Burn),
            ActiveTool::Saturate => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Saturate, RasterOp::Saturate),
            ActiveTool::Desaturate => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Desaturate, RasterOp::Desaturate),
            ActiveTool::Blur => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Blur, RasterOp::Blur),
            ActiveTool::Sharpen => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Sharpen, RasterOp::Sharpen),
            ActiveTool::Smudge => self.handle_smudge(response, view),
            ActiveTool::Measure => self.handle_measure(ctx, response, view),
            ActiveTool::Gradient => self.handle_gradient_drag(response, view),
            _ => self.handle_draw(ctx, response, view),
        }
    }
}
