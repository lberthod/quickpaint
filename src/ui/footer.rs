//! Barre d'état (footer) : infos live sur l'outil, le document et l'aide.

use crate::app::PaintApp;
use crate::tools::ActiveTool;
use egui::{Align, Layout, Ui};

pub fn show(ui: &mut Ui, app: &PaintApp) {
    let (tool_name, size) = match app.active_tool {
        ActiveTool::Select => ("↖ Sélection", app.brush.width),
        ActiveTool::Brush => ("🖌 Pinceau", app.brush.width),
        ActiveTool::Eraser => ("🩹 Gomme", app.eraser.width),
        ActiveTool::Line => ("📏 Ligne", app.brush.width),
        ActiveTool::Arrow => ("➹ Flèche", app.brush.width),
        ActiveTool::Rectangle => ("▭ Rectangle", app.brush.width),
        ActiveTool::Ellipse => ("⬭ Ellipse", app.brush.width),
        ActiveTool::Polygon => ("⬡ Polygone", app.brush.width),
        ActiveTool::Star => ("★ Étoile", app.brush.width),
        ActiveTool::Text => ("🔤 Texte", app.text_size),
        ActiveTool::Pen => ("✒ Plume", app.brush.width),
        ActiveTool::Bucket => ("🪣 Pot", app.brush.width),
        ActiveTool::Eyedropper => ("💉 Pipette", app.brush.width),
        ActiveTool::Pan => ("✋ Main", app.brush.width),
    };
    let layer = app.doc.active_layer;
    let strokes = app.doc.layers[layer].strokes.len();
    let (w, h) = app.doc.size;

    ui.horizontal(|ui| {
        ui.label(format!("{tool_name} · {size:.0} px"));
        ui.separator();
        ui.label(format!("Traits : {strokes}"));
        ui.separator();
        ui.label(format!("Calque {} · {w}×{h}", layer + 1));
        ui.separator();
        ui.label(format!("Zoom {:.0} %", app.zoom * 100.0));

        // À droite : message d'état si présent, sinon l'aide raccourcis.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &app.status {
            Some(msg) => {
                ui.colored_label(egui::Color32::from_rgb(40, 130, 60), msg);
            }
            None => {
                ui.label("V/B/E/L/R/O/T/I/H · Suppr efface · ⌘D duplique · Espace=pan · ⌘±/0 zoom · ⌘Z · ⌘N/O/S/E");
            }
        });
    });
}
