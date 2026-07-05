//! Barre de menu macOS native (`NSMenu`) — UIX_ANALYSE.md §2 C1 / Épic U1.
//!
//! `ui/toolbar.rs` garde ses `egui::menu_button` (Fichier/Calque/Sélection/
//! Vue/Aide) : ce module ajoute en parallèle ce qu'un utilisateur macOS
//! s'attend à trouver au même endroit que dans toute autre app AppKit — le
//! menu ⌘ à gauche du Dock (À propos, Services, Quitter) et un menu Édition
//! basique — sans réécrire le chrome existant (cf. risque documenté dans
//! UIX_ANALYSE.md §8).
//!
//! Annuler/Rétablir/Couper/Copier/Coller sont volontairement des `MenuItem`
//! ordinaires, pas les `PredefinedMenuItem` correspondants : ces derniers
//! envoient un sélecteur Cocoa (`copy:`, `undo:`...) au premier répondeur, ce
//! qui suppose une vraie `NSTextView`/`NSResponder` — le canevas dessiné par
//! egui/winit n'en est pas un, l'action serait silencieusement ignorée.
//! [`poll_events`] route donc les clics vers les mêmes méthodes que les
//! raccourcis clavier déjà câblés dans `app/mod.rs::handle_shortcuts`.

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

/// Identifiants des entrées « Édition » à faire correspondre aux méthodes de
/// [`crate::app::PaintApp`] lors du dépouillement des évènements de menu.
pub struct EditMenuIds {
    pub undo: MenuId,
    pub redo: MenuId,
    pub cut: MenuId,
    pub copy: MenuId,
    pub paste: MenuId,
}

/// Construit et installe le menu ⌘/Édition natif. À appeler une seule fois,
/// au démarrage — `Menu::init_for_nsapp` doit tourner sur le thread principal
/// (contrainte macOS de `muda`), donc depuis `PaintApp::new`.
pub fn install() -> EditMenuIds {
    let menu = Menu::new();

    // Métadonnées explicites : sans elles, `about(None, None)` délègue à
    // `orderFrontStandardAboutPanel` (le panneau système), qui lit le nom et
    // la version dans le `Info.plist` du bundle. Ce binaire n'est pas
    // empaqueté en `.app` (lancé directement en dev via `cargo run`, ou tout
    // simplement copié tel quel) : il n'y a pas de bundle, donc pas de
    // plist — AppKit tente quand même de construire les chaînes du panneau à
    // partir d'entrées absentes et plante (SIGSEGV dans
    // `CFStringCreateImmutableFunnel3`, adresse nulle). En fournissant nos
    // propres chaînes, `orderFrontStandardAboutPanelWithOptions` est utilisé
    // à la place et ne dépend d'aucune info de bundle.
    let about_meta = AboutMetadata {
        name: Some("QuickPaint".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        ..Default::default()
    };
    let app_menu = Submenu::new("QuickPaint", true);
    let _ = app_menu.append_items(&[
        &PredefinedMenuItem::about(None, Some(about_meta)),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);

    let undo = MenuItem::new("Annuler", true, Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyZ)));
    let redo = MenuItem::new(
        "Rétablir",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyZ)),
    );
    let cut = MenuItem::new("Couper", true, Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyX)));
    let copy = MenuItem::new("Copier", true, Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)));
    let paste = MenuItem::new("Coller", true, Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)));

    let edit_menu = Submenu::new("Édition", true);
    let _ = edit_menu.append_items(&[&undo, &redo, &PredefinedMenuItem::separator(), &cut, &copy, &paste]);

    let _ = menu.append_items(&[&app_menu, &edit_menu]);

    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();

    EditMenuIds {
        undo: undo.id().clone(),
        redo: redo.id().clone(),
        cut: cut.id().clone(),
        copy: copy.id().clone(),
        paste: paste.id().clone(),
    }
}

/// Dépouille les clics de menu accumulés depuis le dernier appel (non
/// bloquant) — à appeler à chaque frame depuis `PaintApp::update`.
pub fn poll_events() -> Vec<MenuId> {
    let mut ids = Vec::new();
    while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
        ids.push(event.id);
    }
    ids
}
