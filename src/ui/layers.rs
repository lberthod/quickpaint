//! Panneau latéral des calques (idée 1) : sélection, visibilité, ajout,
//! suppression. Les calques sont listés du dessus vers le dessous.

use crate::app::PaintApp;
use crate::i18n::t;
use crate::model::Layer;
use egui::{Sense, Ui, Vec2};

/// Petit bouton carré avec glyphe Phosphor (remplace les émojis d'état —
/// rendu incohérent selon l'OS — par une icône vectorielle nette et fixe).
fn icon_button(ui: &mut Ui, active: bool, glyph: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if active {
        ui.painter().rect_filled(rect.shrink(1.0), 4.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(15.0),
        ui.visuals().text_color(),
    );
    resp
}

pub fn show(ui: &mut Ui, app: &mut PaintApp) {
    ui.heading(t("Calques", "Layers"));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        use egui_phosphor::regular as ic;
        if ui.button(format!("{} {}", ic::PLUS, t("Ajouter", "Add"))).clicked() {
            app.add_layer();
        }
        if ui
            .button(format!("{} {}", ic::COPY, t("Dupliquer", "Duplicate")))
            .on_hover_text(t("Dupliquer le calque actif", "Duplicate the active layer"))
            .clicked()
        {
            app.duplicate_layer();
        }
        let can_delete = app.doc.layers.len() > 1;
        if ui
            .add_enabled(can_delete, egui::Button::new(format!("{} {}", ic::TRASH, t("Supprimer", "Delete"))))
            .clicked()
        {
            app.delete_active_layer();
        }
    });

    ui.horizontal(|ui| {
        let n = app.doc.layers.len();
        let active = app.doc.active_layer;
        if ui
            .add_enabled(active + 1 < n, egui::Button::new(t("▲ Monter", "▲ Move up")))
            .clicked()
        {
            app.move_active_layer(1);
        }
        if ui
            .add_enabled(active > 0, egui::Button::new(t("▼ Descendre", "▼ Move down")))
            .clicked()
        {
            app.move_active_layer(-1);
        }
    });

    ui.horizontal(|ui| {
        let active = app.doc.active_layer;
        if ui
            .add_enabled(active > 0, egui::Button::new(t("Fusionner", "Merge")))
            .on_hover_text(t("Fusionner avec le calque du dessous", "Merge with the layer below"))
            .clicked()
        {
            app.merge_down();
        }
        if ui
            .add_enabled(app.doc.layers.len() > 1, egui::Button::new(t("Aplatir", "Flatten")))
            .on_hover_text(t("Aplatir tous les calques en un seul", "Flatten all layers into one"))
            .clicked()
        {
            app.flatten();
        }
    });

    ui.separator();

    // Du dessus (dernier) vers le dessous (premier). En-tête par groupe.
    let count = app.doc.layers.len();
    let mut select: Option<usize> = None;
    let mut toggle_group: Option<String> = None;
    let mut prev_group: Option<String> = None;
    for idx in (0..count).rev() {
        let group = app.doc.layers[idx].group.clone();
        if group != prev_group {
            if let Some(name) = &group {
                ui.horizontal(|ui| {
                    if icon_button(ui, false, egui_phosphor::regular::FOLDER)
                        .on_hover_text(t("Afficher / masquer le groupe", "Show / hide the group"))
                        .clicked()
                    {
                        toggle_group = Some(name.clone());
                    }
                    ui.label(egui::RichText::new(name).strong());
                });
            }
            prev_group = group.clone();
        }
        let layer: &mut Layer = &mut app.doc.layers[idx];
        let is_active = idx == app.doc.active_layer;
        ui.horizontal(|ui| {
            if group.is_some() {
                ui.add_space(12.0); // indentation des calques groupés
            }
            let eye = if layer.visible { egui_phosphor::regular::EYE } else { egui_phosphor::regular::EYE_SLASH };
            if icon_button(ui, false, eye)
                .on_hover_text(t("Afficher / masquer", "Show / hide"))
                .clicked()
            {
                layer.visible = !layer.visible;
            }
            let dim = if layer.opacity < 0.999 {
                format!(" · {:.0}%", layer.opacity * 100.0)
            } else {
                String::new()
            };
            let clip = if layer.clip { "[clip] " } else { "" };
            let label = if layer.adjustment.is_some() {
                format!("{}{} ({}){}", clip, layer.name, t("réglage", "adjustment"), dim)
            } else {
                format!("{}{} ({}){}", clip, layer.name, layer.strokes.len(), dim)
            };
            if ui.selectable_label(is_active, label).clicked() {
                select = Some(idx);
            }
        });
    }
    if let Some(idx) = select {
        app.doc.active_layer = idx;
    }
    if let Some(name) = toggle_group {
        app.toggle_group(&name);
    }

    // --- Réglages du calque actif (non destructif, façon Photoshop/GIMP) ---
    ui.separator();
    let active = app.doc.active_layer;
    let layer = &mut app.doc.layers[active];
    ui.label(t("Calque actif :", "Active layer:"));
    ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
    ui.horizontal(|ui| {
        ui.label(t("Opacité", "Opacity"));
        let mut pct = (layer.opacity * 100.0).round();
        if ui
            .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %"))
            .changed()
        {
            layer.opacity = pct / 100.0;
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("Fusion", "Blend"));
        egui::ComboBox::from_id_salt("blend")
            .selected_text(layer.blend.label())
            .show_ui(ui, |ui| {
                for mode in crate::model::BlendMode::ALL {
                    ui.selectable_value(&mut layer.blend, mode, mode.label());
                }
            });
    });
    // Masque d'écrêtage : visible seulement à travers le calque du dessous.
    // Indisponible pour le calque du bas (rien en dessous).
    ui.add_enabled_ui(active > 0, |ui| {
        ui.checkbox(&mut layer.clip, t("Écrêter sur le calque du dessous", "Clip to layer below"))
            .on_hover_text(if layer.adjustment.is_some() {
                t(
                    "N'ajuste que le calque juste en dessous (au lieu de tout ce qui est en dessous)",
                    "Only adjusts the layer directly below (instead of everything below)",
                )
            } else {
                t(
                    "Le calque n'apparaît qu'à travers l'opacité du calque inférieur",
                    "The layer only shows through the opacity of the layer below",
                )
            });
    });
    // Calque d'ajustement (F3, Sprint 8.1/8.2) : re-réglable à tout moment,
    // sans jamais toucher aux pixels d'origine — change juste le rendu
    // composé. Le menu déroulant choisit le *type* (preset discret, niveaux,
    // teinte/saturation, courbes) ; les paramètres continus s'éditent avec
    // des sliders juste en dessous.
    if let Some(mut adj) = layer.adjustment {
        use crate::tools::filter::{Adjustment, Filter};
        ui.horizontal(|ui| {
            ui.label(t("Réglage", "Adjustment"));
            egui::ComboBox::from_id_salt("adjustment").selected_text(adj.label()).show_ui(ui, |ui| {
                for f in Filter::ALL {
                    ui.selectable_value(&mut adj, Adjustment::Preset(f), f.label());
                }
                ui.separator();
                ui.selectable_value(&mut adj, Adjustment::default_levels(), t("Niveaux", "Levels"));
                ui.selectable_value(&mut adj, Adjustment::default_hue_saturation(), t("Teinte/Saturation", "Hue/Saturation"));
                ui.selectable_value(&mut adj, Adjustment::default_curves(), t("Courbes", "Curves"));
            });
        });
        match &mut adj {
            Adjustment::Levels { black, white, gamma } => {
                let mut b = *black as f32;
                let mut w = *white as f32;
                ui.horizontal(|ui| {
                    ui.label(t("Noir", "Black"));
                    if ui.add(egui::Slider::new(&mut b, 0.0..=254.0)).changed() {
                        *black = (b.round() as u8).min(white.saturating_sub(1));
                    }
                    ui.label(t("Blanc", "White"));
                    if ui.add(egui::Slider::new(&mut w, 1.0..=255.0)).changed() {
                        *white = (w.round() as u8).max(black.saturating_add(1));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(t("Gamma", "Gamma"));
                    ui.add(egui::Slider::new(gamma, 0.1..=3.0));
                });
            }
            Adjustment::HueSaturation { hue, sat, light } => {
                ui.horizontal(|ui| {
                    ui.label(t("Teinte", "Hue"));
                    ui.add(egui::Slider::new(hue, -180.0..=180.0).suffix("°"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Saturation", "Saturation"));
                    ui.add(egui::Slider::new(sat, -1.0..=1.0));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Luminosité", "Lightness"));
                    ui.add(egui::Slider::new(light, -1.0..=1.0));
                });
            }
            Adjustment::Curves { shadow, mid, highlight } => {
                ui.horizontal(|ui| {
                    ui.label(t("Ombres", "Shadows"));
                    ui.add(egui::Slider::new(shadow, 0..=255));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Tons moyens", "Midtones"));
                    ui.add(egui::Slider::new(mid, 0..=255));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Hautes lumières", "Highlights"));
                    ui.add(egui::Slider::new(highlight, 0..=255));
                });
            }
            Adjustment::Preset(_) => {}
        }
        layer.adjustment = Some(adj);
    }

    // Masque de calque peint (roadmap P2 #14) : peint en niveaux de gris,
    // multiplie l'alpha du calque au rendu. Réutilise le pinceau/gomme pixel
    // existants une fois « Éditer le masque » activé.
    let has_mask = layer.mask.is_some();
    ui.horizontal(|ui| {
        let label = if has_mask {
            t("Retirer le masque", "Remove mask")
        } else {
            t("Ajouter un masque", "Add mask")
        };
        if ui.button(label).clicked() {
            app.toggle_active_layer_mask();
        }
        if has_mask {
            ui.checkbox(&mut app.editing_mask, t("Éditer le masque", "Edit mask")).on_hover_text(t(
                "Le pinceau/gomme pixel peint le masque (blanc = visible, noir = masqué) au lieu du calque",
                "The pixel brush/eraser paints the mask (white = visible, black = hidden) instead of the layer",
            ));
        }
    });

    // --- Liste des éléments du calque actif (voir / sélectionner) -----------
    ui.separator();
    ui.horizontal(|ui| {
        let active = app.doc.active_layer;
        let l = &app.doc.layers[active];
        ui.label(format!(
            "{} ({}) :",
            t("Éléments", "Elements"),
            l.images.len() + l.texts.len() + l.strokes.len()
        ));
        if ui
            .button(t("Aligner", "Align"))
            .on_hover_text(t("Images côte à côte (comparer)", "Images side by side (compare)"))
            .clicked()
        {
            app.align_images_row();
        }
        if ui
            .button(t("Rogner", "Crop"))
            .on_hover_text(t("Recadrer l'image sélectionnée", "Crop the selected image"))
            .clicked()
        {
            app.start_crop();
        }
    });

    // Contrainte de ratio du recadrage (proposée pendant le mode rognage).
    if app.is_cropping() {
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Ratio :", "Ratio:"));
            let choices: &[(&str, Option<f32>)] = &[
                (t("Libre", "Free"), None),
                ("1:1", Some(1.0)),
                ("4:3", Some(4.0 / 3.0)),
                ("16:9", Some(16.0 / 9.0)),
                ("A4", Some(210.0 / 297.0)),
            ];
            for (label, ratio) in choices {
                ui.selectable_value(&mut app.crop_ratio, *ratio, *label);
            }
        });
    }

    // Disposition (z-order) de la sélection — boutons directs.
    ui.horizontal_wrapped(|ui| {
        use crate::app::ZMove;
        let has = !app.selection.is_empty();
        ui.label(t("Ordre :", "Order:"));
        if ui
            .add_enabled(has, egui::Button::new(t("Devant", "Front")))
            .on_hover_text(t("Premier plan (⌘⇧])", "Bring to front (⌘⇧])"))
            .clicked()
        {
            app.reorder(ZMove::Front);
        }
        if ui
            .add_enabled(has, egui::Button::new(t("Avancer", "Forward")))
            .on_hover_text(t("Avancer (⌘])", "Bring forward (⌘])"))
            .clicked()
        {
            app.reorder(ZMove::Forward);
        }
        if ui
            .add_enabled(has, egui::Button::new(t("Reculer", "Backward")))
            .on_hover_text(t("Reculer (⌘[)", "Send backward (⌘[)"))
            .clicked()
        {
            app.reorder(ZMove::Backward);
        }
        if ui
            .add_enabled(has, egui::Button::new(t("Fond", "Back")))
            .on_hover_text(t("Arrière-plan (⌘⇧[)", "Send to back (⌘⇧[)"))
            .clicked()
        {
            app.reorder(ZMove::Back);
        }
    });

    // Liste virtualisée (show_rows) : seules les lignes visibles sont
    // construites → reste rapide même avec des milliers d'éléments.
    let mut pick: Option<u64> = None;
    let active = app.doc.active_layer;
    let (nt, ni, ns) = {
        let l = &app.doc.layers[active];
        (l.texts.len(), l.images.len(), l.strokes.len())
    };
    let total = nt + ni + ns;
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    egui::ScrollArea::vertical().max_height(220.0).show_rows(ui, row_h, total, |ui, range| {
        let l = &app.doc.layers[active];
        for row in range {
            // Ordre d'affichage : textes, puis images, puis traits (du dessus).
            let (id, lbl) = if row < nt {
                let txt = &l.texts[nt - 1 - row];
                (txt.id, format!("{}", short(&txt.text)))
            } else if row < nt + ni {
                let im = &l.images[ni - 1 - (row - nt)];
                (im.id, format!("{} {}×{}", t("Image", "Image"), im.w, im.h))
            } else {
                let s = &l.strokes[ns - 1 - (row - nt - ni)];
                let kind = if s.fill { t("forme", "shape") } else { t("trait", "stroke") };
                (s.id, format!("{kind} ({} pts)", s.points.len()))
            };
            if ui.selectable_label(app.selection.contains(&id), lbl).clicked() {
                pick = Some(id);
            }
        }
    });
    if let Some(id) = pick {
        app.selection.clear();
        app.selection.insert(id);
        app.active_tool = crate::tools::ActiveTool::Select;
    }

    history_panel(ui, app);
}

/// Panneau d'historique : frise des actions ; clic = retour à cet état.
fn history_panel(ui: &mut Ui, app: &mut PaintApp) {
    ui.separator();
    egui::CollapsingHeader::new(t("Historique", "History")).default_open(false).show(ui, |ui| {
        let timeline = app.history.timeline();
        let pos = app.history.position();
        let mut goto: Option<usize> = None;
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        egui::ScrollArea::vertical().max_height(180.0).id_salt("hist").show_rows(
            ui,
            row_h,
            timeline.len() + 1,
            |ui, range| {
                for row in range {
                    if row == 0 {
                        if ui.selectable_label(pos == 0, t("● État initial", "● Initial state")).clicked() {
                            goto = Some(0);
                        }
                    } else {
                        let i = row - 1;
                        let label = format!("{}. {}", row, timeline[i]);
                        if ui.selectable_label(pos == row, label).clicked() {
                            goto = Some(row);
                        }
                    }
                }
            },
        );
        if let Some(step) = goto {
            app.history_goto(step);
        }
    });
}

fn short(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        t("(vide)", "(empty)").to_string()
    } else if s.chars().count() > 18 {
        format!("{}…", s.chars().take(18).collect::<String>())
    } else {
        s.to_string()
    }
}
