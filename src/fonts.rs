//! Polices système (roadmap P1 #7) : condition du « Canva-like » — sans
//! vraie typographie, aucune composition ne ressemble à un visuel fini.
//!
//! `fontdb` scanne les dossiers système (`/System/Library/Fonts`,
//! `/Library/Fonts`, `~/Library/Fonts`…) une fois au démarrage et garde
//! seulement les **métadonnées** (nom, poids, style) — les octets d'une
//! police ne sont lus et enregistrés auprès d'egui qu'à la première
//! utilisation (`ensure_loaded`), pour ne pas charger des centaines de
//! fichiers inutilisés.

use std::collections::{BTreeSet, HashSet};

pub struct FontManager {
    db: fontdb::Database,
    /// Familles déjà enregistrées auprès d'egui (évite un rechargement).
    loaded: HashSet<String>,
    /// `Context::set_fonts` **remplace** toutes les polices à chaque appel
    /// (pas d'API d'ajout incrémental côté egui) : on garde donc notre
    /// propre copie à jour, complétée à chaque nouvelle police chargée.
    defs: egui::FontDefinitions,
}

impl FontManager {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self { db, loaded: HashSet::new(), defs: egui::FontDefinitions::default() }
    }

    /// Nombre de polices détectées (diagnostic / affichage).
    pub fn face_count(&self) -> usize {
        self.db.len()
    }

    /// Noms de familles disponibles, triés et dédupliqués (une police a
    /// souvent plusieurs graisses/styles qui partagent le même nom de
    /// famille — on ne montre que le nom, pas chaque variante).
    pub fn family_names(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        for face in self.db.faces() {
            if let Some((name, _)) = face.families.first() {
                set.insert(name.clone());
            }
        }
        set.into_iter().collect()
    }

    /// Nom de la famille egui portant la variante **italique** d'une famille
    /// système (Sprint Q, point 82). Toujours enregistrée par
    /// [`Self::ensure_loaded`] — avec les octets italiques réels si la
    /// famille en a, sinon les octets romains (repli sans effet visuel, mais
    /// jamais une famille egui inconnue, qui ferait paniquer le layout).
    pub fn italic_key(family: &str) -> String {
        format!("{family}#italic")
    }

    /// Charge (si nécessaire) `family` dans egui sous `FontFamily::Name`,
    /// ainsi que sa variante italique sous [`Self::italic_key`].
    /// Renvoie `false` si la police est introuvable dans la base système.
    pub fn ensure_loaded(&mut self, ctx: &egui::Context, family: &str) -> bool {
        if self.loaded.contains(family) {
            return true;
        }
        let query = fontdb::Query { families: &[fontdb::Family::Name(family)], ..Default::default() };
        let Some(id) = self.db.query(&query) else { return false };
        let Some(bytes) = self.db.with_face_data(id, |data, _face_index| data.to_vec()) else {
            return false;
        };
        self.defs.font_data.insert(family.to_string(), egui::FontData::from_owned(bytes));
        self.defs
            .families
            .entry(egui::FontFamily::Name(family.into()))
            .or_default()
            .insert(0, family.to_string());

        // Variante italique (Sprint Q, point 82) : cherche une vraie fonte
        // italique/oblique de la même famille ; à défaut, la famille egui
        // italique pointe sur les octets romains déjà enregistrés.
        let italic_key = Self::italic_key(family);
        let italic_query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            style: fontdb::Style::Italic,
            ..Default::default()
        };
        let real_italic = self.db.query(&italic_query).and_then(|iid| {
            let is_italic = self
                .db
                .faces()
                .find(|f| f.id == iid)
                .map(|f| f.style != fontdb::Style::Normal)
                .unwrap_or(false);
            if !is_italic {
                return None;
            }
            self.db.with_face_data(iid, |data, _idx| data.to_vec())
        });
        let italic_data_key = match real_italic {
            Some(italic_bytes) => {
                self.defs.font_data.insert(italic_key.clone(), egui::FontData::from_owned(italic_bytes));
                italic_key.clone()
            }
            None => family.to_string(), // repli : mêmes octets que le romain
        };
        self.defs
            .families
            .entry(egui::FontFamily::Name(italic_key.as_str().into()))
            .or_default()
            .insert(0, italic_data_key);

        ctx.set_fonts(self.defs.clone());
        self.loaded.insert(family.to_string());
        true
    }

    /// Octets bruts de la police système `family` (audit_100_features.md
    /// #64, extraction de contours de glyphes via `ttf-parser`) — au plus
    /// proche du style demandé (gras/italique), avec repli sur la variante
    /// normale si la famille n'a pas exactement ce style. `None` si la
    /// famille est introuvable dans la base système (polices intégrées
    /// Sans/Mono : pas dans `fontdb`, pas de conversion possible pour elles).
    pub fn font_bytes(&self, family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
        let weight = if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL };
        let style = if italic { fontdb::Style::Italic } else { fontdb::Style::Normal };
        let query = fontdb::Query { families: &[fontdb::Family::Name(family)], weight, style, ..Default::default() };
        let id = self.db.query(&query).or_else(|| {
            // Repli : la famille existe mais pas dans ce style exact.
            self.db.query(&fontdb::Query { families: &[fontdb::Family::Name(family)], ..Default::default() })
        })?;
        self.db.with_face_data(id, |data, _face_index| data.to_vec())
    }
}

impl Default for FontManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_names_are_sorted_and_deduplicated() {
        let mgr = FontManager::new();
        let names = mgr.family_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        let mut dedup = names.clone();
        dedup.dedup();
        assert_eq!(names.len(), dedup.len());
    }

    #[test]
    fn ensure_loaded_returns_false_for_unknown_family() {
        let mut mgr = FontManager::new();
        let ctx = egui::Context::default();
        assert!(!mgr.ensure_loaded(&ctx, "NoSuchFontFamilyXYZ123"));
    }
}
