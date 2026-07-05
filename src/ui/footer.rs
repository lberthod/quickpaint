//! Barre d'état (footer) : infos live sur l'outil, le document et l'aide.

use crate::app::PaintApp;
use crate::i18n::t;
use crate::tools::ActiveTool;
use egui::{Align, Color32, Layout, Ui};

pub fn show(ui: &mut Ui, app: &mut PaintApp) {
    let (tool_name, size) = match app.active_tool {
        ActiveTool::Select => (t("↖ Sélection", "↖ Select"), app.brush.width),
        ActiveTool::Brush => (t("Pinceau", "Brush"), app.brush.width),
        ActiveTool::Eraser => (t("Gomme", "Eraser"), app.eraser.width),
        ActiveTool::PixelBrush => (t("Pinceau pixel", "Pixel brush"), app.brush.width),
        ActiveTool::Airbrush => (t("Aérographe", "Airbrush"), app.brush.width),
        ActiveTool::PixelEraser => (t("Gomme pixel", "Pixel eraser"), app.eraser.width),
        ActiveTool::CloneStamp => (t("Tampon de clonage", "Clone stamp"), app.brush.width),
        ActiveTool::Healing => (t("Correcteur", "Healing brush"), app.brush.width),
        ActiveTool::Line => (t("Ligne", "Line"), app.brush.width),
        ActiveTool::Arrow => (t("Flèche", "Arrow"), app.brush.width),
        ActiveTool::Rectangle => (t("▭ Rectangle", "▭ Rectangle"), app.brush.width),
        ActiveTool::Ellipse => (t("⬭ Ellipse", "⬭ Ellipse"), app.brush.width),
        ActiveTool::Polygon => (t("⬡ Polygone", "⬡ Polygon"), app.brush.width),
        ActiveTool::Star => (t("★ Étoile", "★ Star"), app.brush.width),
        ActiveTool::Text => (t("Texte", "Text"), app.text_size),
        ActiveTool::Pen => (t("Plume", "Pen"), app.brush.width),
        ActiveTool::Bucket => (t("Pot", "Bucket"), app.brush.width),
        ActiveTool::Eyedropper => (t("Pipette", "Eyedropper"), app.brush.width),
        ActiveTool::Pan => (t("Main", "Hand"), app.brush.width),
        ActiveTool::Cutout => (t("Détourage", "Cutout"), app.brush.width),
        ActiveTool::Dodge => (t("Densité -", "Dodge"), app.brush.width),
        ActiveTool::Burn => (t("Densité +", "Burn"), app.brush.width),
        ActiveTool::Saturate => (t("Éponge (saturer)", "Sponge (saturate)"), app.brush.width),
        ActiveTool::Desaturate => (t("Éponge (désaturer)", "Sponge (desaturate)"), app.brush.width),
        ActiveTool::Blur => (t("Flou localisé", "Local blur"), app.brush.width),
        ActiveTool::Sharpen => (t("Netteté localisée", "Local sharpen"), app.brush.width),
        ActiveTool::Smudge => (t("Estompe", "Smudge"), app.brush.width),
        ActiveTool::Measure => (t("Règle", "Measure"), app.brush.width),
        ActiveTool::Symmetry => (t("Miroir", "Symmetry"), app.brush.width),
        ActiveTool::Gradient => (t("Dégradé", "Gradient"), app.brush.width),
    };
    let layer = app.doc.active_layer;
    let strokes = app.doc.layers[layer].strokes.len();
    let (w, h) = app.doc.size;

    ui.horizontal(|ui| {
        ui.label(format!("{tool_name} · {size:.0} px"));
        ui.separator();
        ui.label(format!("{} : {strokes}", t("Traits", "Strokes")));
        ui.separator();
        ui.label(format!("{} {} · {w}×{h}", t("Calque", "Layer"), layer + 1));
        ui.separator();
        zoom_controls(ui, app);

        // Zone de droite : message de statut (coloré par sévérité — UX-1.2),
        // sinon l'aide raccourcis. Repliée en icône ⓘ si la fenêtre est trop
        // étroite pour afficher le pavé d'aide en entier : les deux blocs se
        // chevauchaient et devenaient illisibles avant ce correctif (constat
        // C1, UX_SPRINTS.md).
        let remaining = ui.available_width();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &app.status {
            Some(msg) => {
                let color = if app.status_error {
                    Color32::from_rgb(200, 60, 55) // échec
                } else {
                    Color32::from_rgb(40, 130, 60) // succès / information
                };
                ui.colored_label(color, msg);
            }
            None => {
                let hint = t(
                    "V/B/E/L/R/O/T/I/H · Suppr efface · ⌘D duplique · Espace=pan · ⌘±/0 zoom · ⌘Z · ⌘N/O/S/E",
                    "V/B/E/L/R/O/T/I/H · Del erases · ⌘D duplicate · Space=pan · ⌘±/0 zoom · ⌘Z · ⌘N/O/S/E",
                );
                // Largeur de police par défaut ≈ 6px/caractère : pas une
                // mesure exacte (indisponible avant le rendu du label), mais
                // un seuil prudent qui évite le chevauchement à toute
                // largeur de fenêtre usuelle.
                if remaining > hint.len() as f32 * 6.0 {
                    ui.label(hint);
                } else {
                    ui.label(egui_phosphor::regular::INFO).on_hover_text(hint);
                }
            }
        });
    });
}

/// Contrôles de zoom persistants (UX-4.1) : `−` / pourcentage cliquable
/// (remet à 100 %) / `+`, plus un bouton « Ajuster ». Avant, le zoom n'était
/// réglable qu'en ouvrant le menu Vue (2 clics) ou au clavier — friction
/// disproportionnée pour un geste aussi fréquent, notamment sur l'origine
/// tactile du projet (constat C4, UX_SPRINTS.md). Le menu Vue garde les
/// mêmes actions (`zoom_in`/`zoom_out`/`reset_view`/`fit_view`) : aucune
/// régression, juste un deuxième accès plus rapide.
fn zoom_controls(ui: &mut Ui, app: &mut PaintApp) {
    if ui.small_button("−").on_hover_text(t("Zoom arrière", "Zoom out")).clicked() {
        app.zoom_out();
    }
    if ui
        .small_button(format!("{:.0} %", app.zoom * 100.0))
        .on_hover_text(t("Réinitialiser le zoom (100 %)", "Reset zoom (100%)"))
        .clicked()
    {
        app.reset_view();
    }
    if ui.small_button("+").on_hover_text(t("Zoom avant", "Zoom in")).clicked() {
        app.zoom_in();
    }
    if ui
        .small_button(egui_phosphor::regular::FRAME_CORNERS)
        .on_hover_text(t("Ajuster à la fenêtre", "Fit to window"))
        .clicked()
    {
        app.fit_view();
    }
}
