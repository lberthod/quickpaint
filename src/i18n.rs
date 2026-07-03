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
}

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join("Library/Application Support/QuickPaint/settings.json"))
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

fn save_settings(lang: Lang) {
    let Some(path) = settings_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let settings = Settings {
        lang: Some(lang.code().to_string()),
    };
    if let Ok(data) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, data);
    }
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
}
