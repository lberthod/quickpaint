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

    // Mode utilitaire : `quickpaint --sandbox-selftest` (SPRINTS.md 13.9)
    // exerce sans interface les sous-systèmes qui touchent le disque ou un
    // sous-processus, pour vérifier avant une soumission App Store qu'ils
    // survivent à l'App Sandbox. Voir packaging/SANDBOX_NOTES.md pour la
    // procédure (à lancer sur le binaire *dans son bundle*, signé avec
    // packaging/QuickPaint.entitlements — hors d'un .app, l'initialisation
    // du sandbox lui-même échoue, indépendamment du code applicatif).
    if args.len() >= 2 && args[1] == "--sandbox-selftest" {
        sandbox_selftest();
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

/// Voir l'appel dans `main` : diagnostic App Sandbox, pas un chemin de
/// démarrage normal.
fn sandbox_selftest() {
    println!("[sandbox-selftest] langue détectée : {:?}", i18n::current());

    let mut mgr = fonts::FontManager::new();
    let names = mgr.family_names();
    println!("[sandbox-selftest] polices système énumérées : {}", names.len());

    // Le chargement des octets d'une police (`with_face_data`) est le point
    // le plus susceptible d'être bloqué par le sandbox : l'énumération peut
    // passer par CoreText (sandbox-safe) alors que la lecture du fichier
    // pourrait rouvrir le chemin en direct.
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let (mut ok, mut fail) = (0usize, 0usize);
    for name in names.iter().take(10) {
        if mgr.ensure_loaded(&ctx, name) {
            ok += 1;
        } else {
            fail += 1;
        }
    }
    println!("[sandbox-selftest] octets de police chargés : {ok} ok / {fail} échec (sur {} testées)", ok + fail);

    // Écriture/lecture disque au même endroit que settings.json — sous
    // sandbox, HOME est redirigé vers le conteneur de l'app ; ça doit
    // fonctionner sans entitlement particulier (contrairement aux polices
    // système, hors du conteneur). Fichier dédié, jamais `settings.json`
    // lui-même : ce diagnostic ne doit pas pouvoir écraser les préférences
    // réelles d'un utilisateur qui le lancerait par erreur sur l'app installée.
    match std::env::var("HOME") {
        Ok(home) => {
            let dir = std::path::PathBuf::from(home).join("Library/Application Support/QuickPaint");
            let probe = dir.join("sandbox-selftest.tmp");
            let ok = std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&probe, b"ok"))
                .and_then(|_| std::fs::read(&probe))
                .map(|d| d == b"ok")
                .unwrap_or(false);
            let _ = std::fs::remove_file(&probe);
            println!("[sandbox-selftest] écriture/lecture disque (Application Support) : {}", if ok { "ok" } else { "ÉCHEC" });
        }
        Err(_) => println!("[sandbox-selftest] écriture/lecture disque : ÉCHEC (HOME absent)"),
    }

    println!("[sandbox-selftest] terminé.");
}
