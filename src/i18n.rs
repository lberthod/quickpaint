//! Internationalisation FR/EN. Approche volontairement minimale (2 langues,
//! pas de fichiers de ressources) : chaque site d'appel porte les deux
//! traductions côte à côte via `t(fr, en)`, ce qui évite qu'une table de clés
//! centrale se désynchronise du texte réel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    fn from_code(code: &str) -> Lang {
        if code.eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::Fr
        }
    }

    fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    fn from_u8(v: u8) -> Lang {
        if v == 1 {
            Lang::En
        } else {
            Lang::Fr
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Lang::Fr => 0,
            Lang::En => 1,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Langue actuellement affichée.
pub fn current() -> Lang {
    Lang::from_u8(CURRENT.load(Ordering::Relaxed))
}

/// Change la langue et persiste le choix (préférence explicite de l'utilisateur).
pub fn set(lang: Lang) {
    CURRENT.store(lang.to_u8(), Ordering::Relaxed);
    save_settings(lang);
}

/// Traduction directe : `t("Nouveau", "New")` renvoie la version qui
/// correspond à la langue courante.
pub fn t(fr: &'static str, en: &'static str) -> &'static str {
    match current() {
        Lang::Fr => fr,
        Lang::En => en,
    }
}

pub fn is_french() -> bool {
    current() == Lang::Fr
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    lang: Option<String>,
    #[serde(default)]
    palette: Vec<[u8; 3]>,
    /// Raccourcis clavier personnalisés (Sprint 7.2) : id d'action → nom de
    /// touche egui (`Key::name()`). Absent d'une entrée = valeur par défaut.
    #[serde(default)]
    shortcuts: std::collections::HashMap<String, String>,
    /// Presets de style nommés (Sprint 10.3) : couleur/épaisseur/remplissage
    /// + dégradé optionnel, réutilisables en un clic.
    #[serde(default)]
    style_presets: Vec<crate::model::StylePreset>,
    /// Préréglages de pinceau nommés par l'utilisateur (Sprint 3.4), en plus
    /// des préréglages fournis (`BrushPreset::builtins`, jamais persistés ici).
    #[serde(default)]
    brush_presets: Vec<crate::model::BrushPreset>,
    /// Groupes de la barre d'outils repliés (UX-2.1). `#[serde(default =
    /// ...)]`, pas `Default::default()` : un fichier qui n'a jamais eu ce
    /// champ (installation existante) obtient les groupes secondaires
    /// repliés par défaut ; un fichier qui l'a déjà écrit une fois (même
    /// vide, tout déplié par l'utilisateur) est respecté tel quel.
    #[serde(default = "default_collapsed_toolbar_groups")]
    collapsed_toolbar_groups: Vec<String>,
    /// Largeur du panneau des calques (UX-3.2), en points egui.
    #[serde(default = "default_layers_panel_width")]
    layers_panel_width: f32,
    /// Projets récents (UX-4.3), le plus récent en tête, bornés à
    /// `MAX_RECENT_PROJECTS`.
    #[serde(default)]
    recent_projects: Vec<String>,
}

/// Groupes de la barre d'outils repliés par défaut (UX-2.1) : les familles
/// spécialisées, moins utilisées au quotidien que Navigation/Dessin/Formes/
/// Texte. Clés stables (pas de texte traduit — voir `tool_group_key` dans
/// `ui/toolbar.rs`), pour rester valides quelle que soit la langue courante.
fn default_collapsed_toolbar_groups() -> Vec<String> {
    vec!["photo_retouch".into(), "local_effects".into(), "composition".into()]
}

fn default_layers_panel_width() -> f32 {
    170.0
}

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join("Library/Application Support/QuickPaint/settings.json"))
}

/// `true` si aucun `settings.json` n'existe encore (UX-5.1, écran d'accueil).
/// N'a de sens qu'appelé avant toute écriture de préférence dans la session
/// courante — c'est le cas à la construction de `PaintApp`, avant toute
/// interaction utilisateur.
pub fn is_first_launch() -> bool {
    settings_path().map(|p| !p.exists()).unwrap_or(false)
}

/// À appeler une fois au démarrage : charge la préférence persistée, sinon
/// détecte la langue système (macOS `AppleLocale`, avec repli sur les
/// variables d'environnement POSIX).
pub fn init() {
    if let Some(path) = settings_path() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<Settings>(&data) {
                if let Some(code) = settings.lang {
                    CURRENT.store(Lang::from_code(&code).to_u8(), Ordering::Relaxed);
                    return;
                }
            }
        }
    }
    CURRENT.store(detect_system_lang().to_u8(), Ordering::Relaxed);
}

fn detect_system_lang() -> Lang {
    if let Ok(out) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
    {
        if out.status.success() {
            let locale = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
            if !locale.is_empty() {
                return if locale.starts_with("fr") { Lang::Fr } else { Lang::En };
            }
        }
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.to_lowercase();
            if v.starts_with("fr") {
                return Lang::Fr;
            }
            if !v.is_empty() {
                return Lang::En;
            }
        }
    }
    // Repli par défaut : application francophone à l'origine.
    Lang::Fr
}

fn read_settings() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        // Bug trouvé en vérifiant UX-2.1/UX-3.2 à l'écran : `.unwrap_or_default()`
        // construisait `Settings` via `#[derive(Default)]`, qui ignore les
        // `#[serde(default = "...")]` par champ (`Vec::new()`/`0.0` au lieu de
        // `default_collapsed_toolbar_groups()`/`default_layers_panel_width()`).
        // Résultat au tout premier lancement (fichier absent) : aucun groupe
        // d'outils replié, largeur de panneau à 0 au lieu de 170 — l'exact
        // opposé du comportement voulu. Parser `"{}"` traverse le même chemin
        // de désérialisation que n'importe quel fichier existant, donc les
        // défauts par champ s'appliquent uniformément, fichier présent ou non.
        .unwrap_or_else(|| serde_json::from_str("{}").expect("Settings: tous les champs ont un default"))
}

fn write_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, data);
    }
}

fn save_settings(lang: Lang) {
    let mut settings = read_settings();
    settings.lang = Some(lang.code().to_string());
    write_settings(&settings);
}

/// Palette de couleurs personnalisable (Sprint 7.1) : persistée dans le même
/// `settings.json` local que la langue, jamais synchronisée.
pub fn load_custom_palette() -> Vec<[u8; 3]> {
    read_settings().palette
}

pub fn save_custom_palette(palette: &[[u8; 3]]) {
    let mut settings = read_settings();
    settings.palette = palette.to_vec();
    write_settings(&settings);
}

/// Raccourcis clavier personnalisés (Sprint 7.2), même fichier local.
pub fn load_shortcuts() -> std::collections::HashMap<String, String> {
    read_settings().shortcuts
}

pub fn save_shortcuts(shortcuts: &std::collections::HashMap<String, String>) {
    let mut settings = read_settings();
    settings.shortcuts = shortcuts.clone();
    write_settings(&settings);
}

/// Presets de style nommés (Sprint 10.3), même fichier local.
pub fn load_style_presets() -> Vec<crate::model::StylePreset> {
    read_settings().style_presets
}

pub fn save_style_presets(presets: &[crate::model::StylePreset]) {
    let mut settings = read_settings();
    settings.style_presets = presets.to_vec();
    write_settings(&settings);
}

/// Préréglages de pinceau nommés par l'utilisateur (Sprint 3.4), même
/// fichier local — les préréglages fournis ne sont jamais persistés ici.
pub fn load_brush_presets() -> Vec<crate::model::BrushPreset> {
    read_settings().brush_presets
}

pub fn save_brush_presets(presets: &[crate::model::BrushPreset]) {
    let mut settings = read_settings();
    settings.brush_presets = presets.to_vec();
    write_settings(&settings);
}

/// Groupes de la barre d'outils repliés (UX-2.1), même fichier local.
pub fn load_collapsed_toolbar_groups() -> Vec<String> {
    read_settings().collapsed_toolbar_groups
}

pub fn save_collapsed_toolbar_groups(groups: &[String]) {
    let mut settings = read_settings();
    settings.collapsed_toolbar_groups = groups.to_vec();
    write_settings(&settings);
}

/// Largeur du panneau des calques (UX-3.2), même fichier local.
pub fn load_layers_panel_width() -> f32 {
    read_settings().layers_panel_width
}

pub fn save_layers_panel_width(width: f32) {
    let mut settings = read_settings();
    settings.layers_panel_width = width;
    write_settings(&settings);
}

/// Nombre maximal de projets récents conservés (UX-4.3).
pub const MAX_RECENT_PROJECTS: usize = 8;

/// Projets récents, le plus récent en tête, même fichier local.
pub fn load_recent_projects() -> Vec<String> {
    read_settings().recent_projects
}

/// Ajoute (ou remonte en tête) un chemin de projet, borné à
/// `MAX_RECENT_PROJECTS`. Appelé après chaque sauvegarde/ouverture réussie.
pub fn push_recent_project(path: &str) {
    let mut settings = read_settings();
    settings.recent_projects.retain(|p| p != path);
    settings.recent_projects.insert(0, path.to_string());
    settings.recent_projects.truncate(MAX_RECENT_PROJECTS);
    write_settings(&settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_switches_with_current_lang() {
        set(Lang::Fr);
        assert_eq!(t("Nouveau", "New"), "Nouveau");
        set(Lang::En);
        assert_eq!(t("Nouveau", "New"), "New");
        set(Lang::Fr); // ne pas polluer les autres tests
    }

    /// Régression (constatée en vérifiant UX-2.1/UX-3.2 à l'écran) : au tout
    /// premier lancement, `read_settings()` ne doit jamais passer par
    /// `Settings::default()` (dérivé — ignore les `#[serde(default = ...)]`
    /// par champ) mais toujours par le même chemin de désérialisation serde
    /// qu'un fichier existant, pour que les défauts par champ s'appliquent.
    /// Testé directement sur `"{}"` plutôt que sur `read_settings()` (qui
    /// touche le vrai `settings.json` du poste) pour rester déterministe et
    /// ne pas interférer avec l'état réel de la machine de test.
    #[test]
    fn settings_from_empty_json_use_field_level_defaults_not_derived_default() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.collapsed_toolbar_groups, default_collapsed_toolbar_groups());
        assert_eq!(settings.layers_panel_width, default_layers_panel_width());
        // Le derive `Default` donnerait `Vec::new()` / `0.0` ici — exactement
        // le bug corrigé : vérifie qu'on n'est pas revenu en arrière.
        assert_ne!(settings.collapsed_toolbar_groups, Vec::<String>::new());
        assert_ne!(settings.layers_panel_width, 0.0);
    }
}
