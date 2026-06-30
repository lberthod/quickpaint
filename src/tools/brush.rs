//! Réglages de l'outil pinceau.

#[derive(Clone, Debug)]
pub struct Brush {
    pub color: [u8; 4],
    pub width: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self { color: [20, 20, 30, 255], width: 6.0 }
    }
}
