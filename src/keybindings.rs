//! Raccourcis clavier personnalisables pour le changement d'outil
//! (Sprint 7.2). Les combinaisons ⌘ (fichier, édition, zoom) restent fixes —
//! ce sont des conventions macOS ; seuls les raccourcis « une touche, un
//! outil » (V/B/E/L…) sont rebindables, car ce sont ceux qui entrent le plus
//! souvent en conflit avec les habitudes de chacun. Persisté localement dans
//! le même `settings.json` que la langue et la palette — jamais synchronisé.

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

/// Table de correspondance touche ↔ outil, éditable et persistée.
pub struct KeyBindings {
    map: HashMap<ShortcutAction, Key>,
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
        Self { map }
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
        self.persist();
    }

    fn persist(&self) {
        let out = self.map.iter().map(|(a, k)| (a.id().to_string(), k.name().to_string())).collect();
        crate::i18n::save_shortcuts(&out);
    }
}
