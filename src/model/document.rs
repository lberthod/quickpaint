//! Document = couches + traits. Modèle pur, ne sait rien du rendu (section 1).

use super::image::ImageItem;
use super::stroke::Stroke;
use super::text::TextItem;
use serde::{Deserialize, Serialize};

/// Mode de fusion d'un calque (roadmap #8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
}

impl BlendMode {
    pub const ALL: [BlendMode; 6] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Produit",
            BlendMode::Screen => "Écran",
            BlendMode::Overlay => "Incrustation",
            BlendMode::Darken => "Obscurcir",
            BlendMode::Lighten => "Éclaircir",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    /// Identifiant stable : l'historique référence les calques par id, pas par
    /// index, pour rester cohérent après suppression / réordonnancement.
    #[serde(default)]
    pub id: u64,
    /// Nom affiché dans le panneau de calques.
    pub name: String,
    pub visible: bool,
    /// Opacité du calque (0 = transparent, 1 = opaque). Non destructif.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Mode de fusion avec les calques inférieurs (roadmap #8).
    #[serde(default)]
    pub blend: BlendMode,
    /// Masque d'écrêtage (Sprint 4) : si vrai, le calque n'est visible qu'à
    /// travers l'alpha du calque non écrêté situé immédiatement en dessous.
    #[serde(default)]
    pub clip: bool,
    /// Groupe (dossier) auquel appartient le calque, le cas échéant.
    #[serde(default)]
    pub group: Option<String>,
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub texts: Vec<TextItem>,
    #[serde(default)]
    pub images: Vec<ImageItem>,
}

fn default_opacity() -> f32 {
    1.0
}

/// Référence d'un élément d'un calque (index dans le vec de son type).
#[derive(Clone, Copy, Debug)]
pub enum ElemRef {
    Stroke(usize),
    Image(usize),
    Text(usize),
}

impl Layer {
    /// Éléments du calque triés par profondeur `z` (du dessous au-dessus).
    /// Tri stable : à `z` égal, l'ordre reste traits → images → textes.
    pub fn z_order(&self) -> Vec<ElemRef> {
        let mut v: Vec<(f64, ElemRef)> = Vec::with_capacity(
            self.strokes.len() + self.images.len() + self.texts.len(),
        );
        for (i, s) in self.strokes.iter().enumerate() {
            v.push((s.z, ElemRef::Stroke(i)));
        }
        for (i, im) in self.images.iter().enumerate() {
            v.push((im.z, ElemRef::Image(i)));
        }
        for (i, t) in self.texts.iter().enumerate() {
            v.push((t.z, ElemRef::Text(i)));
        }
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(_, r)| r).collect()
    }

    /// (id, z) de tous les éléments.
    pub fn each_z(&self) -> Vec<(u64, f64)> {
        let mut v = Vec::new();
        v.extend(self.strokes.iter().map(|s| (s.id, s.z)));
        v.extend(self.images.iter().map(|im| (im.id, im.z)));
        v.extend(self.texts.iter().map(|t| (t.id, t.z)));
        v
    }

    /// Affecte la profondeur d'un élément par id.
    pub fn set_elem_z(&mut self, id: u64, z: f64) {
        if let Some(s) = self.strokes.iter_mut().find(|s| s.id == id) {
            s.z = z;
        } else if let Some(im) = self.images.iter_mut().find(|im| im.id == id) {
            im.z = z;
        } else if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
            t.z = z;
        }
    }
}

impl Layer {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: false,
            group: None,
            strokes: Vec::new(),
            texts: Vec::new(),
            images: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub size: (u32, u32),
    /// Prochain id de calque à attribuer.
    #[serde(default)]
    pub next_layer_id: u64,
    /// Prochaine profondeur à attribuer (compteur monotone, superposition).
    #[serde(default)]
    pub next_z: f64,
}

impl Document {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            layers: vec![Layer::new(1, "Calque 1")],
            active_layer: 0,
            size,
            next_layer_id: 2,
            next_z: 1.0,
        }
    }

    /// Id du calque actif.
    pub fn active_id(&self) -> u64 {
        self.layers[self.active_layer].id
    }

    /// Répare les id après chargement d'un ancien projet (id manquants /
    /// dupliqués) : réattribue des id uniques et recalcule le compteur.
    pub fn normalize_ids(&mut self) {
        for (i, layer) in self.layers.iter_mut().enumerate() {
            layer.id = i as u64 + 1;
        }
        self.next_layer_id = self.layers.len() as u64 + 1;
        if self.active_layer >= self.layers.len() {
            self.active_layer = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_has_one_layer_at_start() {
        let doc = Document::new((800, 600));
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.active_layer, 0);
    }
}
