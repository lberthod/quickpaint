//! Document = couches + traits. Modèle pur, ne sait rien du rendu (section 1).

use super::image::ImageItem;
use super::raster::{self, RasterLayer};
use super::stroke::Stroke;
use super::text::TextItem;
use crate::i18n::t;
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
            BlendMode::Normal => t("Normal", "Normal"),
            BlendMode::Multiply => t("Produit", "Multiply"),
            BlendMode::Screen => t("Écran", "Screen"),
            BlendMode::Overlay => t("Incrustation", "Overlay"),
            BlendMode::Darken => t("Obscurcir", "Darken"),
            BlendMode::Lighten => t("Éclaircir", "Lighten"),
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
    /// Contenu peint (pinceau/gomme pixel, roadmap F1). Rendu **sous** les
    /// éléments vectoriels du calque (le pinceau pixel peint le « fond » du
    /// calque, comme dans GIMP/Photoshop).
    #[serde(skip)]
    pub raster: RasterLayer,
    /// Persistance paresseuse du raster : PNG base64 borné à sa boîte
    /// englobante + origine, reconstruit au chargement (cf. `ImageItem`).
    #[serde(default)]
    raster_png: String,
    #[serde(default)]
    raster_origin: (i32, i32),
    /// Calque d'ajustement non destructif (roadmap F3) : si présent, ce
    /// calque ne porte aucun contenu propre — il applique le filtre en
    /// direct au rendu déjà composé des calques du dessous (tout, ou
    /// seulement le calque juste en dessous si `clip` est actif),
    /// réversible et re-réglable à tout moment (≠ filtre destructif).
    #[serde(default)]
    pub adjustment: Option<crate::tools::filter::Filter>,
    /// Masque de calque peint (roadmap P2 #14) : réutilise le moteur raster
    /// (F1) comme surface peignable en niveaux de gris. Un pixel jamais peint
    /// est **visible** par défaut (comme un masque tout juste créé, blanc) ;
    /// peindre en noir masque, en blanc redonne la visibilité — convention
    /// Photoshop/GIMP. Multiplie l'alpha du calque au compositing.
    #[serde(skip)]
    pub mask: Option<RasterLayer>,
    #[serde(default)]
    mask_png: String,
    #[serde(default)]
    mask_origin: (i32, i32),
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
            raster: RasterLayer::default(),
            raster_png: String::new(),
            raster_origin: (0, 0),
            adjustment: None,
            mask: None,
            mask_png: String::new(),
            mask_origin: (0, 0),
        }
    }

    /// Calque d'ajustement (roadmap F3) : aucun contenu propre, applique
    /// `filter` en direct au rendu des calques du dessous.
    pub fn new_adjustment(id: u64, name: impl Into<String>, filter: crate::tools::filter::Filter) -> Self {
        Self { adjustment: Some(filter), ..Self::new(id, name) }
    }

    /// Encode le raster en PNG base64 si nécessaire (avant sérialisation).
    pub fn ensure_raster_encoded(&mut self) {
        if self.raster.is_empty() {
            self.raster_png.clear();
            return;
        }
        let enc = raster::encode(&self.raster);
        self.raster_png = enc.png_b64;
        self.raster_origin = enc.origin;
    }

    /// Reconstruit le raster depuis son PNG base64 (après chargement projet).
    pub fn decode_raster(&mut self) {
        if self.raster_png.is_empty() {
            return;
        }
        self.raster = raster::decode(&raster::RasterEncoded {
            png_b64: std::mem::take(&mut self.raster_png),
            origin: self.raster_origin,
        });
    }

    /// Encode le masque en PNG base64 si nécessaire (avant sérialisation).
    pub fn ensure_mask_encoded(&mut self) {
        let Some(mask) = &self.mask else {
            self.mask_png.clear();
            return;
        };
        let enc = raster::encode(mask);
        self.mask_png = enc.png_b64;
        self.mask_origin = enc.origin;
    }

    /// Reconstruit le masque depuis son PNG base64 (après chargement projet).
    pub fn decode_mask(&mut self) {
        if self.mask_png.is_empty() {
            return;
        }
        self.mask = Some(raster::decode(&raster::RasterEncoded {
            png_b64: std::mem::take(&mut self.mask_png),
            origin: self.mask_origin,
        }));
    }

    /// Ajoute un masque vide (tout visible tant que rien n'est peint dessus).
    pub fn add_mask(&mut self) {
        self.mask = Some(RasterLayer::default());
    }

    pub fn remove_mask(&mut self) {
        self.mask = None;
        self.mask_png.clear();
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
            layers: vec![Layer::new(1, t("Calque 1", "Layer 1"))],
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

    /// Met à l'échelle tout le contenu (traits, textes, images) depuis
    /// l'origine du document. Les épaisseurs et tailles de texte suivent le
    /// facteur uniforme √(sx·sy) — redimensionnement d'image façon PhotoFiltre.
    pub fn scale_content(&mut self, sx: f32, sy: f32) {
        let uni = (sx.abs() * sy.abs()).sqrt();
        for layer in &mut self.layers {
            for s in &mut layer.strokes {
                for p in &mut s.points {
                    p.pos.0 *= sx;
                    p.pos.1 *= sy;
                    p.width *= uni;
                }
                s.base_width *= uni;
            }
            for t in &mut layer.texts {
                t.pos.0 *= sx;
                t.pos.1 *= sy;
                t.size = (t.size * uni).max(1.0);
                t.outline_w *= uni;
            }
            for im in &mut layer.images {
                im.pos.0 *= sx;
                im.pos.1 *= sy;
                im.size.0 *= sx;
                im.size.1 *= sy;
            }
            if !layer.raster.is_empty() {
                layer.raster = layer.raster.scaled(sx, sy);
            }
        }
    }

    /// Translate tout le contenu (taille du canevas avec ancrage : le document
    /// change de taille, le contenu est décalé, jamais déformé).
    pub fn translate_content(&mut self, dx: f32, dy: f32) {
        for layer in &mut self.layers {
            for s in &mut layer.strokes {
                for p in &mut s.points {
                    p.pos.0 += dx;
                    p.pos.1 += dy;
                }
            }
            for t in &mut layer.texts {
                t.pos.0 += dx;
                t.pos.1 += dy;
            }
            for im in &mut layer.images {
                im.pos.0 += dx;
                im.pos.1 += dy;
            }
            if !layer.raster.is_empty() {
                layer.raster = layer.raster.translated(dx.round() as i32, dy.round() as i32);
            }
        }
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

    #[test]
    fn scale_content_scales_positions_and_widths() {
        let mut doc = Document::new((100, 100));
        let mut s = crate::model::Stroke::new([0, 0, 0, 255], 4.0, crate::model::Tool::Brush);
        s.points.push(crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 });
        doc.layers[0].strokes.push(s);
        doc.scale_content(2.0, 2.0);
        let p = doc.layers[0].strokes[0].points[0];
        assert_eq!(p.pos, (20.0, 40.0));
        assert!((p.width - 4.0).abs() < 1e-5);
        assert!((doc.layers[0].strokes[0].base_width - 8.0).abs() < 1e-5);
    }

    #[test]
    fn translate_content_offsets_everything() {
        let mut doc = Document::new((100, 100));
        let mut s = crate::model::Stroke::new([0, 0, 0, 255], 4.0, crate::model::Tool::Brush);
        s.points.push(crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 });
        doc.layers[0].strokes.push(s);
        doc.translate_content(5.0, -5.0);
        assert_eq!(doc.layers[0].strokes[0].points[0].pos, (15.0, 15.0));
    }
}
