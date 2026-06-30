//! Panneau latéral des calques (idée 1) : sélection, visibilité, ajout,
//! suppression. Les calques sont listés du dessus vers le dessous.

use crate::app::PaintApp;
use crate::model::Layer;
use egui::Ui;

pub fn show(ui: &mut Ui, app: &mut PaintApp) {
    ui.heading("Calques");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui.button("➕ Ajouter").clicked() {
            app.add_layer();
        }
        if ui.button("⧉ Dupliquer").on_hover_text("Dupliquer le calque actif").clicked() {
            app.duplicate_layer();
        }
        let can_delete = app.doc.layers.len() > 1;
        if ui
            .add_enabled(can_delete, egui::Button::new("🗑 Supprimer"))
            .clicked()
        {
            app.delete_active_layer();
        }
    });

    ui.horizontal(|ui| {
        let n = app.doc.layers.len();
        let active = app.doc.active_layer;
        if ui
            .add_enabled(active + 1 < n, egui::Button::new("⬆ Monter"))
            .clicked()
        {
            app.move_active_layer(1);
        }
        if ui
            .add_enabled(active > 0, egui::Button::new("⬇ Descendre"))
            .clicked()
        {
            app.move_active_layer(-1);
        }
    });

    ui.horizontal(|ui| {
        let active = app.doc.active_layer;
        if ui
            .add_enabled(active > 0, egui::Button::new("⤓ Fusionner"))
            .on_hover_text("Fusionner avec le calque du dessous")
            .clicked()
        {
            app.merge_down();
        }
        if ui
            .add_enabled(app.doc.layers.len() > 1, egui::Button::new("▦ Aplatir"))
            .on_hover_text("Aplatir tous les calques en un seul")
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
                    if ui.selectable_label(false, "📁").on_hover_text("Afficher / masquer le groupe").clicked() {
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
            let eye = if layer.visible { "👁" } else { "—" };
            if ui
                .selectable_label(false, eye)
                .on_hover_text("Afficher / masquer")
                .clicked()
            {
                layer.visible = !layer.visible;
            }
            let dim = if layer.opacity < 0.999 {
                format!(" · {:.0}%", layer.opacity * 100.0)
            } else {
                String::new()
            };
            let label = format!("{} ({}){}", layer.name, layer.strokes.len(), dim);
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
    ui.label("Calque actif :");
    ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
    ui.horizontal(|ui| {
        ui.label("Opacité");
        let mut pct = (layer.opacity * 100.0).round();
        if ui
            .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %"))
            .changed()
        {
            layer.opacity = pct / 100.0;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Fusion");
        egui::ComboBox::from_id_salt("blend")
            .selected_text(layer.blend.label())
            .show_ui(ui, |ui| {
                for mode in crate::model::BlendMode::ALL {
                    ui.selectable_value(&mut layer.blend, mode, mode.label());
                }
            });
    });

    // --- Liste des éléments du calque actif (voir / sélectionner) -----------
    ui.separator();
    ui.horizontal(|ui| {
        let active = app.doc.active_layer;
        let l = &app.doc.layers[active];
        ui.label(format!(
            "Éléments ({}) :",
            l.images.len() + l.texts.len() + l.strokes.len()
        ));
        if ui.button("↔ Aligner").on_hover_text("Images côte à côte (comparer)").clicked() {
            app.align_images_row();
        }
        if ui.button("✂ Rogner").on_hover_text("Recadrer l'image sélectionnée").clicked() {
            app.start_crop();
        }
    });

    // Disposition (z-order) de la sélection — boutons directs.
    ui.horizontal_wrapped(|ui| {
        use crate::app::ZMove;
        let has = !app.selection.is_empty();
        ui.label("Ordre :");
        if ui.add_enabled(has, egui::Button::new("Devant")).on_hover_text("Premier plan (⌘⇧])").clicked() {
            app.reorder(ZMove::Front);
        }
        if ui.add_enabled(has, egui::Button::new("Avancer")).on_hover_text("Avancer (⌘])").clicked() {
            app.reorder(ZMove::Forward);
        }
        if ui.add_enabled(has, egui::Button::new("Reculer")).on_hover_text("Reculer (⌘[)").clicked() {
            app.reorder(ZMove::Backward);
        }
        if ui.add_enabled(has, egui::Button::new("Fond")).on_hover_text("Arrière-plan (⌘⇧[)").clicked() {
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
                let t = &l.texts[nt - 1 - row];
                (t.id, format!("🔤 {}", short(&t.text)))
            } else if row < nt + ni {
                let im = &l.images[ni - 1 - (row - nt)];
                (im.id, format!("🖼 Image {}×{}", im.w, im.h))
            } else {
                let s = &l.strokes[ns - 1 - (row - nt - ni)];
                let kind = if s.fill { "forme" } else { "trait" };
                (s.id, format!("✏ {kind} ({} pts)", s.points.len()))
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
    egui::CollapsingHeader::new("Historique").default_open(false).show(ui, |ui| {
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
                        if ui.selectable_label(pos == 0, "● État initial").clicked() {
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
        if let Some(t) = goto {
            app.history_goto(t);
        }
    });
}

fn short(t: &str) -> String {
    let t = t.trim();
    if t.is_empty() {
        "(vide)".into()
    } else if t.chars().count() > 18 {
        format!("{}…", t.chars().take(18).collect::<String>())
    } else {
        t.to_string()
    }
}
