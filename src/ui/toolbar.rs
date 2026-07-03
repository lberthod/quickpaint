//! Barre supérieure : **menu** (Fichier / Édition / Calque / Aligner / Vue /
//! Filtres), puis rangée d'**outils** à icônes, puis rangée d'**options** de
//! l'outil. Ne dessine pas le canvas.

use crate::app::{AlignMode, PaintApp};
use crate::export::ExportFormat;
use crate::i18n::{self, t, Lang};
use crate::tools::{ActiveTool, SelectMode};
use egui::{Align, Color32, Layout, Sense, Ui, Vec2};

/// Modèles de document (roadmap P1 #9), groupés par catégorie — utilisés à
/// la fois pour changer la taille du document courant (menu Vue, garde le
/// contenu) et pour créer un nouveau document à cette taille (Fichier ▸
/// Nouveau depuis un modèle, galerie). Calculé à chaque appel (pas `const`)
/// pour suivre la langue courante — coût négligeable, quelques dizaines
/// d'entrées.
fn templates() -> Vec<(&'static str, Vec<(&'static str, u32, u32)>)> {
    vec![
        (
            t("Réseaux sociaux", "Social media"),
            vec![
                (t("Post carré (Instagram)", "Square post (Instagram)"), 1080, 1080),
                (t("Story / Reel", "Story / Reel"), 1080, 1920),
                (t("Post portrait", "Portrait post"), 1080, 1350),
                (t("Bannière Facebook", "Facebook banner"), 1200, 630),
                (t("Miniature YouTube", "YouTube thumbnail"), 1280, 720),
                (t("Bannière YouTube", "YouTube banner"), 2560, 1440),
            ],
        ),
        (
            t("Impression", "Print"),
            vec![
                (t("Affiche A4", "A4 poster"), 1748, 2480),
                (t("Carte de visite", "Business card"), 1050, 600),
                (t("Carte postale", "Postcard"), 1748, 1240),
            ],
        ),
        (
            t("Écran", "Screen"),
            vec![
                (t("Présentation 16:9", "16:9 presentation"), 1920, 1080),
                ("HD 1280×720", 1280, 720),
                (t("Document par défaut", "Default document"), 1280, 800),
            ],
        ),
    ]
}

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
/// `draw_icon`) pour rester nettes et toujours visibles. Calculé à chaque
/// appel pour suivre la langue courante.
fn tool_groups() -> Vec<Vec<(ActiveTool, &'static str, &'static str)>> {
    vec![
        vec![
            (
                ActiveTool::Select,
                t("Sélection (V)", "Select (V)"),
                t(
                    "Sélectionner, déplacer, redimensionner et tourner des éléments",
                    "Select, move, resize and rotate elements",
                ),
            ),
            (
                ActiveTool::Pan,
                t("Main (H)", "Hand (H)"),
                t("Déplacer la vue · ou Espace + glisser", "Pan the view · or Space + drag"),
            ),
        ],
        vec![
            (
                ActiveTool::Brush,
                t("Pinceau (B)", "Brush (B)"),
                t("Trait à main levée, épaisseur selon la vitesse", "Freehand stroke, width follows speed"),
            ),
            (
                ActiveTool::Eraser,
                t("Gomme (E)", "Eraser (E)"),
                t("Efface les traits survolés du calque actif", "Erases hovered strokes on the active layer"),
            ),
            (
                ActiveTool::Bucket,
                t("Pot de peinture (G)", "Paint bucket (G)"),
                t("Remplit une zone fermée de la couleur courante", "Fills a closed area with the current color"),
            ),
            (
                ActiveTool::Eyedropper,
                t("Pipette (I)", "Eyedropper (I)"),
                t("Prélève une couleur du dessin", "Picks a color from the drawing"),
            ),
        ],
        vec![
            (
                ActiveTool::PixelBrush,
                t("Pinceau pixel", "Pixel brush"),
                t(
                    "Peint des pixels (dureté réglable) dans le calque actif — comme GIMP/Photoshop",
                    "Paints pixels (adjustable hardness) on the active layer — like GIMP/Photoshop",
                ),
            ),
            (
                ActiveTool::PixelEraser,
                t("Gomme pixel", "Pixel eraser"),
                t(
                    "Efface des pixels (retire de l'alpha) dans le calque actif",
                    "Erases pixels (removes alpha) on the active layer",
                ),
            ),
            (
                ActiveTool::CloneStamp,
                t("Tampon de clonage", "Clone stamp"),
                t(
                    "⌥+clic = définir la source ; glisser = peindre en recopiant depuis la source",
                    "⌥+click = set source; drag = paint by copying from the source",
                ),
            ),
            (
                ActiveTool::Healing,
                t("Correcteur", "Healing brush"),
                t(
                    "⌥+clic = définir la source ; glisser = recopie la texture en s'adaptant à la couleur environnante",
                    "⌥+click = set source; drag = copies texture while blending toward the surrounding color",
                ),
            ),
            (
                ActiveTool::Cutout,
                t("Détourage", "Cutout"),
                t(
                    "Clic sur le fond à retirer : détoure en un clic (masque de calque, 100% local)",
                    "Click the background to remove: one-click cutout (layer mask, 100% local)",
                ),
            ),
        ],
        vec![
            (
                ActiveTool::Line,
                t("Ligne (L)", "Line (L)"),
                t("Segment droit · Maj = horizontale/verticale", "Straight segment · Shift = horizontal/vertical"),
            ),
            (
                ActiveTool::Arrow,
                t("Flèche (A)", "Arrow (A)"),
                t("Segment avec pointe · idéal pour annoter", "Segment with tip · great for annotating"),
            ),
            (
                ActiveTool::Rectangle,
                t("Rectangle (R)", "Rectangle (R)"),
                t("Rectangle · Maj = carré · option Rempli", "Rectangle · Shift = square · Filled option"),
            ),
            (
                ActiveTool::Ellipse,
                t("Ellipse (O)", "Ellipse (O)"),
                t("Ellipse · Maj = cercle · option Rempli", "Ellipse · Shift = circle · Filled option"),
            ),
            (
                ActiveTool::Polygon,
                t("Polygone", "Polygon"),
                t("Polygone régulier · nombre de côtés réglable", "Regular polygon · adjustable side count"),
            ),
            (
                ActiveTool::Star,
                t("Étoile", "Star"),
                t("Étoile · nombre de branches réglable", "Star · adjustable point count"),
            ),
        ],
        vec![
            (
                ActiveTool::Pen,
                t("Plume (P)", "Pen (P)"),
                t(
                    "Chemin de Bézier · clic = sommet, glissé = courbe ; Entrée valide",
                    "Bézier path · click = vertex, drag = curve; Enter confirms",
                ),
            ),
            (
                ActiveTool::Text,
                t("Texte (T)", "Text (T)"),
                t("Cliquer pour écrire ; double-clic pour éditer", "Click to write; double-click to edit"),
            ),
        ],
    ]
}

pub fn show(ui: &mut Ui, app: &mut PaintApp, ctx: &egui::Context) {
    menu_bar(ui, app, ctx);
    ui.separator();
    tools_row(ui, app);
    options_row(ui, app);
    template_gallery(ctx, app);
    shortcuts_prefs_window(ctx, app);
    batch_export_window(ctx, app);
}

/// Panneau « Exporter en plusieurs tailles » (Sprint 7.3) : coche des
/// multiples du document + une largeur personnalisée, un seul dossier choisi
/// pour tous les fichiers écrits.
fn batch_export_window(ctx: &egui::Context, app: &mut PaintApp) {
    if !app.show_batch_export {
        return;
    }
    let (dw, dh) = app.doc.size;
    let mut open = true;
    let mut want_export = false;
    let mut want_cancel = false;
    egui::Window::new(t("Exporter en plusieurs tailles", "Export multiple sizes"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(300.0);
            ui.label(t("Format :", "Format:"));
            ui.horizontal(|ui| {
                for fmt in [ExportFormat::Png, ExportFormat::Jpg, ExportFormat::Webp, ExportFormat::Pdf] {
                    ui.radio_value(&mut app.batch_export.format, fmt, fmt.label());
                }
            });
            ui.separator();
            ui.label(t("Tailles à exporter :", "Sizes to export:"));
            let dim = |m: f32| format!("{}×{}", (dw as f32 * m).round() as u32, (dh as f32 * m).round() as u32);
            ui.checkbox(&mut app.batch_export.scale_half, format!("0.5× ({})", dim(0.5)));
            ui.checkbox(&mut app.batch_export.scale_1, format!("1× ({})", dim(1.0)));
            ui.checkbox(&mut app.batch_export.scale_2, format!("2× ({})", dim(2.0)));
            ui.checkbox(&mut app.batch_export.scale_3, format!("3× ({})", dim(3.0)));
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.batch_export.custom_enabled, t("Largeur personnalisée (px) :", "Custom width (px):"));
                ui.add_enabled(
                    app.batch_export.custom_enabled,
                    egui::TextEdit::singleline(&mut app.batch_export.custom_width).desired_width(70.0),
                );
            });
            ui.separator();
            ui.label(t(
                "Un seul dossier sera demandé pour tous les fichiers.",
                "You'll be asked for one folder for all files.",
            ));
            ui.horizontal(|ui| {
                if ui.button(t("Exporter…", "Export…")).clicked() {
                    want_export = true;
                }
                if ui.button(t("Annuler", "Cancel")).clicked() {
                    want_cancel = true;
                }
            });
        });
    if want_export {
        app.request_batch_export(ctx);
    } else if want_cancel || !open {
        app.show_batch_export = false;
    }
}

/// Panneau de préférences des raccourcis d'outils (Sprint 7.2) : liste des
/// actions, touche actuelle, bouton « Changer » qui arme la capture de la
/// prochaine touche pressée (`app.capturing_shortcut`).
fn shortcuts_prefs_window(ctx: &egui::Context, app: &mut PaintApp) {
    if !app.show_shortcuts_prefs {
        return;
    }
    let mut open = true;
    egui::Window::new(t("Raccourcis clavier", "Keyboard shortcuts"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(t(
                "Un clic sur « Changer », puis appuyez sur la touche voulue.",
                "Click \"Change\", then press the key you want.",
            ));
            ui.separator();
            egui::Grid::new("shortcuts_grid").num_columns(2).striped(true).show(ui, |ui| {
                for action in crate::keybindings::ShortcutAction::ALL {
                    ui.label(action.label());
                    let capturing = app.capturing_shortcut == Some(action);
                    let btn_label = if capturing {
                        t("Appuyez sur une touche…", "Press a key…").to_string()
                    } else {
                        app.keybindings.key_for(action).name().to_string()
                    };
                    if ui.button(btn_label).clicked() {
                        app.capturing_shortcut = Some(action);
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            if ui.button(t("Réinitialiser les valeurs par défaut", "Reset to defaults")).clicked() {
                app.keybindings.reset_defaults();
                app.capturing_shortcut = None;
            }
        });
    if !open {
        app.show_shortcuts_prefs = false;
        app.capturing_shortcut = None;
    }
}

/// Galerie « Nouveau depuis un modèle » (roadmap P1 #9) : formats prédéfinis
/// groupés par catégorie, façon Canva. Clic = nouveau document vierge à
/// cette taille (contenu actuel perdu, comme « Nouveau »).
fn template_gallery(ctx: &egui::Context, app: &mut PaintApp) {
    if !app.show_template_gallery {
        return;
    }
    let mut open = true;
    let mut picked: Option<(u32, u32)> = None;
    egui::Window::new(t("Nouveau depuis un modèle", "New from template"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            for (cat, items) in templates() {
                ui.label(egui::RichText::new(cat).strong());
                ui.horizontal_wrapped(|ui| {
                    for (label, w, h) in items {
                        let text = format!("{label}\n{w}×{h}");
                        if ui.add(egui::Button::new(text).min_size(egui::vec2(150.0, 40.0))).clicked() {
                            picked = Some((w, h));
                        }
                    }
                });
                ui.separator();
            }
            if ui.button(t("Annuler", "Cancel")).clicked() {
                app.show_template_gallery = false;
            }
        });
    if let Some((w, h)) = picked {
        app.new_document_sized(w, h);
        app.show_template_gallery = false;
    } else if !open {
        app.show_template_gallery = false;
    }
}

fn menu_bar(ui: &mut Ui, app: &mut PaintApp, ctx: &egui::Context) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button(t("Fichier", "File"), |ui| {
            if ui.button(t("Nouveau (⌘N)", "New (⌘N)")).clicked() {
                app.new_document();
                ui.close_menu();
            }
            if ui.button(t("Nouveau depuis un modèle…", "New from template…")).clicked() {
                app.show_template_gallery = true;
                ui.close_menu();
            }
            if ui.button(t("Ouvrir… (⌘O)", "Open… (⌘O)")).clicked() {
                app.open_project();
                ui.close_menu();
            }
            if ui.button(t("Enregistrer le projet (⌘S)", "Save project (⌘S)")).clicked() {
                app.save_project();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(t("Importer une image…", "Import image…")).clicked() {
                app.import_image();
                ui.close_menu();
            }
            if ui.button(t("Coller une image (⌘V)", "Paste image (⌘V)")).clicked() {
                app.paste_image();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(t("Exporter en PNG (⌘E)", "Export as PNG (⌘E)")).clicked() {
                app.request_export(ctx, ExportFormat::Png);
                ui.close_menu();
            }
            ui.menu_button(t("Exporter sous…", "Export as…"), |ui| {
                for fmt in [ExportFormat::Png, ExportFormat::Jpg, ExportFormat::Webp, ExportFormat::Pdf] {
                    if ui.button(fmt.label()).clicked() {
                        app.request_export(ctx, fmt);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button(t("SVG (vectoriel)", "SVG (vector)")).clicked() {
                    app.export_svg();
                    ui.close_menu();
                }
            });
            if ui.button(t("Exporter en plusieurs tailles…", "Export multiple sizes…")).clicked() {
                app.show_batch_export = true;
                ui.close_menu();
            }
        });

        ui.menu_button(t("Édition", "Edit"), |ui| {
            if ui.add_enabled(app.history.can_undo(), egui::Button::new(t("↶ Annuler (⌘Z)", "↶ Undo (⌘Z)"))).clicked() {
                app.undo();
                ui.close_menu();
            }
            if ui
                .add_enabled(app.history.can_redo(), egui::Button::new(t("↷ Rétablir (⌘⇧Z)", "↷ Redo (⌘⇧Z)")))
                .clicked()
            {
                app.redo();
                ui.close_menu();
            }
            ui.separator();
            let has_sel = !app.selection.is_empty();
            if ui.add_enabled(has_sel, egui::Button::new(t("Copier (⌘C)", "Copy (⌘C)"))).clicked() {
                app.copy_selection();
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new(t("Couper (⌘X)", "Cut (⌘X)"))).clicked() {
                app.cut_selection();
                ui.close_menu();
            }
            if ui.button(t("Coller (⌘V)", "Paste (⌘V)")).clicked() {
                if !app.paste_clipboard() {
                    app.paste_image();
                }
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new(t("Dupliquer (⌘D)", "Duplicate (⌘D)"))).clicked() {
                app.duplicate_selection();
                ui.close_menu();
            }
            if ui.add_enabled(has_sel, egui::Button::new(t("Supprimer (Suppr)", "Delete (Del)"))).clicked() {
                app.delete_selection();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(has_sel, egui::Button::new(t("Copier le style (⌥⌘C)", "Copy style (⌥⌘C)")))
                .clicked()
            {
                app.copy_style();
                ui.close_menu();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new(t("Coller le style (⌥⌘V)", "Paste style (⌥⌘V)")))
                .clicked()
            {
                app.paste_style();
                ui.close_menu();
            }
            ui.separator();
            ui.add_enabled_ui(has_sel, |ui| {
                ui.menu_button(t("Dégradé", "Gradient"), |ui| {
                    if ui.button(t("Linéaire", "Linear")).clicked() {
                        app.apply_gradient(crate::model::GradientKind::Linear);
                        ui.close_menu();
                    }
                    if ui.button(t("Radial", "Radial")).clicked() {
                        app.apply_gradient(crate::model::GradientKind::Radial);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("Retirer le dégradé", "Remove gradient")).clicked() {
                        app.remove_gradient();
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text(t(
                    "S'applique aux formes pleines (option « Rempli ») sélectionnées",
                    "Applies to selected filled shapes (\"Filled\" option)",
                ));
            });
            ui.separator();
            let two_filled_shapes = app.selection.len() == 2
                && app.doc.layers[app.doc.active_layer]
                    .strokes
                    .iter()
                    .filter(|s| app.selection.contains(&s.id) && s.fill)
                    .count()
                    == 2;
            ui.add_enabled_ui(two_filled_shapes, |ui| {
                ui.menu_button(t("Booléens", "Booleans"), |ui| {
                    use crate::tools::boolean::BooleanKind;
                    if ui.button(BooleanKind::Union.label()).clicked() {
                        app.boolean_op(BooleanKind::Union);
                        ui.close_menu();
                    }
                    if ui.button(BooleanKind::Subtract.label()).clicked() {
                        app.boolean_op(BooleanKind::Subtract);
                        ui.close_menu();
                    }
                    if ui.button(BooleanKind::Intersect.label()).clicked() {
                        app.boolean_op(BooleanKind::Intersect);
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text(t(
                    "Sélectionne exactement 2 formes pleines (option « Rempli »)",
                    "Select exactly 2 filled shapes (\"Filled\" option)",
                ));
            });
            ui.separator();
            ui.menu_button(t("Disposition", "Order"), |ui| {
                use crate::app::ZMove;
                if ui.button(t("Premier plan (⌘⇧])", "Bring to front (⌘⇧])")).clicked() {
                    app.reorder(ZMove::Front);
                    ui.close_menu();
                }
                if ui.button(t("Avancer (⌘])", "Bring forward (⌘])")).clicked() {
                    app.reorder(ZMove::Forward);
                    ui.close_menu();
                }
                if ui.button(t("Reculer (⌘[)", "Send backward (⌘[)")).clicked() {
                    app.reorder(ZMove::Backward);
                    ui.close_menu();
                }
                if ui.button(t("Arrière-plan (⌘⇧[)", "Send to back (⌘⇧[)")).clicked() {
                    app.reorder(ZMove::Back);
                    ui.close_menu();
                }
            });
            if ui.button(t("Effacer le calque", "Clear layer")).clicked() {
                app.clear_active_layer();
                ui.close_menu();
            }
        });

        ui.menu_button(t("Calque", "Layer"), |ui| {
            if ui.button(t("Ajouter", "Add")).clicked() {
                app.add_layer();
                ui.close_menu();
            }
            ui.menu_button(t("Ajouter un calque d'ajustement", "Add adjustment layer"), |ui| {
                for f in crate::tools::filter::Filter::ALL {
                    if ui.button(f.label()).clicked() {
                        app.add_adjustment_layer(crate::tools::filter::Adjustment::Preset(f));
                        ui.close_menu();
                    }
                }
                ui.separator();
                use crate::tools::filter::Adjustment;
                for (label, make) in [
                    (t("Niveaux…", "Levels…"), Adjustment::default_levels as fn() -> Adjustment),
                    (t("Teinte/Saturation…", "Hue/Saturation…"), Adjustment::default_hue_saturation),
                    (t("Courbes…", "Curves…"), Adjustment::default_curves),
                ] {
                    if ui.button(label).clicked() {
                        app.add_adjustment_layer(make());
                        ui.close_menu();
                    }
                }
            })
            .response
            .on_hover_text(t(
                "Non destructif : réversible, re-réglable (change le filtre à tout moment)",
                "Non-destructive: reversible, re-adjustable (change the filter anytime)",
            ));
            if ui.button(t("Dupliquer", "Duplicate")).clicked() {
                app.duplicate_layer();
                ui.close_menu();
            }
            let can = app.doc.active_layer > 0;
            if ui.add_enabled(can, egui::Button::new(t("Fusionner vers le bas", "Merge down"))).clicked() {
                app.merge_down();
                ui.close_menu();
            }
            if ui.add_enabled(app.doc.layers.len() > 1, egui::Button::new(t("Aplatir", "Flatten"))).clicked() {
                app.flatten();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(can, egui::Button::new(t("Grouper avec le dessous", "Group with below")))
                .clicked()
            {
                app.group_with_below();
                ui.close_menu();
            }
            if ui.button(t("Dégrouper ce calque", "Ungroup this layer")).clicked() {
                app.ungroup_active();
                ui.close_menu();
            }
        });

        ui.menu_button(t("Image", "Image"), |ui| {
            if ui.button(t("Redimensionner l'image…", "Resize image…")).clicked() {
                app.open_resize_dialog(false);
                ui.close_menu();
            }
            if ui.button(t("Taille du canevas…", "Canvas size…")).clicked() {
                app.open_resize_dialog(true);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(t("Taille du document", "Document size"), |ui| {
                for (cat, items) in templates() {
                    ui.menu_button(cat, |ui| {
                        for (label, w, h) in items {
                            if ui.button(format!("{label} ({w}×{h})")).clicked() {
                                app.set_canvas_size(w, h);
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        });

        ui.menu_button(t("Aligner", "Align"), |ui| {
            let items: &[(&str, AlignMode)] = &[
                (t("Bords gauches", "Left edges"), AlignMode::Left),
                (t("centres (H)", "centers (H)"), AlignMode::CenterH),
                (t("Bords droits", "Right edges"), AlignMode::Right),
                (t("Bords hauts", "Top edges"), AlignMode::Top),
                (t("centres (V)", "centers (V)"), AlignMode::MiddleV),
                (t("Bords bas", "Bottom edges"), AlignMode::Bottom),
            ];
            for (label, mode) in items {
                if ui.button(*label).clicked() {
                    app.align(*mode);
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button(t("Répartir horizontalement", "Distribute horizontally")).clicked() {
                app.align(AlignMode::DistributeH);
                ui.close_menu();
            }
            if ui.button(t("Répartir verticalement", "Distribute vertically")).clicked() {
                app.align(AlignMode::DistributeV);
                ui.close_menu();
            }
        });

        ui.menu_button(t("Vue", "View"), |ui| {
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("−").monospace()).clicked() {
                    app.zoom_out();
                }
                if ui.button(egui::RichText::new("+").monospace()).clicked() {
                    app.zoom_in();
                }
                if ui.button("100 %").clicked() {
                    app.reset_view();
                }
                if ui.button(t("Ajuster", "Fit")).clicked() {
                    app.fit_view();
                }
            });
            ui.separator();
            ui.checkbox(&mut app.show_grid, t("Grille", "Grid"));
            ui.checkbox(&mut app.show_rulers, t("Règles", "Rulers"));
            ui.checkbox(&mut app.snap_enabled, t("Magnétisme", "Snap"));
            ui.add(
                egui::DragValue::new(&mut app.grid_size).speed(1.0).range(5.0..=200.0).prefix(t("pas ", "step ")),
            );
        });

        ui.menu_button(t("Filtres", "Filters"), |ui| {
            for f in crate::tools::filter::Filter::ALL {
                if ui.button(f.label()).clicked() {
                    app.filter_selection(f);
                    ui.close_menu();
                }
            }
        });

        ui.menu_button(t("Préférences", "Preferences"), |ui| {
            if ui.button(t("Raccourcis clavier…", "Keyboard shortcuts…")).clicked() {
                app.show_shortcuts_prefs = true;
                ui.close_menu();
            }
        });

        ui.menu_button(t("À propos", "About"), |ui| {
            ui.strong("QuickPaint");
            ui.label(t("Éditeur de dessin tactile · Rust + egui", "Touch drawing editor · Rust + egui"));
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t("Auteur :", "Author:"));
                ui.hyperlink_to("Loïc Berthod", "https://github.com/lberthod");
            });
            ui.hyperlink_to("github.com/lberthod/quickpaint", "https://github.com/lberthod/quickpaint");
        });

        // Côté droit : langue + zoom + annuler/rétablir rapides.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            language_switch(ui);
            ui.separator();
            ui.label(format!("{} {:.0} %", t("Zoom", "Zoom"), app.zoom * 100.0));
            ui.separator();
            if ui
                .add_enabled(app.history.can_redo(), egui::Button::new("↷"))
                .on_hover_text(t("Rétablir (⌘⇧Z)", "Redo (⌘⇧Z)"))
                .clicked()
            {
                app.redo();
            }
            if ui
                .add_enabled(app.history.can_undo(), egui::Button::new("↶"))
                .on_hover_text(t("Annuler (⌘Z)", "Undo (⌘Z)"))
                .clicked()
            {
                app.undo();
            }
        });
    });
}

/// Sélecteur de langue FR/EN (roadmap transversal — i18n). Persisté entre
/// lancements ; la détection système ne sert que de valeur initiale.
fn language_switch(ui: &mut Ui) {
    let fr_selected = i18n::is_french();
    if ui.selectable_label(!fr_selected, "EN").clicked() && fr_selected {
        i18n::set(Lang::En);
    }
    if ui.selectable_label(fr_selected, "FR").clicked() && !fr_selected {
        i18n::set(Lang::Fr);
    }
}

fn tools_row(ui: &mut Ui, app: &mut PaintApp) {
    ui.horizontal_wrapped(|ui| {
        for (gi, group) in tool_groups().iter().enumerate() {
            if gi > 0 {
                ui.separator();
            }
            for (tool, name, hint) in group {
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
        ActiveTool::PixelBrush => {
            p.line_segment([at(0.2, 0.85), at(0.66, 0.34)], st);
            p.circle_filled(at(0.72, 0.27), 0.13 * b.width(), col);
            // Petit damier : signale le pixel (par opposition au trait vectoriel).
            let s = 0.09 * b.width();
            p.rect_filled(egui::Rect::from_center_size(at(0.24, 0.24), Vec2::splat(s)), 0.0, col);
            p.rect_stroke(egui::Rect::from_center_size(at(0.24, 0.24), Vec2::splat(s * 2.0)), 0.0, st);
        }
        ActiveTool::PixelEraser => {
            let pts = vec![at(0.18, 0.6), at(0.55, 0.24), at(0.82, 0.46), at(0.45, 0.82)];
            p.add(Shape::closed_line(pts, st));
            p.line_segment([at(0.45, 0.82), at(0.82, 0.46)], st);
            let s = 0.09 * b.width();
            p.rect_filled(egui::Rect::from_center_size(at(0.24, 0.24), Vec2::splat(s)), 0.0, col);
            p.rect_stroke(egui::Rect::from_center_size(at(0.24, 0.24), Vec2::splat(s * 2.0)), 0.0, st);
        }
        ActiveTool::CloneStamp => {
            // Tampon (rectangle arrondi stylisé) + petite croix = point source.
            p.rect_stroke(egui::Rect::from_min_max(at(0.22, 0.34), at(0.68, 0.74)), 3.0, st);
            p.line_segment([at(0.3, 0.74), at(0.6, 0.9)], st);
            p.line_segment([at(0.78, 0.16), at(0.78, 0.3)], st);
            p.line_segment([at(0.71, 0.23), at(0.85, 0.23)], st);
        }
        ActiveTool::Healing => {
            // Pansement (rectangle arrondi) + étincelle = correction douce.
            p.rect_stroke(egui::Rect::from_min_max(at(0.2, 0.36), at(0.66, 0.72)), 6.0, st);
            p.line_segment([at(0.78, 0.16), at(0.78, 0.32)], st);
            p.line_segment([at(0.7, 0.24), at(0.86, 0.24)], st);
            p.circle_filled(at(0.78, 0.24), 0.05 * b.width(), col);
        }
        ActiveTool::Bucket => {
            let pts = vec![at(0.28, 0.32), at(0.72, 0.32), at(0.62, 0.82), at(0.38, 0.82)];
            p.add(Shape::closed_line(pts, st));
            p.circle_filled(at(0.82, 0.6), 0.07 * b.width(), col);
        }
        ActiveTool::Cutout => {
            // Ciseaux stylisés le long d'un contour pointillé = détourage.
            p.line_segment([at(0.16, 0.5), at(0.32, 0.5)], st);
            p.line_segment([at(0.4, 0.5), at(0.56, 0.5)], st);
            p.line_segment([at(0.64, 0.5), at(0.8, 0.5)], st);
            p.circle_stroke(at(0.16, 0.34), 0.06 * b.width(), st);
            p.circle_stroke(at(0.16, 0.66), 0.06 * b.width(), st);
            p.line_segment([at(0.16, 0.34), at(0.32, 0.5)], st);
            p.line_segment([at(0.16, 0.66), at(0.32, 0.5)], st);
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

    ui.label(t("Taille :", "Size:"));
    ui.add(egui::Slider::new(&mut app.text_size, 8.0..=200.0));

    ui.separator();
    ui.label(t("Police :", "Font:"));
    for f in TextFont::ALL {
        if ui.selectable_value(&mut app.text_font, f, f.label()).changed() {
            changed = true;
        }
    }
    if ui.selectable_label(app.text_bold, "𝐆").on_hover_text(t("Gras", "Bold")).clicked() {
        app.text_bold = !app.text_bold;
        changed = true;
    }

    ui.separator();
    changed |= font_family_picker(ui, app);

    ui.separator();
    ui.label(t("Aligner :", "Align:"));
    for a in TextAlign::ALL {
        if ui.selectable_value(&mut app.text_align, a, a.label()).changed() {
            changed = true;
        }
    }

    ui.separator();
    ui.label(t("Texte :", "Text:"));
    let c = app.brush.color;
    let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    if ui.color_edit_button_srgba(&mut col).changed() {
        app.brush.color = col.to_srgba_unmultiplied();
        // La couleur du texte suit `brush.color` ; on la pousse aussi au texte ciblé.
        changed = true;
    }

    ui.separator();
    ui.label(t("Contour :", "Outline:"));
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

/// Sélecteur de police système (roadmap P1 #7) : combo avec recherche —
/// une police n'est chargée dans egui qu'au moment où elle est choisie
/// (`ensure_loaded`), pas au survol de la liste.
fn font_family_picker(ui: &mut Ui, app: &mut PaintApp) -> bool {
    let mut changed = false;
    ui.label(t("Police système :", "System font:"));
    // Le filtre vit **hors** du popup du ComboBox : un TextEdit imbriqué
    // dans une popup transitoire egui perd le focus / referme le popup au
    // clic (comportement fragile constaté), donc on garde un champ toujours
    // visible dans la barre d'options — le popup ne fait qu'afficher la
    // liste déjà filtrée.
    ui.add(egui::TextEdit::singleline(&mut app.font_search).hint_text(t("Filtrer…", "Filter…")).desired_width(100.0));
    let current = app.text_font_family.clone().unwrap_or_else(|| t("(intégrée)", "(built-in)").into());
    egui::ComboBox::from_id_salt("sys_font").selected_text(current).width(170.0).show_ui(ui, |ui| {
        if ui
            .selectable_label(app.text_font_family.is_none(), t("(intégrée : Sans/Mono)", "(built-in: Sans/Mono)"))
            .clicked()
        {
            app.text_font_family = None;
            changed = true;
        }
        let query = app.font_search.to_lowercase();
        let names = app.font_manager.family_names();
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for name in names.iter().filter(|n| query.is_empty() || n.to_lowercase().contains(&query)).take(200) {
                let selected = app.text_font_family.as_deref() == Some(name.as_str());
                if ui.selectable_label(selected, name).clicked() {
                    app.font_manager.ensure_loaded(ui.ctx(), name);
                    app.text_font_family = Some(name.clone());
                    changed = true;
                }
            }
        });
    })
    .response
    .on_hover_text(format!("{} {}", app.font_manager.face_count(), t("polices détectées sur ce Mac", "fonts detected on this Mac")));
    changed
}

fn options_row(ui: &mut Ui, app: &mut PaintApp) {
    ui.horizontal_wrapped(|ui| {
        // Outil Sélection : choix du mode (rectangle / lasso / baguette).
        if app.active_tool == ActiveTool::Select {
            ui.label(t("Mode :", "Mode:"));
            for mode in SelectMode::ALL {
                ui.selectable_value(&mut app.select_mode, mode, mode.label());
            }
            if app.select_mode == SelectMode::Wand {
                ui.separator();
                ui.add(
                    egui::Slider::new(&mut app.wand_tol, 0..=128).text(t("Tolérance", "Tolerance")),
                )
                .on_hover_text(t("Écart de couleur toléré par canal", "Color tolerance per channel"));
            }
            ui.separator();
            ui.label(t("Couleur :", "Color:"));
            brush_color_edit(ui, app);
            return;
        }
        // Outil Texte : taille + style riche (police, gras, alignement, contour).
        if app.active_tool == ActiveTool::Text {
            text_options(ui, app);
            return;
        }
        ui.label(t("Taille :", "Size:"));
        let (size, range) = match app.active_tool {
            ActiveTool::Eraser | ActiveTool::PixelEraser => (&mut app.eraser.width, 4.0..=80.0),
            ActiveTool::Text => (&mut app.text_size, 8.0..=200.0),
            _ => (&mut app.brush.width, 1.0..=40.0),
        };
        ui.add(egui::Slider::new(size, range));
        if app.active_tool.as_shape().map(|s| s.closed()).unwrap_or(false) {
            ui.checkbox(&mut app.fill_shapes, t("Rempli", "Filled"));
        }
        if app.active_tool == ActiveTool::Eraser {
            ui.separator();
            ui.label(t("Gomme :", "Eraser:"));
            ui.selectable_value(&mut app.eraser_partial, false, t("Objet", "Object"))
                .on_hover_text(t("Supprime l'élément entier touché", "Removes the whole touched element"));
            ui.selectable_value(&mut app.eraser_partial, true, t("Partielle", "Partial"))
                .on_hover_text(t(
                    "N'efface que la portion de trait touchée (découpe)",
                    "Only erases the touched portion of the stroke (splits it)",
                ));
        }
        if matches!(
            app.active_tool,
            ActiveTool::PixelBrush | ActiveTool::PixelEraser | ActiveTool::CloneStamp | ActiveTool::Healing
        ) {
            ui.separator();
            ui.label(t("Dureté :", "Hardness:"));
            ui.add(egui::Slider::new(&mut app.pixel_hardness, 0.0..=1.0))
                .on_hover_text(t("0 = bord dégradé (aérographe), 1 = bord net", "0 = soft edge (airbrush), 1 = hard edge"));
        }
        if app.active_tool == ActiveTool::Cutout {
            ui.separator();
            ui.label(t("Tolérance :", "Tolerance:"));
            ui.add(egui::Slider::new(&mut app.cutout_tolerance, 0..=100)).on_hover_text(t(
                "Écart de couleur toléré par rapport au point cliqué — augmenter pour un fond dégradé/bruité",
                "Color difference tolerated from the clicked point — raise it for a gradient/noisy background",
            ));
            ui.checkbox(&mut app.cutout_global, t("Global", "Global")).on_hover_text(t(
                "Sélectionne toute la couleur proche dans la zone visible, pas seulement la région connectée au clic — utile pour un fond visible par bouts (feuillage…)",
                "Selects all similar color in the visible area, not just the region connected to the click — useful for a background visible through gaps (foliage…)",
            ));
            ui.label(t("⌥+clic : restaurer", "⌥+click: restore"));
        }
        if matches!(app.active_tool, ActiveTool::CloneStamp | ActiveTool::Healing) {
            ui.separator();
            let label = if app.clone_source.is_some() {
                t("Source définie ✓", "Source set ✓")
            } else {
                t("⌥+clic pour définir la source", "⌥+click to set the source")
            };
            ui.label(label);
        }
        if matches!(app.active_tool, ActiveTool::Polygon | ActiveTool::Star) {
            ui.add(egui::DragValue::new(&mut app.poly_sides).range(3..=16).prefix(t("côtés ", "sides ")));
        }
        ui.separator();

        ui.label(t("Couleur :", "Color:"));
        brush_color_edit(ui, app);

        ui.label(t("Opacité :", "Opacity:"));
        let mut alpha = app.brush.color[3];
        if ui.add(egui::Slider::new(&mut alpha, 1..=255)).changed() {
            app.brush.color[3] = alpha;
        }

        ui.menu_button(t("Style", "Style"), |ui| {
            let presets: &[(&str, f32, u8)] = &[
                (t("Pinceau", "Brush"), 0.8, 255),
                (t("Marqueur", "Marker"), 0.1, 150),
                (t("Calligraphie", "Calligraphy"), 1.0, 255),
                (t("Aérographe", "Airbrush"), 0.25, 90),
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
        .on_hover_text(t("Presets de pinceau", "Brush presets"));

        ui.separator();
        ui.label(t("Fond :", "Background:"));
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
            ui.label(t("Récentes :", "Recent:"));
            for c in app.recent_colors.clone() {
                if swatch(ui, c).clicked() {
                    app.brush.color = [c[0], c[1], c[2], app.brush.color[3]];
                }
            }
        }

        ui.separator();
        ui.label(t("Palette :", "Palette:"));
        let mut to_remove = None;
        for (i, c) in app.custom_palette.clone().into_iter().enumerate() {
            let resp = swatch(ui, c);
            if resp.clicked() {
                app.brush.color = [c[0], c[1], c[2], app.brush.color[3]];
            }
            if resp.secondary_clicked() {
                to_remove = Some(i);
            }
            resp.on_hover_text(t("Clic : sélectionner · clic droit : retirer", "Click: select · right-click: remove"));
        }
        if let Some(i) = to_remove {
            app.remove_from_palette(i);
        }
        let current = [app.brush.color[0], app.brush.color[1], app.brush.color[2]];
        if ui
            .add(egui::Button::new("+").min_size(Vec2::splat(20.0)))
            .on_hover_text(t("Ajouter la couleur courante à la palette", "Add current color to palette"))
            .clicked()
        {
            app.add_to_palette(current);
        }

        ui.separator();
        ui.label(t("Pression", "Pressure"));
        ui.add(egui::Slider::new(&mut app.capture_pressure_strength, 0.0..=1.0))
            .on_hover_text(t("Intensité de la pression simulée (vitesse → épaisseur)", "Simulated pressure strength (speed → thickness)"));
    });
}

/// Couleur du pinceau : pastille (picker HSV egui) + saisie hexadécimale
/// `#RGB` / `#RRGGBB` / `#RRGGBBAA` (roadmap P0 #6).
fn brush_color_edit(ui: &mut Ui, app: &mut PaintApp) {
    let c = app.brush.color;
    let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
    if ui.color_edit_button_srgba(&mut col).changed() {
        app.brush.color = col.to_srgba_unmultiplied();
    }
    let resp = ui.add(
        egui::TextEdit::singleline(&mut app.hex_field)
            .desired_width(78.0)
            .font(egui::TextStyle::Monospace)
            .hint_text("#RRGGBB"),
    );
    if resp.changed() {
        if let Some(rgba) = parse_hex_color(&app.hex_field) {
            app.brush.color = rgba;
        }
    }
    // Hors saisie, le champ reflète la couleur courante (normalisé).
    if !resp.has_focus() {
        let c = app.brush.color;
        app.hex_field = if c[3] == 255 {
            format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3])
        };
    }
}

/// Analyse `#RGB`, `#RRGGBB` ou `#RRGGBBAA` (dièse optionnel, casse libre).
fn parse_hex_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().trim_start_matches('#');
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    match s.len() {
        3 => {
            let nib = |i: usize| u8::from_str_radix(&s[i..i + 1], 16).ok().map(|v| v * 17);
            Some([nib(0)?, nib(1)?, nib(2)?, 255])
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::parse_hex_color;

    #[test]
    fn parses_common_hex_forms() {
        assert_eq!(parse_hex_color("#FF8000"), Some([255, 128, 0, 255]));
        assert_eq!(parse_hex_color("ff8000"), Some([255, 128, 0, 255]));
        assert_eq!(parse_hex_color("#F80"), Some([255, 136, 0, 255]));
        assert_eq!(parse_hex_color("#FF800080"), Some([255, 128, 0, 128]));
    }

    #[test]
    fn rejects_invalid_hex() {
        assert_eq!(parse_hex_color("#GG0000"), None);
        assert_eq!(parse_hex_color("#FF80"), None);
        assert_eq!(parse_hex_color(""), None);
    }
}
