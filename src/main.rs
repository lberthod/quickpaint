//! QuickPaint — éditeur de dessin tactile macOS en Rust (egui). Point d'entrée.
//!
//! Auteur : Loïc Berthod — https://github.com/lberthod/quickpaint
//!
//! Architecture en couches : `model` (données) / `input` (capture du geste) /
//! `render` (pixels) / `history` (undo/redo) / `ui` (palette).

mod app;
mod export;
mod fonts;
mod history;
mod i18n;
mod icon;
mod input;
mod keybindings;
mod model;
mod project;
mod render;
mod svg;
mod tools;
mod ui;

use app::PaintApp;

fn main() -> eframe::Result<()> {
    i18n::init();

    // Mode utilitaire : `quickpaint --dump-icon <fichier.png> <taille>` écrit
    // l'icône en PNG (sert à générer l'.icns du bundle .app), puis quitte.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--dump-icon" {
        let size: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1024);
        let (rgba, w, h) = icon::rgba_at(size);
        let img = image::RgbaImage::from_raw(w, h, rgba).expect("icône");
        img.save(&args[2]).expect("écriture PNG");
        println!("Icône écrite : {} ({size}px)", args[2]);
        return Ok(());
    }

    let options = eframe::NativeOptions {
        // MSAA 4× : anti-aliasing matériel des traits/formes (roadmap #4),
        // sans surcoût par frame ni invalidation du cache de maillage.
        multisampling: 4,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_title("QuickPaint")
            .with_icon(std::sync::Arc::new(icon::app_icon())),
        ..Default::default()
    };

    eframe::run_native(
        "QuickPaint",
        options,
        Box::new(|cc| Ok(Box::new(PaintApp::new(cc)))),
    )
}
