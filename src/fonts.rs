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

    /// Charge (si nécessaire) `family` dans egui sous `FontFamily::Name`.
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
        ctx.set_fonts(self.defs.clone());
        self.loaded.insert(family.to_string());
        true
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
