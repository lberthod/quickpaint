//! Raccourcis clavier personnalisables (Sprint 7.2, étendu Sprint R point
//! 97) : raccourcis « une touche, un outil » (V/B/E/L…) **et** une partie
//! des combinaisons ⌘ ([`CommandAction`] : export, dupliquer, inversion de
//! sélection, zoom). Les conventions macOS intouchables (⌘Z/⌘C/⌘V/⌘X/⌘S/
//! ⌘O/⌘N, ⌘[/⌘]) restent fixes — et sont refusées comme cible de rebind
//! pour ne pas masquer une convention. Persisté localement dans le même
//! `settings.json` que la langue et la palette — jamais synchronisé.

use std::collections::HashMap;

use egui::Key;

use crate::i18n::t;
use crate::tools::ActiveTool;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    Select,
    Brush,
    Eraser,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Text,
    Bucket,
    Pen,
    Eyedropper,
    Pan,
}

impl ShortcutAction {
    pub const ALL: [ShortcutAction; 12] = [
        ShortcutAction::Select,
        ShortcutAction::Brush,
        ShortcutAction::Eraser,
        ShortcutAction::Line,
        ShortcutAction::Arrow,
        ShortcutAction::Rectangle,
        ShortcutAction::Ellipse,
        ShortcutAction::Text,
        ShortcutAction::Bucket,
        ShortcutAction::Pen,
        ShortcutAction::Eyedropper,
        ShortcutAction::Pan,
    ];

    fn id(self) -> &'static str {
        match self {
            ShortcutAction::Select => "select",
            ShortcutAction::Brush => "brush",
            ShortcutAction::Eraser => "eraser",
            ShortcutAction::Line => "line",
            ShortcutAction::Arrow => "arrow",
            ShortcutAction::Rectangle => "rectangle",
            ShortcutAction::Ellipse => "ellipse",
            ShortcutAction::Text => "text",
            ShortcutAction::Bucket => "bucket",
            ShortcutAction::Pen => "pen",
            ShortcutAction::Eyedropper => "eyedropper",
            ShortcutAction::Pan => "pan",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShortcutAction::Select => t("Sélection", "Select"),
            ShortcutAction::Brush => t("Pinceau", "Brush"),
            ShortcutAction::Eraser => t("Gomme", "Eraser"),
            ShortcutAction::Line => t("Ligne", "Line"),
            ShortcutAction::Arrow => t("Flèche", "Arrow"),
            ShortcutAction::Rectangle => t("Rectangle", "Rectangle"),
            ShortcutAction::Ellipse => t("Ellipse", "Ellipse"),
            ShortcutAction::Text => t("Texte", "Text"),
            ShortcutAction::Bucket => t("Pot de peinture", "Paint bucket"),
            ShortcutAction::Pen => t("Plume", "Pen"),
            ShortcutAction::Eyedropper => t("Pipette", "Eyedropper"),
            ShortcutAction::Pan => t("Main (panoramique)", "Hand (pan)"),
        }
    }

    pub fn tool(self) -> ActiveTool {
        match self {
            ShortcutAction::Select => ActiveTool::Select,
            ShortcutAction::Brush => ActiveTool::Brush,
            ShortcutAction::Eraser => ActiveTool::Eraser,
            ShortcutAction::Line => ActiveTool::Line,
            ShortcutAction::Arrow => ActiveTool::Arrow,
            ShortcutAction::Rectangle => ActiveTool::Rectangle,
            ShortcutAction::Ellipse => ActiveTool::Ellipse,
            ShortcutAction::Text => ActiveTool::Text,
            ShortcutAction::Bucket => ActiveTool::Bucket,
            ShortcutAction::Pen => ActiveTool::Pen,
            ShortcutAction::Eyedropper => ActiveTool::Eyedropper,
            ShortcutAction::Pan => ActiveTool::Pan,
        }
    }

    fn default_key(self) -> Key {
        match self {
            ShortcutAction::Select => Key::V,
            ShortcutAction::Brush => Key::B,
            ShortcutAction::Eraser => Key::E,
            ShortcutAction::Line => Key::L,
            ShortcutAction::Arrow => Key::A,
            ShortcutAction::Rectangle => Key::R,
            ShortcutAction::Ellipse => Key::O,
            ShortcutAction::Text => Key::T,
            ShortcutAction::Bucket => Key::G,
            ShortcutAction::Pen => Key::P,
            ShortcutAction::Eyedropper => Key::I,
            ShortcutAction::Pan => Key::H,
        }
    }
}

/// Action de commande ⌘ rebindable (Sprint R, point 97). La touche est
/// personnalisable, le modificateur ⌘ (et l'éventuel ⇧) reste requis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandAction {
    Export,
    Duplicate,
    InvertSelection,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

impl CommandAction {
    pub const ALL: [CommandAction; 6] = [
        CommandAction::Export,
        CommandAction::Duplicate,
        CommandAction::InvertSelection,
        CommandAction::ZoomIn,
        CommandAction::ZoomOut,
        CommandAction::ZoomReset,
    ];

    fn id(self) -> &'static str {
        match self {
            CommandAction::Export => "cmd_export",
            CommandAction::Duplicate => "cmd_duplicate",
            CommandAction::InvertSelection => "cmd_invert_selection",
            CommandAction::ZoomIn => "cmd_zoom_in",
            CommandAction::ZoomOut => "cmd_zoom_out",
            CommandAction::ZoomReset => "cmd_zoom_reset",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CommandAction::Export => t("Exporter", "Export"),
            CommandAction::Duplicate => t("Dupliquer la sélection", "Duplicate selection"),
            CommandAction::InvertSelection => t("Inverser la sélection", "Invert selection"),
            CommandAction::ZoomIn => t("Zoom avant", "Zoom in"),
            CommandAction::ZoomOut => t("Zoom arrière", "Zoom out"),
            CommandAction::ZoomReset => t("Zoom 100 %", "Zoom 100%"),
        }
    }

    /// `(touche, ⇧ requis)` par défaut — les valeurs historiques câblées.
    fn default_binding(self) -> (Key, bool) {
        match self {
            CommandAction::Export => (Key::E, false),
            CommandAction::Duplicate => (Key::D, false),
            CommandAction::InvertSelection => (Key::I, true),
            CommandAction::ZoomIn => (Key::Plus, false),
            CommandAction::ZoomOut => (Key::Minus, false),
            CommandAction::ZoomReset => (Key::Num0, false),
        }
    }
}

/// Touches ⌘ **non** rebindables (conventions macOS) : refusées comme cible
/// d'un rebind de [`CommandAction`] pour ne jamais masquer ⌘Z/⌘C/⌘V/⌘X/
/// ⌘S/⌘O/⌘N ni l'ordre de superposition ⌘[/⌘].
const RESERVED_CMD_KEYS: [Key; 10] = [
    Key::Z,
    Key::C,
    Key::V,
    Key::X,
    Key::S,
    Key::O,
    Key::N,
    Key::P, // ⌘P = Imprimer (Sprint T, point 20)
    Key::OpenBracket,
    Key::CloseBracket,
];

/// Table de correspondance touche ↔ outil (+ commandes ⌘, Sprint R point
/// 97), éditable et persistée.
pub struct KeyBindings {
    map: HashMap<ShortcutAction, Key>,
    cmd: HashMap<CommandAction, (Key, bool)>,
}

impl KeyBindings {
    /// Charge la personnalisation sauvegardée ; toute action absente ou
    /// invalide retombe sur son raccourci par défaut.
    pub fn load() -> Self {
        let saved = crate::i18n::load_shortcuts();
        let map = ShortcutAction::ALL
            .into_iter()
            .map(|action| {
                let key = saved
                    .get(action.id())
                    .and_then(|name| Key::from_name(name))
                    .unwrap_or_else(|| action.default_key());
                (action, key)
            })
            .collect();
        // Commandes ⌘ (Sprint R, point 97) : persistées "Touche" ou
        // "Shift+Touche" dans la même table que les outils.
        let cmd = CommandAction::ALL
            .into_iter()
            .map(|action| {
                let binding = saved
                    .get(action.id())
                    .and_then(|v| {
                        let (shift, name) = match v.strip_prefix("Shift+") {
                            Some(rest) => (true, rest),
                            None => (false, v.as_str()),
                        };
                        Key::from_name(name).map(|k| (k, shift))
                    })
                    .unwrap_or_else(|| action.default_binding());
                (action, binding)
            })
            .collect();
        Self { map, cmd }
    }

    /// `(touche, ⇧ requis)` d'une commande ⌘ (Sprint R, point 97).
    pub fn cmd_binding(&self, action: CommandAction) -> (Key, bool) {
        self.cmd[&action]
    }

    /// `true` si la combinaison ⌘ de `action` vient d'être pressée. Le ⇧
    /// doit correspondre exactement (⌘⇧I ne déclenche pas ⌘I et
    /// inversement) — sauf pour le zoom avant, où ⌘⇧= (le + des claviers
    /// où = porte le +) reste accepté comme avant.
    pub fn cmd_pressed(&self, action: CommandAction, i: &egui::InputState) -> bool {
        let cmd = i.modifiers.command || i.modifiers.ctrl;
        if !cmd {
            return false;
        }
        let (key, shift) = self.cmd[&action];
        let shift_ok = i.modifiers.shift == shift
            || (action == CommandAction::ZoomIn && key == Key::Plus);
        // ⌘+ historique : accepte aussi ⌘= (touche physique du +).
        let key_ok = i.key_pressed(key)
            || (action == CommandAction::ZoomIn && key == Key::Plus && i.key_pressed(Key::Equals));
        key_ok && shift_ok
    }

    /// Attribue `(key, shift)` à la commande ⌘ `action`. Refuse les touches
    /// des conventions macOS fixes (renvoie `false`) ; en cas de collision
    /// avec une autre commande rebindable, les deux échangent leur
    /// combinaison, comme pour les outils.
    pub fn set_cmd(&mut self, action: CommandAction, key: Key, shift: bool) -> bool {
        if !shift && RESERVED_CMD_KEYS.contains(&key) {
            return false;
        }
        let previous = self.cmd.get(&action).copied();
        if let Some(other) = self
            .cmd
            .iter()
            .find(|(a, b)| **a != action && **b == (key, shift))
            .map(|(a, _)| *a)
        {
            if let Some(prev) = previous {
                self.cmd.insert(other, prev);
            }
        }
        self.cmd.insert(action, (key, shift));
        self.persist();
        true
    }

    pub fn key_for(&self, action: ShortcutAction) -> Key {
        self.map[&action]
    }

    pub fn action_pressed(&self, action: ShortcutAction, i: &egui::InputState) -> bool {
        i.key_pressed(self.key_for(action))
    }

    /// Attribue `key` à `action`. Si `key` était déjà utilisée par une autre
    /// action, les deux échangent leur touche (pas de raccourci perdu en
    /// silence, pas deux outils sur la même touche).
    pub fn set(&mut self, action: ShortcutAction, key: Key) {
        let previous = self.map.get(&action).copied();
        if let Some(other) = self.map.iter().find(|(a, k)| **a != action && **k == key).map(|(a, _)| *a) {
            if let Some(prev) = previous {
                self.map.insert(other, prev);
            }
        }
        self.map.insert(action, key);
        self.persist();
    }

    pub fn reset_defaults(&mut self) {
        self.map = ShortcutAction::ALL.into_iter().map(|a| (a, a.default_key())).collect();
        self.cmd = CommandAction::ALL.into_iter().map(|a| (a, a.default_binding())).collect();
        self.persist();
    }

    fn persist(&self) {
        let mut out: HashMap<String, String> =
            self.map.iter().map(|(a, k)| (a.id().to_string(), k.name().to_string())).collect();
        for (a, (k, shift)) in &self.cmd {
            let name = if *shift { format!("Shift+{}", k.name()) } else { k.name().to_string() };
            out.insert(a.id().to_string(), name);
        }
        crate::i18n::save_shortcuts(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `KeyBindings::load`/`persist` lisent et écrivent le vrai
    /// `settings.json` du poste (dérivé de `$HOME`) : chaque test redirige
    /// `$HOME` vers un dossier temporaire dédié, sous le verrou global
    /// partagé — même précaution que `app::tests::with_temp_home`.
    fn with_temp_home(name: &str, f: impl FnOnce()) {
        let _guard = crate::project::home_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var("HOME").ok();
        let tmp = std::env::temp_dir().join(format!("quickpaint-test-keybindings-{name}"));
        std::env::set_var("HOME", &tmp);
        f();
        if let Some(home) = real_home {
            std::env::set_var("HOME", home);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Sprint R (point 97) : les défauts des commandes ⌘ reprennent les
    /// valeurs historiquement câblées en dur.
    #[test]
    fn cmd_defaults_match_the_historical_bindings() {
        with_temp_home("defaults", || {
            let kb = KeyBindings::load();
            assert_eq!(kb.cmd_binding(CommandAction::Export).0, Key::E);
            assert_eq!(kb.cmd_binding(CommandAction::InvertSelection), (Key::I, true));
        });
    }

    /// Une touche des conventions macOS fixes est refusée comme cible.
    #[test]
    fn set_cmd_refuses_reserved_macos_keys() {
        with_temp_home("reserved", || {
            let mut kb = KeyBindings::load();
            assert!(!kb.set_cmd(CommandAction::Export, Key::S, false), "⌘S (Enregistrer) doit être refusé");
            assert_eq!(kb.cmd_binding(CommandAction::Export).0, Key::E, "binding inchangé après refus");
            assert!(kb.set_cmd(CommandAction::Export, Key::S, true), "⌘⇧S n'est pas réservé");
        });
    }

    /// Collision entre deux commandes rebindables : échange, pas d'écrasement.
    #[test]
    fn set_cmd_swaps_on_collision() {
        with_temp_home("swap", || {
            let mut kb = KeyBindings::load();
            assert!(kb.set_cmd(CommandAction::Duplicate, Key::E, false)); // prend ⌘E (Export)
            assert_eq!(kb.cmd_binding(CommandAction::Duplicate), (Key::E, false));
            assert_eq!(kb.cmd_binding(CommandAction::Export), (Key::D, false), "Export récupère l'ancien ⌘D");
        });
    }
}
