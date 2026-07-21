//! Dispatch clavier (raccourcis globaux, capture de raccourci personnalisé),
//! menu Édition natif macOS, et glisser-déposer de fichiers — extrait de
//! `app` en sous-module (sprint.md T3.4, suite de T3.1/T3.2/T3.3) : le gros
//! `match` d'évènements qui route vers les actions déjà définies ailleurs
//! dans `impl PaintApp`, pas une nouvelle logique.

use super::{t, PaintApp, ZMove};

impl PaintApp {
    /// Dépouille les clics du menu Édition natif (UIX_ANALYSE.md U1) et les
    /// route vers les mêmes méthodes que les raccourcis clavier
    /// (`handle_shortcuts`) — le menu ⌘ macOS n'est qu'une autre entrée vers
    /// les actions déjà existantes, pas un chemin d'exécution séparé.
    pub(super) fn handle_native_menu(&mut self) {
        let Some(ids) = &self.native_edit_menu else { return };
        let (undo, redo, cut, copy, paste) =
            (ids.undo.clone(), ids.redo.clone(), ids.cut.clone(), ids.copy.clone(), ids.paste.clone());
        for id in crate::native_menu::poll_events() {
            if id == undo {
                self.undo();
            } else if id == redo {
                self.redo();
            } else if id == cut {
                self.cut_selection();
            } else if id == copy {
                self.copy_selection();
            } else if id == paste && !self.paste_clipboard() {
                self.paste_image();
            }
        }
    }

    /// Glisser-déposer de fichiers (Sprint L.4) : `egui::Event::Dropped`
    /// (déjà géré nativement par `eframe`/`winit`), dispatché selon
    /// l'extension — `.psd` → nouveau document multi-calques (comme
    /// `import_psd`), `.json` → projet natif (comme `open_project`), tout le
    /// reste tenté comme image posée dans le document courant (comme
    /// `import_image`).
    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let paths: Vec<std::path::PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for path in paths {
            self.import_dropped_file(&path);
        }
    }

    /// Importe un seul fichier déposé, sans dialogue (chemin déjà connu).
    pub fn import_dropped_file(&mut self, path: &std::path::Path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "psd" => match crate::psd_import::import_psd(path) {
                Ok(doc) => {
                    self.apply_loaded(doc);
                    self.info(t("Fichier PSD importé.", "PSD file imported."));
                }
                Err(msg) => self.fail(format!("{} : {msg}", t("Impossible d'importer le PSD", "Couldn't import the PSD"))),
            },
            "json" => match crate::project::open_path(path) {
                Ok(doc) => {
                    self.apply_loaded(doc);
                    crate::i18n::push_recent_project(&path.display().to_string());
                    self.info(t("Projet ouvert.", "Project opened."));
                }
                Err(msg) => self.fail(format!("{} : {msg}", t("Impossible d'ouvrir le projet", "Couldn't open the project"))),
            },
            "svg" => match crate::svg_import::import_svg_file(path) {
                Ok(doc) => {
                    self.apply_loaded(doc);
                    self.info(t("Fichier SVG importé.", "SVG file imported."));
                }
                Err(msg) => self.fail(format!("{} : {msg}", t("Impossible d'importer le SVG", "Couldn't import the SVG"))),
            },
            _ => match crate::project::import_image_from_path(path) {
                Ok((w, h, rgba)) => {
                    self.place_image(w, h, rgba);
                    self.info(t("Image importée — déplacez-la (outil Sélection).", "Image imported — move it (Select tool)."));
                }
                Err(msg) => self.fail(format!("{} : {msg}", t("Image refusée", "Image rejected"))),
            },
        }
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Capture d'un nouveau raccourci en cours (panneau de préférences,
        // Sprint 7.2) : la prochaine touche pressée devient le raccourci de
        // l'action visée, prioritaire sur tout le reste.
        // Capture d'une commande ⌘ rebindable (Sprint R, point 97) : même
        // principe que les outils, mais la combinaison enregistrée retient
        // aussi l'état de ⇧ (⌘E vs ⌘⇧E).
        if let Some(action) = self.capturing_cmd_shortcut {
            let captured = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, modifiers.shift)),
                _ => None,
            }));
            if let Some((key, shift)) = captured {
                if key != egui::Key::Escape && !self.keybindings.set_cmd(action, key, shift) {
                    self.info(t(
                        "Touche réservée à une convention macOS (⌘Z/⌘C/⌘V/⌘X/⌘S/⌘O/⌘N/⌘[/⌘]).",
                        "Key reserved by a macOS convention (⌘Z/⌘C/⌘V/⌘X/⌘S/⌘O/⌘N/⌘[/⌘]).",
                    ));
                }
                self.capturing_cmd_shortcut = None;
            }
            return;
        }
        if let Some(action) = self.capturing_shortcut {
            let captured = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, .. } => Some(*key),
                _ => None,
            }));
            if let Some(key) = captured {
                if key == egui::Key::Escape {
                    // Échap annule la capture sans rien changer.
                } else {
                    self.keybindings.set(action, key);
                }
                self.capturing_shortcut = None;
            }
            return;
        }
        let typing = ctx.wants_keyboard_input();
        // Les actions ouvrant une boîte de dialogue native sont exécutées
        // APRÈS la fermeture du verrou d'entrée (évite tout blocage modal).
        let mut want_export = false;
        let mut want_print = false;
        let mut want_new = false;
        let mut want_open = false;
        let mut want_save = false;
        let mut want_paste = false;
        ctx.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.ctrl;
            if cmd && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::Duplicate, i) {
                self.duplicate_selection();
            }
            if cmd && i.modifiers.alt && i.key_pressed(egui::Key::C) {
                self.copy_style();
            } else if cmd && i.key_pressed(egui::Key::C) {
                self.copy_selection();
            }
            if cmd && i.modifiers.alt && i.key_pressed(egui::Key::V) {
                self.paste_style();
            }
            if cmd && i.key_pressed(egui::Key::X) {
                self.cut_selection();
            }
            if !typing && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                self.delete_selection();
            }
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::InvertSelection, i) {
                self.invert_selection();
            }
            // Fichier (conventions macOS) : ⌘N / ⌘O / ⌘S / ⌘E.
            if cmd && i.key_pressed(egui::Key::N) {
                want_new = true;
            }
            if cmd && i.key_pressed(egui::Key::O) {
                want_open = true;
            }
            if cmd && i.key_pressed(egui::Key::S) {
                want_save = true;
            }
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::Export, i) {
                want_export = true;
            }
            // Impression (Sprint T, point 20) : ⌘P, convention macOS fixe.
            if cmd && i.key_pressed(egui::Key::P) {
                want_print = true;
            }
            if cmd && !i.modifiers.alt && i.key_pressed(egui::Key::V) {
                want_paste = true;
            }
            // Ordre de superposition : ⌘] avancer / ⌘⇧] premier plan, ⌘[ reculer / ⌘⇧[ arrière.
            if cmd && i.key_pressed(egui::Key::CloseBracket) {
                self.reorder(if i.modifiers.shift { ZMove::Front } else { ZMove::Forward });
            }
            if cmd && i.key_pressed(egui::Key::OpenBracket) {
                self.reorder(if i.modifiers.shift { ZMove::Back } else { ZMove::Backward });
            }
            // Zoom clavier (⌘0 = 100 %, ⌘+ / ⌘-), rebindable (Sprint R, point 97).
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::ZoomReset, i) {
                self.reset_view();
            }
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::ZoomIn, i) {
                self.zoom_in();
            }
            if self.keybindings.cmd_pressed(crate::keybindings::CommandAction::ZoomOut, i) {
                self.zoom_out();
            }
            if !cmd && !typing {
                use egui::Key;
                // Changement d'outil : raccourcis personnalisables
                // (Sprint 7.2, `crate::keybindings`) plutôt que câblés en dur.
                for action in crate::keybindings::ShortcutAction::ALL {
                    if self.keybindings.action_pressed(action, i) {
                        self.active_tool = action.tool();
                    }
                }
                // Plume : Entrée valide, Échap annule le chemin en cours.
                if !self.pen.is_empty() {
                    if i.key_pressed(Key::Enter) {
                        self.commit_pen(false);
                    }
                    if i.key_pressed(Key::Escape) {
                        self.pen.clear();
                    }
                }
                // Nudge clavier de la sélection (flèches ; Maj = pas de 10).
                if !self.selection.is_empty() {
                    let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                    let mut nx = 0.0;
                    let mut ny = 0.0;
                    if i.key_pressed(Key::ArrowLeft) {
                        nx -= step;
                    }
                    if i.key_pressed(Key::ArrowRight) {
                        nx += step;
                    }
                    if i.key_pressed(Key::ArrowUp) {
                        ny -= step;
                    }
                    if i.key_pressed(Key::ArrowDown) {
                        ny += step;
                    }
                    if nx != 0.0 || ny != 0.0 {
                        self.push_move(nx, ny);
                    }
                }
                if i.key_pressed(Key::OpenBracket) {
                    self.adjust_size(-1.0);
                }
                if i.key_pressed(Key::CloseBracket) {
                    self.adjust_size(1.0);
                }
            }
        });
        if want_new {
            self.new_document();
        }
        if want_open {
            self.open_project();
        }
        if want_save {
            self.save_project();
        }
        if want_paste {
            // Priorité au presse-papiers interne (éléments), sinon image système.
            if !self.paste_clipboard() {
                self.paste_image();
            }
        }
        if want_export {
            self.request_export(ctx, crate::export::ExportFormat::Png);
        }
        if want_print {
            self.print_document();
        }
    }
}
