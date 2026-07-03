//! Barre d'état (footer) : infos live sur l'outil, le document et l'aide.

use crate::app::PaintApp;
use crate::i18n::t;
use crate::tools::ActiveTool;
use egui::{Align, Layout, Ui};

pub fn show(ui: &mut Ui, app: &PaintApp) {
    let (tool_name, size) = match app.active_tool {
        ActiveTool::Select => (t("↖ Sélection", "↖ Select"), app.brush.width),
        ActiveTool::Brush => (t("Pinceau", "Brush"), app.brush.width),
        ActiveTool::Eraser => (t("Gomme", "Eraser"), app.eraser.width),
        ActiveTool::PixelBrush => (t("Pinceau pixel", "Pixel brush"), app.brush.width),
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
        ui.label(format!("{} {:.0} %", t("Zoom", "Zoom"), app.zoom * 100.0));

        // À droite : message d'état si présent, sinon l'aide raccourcis.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &app.status {
            Some(msg) => {
                ui.colored_label(egui::Color32::from_rgb(40, 130, 60), msg);
            }
            None => {
                ui.label(t(
                    "V/B/E/L/R/O/T/I/H · Suppr efface · ⌘D duplique · Espace=pan · ⌘±/0 zoom · ⌘Z · ⌘N/O/S/E",
                    "V/B/E/L/R/O/T/I/H · Del erases · ⌘D duplicate · Space=pan · ⌘±/0 zoom · ⌘Z · ⌘N/O/S/E",
                ));
            }
        });
    });
}
