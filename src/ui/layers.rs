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

    // Répartition multi-calque (point 36 de l'audit) : ⇧/⌘+clic sur un nom de
    // calque ci-dessous peuple `layer_multi_select` — au moins 3 calques non
    // vides nécessaires (les deux extrêmes restent fixes, à répartir entre
    // eux).
    let multi = app.layer_multi_select.len();
    if multi > 0 {
        ui.horizontal(|ui| {
            ui.label(format!("{} : {multi}", t("Calques sélectionnés", "Selected layers")));
            if ui.small_button(t("Effacer", "Clear")).clicked() {
                app.layer_multi_select.clear();
            }
        });
    }
    ui.horizontal(|ui| {
        let can_distribute = multi >= 3;
        ui.label(t("Répartir :", "Distribute:"));
        if ui
            .add_enabled(can_distribute, egui::Button::new(t("↔ Horizontal", "↔ Horizontal")))
            .on_hover_text(t(
                "Espace uniformément les calques sélectionnés (centres), les deux extrêmes restent fixes",
                "Evenly spaces the selected layers (centers), the two outer ones stay fixed",
            ))
            .clicked()
        {
            app.distribute_layers(true);
        }
        if ui
            .add_enabled(can_distribute, egui::Button::new(t("↕ Vertical", "↕ Vertical")))
            .clicked()
        {
            app.distribute_layers(false);
        }
    });

    ui.separator();

    // Recherche/filtre (Sprint I.4) : complément naturel du renommage
    // existant, utile seulement à partir d'un nombre significatif de
    // calques — n'alourdit pas l'UI des petits documents.
    const SEARCH_THRESHOLD: usize = 8;
    let filter_active = app.doc.layers.len() > SEARCH_THRESHOLD;
    if filter_active {
        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add(
                egui::TextEdit::singleline(&mut app.layer_search)
                    .hint_text(t("Filtrer les calques…", "Filter layers…"))
                    .desired_width(f32::INFINITY),
            );
        });
        ui.separator();
    }
    let query = app.layer_search.to_lowercase();
    let ctx = ui.ctx().clone();

    // Du dessus (dernier) vers le dessous (premier). En-tête par groupe.
    let count = app.doc.layers.len();
    let mut select: Option<(usize, u64)> = None;
    let mut toggle_group: Option<String> = None;
    let mut prev_group: Option<String> = None;
    let mut reorder: Option<(u64, u64)> = None;
    let mut start_rename: Option<(u64, String)> = None;
    let mut commit_rename = false;
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
        if filter_active && !query.is_empty() && !app.doc.layers[idx].name.to_lowercase().contains(&query) {
            continue;
        }
        // Vignette (Sprint I.3) : calculée avant d'emprunter `layer` en
        // mutable (le cache vit sur `app`, pas sur le calque lui-même).
        let thumbnail = app.layer_thumbnail(&ctx, app.doc.layers[idx].id);
        let layer: &mut Layer = &mut app.doc.layers[idx];
        let is_active = idx == app.doc.active_layer;
        let renaming = app.layer_rename.as_ref().is_some_and(|(id, _)| *id == layer.id);

        // Glisser-déposer pour réordonner (UX-3.1) : chaque ligne est à la
        // fois une source (payload = l'id du calque — pas son index, qui
        // pourrait dater d'une frame précédente) et une zone de dépôt ; un
        // dépôt sur la ligne du calque `layer_id` l'y déplace. Avant, seuls
        // les boutons ▲ Monter / ▼ Descendre existaient (un clic par
        // position à franchir, constat C5, UX_SPRINTS.md).
        let layer_id = layer.id;
        let row_id = egui::Id::new("layer_row").with(layer_id);
        let frame = egui::Frame::default().inner_margin(2.0);
        // Seule la poignée (DOTS_SIX_VERTICAL) est enveloppée dans
        // dnd_drag_source : avant, toute la ligne l'était, et la détection de
        // glisser-déposer captait le clic avant les boutons œil/cadenas/etc.,
        // qui devenaient difficiles à activer (surtout au trackpad). La zone
        // de dépôt (dnd_drop_zone), elle, reste sur toute la ligne.
        let (_, dropped) = ui.dnd_drop_zone::<u64, ()>(frame, |ui| {
            ui.horizontal(|ui| {
                if group.is_some() {
                    ui.add_space(12.0); // indentation des calques groupés
                }
                ui.dnd_drag_source(row_id, layer_id, |ui| {
                    ui.label(egui_phosphor::regular::DOTS_SIX_VERTICAL);
                })
                .response
                .on_hover_text(t("Glisser pour réordonner", "Drag to reorder"));
                {
                    let eye = if layer.visible { egui_phosphor::regular::EYE } else { egui_phosphor::regular::EYE_SLASH };
                    if icon_button(ui, false, eye)
                        .on_hover_text(t("Afficher / masquer", "Show / hide"))
                        .clicked()
                    {
                        layer.visible = !layer.visible;
                    }
                    // Verrouillage (audit_sprint_xx.md B.1) : bloque la
                    // peinture/édition de contenu sur ce calque tant qu'actif
                    // (voir `layer_lock_blocks_tool` dans `app/mod.rs`) — la
                    // visibilité, l'opacité et le réordonnancement restent
                    // possibles, volontairement pas bloqués ici.
                    let lock_icon = if layer.locked { egui_phosphor::regular::LOCK } else { egui_phosphor::regular::LOCK_OPEN };
                    if icon_button(ui, layer.locked, lock_icon)
                        .on_hover_text(t(
                            "Verrouiller / déverrouiller (bloque la peinture, pas la visibilité)",
                            "Lock / unlock (blocks painting, not visibility)",
                        ))
                        .clicked()
                    {
                        layer.locked = !layer.locked;
                    }
                    // Verrouillage granulaire (audit point 28) : position et
                    // transparence, indépendants du verrou global ci-dessus —
                    // affichés seulement s'ils sont actifs (icônes discrètes,
                    // pas une 3e icône permanente pour un cas d'usage plus
                    // rare que le verrou global) ; se règlent depuis le
                    // panneau « Calque actif » plus bas.
                    if layer.lock_position {
                        icon_button(ui, true, egui_phosphor::regular::ARROWS_OUT_CARDINAL).on_hover_text(t(
                            "Position verrouillée (glisser-déplacer bloqué) — réglable plus bas",
                            "Position locked (drag-move blocked) — adjustable below",
                        ));
                    }
                    if layer.lock_alpha {
                        icon_button(ui, true, egui_phosphor::regular::CHECKERBOARD).on_hover_text(t(
                            "Transparence verrouillée — réglable plus bas",
                            "Transparency locked — adjustable below",
                        ));
                    }
                    // Code couleur (Sprint I.5) : étiquette visuelle,
                    // aucun effet sur le rendu — palette prédéfinie plutôt
                    // qu'un sélecteur complet, plus rapide à l'usage.
                    let (swatch_rect, swatch_resp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::click());
                    let swatch_color = layer
                        .color_tag
                        .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                        .unwrap_or(egui::Color32::TRANSPARENT);
                    ui.painter().rect_filled(swatch_rect, 3.0, swatch_color);
                    ui.painter().rect_stroke(swatch_rect, 3.0, ui.visuals().widgets.noninteractive.bg_stroke);
                    let swatch_resp = swatch_resp.on_hover_text(t("Code couleur du calque", "Layer color tag"));
                    let popup_id = ui.make_persistent_id("layer_color_tag").with(layer_id);
                    if swatch_resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup_id));
                    }
                    egui::popup_below_widget(ui, popup_id, &swatch_resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.horizontal(|ui| {
                            const PALETTE: [[u8; 3]; 8] = [
                                [237, 85, 101],  // rouge
                                [242, 153, 74],  // orange
                                [235, 210, 80],  // jaune
                                [95, 191, 128],  // vert
                                [90, 170, 220],  // bleu
                                [154, 120, 219], // violet
                                [200, 200, 200], // gris
                                [60, 60, 60],    // noir
                            ];
                            for c in PALETTE {
                                let (r, rr) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                                ui.painter().rect_filled(r, 3.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                                if rr.clicked() {
                                    layer.color_tag = Some(c);
                                }
                            }
                            if ui.button(t("Aucune", "None")).clicked() {
                                layer.color_tag = None;
                            }
                        });
                    });
                    let dim = if layer.opacity < 0.999 {
                        format!(" · {:.0}%", layer.opacity * 100.0)
                    } else {
                        String::new()
                    };
                    let clip = if layer.clip { "[clip] " } else { "" };
                    // Vignette (Sprint I.3) : à gauche du nom, complète le
                    // texte « (N traits) » plutôt que le remplacer entièrement.
                    if let Some(tex) = &thumbnail {
                        ui.add(egui::Image::from_texture((tex.id(), tex.size_vec2())).fit_to_exact_size(Vec2::splat(20.0)));
                    }
                    if renaming {
                        let (_, buf) = app.layer_rename.as_mut().expect("renaming checked above");
                        let resp = ui.add(egui::TextEdit::singleline(buf).desired_width(100.0));
                        resp.request_focus();
                        if resp.lost_focus() {
                            commit_rename = true;
                        }
                    } else {
                        let suffix = if layer.adjustment.is_some() {
                            format!("({}){dim}", t("réglage", "adjustment"))
                        } else {
                            format!("({}){dim}", layer.strokes.len())
                        };
                        let highlighted = is_active || app.layer_multi_select.contains(&layer.id);
                        let label = ui
                            .selectable_label(highlighted, format!("{clip}{} {suffix}", layer.name))
                            .on_hover_text(t(
                                "Clic : calque actif · ⇧/⌘+clic : ajouter à la sélection multi-calque (pour Répartir)",
                                "Click: active layer · Shift/Cmd+click: add to multi-layer selection (for Distribute)",
                            ));
                        if label.clicked() {
                            select = Some((idx, layer.id));
                        }
                        if label.double_clicked() {
                            start_rename = Some((layer.id, layer.name.clone()));
                        }
                    }
                }
            });
        });
        if let Some(from_id) = dropped {
            reorder = Some((*from_id, layer_id));
        }
    }
    if let Some((idx, id)) = select {
        let (shift, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.command || i.modifiers.ctrl));
        if shift {
            let anchor = app.layer_select_anchor.unwrap_or(idx);
            let (lo, hi) = (anchor.min(idx), anchor.max(idx));
            for i in lo..=hi {
                app.layer_multi_select.insert(app.doc.layers[i].id);
            }
        } else if cmd {
            if !app.layer_multi_select.remove(&id) {
                app.layer_multi_select.insert(id);
            }
            app.layer_select_anchor = Some(idx);
        } else {
            app.layer_multi_select.clear();
            app.layer_multi_select.insert(id);
            app.layer_select_anchor = Some(idx);
        }
        app.doc.active_layer = idx;
    }
    if let Some(name) = toggle_group {
        app.toggle_group(&name);
    }
    if let Some((from, to)) = reorder {
        app.reorder_layer(from, to);
    }
    if let Some(pair) = start_rename {
        app.layer_rename = Some(pair);
    }
    if commit_rename {
        if let Some((id, new_name)) = app.layer_rename.take() {
            let trimmed = new_name.trim();
            if !trimmed.is_empty() {
                if let Some(l) = app.doc.layers.iter_mut().find(|l| l.id == id) {
                    l.name = trimmed.to_string();
                }
            }
        }
    }

    // --- Réglages du calque actif (non destructif, façon Photoshop/GIMP) ---
    ui.separator();
    let active = app.doc.active_layer;
    let layer = &mut app.doc.layers[active];
    ui.label(t("Calque actif :", "Active layer:"));
    ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
    // Verrouillage granulaire (audit point 28) : en plus du verrou global
    // (icône cadenas ci-dessus, tout ou rien), ces deux-là restent
    // indépendants et cumulables — verrouiller la position seule permet par
    // exemple de peindre sur un calque de fond sans risquer de le décaler
    // par erreur, sans passer par le verrou global qui bloquerait aussi la
    // peinture.
    ui.horizontal(|ui| {
        ui.checkbox(&mut layer.lock_position, t("🔒 Position", "🔒 Position")).on_hover_text(t(
            "Bloque le glisser-déplacer des éléments de ce calque (peinture/édition de contenu toujours possibles)",
            "Blocks drag-moving elements on this layer (painting/editing content still allowed)",
        ));
        ui.checkbox(&mut layer.lock_alpha, t("🔒 Transparence", "🔒 Transparency")).on_hover_text(t(
            "Peindre ne peut plus rendre opaque un pixel transparent, ni la gomme en rendre un transparent",
            "Painting can no longer make a transparent pixel opaque, nor can erasing make one transparent",
        ));
    });
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
    if let Some(mut adj) = layer.adjustment.clone() {
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
                ui.selectable_value(&mut adj, Adjustment::default_distortion(), t("Distorsion", "Distortion"));
                ui.selectable_value(
                    &mut adj,
                    Adjustment::default_chromatic_aberration(),
                    t("Aberration chromatique", "Chromatic aberration"),
                );
                ui.selectable_value(&mut adj, Adjustment::default_motion_blur(), t("Flou de mouvement", "Motion blur"));
                ui.selectable_value(&mut adj, Adjustment::default_bokeh(), t("Bokeh", "Bokeh"));
                ui.selectable_value(&mut adj, Adjustment::default_duotone(), t("Duotone", "Duotone"));
                ui.selectable_value(&mut adj, Adjustment::default_arc_warp(), t("Warp : Arc", "Warp: Arc"));
                ui.separator();
                ui.selectable_value(&mut adj, Adjustment::default_exposure(), t("Exposition", "Exposure"));
                ui.selectable_value(&mut adj, Adjustment::default_vibrance(), t("Vibrance", "Vibrance"));
                ui.selectable_value(&mut adj, Adjustment::default_white_balance(), t("Balance des blancs", "White balance"));
                ui.selectable_value(&mut adj, Adjustment::default_denoise(), t("Réduction de bruit", "Noise reduction"));
                ui.selectable_value(&mut adj, Adjustment::default_gaussian_blur(), t("Flou gaussien", "Gaussian blur"));
                ui.separator();
                ui.selectable_value(&mut adj, Adjustment::default_pixelate(), t("Pixelisation", "Pixelate"));
                ui.selectable_value(&mut adj, Adjustment::default_halftone(), t("Halftone", "Halftone"));
                ui.selectable_value(&mut adj, Adjustment::default_wave(), t("Warp : Vague", "Warp: Wave"));
                ui.selectable_value(&mut adj, Adjustment::default_sphere(), t("Warp : Sphère", "Warp: Sphere"));
                ui.selectable_value(&mut adj, Adjustment::default_vortex(), t("Warp : Tourbillon", "Warp: Vortex"));
                ui.selectable_value(&mut adj, Adjustment::default_radial_blur(), t("Flou radial", "Radial blur"));
                ui.selectable_value(&mut adj, Adjustment::default_vignette(), t("Vignette", "Vignette"));
                ui.selectable_value(
                    &mut adj,
                    Adjustment::default_channel_mixer_bw(),
                    t("Mixeur de canaux N&B", "Channel mixer B&W"),
                );
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
            Adjustment::CurvesFree { master, r, g, b } => {
                curves_free_editor(ui, master, r, g, b);
            }
            Adjustment::Distortion { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Quantité", "Amount"));
                    ui.add(egui::Slider::new(amount, -1.0..=1.0))
                        .on_hover_text(t("Positif = bombé, négatif = creusé", "Positive = bulge, negative = pinch"));
                });
            }
            Adjustment::ChromaticAberration { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Intensité", "Strength"));
                    ui.add(egui::Slider::new(amount, 0.0..=1.0));
                });
            }
            Adjustment::MotionBlur { angle, distance } => {
                ui.horizontal(|ui| {
                    ui.label(t("Angle", "Angle"));
                    ui.add(egui::Slider::new(angle, -180.0..=180.0).suffix("°"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Distance", "Distance"));
                    ui.add(egui::Slider::new(distance, 0.0..=60.0).suffix(" px"));
                });
            }
            Adjustment::Bokeh { radius, boost } => {
                ui.horizontal(|ui| {
                    ui.label(t("Rayon", "Radius"));
                    ui.add(egui::Slider::new(radius, 0.0..=40.0).suffix(" px"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Hautes lumières", "Highlights"));
                    ui.add(egui::Slider::new(boost, 0.0..=1.0));
                });
            }
            Adjustment::Duotone { shadow, highlight } => {
                ui.horizontal(|ui| {
                    ui.label(t("Ombres", "Shadows"));
                    ui.color_edit_button_srgb(shadow);
                    ui.label(t("Lumières", "Highlights"));
                    ui.color_edit_button_srgb(highlight);
                });
            }
            Adjustment::ArcWarp { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Quantité", "Amount"));
                    ui.add(egui::Slider::new(amount, -1.0..=1.0))
                        .on_hover_text(t("Positif = bombé vers le haut, négatif = vers le bas", "Positive = bulges upward, negative = downward"));
                });
            }
            Adjustment::Exposure { ev } => {
                ui.horizontal(|ui| {
                    ui.label(t("Exposition", "Exposure"));
                    ui.add(egui::Slider::new(ev, -3.0..=3.0).suffix(" EV"));
                });
            }
            Adjustment::Vibrance { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Vibrance", "Vibrance"));
                    ui.add(egui::Slider::new(amount, -1.0..=1.0));
                });
            }
            Adjustment::WhiteBalance { temp, tint } => {
                ui.horizontal(|ui| {
                    ui.label(t("Température", "Temperature"));
                    ui.add(egui::Slider::new(temp, -1.0..=1.0))
                        .on_hover_text(t("Positif = plus chaud (orange), négatif = plus froid (bleu)", "Positive = warmer (orange), negative = cooler (blue)"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Teinte", "Tint"));
                    ui.add(egui::Slider::new(tint, -1.0..=1.0))
                        .on_hover_text(t("Positif = magenta, négatif = vert", "Positive = magenta, negative = green"));
                });
            }
            Adjustment::Denoise { strength } => {
                ui.horizontal(|ui| {
                    ui.label(t("Intensité", "Strength"));
                    ui.add(egui::Slider::new(strength, 0.0..=1.0));
                });
            }
            Adjustment::GaussianBlur { radius } => {
                ui.horizontal(|ui| {
                    ui.label(t("Rayon", "Radius"));
                    ui.add(egui::Slider::new(radius, 0.0..=40.0).suffix(" px"));
                });
            }
            Adjustment::Pixelate { block } => {
                ui.horizontal(|ui| {
                    ui.label(t("Taille du bloc", "Block size"));
                    ui.add(egui::Slider::new(block, 1.0..=64.0).suffix(" px"));
                });
            }
            Adjustment::Halftone { cell, angle } => {
                ui.horizontal(|ui| {
                    ui.label(t("Taille de cellule", "Cell size"));
                    ui.add(egui::Slider::new(cell, 2.0..=40.0).suffix(" px"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Angle", "Angle"));
                    ui.add(egui::Slider::new(angle, 0.0..=90.0).suffix("°"));
                });
            }
            Adjustment::Wave { amplitude, wavelength } => {
                ui.horizontal(|ui| {
                    ui.label(t("Amplitude", "Amplitude"));
                    ui.add(egui::Slider::new(amplitude, -60.0..=60.0).suffix(" px"));
                });
                ui.horizontal(|ui| {
                    ui.label(t("Longueur d'onde", "Wavelength"));
                    ui.add(egui::Slider::new(wavelength, 4.0..=200.0).suffix(" px"));
                });
            }
            Adjustment::Sphere { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Quantité", "Amount"));
                    ui.add(egui::Slider::new(amount, -1.0..=1.0));
                });
            }
            Adjustment::Vortex { angle } => {
                ui.horizontal(|ui| {
                    ui.label(t("Angle", "Angle"));
                    ui.add(egui::Slider::new(angle, -180.0..=180.0).suffix("°"));
                });
            }
            Adjustment::RadialBlur { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Intensité", "Strength"));
                    ui.add(egui::Slider::new(amount, 0.0..=1.0));
                });
            }
            Adjustment::Vignette { amount } => {
                ui.horizontal(|ui| {
                    ui.label(t("Intensité", "Strength"));
                    ui.add(egui::Slider::new(amount, 0.0..=1.0));
                });
            }
            Adjustment::ChannelMixerBw { r, g, b } => {
                ui.horizontal(|ui| {
                    ui.label(t("Rouge", "Red"));
                    ui.add(egui::Slider::new(r, -2.0..=2.0));
                    ui.label(t("Vert", "Green"));
                    ui.add(egui::Slider::new(g, -2.0..=2.0));
                    ui.label(t("Bleu", "Blue"));
                    ui.add(egui::Slider::new(b, -2.0..=2.0));
                });
            }
            Adjustment::Preset(_) => {}
        }
        layer.adjustment = Some(adj);
    }

    // Calque de remplissage (Sprint I.1) : uni ou dégradé, édité directement
    // ici comme les calques d'ajustement — pas de fenêtre séparée.
    if let Some(mut fill) = layer.fill.clone() {
        use crate::model::{FillKind, Gradient};
        ui.horizontal(|ui| {
            ui.label(t("Remplissage", "Fill"));
            ui.label(fill.label());
        });
        match &mut fill {
            FillKind::Solid(color) => {
                ui.horizontal(|ui| {
                    ui.label(t("Couleur", "Color"));
                    ui.color_edit_button_srgba_unmultiplied(color);
                });
            }
            FillKind::Linear(g) | FillKind::Radial(g) => {
                let Gradient { stops, .. } = g;
                ui.horizontal(|ui| {
                    for (_, c) in stops.iter_mut() {
                        let mut rgb = [c[0], c[1], c[2]];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            c[0] = rgb[0];
                            c[1] = rgb[1];
                            c[2] = rgb[2];
                        }
                    }
                });
            }
        }
        layer.fill = Some(fill);
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

    // Styles de calque (Sprint 6.1) : ombre portée, contour, lueur — non
    // destructifs, dérivés de l'alpha du calque au rendu (voir
    // `render::compositor::apply_layer_styles`). Ré-emprunte `layer` (plutôt
    // que réutiliser la liaison plus haut) : le bloc du masque, juste
    // au-dessus, emprunte `app` dans une fermeture — NLL a besoin que
    // l'emprunt précédent de `layer` soit terminé avant ce point.
    let layer = &mut app.doc.layers[active];
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(t("Styles", "Styles"));
        egui::ComboBox::from_id_salt("layer_style_add")
            .selected_text(t("Ajouter…", "Add…"))
            .show_ui(ui, |ui| {
                use crate::model::LayerStyle;
                for (label, make) in [
                    (t("Ombre portée", "Drop shadow"), LayerStyle::default_drop_shadow as fn() -> LayerStyle),
                    (t("Contour", "Stroke"), LayerStyle::default_stroke),
                    (t("Lueur externe", "Outer glow"), LayerStyle::default_outer_glow),
                    (t("Lueur interne", "Inner glow"), LayerStyle::default_inner_glow),
                ] {
                    if ui.button(label).clicked() {
                        layer.styles.push(make());
                    }
                }
            });
    });
    {
        use crate::model::LayerStyle;
        let mut to_remove: Option<usize> = None;
        for (i, style) in layer.styles.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(style.label());
                if ui.small_button("🗑").on_hover_text(t("Supprimer ce style", "Remove this style")).clicked() {
                    to_remove = Some(i);
                }
            });
            match style {
                LayerStyle::DropShadow { color, offset, blur } => {
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba_unmultiplied(color);
                        ui.label(t("Décalage X", "Offset X"));
                        ui.add(egui::DragValue::new(&mut offset.0).speed(0.5));
                        ui.label(t("Y", "Y"));
                        ui.add(egui::DragValue::new(&mut offset.1).speed(0.5));
                        ui.label(t("Flou", "Blur"));
                        ui.add(egui::Slider::new(blur, 0.0..=30.0));
                    });
                }
                LayerStyle::Stroke { color, width } => {
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba_unmultiplied(color);
                        ui.label(t("Épaisseur", "Width"));
                        ui.add(egui::Slider::new(width, 0.0..=20.0));
                    });
                }
                LayerStyle::Glow { color, blur, .. } => {
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba_unmultiplied(color);
                        ui.label(t("Flou", "Blur"));
                        ui.add(egui::Slider::new(blur, 0.0..=30.0));
                    });
                }
            }
        }
        if let Some(i) = to_remove {
            layer.styles.remove(i);
        }
    }

    // Les actions sur les ÉLÉMENTS sélectionnés (aligner/rogner/ordre)
    // vivaient ici avant UX-3.4 — déplacées dans la barre d'options de
    // l'outil Sélection (`toolbar::options_row`), qui n'apparaît que
    // pertinent : ce panneau ne porte plus que des actions sur des calques
    // (constat C6, UX_SPRINTS.md).

    // --- Liste des éléments du calque actif (voir / sélectionner) -----------
    ui.separator();
    ui.label(format!("{} :", t("Éléments du calque", "Layer elements")));

    // Liste virtualisée (show_rows) : seules les lignes visibles sont
    // construites → reste rapide même avec des milliers d'éléments.
    //
    // Sélection multiple (⇧ = plage depuis la dernière ancre, ⌘/Ctrl =
    // ajouter/retirer un élément, clic simple = remplacer) : réutilise
    // `app.selection`, la même sélection multi-élément que le canevas
    // (rectangle/lasso/baguette), ce qui donne accès aux actions déjà
    // câblées dessus (aligner/rogner/ordre/suréchantillonner dans
    // `toolbar::selection_actions`) sans dupliquer ce mécanisme.
    let mut pick: Option<(usize, u64)> = None;
    let active = app.doc.active_layer;
    let (nt, ni, ns) = {
        let l = &app.doc.layers[active];
        (l.texts.len(), l.images.len(), l.strokes.len())
    };
    let total = nt + ni + ns;
    // Résout l'id d'une ligne par index, indépendamment de ce qui est
    // effectivement construit par `show_rows` — nécessaire pour la plage ⇧,
    // dont l'ancre peut être hors de la zone visible (virtualisation).
    let id_at_row = |l: &Layer, row: usize| -> u64 {
        if row < nt {
            l.texts[nt - 1 - row].id
        } else if row < nt + ni {
            l.images[ni - 1 - (row - nt)].id
        } else {
            l.strokes[ns - 1 - (row - nt - ni)].id
        }
    };
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    egui::ScrollArea::vertical().max_height(220.0).show_rows(ui, row_h, total, |ui, range| {
        let l = &app.doc.layers[active];
        for row in range {
            // Ordre d'affichage : textes, puis images, puis traits (du dessus).
            let (id, lbl) = if row < nt {
                let txt = &l.texts[nt - 1 - row];
                (txt.id, short(&txt.text).to_string())
            } else if row < nt + ni {
                let im = &l.images[ni - 1 - (row - nt)];
                // Objet intelligent léger (audit_sprint_xx.md B.3) : `size`
                // (affichage) est toujours découplé de `w`/`h` (résolution
                // source conservée telle quelle, jamais rééchantillonnée par
                // un redimensionnement) — d'où l'indicateur : tant que la
                // taille affichée reste sous la résolution source, on peut
                // encore agrandir sans perte ; au-delà, l'image est
                // suréchantillonnée (badge « ↑ » pour le signaler).
                let native = format!("{}×{}", im.w, im.h);
                let shown = (im.size.0.round() as i64, im.size.1.round() as i64);
                let badge = if shown.0 > im.w as i64 || shown.1 > im.h as i64 {
                    format!(" ⚠ {}×{} > {native}", shown.0, shown.1)
                } else {
                    format!(" · {native}")
                };
                (im.id, format!("{}{badge}", t("Image", "Image")))
            } else {
                let s = &l.strokes[ns - 1 - (row - nt - ni)];
                let kind = if s.fill { t("forme", "shape") } else { t("trait", "stroke") };
                (s.id, format!("{kind} ({} pts)", s.points.len()))
            };
            if ui
                .selectable_label(app.selection.contains(&id), lbl)
                .on_hover_text(t(
                    "Clic : sélectionner · ⇧+clic : plage · ⌘/Ctrl+clic : ajouter/retirer",
                    "Click: select · Shift+click: range · Cmd/Ctrl+click: add/remove",
                ))
                .clicked()
            {
                pick = Some((row, id));
            }
        }
    });
    if let Some((row, id)) = pick {
        let (shift, cmd) = ui.input(|i| (i.modifiers.shift, i.modifiers.command || i.modifiers.ctrl));
        if shift {
            // Plage depuis la dernière ancre (clic simple) jusqu'à cette ligne —
            // s'il n'y a pas encore d'ancre, se comporte comme un ajout simple.
            let anchor = app.layer_elements_anchor.unwrap_or(row);
            let (lo, hi) = (anchor.min(row), anchor.max(row));
            let l = &app.doc.layers[active];
            for r in lo..=hi {
                app.selection.insert(id_at_row(l, r));
            }
        } else if cmd {
            if !app.selection.remove(&id) {
                app.selection.insert(id);
            }
            app.layer_elements_anchor = Some(row);
        } else {
            app.selection.clear();
            app.selection.insert(id);
            app.layer_elements_anchor = Some(row);
        }
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

/// Éditeur de courbes libres (Sprint S, point 73) : sélecteur de canal
/// (RVB composite / R / V / B) + petit canevas interactif — glisser un point
/// le déplace, cliquer sur le vide en ajoute un, clic droit en retire un
/// (les deux extrêmes restent). L'état transitoire (canal actif, point en
/// cours de glissé) vit dans la mémoire temporaire d'egui, pas dans le
/// document.
fn curves_free_editor(
    ui: &mut Ui,
    master: &mut Vec<(u8, u8)>,
    r: &mut Vec<(u8, u8)>,
    g: &mut Vec<(u8, u8)>,
    b: &mut Vec<(u8, u8)>,
) {
    let chan_id = ui.id().with("curves_channel");
    let mut chan: u8 = ui.ctx().data_mut(|d| *d.get_temp_mut_or(chan_id, 0u8));
    ui.horizontal(|ui| {
        for (i, label) in [t("RVB", "RGB"), "R", t("V", "G"), "B"].into_iter().enumerate() {
            if ui.selectable_label(chan == i as u8, label).clicked() {
                chan = i as u8;
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(chan_id, chan));
    let points = match chan {
        1 => r,
        2 => g,
        3 => b,
        _ => master,
    };

    let (resp, painter) = ui.allocate_painter(Vec2::new(200.0, 150.0), Sense::click_and_drag());
    let rect = resp.rect;
    let to_screen = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + x / 255.0 * rect.width(),
            rect.bottom() - y / 255.0 * rect.height(),
        )
    };
    let to_curve = |p: egui::Pos2| {
        (
            ((p.x - rect.left()) / rect.width() * 255.0).clamp(0.0, 255.0),
            ((rect.bottom() - p.y) / rect.height() * 255.0).clamp(0.0, 255.0),
        )
    };

    // Fond, quadrillage aux quarts, diagonale identité.
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let grid = egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    for q in 1..4 {
        let f = q as f32 / 4.0;
        painter.line_segment([to_screen(255.0 * f, 0.0), to_screen(255.0 * f, 255.0)], grid);
        painter.line_segment([to_screen(0.0, 255.0 * f), to_screen(255.0, 255.0 * f)], grid);
    }
    painter.line_segment([to_screen(0.0, 0.0), to_screen(255.0, 255.0)], grid);

    // Interaction : glissé d'un point existant, ajout au clic, retrait au
    // clic droit.
    let drag_id = ui.id().with(("curves_drag", chan));
    let nearest = |points: &Vec<(u8, u8)>, p: egui::Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| (i, (to_screen(x as f32, y as f32) - p).length()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };
    if resp.drag_started() {
        if let Some(p) = resp.interact_pointer_pos() {
            let idx = nearest(points, p).filter(|&(_, d)| d <= 10.0).map(|(i, _)| i);
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, idx));
        }
    }
    if resp.dragged() {
        let idx: Option<usize> = ui.ctx().data_mut(|d| *d.get_temp_mut_or(drag_id, None));
        if let (Some(i), Some(p)) = (idx, resp.interact_pointer_pos()) {
            if i < points.len() {
                let (cx, cy) = to_curve(p);
                // L'abscisse reste strictement entre les points voisins
                // (courbe = fonction de l'entrée, jamais deux sorties pour
                // une même entrée).
                let lo = if i == 0 { 0.0 } else { points[i - 1].0 as f32 + 1.0 };
                let hi = if i + 1 == points.len() { 255.0 } else { points[i + 1].0 as f32 - 1.0 };
                points[i] = (cx.clamp(lo, hi).round() as u8, cy.round() as u8);
            }
        }
    }
    if resp.drag_stopped() {
        ui.ctx().data_mut(|d| d.insert_temp(drag_id, None::<usize>));
    }
    if resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let on_point = nearest(points, p).filter(|&(_, d)| d <= 10.0).is_some();
            if !on_point && points.len() < 16 {
                let (cx, cy) = to_curve(p);
                points.push((cx.round() as u8, cy.round() as u8));
                points.sort_by_key(|pt| pt.0);
                points.dedup_by_key(|pt| pt.0);
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            if points.len() > 2 {
                if let Some((i, d)) = nearest(points, p) {
                    if d <= 10.0 {
                        points.remove(i);
                    }
                }
            }
        }
    }

    // Courbe (LUT rééchantillonnée) puis points de contrôle par-dessus.
    let lut = crate::tools::filter::points_lut(points);
    let curve_color = match chan {
        1 => egui::Color32::from_rgb(220, 60, 60),
        2 => egui::Color32::from_rgb(60, 180, 60),
        3 => egui::Color32::from_rgb(70, 110, 230),
        _ => ui.visuals().text_color(),
    };
    let samples: Vec<egui::Pos2> = (0..=64).map(|i| {
        let x = i as f32 / 64.0 * 255.0;
        to_screen(x, lut[(x.round() as usize).min(255)] as f32)
    }).collect();
    painter.add(egui::Shape::line(samples, egui::Stroke::new(1.5_f32, curve_color)));
    for &(x, y) in points.iter() {
        let c = to_screen(x as f32, y as f32);
        painter.circle_filled(c, 3.5, egui::Color32::WHITE);
        painter.circle_stroke(c, 3.5, egui::Stroke::new(1.5_f32, curve_color));
    }
    ui.label(t(
        "Clic : ajouter · glisser : déplacer · clic droit : retirer",
        "Click: add · drag: move · right-click: remove",
    ));
}
