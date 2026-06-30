//! Barre supérieure : **menu** (Fichier / Édition / Calque / Aligner / Vue /
//! Filtres), puis rangée d'**outils** à icônes, puis rangée d'**options** de
//! l'outil. Ne dessine pas le canvas.

use crate::app::{AlignMode, PaintApp};
use crate::export::ExportFormat;
use crate::tools::{ActiveTool, SelectMode};
use egui::{Align, Color32, Layout, Sense, Ui, Vec2};

/// Tailles de document prédéfinies (label, largeur, hauteur).
const CANVAS_PRESETS: &[(&str, u32, u32)] = &[
    ("Carré 1080×1080", 1080, 1080),
    ("HD 1280×720", 1280, 720),
    ("Full HD 1920×1080", 1920, 1080),
    ("Portrait 1080×1350", 1080, 1350),
    ("A4 ~ 794×1123", 794, 1123),
    ("Défaut 1280×800", 1280, 800),
];

/// Couleurs d'accès rapide (palette tactile).
const PRESET_COLORS: &[[u8; 3]] = &[
    [20, 20, 30],
    [240, 240, 240],
    [220, 40, 40],
    [240, 140, 30],
    [240, 210, 40],
    [60, 180, 75],
    [40, 110, 220],
    [130, 60, 200],
    [150, 90, 50],
    [130, 130, 140],
];

/// Groupes d'outils (nom, explication). Les icônes sont dessinées (cf.
/// `draw_icon`) pour rester nettes et toujours visibles.
const TOOL_GROUPS: &[&[(ActiveTool, &str, &str)]] = &[
    &[
        (ActiveTool::Select, "Sélection (V)", "Sélectionner, déplacer, redimensionner et tourner des éléments"),
        (ActiveTool::Pan, "Main (H)", "Déplacer la vue · ou Espace + glisser"),
    ],
    &[
        (ActiveTool::Brush, "Pinceau (B)", "Trait à main levée, épaisseur selon la vitesse"),
        (ActiveTool::Eraser, "Gomme (E)", "Efface les traits survolés du calque actif"),
        (ActiveTool::Bucket, "Pot de peinture (G)", "Remplit une zone fermée de la couleur courante"),
        (ActiveTool::Eyedropper, "Pipette (I)", "Prélève une couleur du dessin"),
    ],
    &[
        (ActiveTool::Line, "Ligne (L)", "Segment droit · Maj = horizontale/verticale"),
        (ActiveTool::Arrow, "Flèche (A)", "Segment avec pointe · idéal pour annoter"),
        (ActiveTool::Rectangle, "Rectangle (R)", "Rectangle · Maj = carré · option Rempli"),
        (ActiveTool::Ellipse, "Ellipse (O)", "Ellipse · Maj = cercle · option Rempli"),
        (ActiveTool::Polygon, "Polygone", "Polygone régulier · nombre de côtés réglable"),
        (ActiveTool::Star, "Étoile", "Étoile · nombre de branches réglable"),
    ],
    &[
        (ActiveTool::Pen, "Plume (P)", "Chemin de Bézier · clic = sommet, glissé = courbe ; Entrée valide"),
        (ActiveTool::Text, "Texte (T)", "Cliquer pour écrire ; double-clic pour éditer"),
    ],
];

pub fn show(ui: &mut Ui, app: &mut PaintApp, ctx: &egui::Context) {
    menu_bar(ui, app, ctx);
    ui.separator();
    tools_row(ui, app);
    options_row(ui, app);
}

fn menu_bar(ui: &mut Ui, app: &mut PaintApp, ctx: &egui::Context) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("Fichier", |ui| {
            if ui.button("📄 Nouveau (⌘N)").clicked() {
                app.new_document();
                ui.close_menu();
            }
            if ui.button("📂 Ouvrir… (⌘O)").clicked() {
                app.open_project();
                ui.close_menu();
            }
            if ui.button("💾 Enregistrer le projet (⌘S)").clicked() {
                app.save_project();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🏞 Importer une image…").clicked() {
                app.import_image();
                ui.close_menu();
            }
            if ui.button("📋 Coller une image (⌘V)").clicked() {
                app.paste_image();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("🖼 Exporter en PNG (⌘E)").clicked() {
                app.request_export(ctx, ExportFormat::Png);
                ui.close_menu();
            }
            ui.menu_button("Exporter sous…", |ui| {
                for fmt in [ExportFormat::Png, ExportFormat::Jpg, ExportFormat::Webp, ExportFormat::Pdf] {
                    if ui.button(fmt.label()).clicked() {
                        app.request_export(ctx, fmt);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("SVG (vectoriel)").clicked() {
                    app.export_svg();
                    ui.close_menu();
                }
            });
        });

        ui.menu_button("Édition", |ui| {
            if ui.add_enabled(app.history.can_undo(), egui::Button::new("↶ Annuler (⌘Z)")).clicked() {
                app.undo();
                ui.close_menu();
            }
            if ui.add_enabled(app.history.can_redo(), egui::Button::new("↷ Rétablir (⌘⇧Z)")).clicked() {
                app.redo();
                ui.close_menu();
            }
            ui.separator();
            let has_sel = !app.selection.is_empty();
            if ui.add_enabled(has_sel, egui::Button::new("Copier (⌘C)")).clicked() {
                app.copy_selection();
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Couper (⌘X)")).clicked() {
                app.cut_selection();
                ui.close_menu();
            }
            if ui.button("Coller (⌘V)").clicked() {
                if !app.paste_clipboard() {
                    app.paste_image();
                }
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Dupliquer (⌘D)")).clicked() {
                app.duplicate_selection();
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Supprimer (Suppr)")).clicked() {
                app.delete_selection();
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("Disposition", |ui| {
                use crate::app::ZMove;
                if ui.button("Premier plan (⌘⇧])").clicked() {
                    app.reorder(ZMove::Front);
                    ui.close_menu();
                }
                if ui.button("Avancer (⌘])").clicked() {
                    app.reorder(ZMove::Forward);
                    ui.close_menu();
                }
                if ui.button("Reculer (⌘[)").clicked() {
                    app.reorder(ZMove::Backward);
                    ui.close_menu();
                }
                if ui.button("Arrière-plan (⌘⇧[)").clicked() {
                    app.reorder(ZMove::Back);
                    ui.close_menu();
                }
            });
            if ui.button("🗑 Effacer le calque").clicked() {
                app.clear_active_layer();
                ui.close_menu();
            }
        });

        ui.menu_button("Calque", |ui| {
            if ui.button("➕ Ajouter").clicked() {
                app.add_layer();
                ui.close_menu();
            }
            if ui.button("⧉ Dupliquer").clicked() {
                app.duplicate_layer();
                ui.close_menu();
            }
            let can = app.doc.active_layer > 0;
            if ui.add_enabled(can, egui::Button::new("⤓ Fusionner vers le bas")).clicked() {
                app.merge_down();
                ui.close_menu();
            }
            if ui.add_enabled(app.doc.layers.len() > 1, egui::Button::new("▦ Aplatir")).clicked() {
                app.flatten();
                ui.close_menu();
            }
            ui.separator();
            if ui.add_enabled(can, egui::Button::new("📁 Grouper avec le dessous")).clicked() {
                app.group_with_below();
                ui.close_menu();
            }
            if ui.button("Dégrouper ce calque").clicked() {
                app.ungroup_active();
                ui.close_menu();
            }
        });

        ui.menu_button("Aligner", |ui| {
            let items: &[(&str, AlignMode)] = &[
                ("⫷ Bords gauches", AlignMode::Left),
                ("centres (H)", AlignMode::CenterH),
                ("⫸ Bords droits", AlignMode::Right),
                ("Bords hauts", AlignMode::Top),
                ("centres (V)", AlignMode::MiddleV),
                ("Bords bas", AlignMode::Bottom),
            ];
            for (label, mode) in items {
                if ui.button(*label).clicked() {
                    app.align(*mode);
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Répartir horizontalement").clicked() {
                app.align(AlignMode::DistributeH);
                ui.close_menu();
            }
            if ui.button("Répartir verticalement").clicked() {
                app.align(AlignMode::DistributeV);
                ui.close_menu();
            }
        });

        ui.menu_button("Vue", |ui| {
            ui.horizontal(|ui| {
                if ui.button("➖").clicked() {
                    app.zoom_out();
                }
                if ui.button("➕").clicked() {
                    app.zoom_in();
                }
                if ui.button("100 %").clicked() {
                    app.reset_view();
                }
                if ui.button("Ajuster").clicked() {
                    app.fit_view();
                }
            });
            ui.separator();
            ui.checkbox(&mut app.show_grid, "Grille");
            ui.checkbox(&mut app.show_rulers, "Règles");
            ui.checkbox(&mut app.snap_enabled, "Magnétisme");
            ui.add(
                egui::DragValue::new(&mut app.grid_size).speed(1.0).range(5.0..=200.0).prefix("pas "),
            );
            ui.separator();
            ui.label("Taille du document :");
            for (label, w, h) in CANVAS_PRESETS {
                if ui.button(*label).clicked() {
                    app.set_canvas_size(*w, *h);
                    ui.close_menu();
                }
            }
        });

        ui.menu_button("Filtres", |ui| {
            for f in crate::tools::filter::Filter::ALL {
                if ui.button(f.label()).clicked() {
                    app.filter_selection(f);
                    ui.close_menu();
                }
            }
        });

        ui.menu_button("À propos", |ui| {
            ui.strong("QuickPaint");
            ui.label("Éditeur de dessin tactile · Rust + egui");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Auteur :");
                ui.hyperlink_to("Loïc Berthod", "https://github.com/lberthod");
            });
            ui.hyperlink_to("github.com/lberthod/quickpaint", "https://github.com/lberthod/quickpaint");
        });

        // Côté droit : zoom + annuler/rétablir rapides.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(format!("Zoom {:.0} %", app.zoom * 100.0));
            ui.separator();
            if ui
                .add_enabled(app.history.can_redo(), egui::Button::new("↷"))
                .on_hover_text("Rétablir (⌘⇧Z)")
                .clicked()
            {
                app.redo();
            }
            if ui
                .add_enabled(app.history.can_undo(), egui::Button::new("↶"))
                .on_hover_text("Annuler (⌘Z)")
                .clicked()
            {
                app.undo();
            }
        });
    });
}

fn tools_row(ui: &mut Ui, app: &mut PaintApp) {
    ui.horizontal_wrapped(|ui| {
        for (gi, group) in TOOL_GROUPS.iter().enumerate() {
            if gi > 0 {
                ui.separator();
            }
            for (tool, name, hint) in *group {
                tool_button(ui, app, *tool, name, hint);
            }
        }
    });
}

fn tool_button(ui: &mut Ui, app: &mut PaintApp, tool: ActiveTool, name: &str, hint: &str) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::click());
    let selected = app.active_tool == tool;
    if selected {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, ui.visuals().selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    draw_icon(ui.painter(), rect, tool, ui.visuals().text_color());
    if resp.clicked() {
        app.active_tool = tool;
    }
    resp.on_hover_text(format!("{name} — {hint}"));
}

/// Dessine l'icône vectorielle d'un outil dans `rect` (toujours visible, net).
fn draw_icon(p: &egui::Painter, rect: egui::Rect, tool: ActiveTool, col: Color32) {
    use egui::Shape;
    let b = egui::Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    let at = |x: f32, y: f32| egui::pos2(b.min.x + x * b.width(), b.min.y + y * b.height());
    let st = egui::Stroke::new(1.8, col);
    // Pointe de flèche en `tip`, pointant dans la direction `dir` (normalisée).
    let chevron = |tip: egui::Pos2, dir: (f32, f32)| {
        let s = 0.16 * b.width();
        let (dx, dy) = dir;
        let (bx, by) = (-dx, -dy);
        let a = 0.6_f32;
        let (c, si) = (a.cos(), a.sin());
        let l = (bx * c - by * si, bx * si + by * c);
        let r = (bx * c + by * si, -bx * si + by * c);
        p.line_segment([tip, egui::pos2(tip.x + l.0 * s, tip.y + l.1 * s)], st);
        p.line_segment([tip, egui::pos2(tip.x + r.0 * s, tip.y + r.1 * s)], st);
    };

    match tool {
        ActiveTool::Select => {
            let pts = vec![
                at(0.24, 0.06), at(0.24, 0.86), at(0.42, 0.66), at(0.54, 0.96),
                at(0.64, 0.91), at(0.52, 0.62), at(0.74, 0.62),
            ];
            p.add(Shape::convex_polygon(pts, col, egui::Stroke::NONE));
        }
        ActiveTool::Pan => {
            p.line_segment([at(0.5, 0.1), at(0.5, 0.9)], st);
            p.line_segment([at(0.1, 0.5), at(0.9, 0.5)], st);
            chevron(at(0.5, 0.1), (0.0, -1.0));
            chevron(at(0.5, 0.9), (0.0, 1.0));
            chevron(at(0.1, 0.5), (-1.0, 0.0));
            chevron(at(0.9, 0.5), (1.0, 0.0));
        }
        ActiveTool::Brush => {
            p.line_segment([at(0.2, 0.85), at(0.66, 0.34)], st);
            p.circle_filled(at(0.72, 0.27), 0.13 * b.width(), col);
        }
        ActiveTool::Eraser => {
            let pts = vec![at(0.18, 0.6), at(0.55, 0.24), at(0.82, 0.46), at(0.45, 0.82)];
            p.add(Shape::closed_line(pts, st));
            p.line_segment([at(0.45, 0.82), at(0.82, 0.46)], st);
        }
        ActiveTool::Bucket => {
            let pts = vec![at(0.28, 0.32), at(0.72, 0.32), at(0.62, 0.82), at(0.38, 0.82)];
            p.add(Shape::closed_line(pts, st));
            p.circle_filled(at(0.82, 0.6), 0.07 * b.width(), col);
        }
        ActiveTool::Eyedropper => {
            p.line_segment([at(0.22, 0.82), at(0.62, 0.42)], st);
            p.circle_stroke(at(0.72, 0.3), 0.13 * b.width(), st);
        }
        ActiveTool::Line => {
            p.line_segment([at(0.14, 0.86), at(0.86, 0.14)], st);
        }
        ActiveTool::Arrow => {
            p.line_segment([at(0.14, 0.86), at(0.84, 0.16)], st);
            chevron(at(0.84, 0.16), (0.7, -0.7));
        }
        ActiveTool::Rectangle => {
            p.rect_stroke(egui::Rect::from_min_max(at(0.16, 0.26), at(0.84, 0.74)), 1.0, st);
        }
        ActiveTool::Ellipse => {
            p.circle_stroke(at(0.5, 0.5), 0.34 * b.width(), st);
        }
        ActiveTool::Polygon => {
            let pts: Vec<egui::Pos2> = (0..6)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 6.0 * std::f32::consts::TAU;
                    at(0.5 + a.cos() * 0.36, 0.5 + a.sin() * 0.36)
                })
                .collect();
            p.add(Shape::closed_line(pts, st));
        }
        ActiveTool::Star => {
            let pts: Vec<egui::Pos2> = (0..10)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 10.0 * std::f32::consts::TAU;
                    let r = if i % 2 == 0 { 0.42 } else { 0.18 };
                    at(0.5 + a.cos() * r, 0.5 + a.sin() * r)
                })
                .collect();
            p.add(Shape::convex_polygon(pts, col, egui::Stroke::NONE));
        }
        ActiveTool::Pen => {
            p.line_segment([at(0.8, 0.2), at(0.42, 0.62)], st);
            let nib = vec![at(0.42, 0.62), at(0.26, 0.74), at(0.34, 0.54)];
            p.add(Shape::convex_polygon(nib, col, egui::Stroke::NONE));
            p.line_segment([at(0.26, 0.74), at(0.2, 0.82)], st);
        }
        ActiveTool::Text => {
            p.line_segment([at(0.24, 0.24), at(0.76, 0.24)], st);
            p.line_segment([at(0.5, 0.24), at(0.5, 0.82)], st);
        }
    }
}

/// Barre d'options de l'outil Texte : taille, police, gras, alignement,
/// contour et couleur. Les changements s'appliquent au texte édité/sélectionné.
fn text_options(ui: &mut Ui, app: &mut PaintApp) {
    use crate::model::text::{TextAlign, TextFont};
    let mut changed = false;

    ui.label("Taille :");
    ui.add(egui::Slider::new(&mut app.text_size, 8.0..=200.0));

    ui.separator();
    ui.label("Police :");
    for f in TextFont::ALL {
        if ui.selectable_value(&mut app.text_font, f, f.label()).changed() {
            changed = true;
        }
    }
    if ui.selectable_label(app.text_bold, "𝐆").on_hover_text("Gras").clicked() {
        app.text_bold = !app.text_bold;
        changed = true;
    }

    ui.separator();
    ui.label("Aligner :");
    for a in TextAlign::ALL {
        if ui.selectable_value(&mut app.text_align, a, a.label()).changed() {
            changed = true;
        }
    }

    ui.separator();
    ui.label("Texte :");
    let c = app.brush.color;
    let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    if ui.color_edit_button_srgba(&mut col).changed() {
        app.brush.color = col.to_srgba_unmultiplied();
        // La couleur du texte suit `brush.color` ; on la pousse aussi au texte ciblé.
        changed = true;
    }

    ui.separator();
    ui.label("Contour :");
    if ui.add(egui::Slider::new(&mut app.text_outline_w, 0.0..=8.0)).changed() {
        changed = true;
    }
    if app.text_outline_w > 0.0 {
        let o = app.text_outline_color;
        let mut oc = Color32::from_rgba_unmultiplied(o[0], o[1], o[2], o[3]);
        if ui.color_edit_button_srgba(&mut oc).changed() {
            app.text_outline_color = oc.to_srgba_unmultiplied();
            changed = true;
        }
    }

    if changed {
        app.sync_text_style();
    }
}

fn options_row(ui: &mut Ui, app: &mut PaintApp) {
    ui.horizontal_wrapped(|ui| {
        // Outil Sélection : choix du mode (rectangle / lasso / baguette).
        if app.active_tool == ActiveTool::Select {
            ui.label("Mode :");
            for mode in SelectMode::ALL {
                ui.selectable_value(&mut app.select_mode, mode, mode.label());
            }
            if app.select_mode == SelectMode::Wand {
                ui.separator();
                ui.add(
                    egui::Slider::new(&mut app.wand_tol, 0..=128).text("Tolérance"),
                )
                .on_hover_text("Écart de couleur toléré par canal");
            }
            ui.separator();
            ui.label("Couleur :");
            let c = app.brush.color;
            let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
            if ui.color_edit_button_srgba(&mut col).changed() {
                app.brush.color = col.to_srgba_unmultiplied();
            }
            return;
        }
        // Outil Texte : taille + style riche (police, gras, alignement, contour).
        if app.active_tool == ActiveTool::Text {
            text_options(ui, app);
            return;
        }
        ui.label("Taille :");
        let (size, range) = match app.active_tool {
            ActiveTool::Eraser => (&mut app.eraser.width, 4.0..=80.0),
            ActiveTool::Text => (&mut app.text_size, 8.0..=200.0),
            _ => (&mut app.brush.width, 1.0..=40.0),
        };
        ui.add(egui::Slider::new(size, range));
        if app.active_tool.as_shape().map(|s| s.closed()).unwrap_or(false) {
            ui.checkbox(&mut app.fill_shapes, "Rempli");
        }
        if app.active_tool == ActiveTool::Eraser {
            ui.separator();
            ui.label("Gomme :");
            ui.selectable_value(&mut app.eraser_partial, false, "Objet")
                .on_hover_text("Supprime l'élément entier touché");
            ui.selectable_value(&mut app.eraser_partial, true, "Partielle")
                .on_hover_text("N'efface que la portion de trait touchée (découpe)");
        }
        if matches!(app.active_tool, ActiveTool::Polygon | ActiveTool::Star) {
            ui.add(egui::DragValue::new(&mut app.poly_sides).range(3..=16).prefix("côtés "));
        }
        ui.separator();

        ui.label("Couleur :");
        let c = app.brush.color;
        let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        if ui.color_edit_button_srgba(&mut col).changed() {
            app.brush.color = col.to_srgba_unmultiplied();
        }

        ui.label("Opacité :");
        let mut alpha = app.brush.color[3];
        if ui.add(egui::Slider::new(&mut alpha, 1..=255)).changed() {
            app.brush.color[3] = alpha;
        }

        ui.menu_button("✨ Style", |ui| {
            let presets: &[(&str, f32, u8)] = &[
                ("Pinceau", 0.8, 255),
                ("Marqueur", 0.1, 150),
                ("Calligraphie", 1.0, 255),
                ("Aérographe", 0.25, 90),
            ];
            for (name, pressure, a) in presets {
                if ui.button(*name).clicked() {
                    app.capture_pressure_strength = *pressure;
                    app.brush.color[3] = *a;
                    ui.close_menu();
                }
            }
        })
        .response
        .on_hover_text("Presets de pinceau");

        ui.separator();
        ui.label("Fond :");
        let mut bg = [app.bg.r(), app.bg.g(), app.bg.b()];
        if ui.color_edit_button_srgb(&mut bg).changed() {
            app.bg = Color32::from_rgb(bg[0], bg[1], bg[2]);
        }

        ui.separator();
        for preset in PRESET_COLORS {
            if swatch(ui, *preset).clicked() {
                app.brush.color = [preset[0], preset[1], preset[2], app.brush.color[3]];
            }
        }
        if !app.recent_colors.is_empty() {
            ui.separator();
            ui.label("Récentes :");
            for c in app.recent_colors.clone() {
                if swatch(ui, c).clicked() {
                    app.brush.color = [c[0], c[1], c[2], app.brush.color[3]];
                }
            }
        }

        ui.separator();
        ui.label("Pression");
        ui.add(egui::Slider::new(&mut app.capture_pressure_strength, 0.0..=1.0))
            .on_hover_text("Intensité de la pression simulée (vitesse → épaisseur)");
    });
}

/// Petite pastille de couleur cliquable.
fn swatch(ui: &mut Ui, rgb: [u8; 3]) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let rounding = 4.0;
    ui.painter().rect_filled(rect, rounding, color);
    let stroke = if response.hovered() {
        egui::Stroke::new(2.0, Color32::from_gray(80))
    } else {
        egui::Stroke::new(1.0, Color32::from_gray(170))
    };
    ui.painter().rect_stroke(rect, rounding, stroke);
    response
}
