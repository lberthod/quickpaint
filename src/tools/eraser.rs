//! Réglages de l'outil gomme. Gomme **vectorielle** : retire les traits du
//! calque actif qu'elle survole (l'effacement est géré dans `app`). `width`
//! définit le rayon d'action.

#[derive(Clone, Debug)]
pub struct Eraser {
    pub width: f32,
}

impl Default for Eraser {
    fn default() -> Self {
        Self { width: 24.0 }
    }
}
