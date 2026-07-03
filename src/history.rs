//! Undo / Redo via pattern Command (section 5).
//!
//! Chaque action est une commande réversible. Les calques sont référencés par
//! **id stable** (pas par index) : suppression / réordonnancement ne corrompent
//! plus la pile. Une commande sur un calque disparu devient un no-op.

use crate::i18n::t;
use crate::model::raster::{Tile, TileKey};
use crate::model::{Document, ImageItem, Layer, Stroke, TextItem};

/// Nature d'une opération de peinture raster (F1), pour l'étiquette d'undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterOp {
    Brush,
    Eraser,
    Bucket,
    Clone,
    Heal,
    Cutout,
    /// Retouche locale (Sprint 11) : densité +/- (dodge/burn), éponge
    /// (saturation +/-), flou, netteté, estompe — chacun une seule variante
    /// pour l'étiquette d'undo, la peinture elle-même vit dans
    /// `model::raster::PixelEffect` / `RasterLayer::smudge_segment`.
    Dodge,
    Burn,
    Saturate,
    Desaturate,
    Blur,
    Sharpen,
    Smudge,
}

/// Surface raster ciblée par une opération de peinture (roadmap P2 #14) :
/// le contenu peint du calque, ou son masque de visibilité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterTarget {
    Content,
    Mask,
}

#[derive(Clone, Debug)]
pub enum Command {
    AddStroke { layer: u64, stroke: Stroke },
    /// Plusieurs traits ajoutés d'un coup (duplication).
    AddMany { layer: u64, strokes: Vec<Stroke> },
    /// Gomme vectorielle / suppression : traits retirés (index pour réinsertion).
    Erase { layer: u64, removed: Vec<(usize, Stroke)> },
    /// Gomme partielle : traits coupés (retirés) → fragments (ajoutés).
    SplitStrokes { layer: u64, removed: Vec<(usize, Stroke)>, added: Vec<Stroke> },
    /// Déplacement d'éléments (sélection) : traits, textes et/ou images.
    Move { layer: u64, strokes: Vec<u64>, texts: Vec<u64>, images: Vec<u64>, delta: (f32, f32) },
    /// Déplacement par élément (alignement / répartition) : chaque id son delta.
    MoveEach { layer: u64, moves: Vec<(u64, (f32, f32))> },
    /// Changement de profondeur (z-order) : (id, avant, après).
    SetZMany { layer: u64, changes: Vec<(u64, f64, f64)> },
    Clear { layer: u64, previous: Vec<Stroke>, previous_raster: crate::model::RasterLayer },
    /// Ajout d'un texte (roadmap #2).
    AddText { layer: u64, text: TextItem },
    /// Suppression de textes (index pour réinsertion).
    DeleteText { layer: u64, removed: Vec<(usize, TextItem)> },
    /// Ajout d'une image (roadmap #7).
    AddImage { layer: u64, image: ImageItem },
    /// Suppression d'images (index pour réinsertion).
    DeleteImage { layer: u64, removed: Vec<(usize, ImageItem)> },
    /// Remplacement du contenu d'une image (filtres).
    ReplaceImage { layer: u64, id: u64, before: Box<ImageItem>, after: Box<ImageItem> },
    /// Ajout d'un calque à un index (ajout / duplication).
    AddLayer { index: usize, layer: Box<Layer> },
    /// Suppression d'un calque (stocké pour réinsertion).
    RemoveLayer { index: usize, layer: Box<Layer> },
    /// Remplacement de toute la pile de calques (fusion / aplatissement).
    SetLayers {
        before: Vec<Layer>,
        before_active: usize,
        after: Vec<Layer>,
        after_active: usize,
    },
    /// Mise à l'échelle d'une sélection autour d'un pivot (réversible).
    Scale {
        layer: u64,
        strokes: Vec<u64>,
        texts: Vec<u64>,
        images: Vec<u64>,
        pivot: (f32, f32),
        sx: f32,
        sy: f32,
    },
    /// Un coup de pinceau/gomme pixel (roadmap F1) : undo par tuile — seules
    /// les tuiles touchées par le geste sont clonées (avant, après).
    PaintRaster {
        layer: u64,
        op: RasterOp,
        target: RasterTarget,
        tiles: Vec<(TileKey, Option<Tile>, Option<Tile>)>,
    },
    /// Remplacement complet du document (redimensionnement image / canevas).
    /// Snapshot avant/après, comme `SetLayers` : robuste (pas d'inverse
    /// flottant approximatif), au prix d'un clone du document.
    SetDoc {
        before: Box<Document>,
        after: Box<Document>,
        label: &'static str,
    },
    /// Édition de nœuds après coup d'un trait de plume (roadmap P2 #12) :
    /// snapshot avant/après des ancres **et** des points échantillonnés (pour
    /// ne pas ré-échantillonner à l'undo — robuste, comme `SetDoc`).
    EditPenPath {
        layer: u64,
        id: u64,
        before_path: crate::tools::pen::PenPath,
        before_points: Vec<crate::model::StrokePoint>,
        after_path: crate::tools::pen::PenPath,
        after_points: Vec<crate::model::StrokePoint>,
    },
    /// Rotation d'une sélection autour d'un pivot (réversible).
    Rotate {
        layer: u64,
        strokes: Vec<u64>,
        texts: Vec<u64>,
        images: Vec<u64>,
        pivot: (f32, f32),
        angle: f32,
    },
}

impl Command {
    /// Vrai si la commande modifie la géométrie de traits existants (→ invalider
    /// leur cache de maillage).
    pub fn mutates_geometry(&self) -> bool {
        matches!(
            self,
            Command::Move { .. }
                | Command::MoveEach { .. }
                | Command::Scale { .. }
                | Command::Rotate { .. }
                | Command::SetDoc { .. }
                | Command::EditPenPath { .. }
        )
    }

    /// Libellé court pour le panneau d'historique.
    pub fn label(&self) -> &'static str {
        match self {
            Command::AddStroke { .. } => t("Trait", "Stroke"),
            Command::AddMany { .. } => t("Coller / dupliquer", "Paste / duplicate"),
            Command::Erase { .. } => t("Effacer", "Erase"),
            Command::SplitStrokes { .. } => t("Gomme partielle", "Partial erase"),
            Command::Move { .. } => t("Déplacer", "Move"),
            Command::MoveEach { .. } => t("Aligner", "Align"),
            Command::SetZMany { .. } => t("Réordonner", "Reorder"),
            Command::Scale { .. } => t("Mise à l'échelle", "Scale"),
            Command::Rotate { .. } => t("Rotation", "Rotate"),
            Command::Clear { .. } => t("Vider le calque", "Clear layer"),
            Command::AddText { .. } => t("Texte", "Text"),
            Command::DeleteText { .. } => t("Suppr. texte", "Delete text"),
            Command::AddImage { .. } => t("Image", "Image"),
            Command::DeleteImage { .. } => t("Suppr. image", "Delete image"),
            Command::ReplaceImage { .. } => t("Filtre / recadrage", "Filter / crop"),
            Command::AddLayer { .. } => t("Ajouter calque", "Add layer"),
            Command::RemoveLayer { .. } => t("Suppr. calque", "Delete layer"),
            Command::SetLayers { .. } => t("Fusion / aplatir", "Merge / flatten"),
            Command::SetDoc { label, .. } => label,
            Command::EditPenPath { .. } => t("Éditer le chemin", "Edit path"),
            Command::PaintRaster { op: RasterOp::Brush, .. } => t("Pinceau pixel", "Pixel brush"),
            Command::PaintRaster { op: RasterOp::Eraser, .. } => t("Gomme pixel", "Pixel eraser"),
            Command::PaintRaster { op: RasterOp::Bucket, .. } => t("Pot de peinture", "Paint bucket"),
            Command::PaintRaster { op: RasterOp::Clone, .. } => t("Tampon de clonage", "Clone stamp"),
            Command::PaintRaster { op: RasterOp::Heal, .. } => t("Correcteur", "Healing brush"),
            Command::PaintRaster { op: RasterOp::Cutout, .. } => t("Détourage", "Cutout"),
            Command::PaintRaster { op: RasterOp::Dodge, .. } => t("Densité -", "Dodge"),
            Command::PaintRaster { op: RasterOp::Burn, .. } => t("Densité +", "Burn"),
            Command::PaintRaster { op: RasterOp::Saturate, .. } => t("Éponge (saturer)", "Sponge (saturate)"),
            Command::PaintRaster { op: RasterOp::Desaturate, .. } => t("Éponge (désaturer)", "Sponge (desaturate)"),
            Command::PaintRaster { op: RasterOp::Blur, .. } => t("Flou localisé", "Local blur"),
            Command::PaintRaster { op: RasterOp::Sharpen, .. } => t("Netteté localisée", "Local sharpen"),
            Command::PaintRaster { op: RasterOp::Smudge, .. } => t("Estompe", "Smudge"),
        }
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
    /// Incrémenté à chaque mutation : sert de clé d'invalidation du compositeur.
    rev: u64,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Numéro de révision : change à chaque modification du document.
    pub fn revision(&self) -> u64 {
        self.rev
    }

    /// Frise chronologique des actions (anciennes → récentes), pour le panneau.
    pub fn timeline(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.undo.iter().map(|c| c.label()).collect();
        v.extend(self.redo.iter().rev().map(|c| c.label()));
        v
    }

    /// Position courante dans la frise (= nombre d'actions appliquées).
    pub fn position(&self) -> usize {
        self.undo.len()
    }

    /// Va à un état précis de la frise (undo/redo répétés).
    pub fn goto(&mut self, doc: &mut Document, target: usize) {
        while self.undo.len() > target {
            self.undo(doc);
        }
        while self.undo.len() < target && !self.redo.is_empty() {
            self.redo(doc);
        }
    }

    /// Signale une mutation directe du document (hors commande) pour invalider
    /// les caches (ex. alignement d'images, fusion de calques).
    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    /// Exécute et empile. Renvoie `true` si de la géométrie existante a bougé.
    pub fn push(&mut self, doc: &mut Document, cmd: Command) -> bool {
        apply(doc, &cmd);
        let moved = cmd.mutates_geometry();
        self.undo.push(cmd);
        self.redo.clear();
        self.rev = self.rev.wrapping_add(1);
        moved
    }

    /// Renvoie `true` si l'opération a modifié de la géométrie existante.
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(cmd) = self.undo.pop() {
            let moved = cmd.mutates_geometry();
            revert(doc, &cmd);
            self.redo.push(cmd);
            self.rev = self.rev.wrapping_add(1);
            moved
        } else {
            false
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(cmd) = self.redo.pop() {
            let moved = cmd.mutates_geometry();
            apply(doc, &cmd);
            self.undo.push(cmd);
            self.rev = self.rev.wrapping_add(1);
            moved
        } else {
            false
        }
    }
}

fn layer_mut(doc: &mut Document, id: u64) -> Option<&mut Layer> {
    doc.layers.iter_mut().find(|l| l.id == id)
}

#[allow(clippy::too_many_arguments)]
fn scale_elements(
    doc: &mut Document,
    layer: u64,
    strokes: &[u64],
    texts: &[u64],
    images: &[u64],
    pivot: (f32, f32),
    sx: f32,
    sy: f32,
) {
    let uni = (sx.abs() * sy.abs()).sqrt().max(0.01);
    if let Some(l) = layer_mut(doc, layer) {
        for s in &mut l.strokes {
            if strokes.contains(&s.id) {
                for p in &mut s.points {
                    p.pos.0 = pivot.0 + (p.pos.0 - pivot.0) * sx;
                    p.pos.1 = pivot.1 + (p.pos.1 - pivot.1) * sy;
                    p.width *= uni;
                }
                s.base_width *= uni;
            }
        }
        for t in &mut l.texts {
            if texts.contains(&t.id) {
                t.pos.0 = pivot.0 + (t.pos.0 - pivot.0) * sx;
                t.pos.1 = pivot.1 + (t.pos.1 - pivot.1) * sy;
                t.size = (t.size * uni).max(4.0);
            }
        }
        for im in &mut l.images {
            if images.contains(&im.id) {
                im.pos.0 = pivot.0 + (im.pos.0 - pivot.0) * sx;
                im.pos.1 = pivot.1 + (im.pos.1 - pivot.1) * sy;
                im.size.0 = (im.size.0 * sx).abs().max(4.0);
                im.size.1 = (im.size.1 * sy).abs().max(4.0);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rotate_elements(
    doc: &mut Document,
    layer: u64,
    strokes: &[u64],
    texts: &[u64],
    images: &[u64],
    pivot: (f32, f32),
    angle: f32,
) {
    let (c, s) = (angle.cos(), angle.sin());
    let rot = |p: (f32, f32)| {
        let (dx, dy) = (p.0 - pivot.0, p.1 - pivot.1);
        (pivot.0 + dx * c - dy * s, pivot.1 + dx * s + dy * c)
    };
    if let Some(l) = layer_mut(doc, layer) {
        for st in &mut l.strokes {
            if strokes.contains(&st.id) {
                for p in &mut st.points {
                    p.pos = rot(p.pos);
                }
            }
        }
        for t in &mut l.texts {
            if texts.contains(&t.id) {
                t.pos = rot(t.pos);
                t.rot += angle;
            }
        }
        for im in &mut l.images {
            if images.contains(&im.id) {
                // Rotation autour du pivot via le centre de l'image.
                let center = (im.pos.0 + im.size.0 * 0.5, im.pos.1 + im.size.1 * 0.5);
                let nc = rot(center);
                im.pos = (nc.0 - im.size.0 * 0.5, nc.1 - im.size.1 * 0.5);
                im.rot += angle;
            }
        }
    }
}

/// Translate un seul élément (trait, texte ou image) par son id.
fn translate_one(doc: &mut Document, layer: u64, id: u64, d: (f32, f32)) {
    if let Some(l) = layer_mut(doc, layer) {
        if let Some(s) = l.strokes.iter_mut().find(|s| s.id == id) {
            for p in &mut s.points {
                p.pos.0 += d.0;
                p.pos.1 += d.1;
            }
        } else if let Some(t) = l.texts.iter_mut().find(|t| t.id == id) {
            t.pos.0 += d.0;
            t.pos.1 += d.1;
        } else if let Some(im) = l.images.iter_mut().find(|im| im.id == id) {
            im.pos.0 += d.0;
            im.pos.1 += d.1;
        }
    }
}

fn translate(
    doc: &mut Document,
    layer: u64,
    strokes: &[u64],
    texts: &[u64],
    images: &[u64],
    delta: (f32, f32),
) {
    if let Some(l) = layer_mut(doc, layer) {
        for s in &mut l.strokes {
            if strokes.contains(&s.id) {
                for p in &mut s.points {
                    p.pos.0 += delta.0;
                    p.pos.1 += delta.1;
                }
            }
        }
        for t in &mut l.texts {
            if texts.contains(&t.id) {
                t.pos.0 += delta.0;
                t.pos.1 += delta.1;
            }
        }
        for im in &mut l.images {
            if images.contains(&im.id) {
                im.pos.0 += delta.0;
                im.pos.1 += delta.1;
            }
        }
    }
}

fn apply(doc: &mut Document, cmd: &Command) {
    match cmd {
        Command::AddStroke { layer, stroke } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes.push(stroke.clone());
            }
        }
        Command::AddMany { layer, strokes } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes.extend(strokes.iter().cloned());
            }
        }
        Command::Move { layer, strokes, texts, images, delta } => {
            translate(doc, *layer, strokes, texts, images, *delta);
        }
        Command::MoveEach { layer, moves } => {
            for (id, d) in moves {
                translate_one(doc, *layer, *id, *d);
            }
        }
        Command::SetZMany { layer, changes } => {
            if let Some(l) = layer_mut(doc, *layer) {
                for (id, _, after) in changes {
                    l.set_elem_z(*id, *after);
                }
            }
        }
        Command::Erase { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                // Index décroissants pour ne pas invalider les suivants.
                let mut idx: Vec<usize> = removed.iter().map(|(i, _)| *i).collect();
                idx.sort_unstable();
                for i in idx.iter().rev() {
                    if *i < l.strokes.len() {
                        l.strokes.remove(*i);
                    }
                }
            }
        }
        Command::SplitStrokes { layer, removed, added } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let mut idx: Vec<usize> = removed.iter().map(|(i, _)| *i).collect();
                idx.sort_unstable();
                for i in idx.iter().rev() {
                    if *i < l.strokes.len() {
                        l.strokes.remove(*i);
                    }
                }
                l.strokes.extend(added.iter().cloned());
            }
        }
        Command::Clear { layer, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes.clear();
                l.raster = crate::model::RasterLayer::default();
            }
        }
        Command::AddText { layer, text } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.texts.push(text.clone());
            }
        }
        Command::DeleteText { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let mut idx: Vec<usize> = removed.iter().map(|(i, _)| *i).collect();
                idx.sort_unstable();
                for i in idx.iter().rev() {
                    if *i < l.texts.len() {
                        l.texts.remove(*i);
                    }
                }
            }
        }
        Command::AddImage { layer, image } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.images.push(image.clone());
            }
        }
        Command::DeleteImage { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let mut idx: Vec<usize> = removed.iter().map(|(i, _)| *i).collect();
                idx.sort_unstable();
                for i in idx.iter().rev() {
                    if *i < l.images.len() {
                        l.images.remove(*i);
                    }
                }
            }
        }
        Command::ReplaceImage { layer, id, after, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                if let Some(im) = l.images.iter_mut().find(|im| im.id == *id) {
                    *im = (**after).clone();
                }
            }
        }
        Command::Scale { layer, strokes, texts, images, pivot, sx, sy } => {
            scale_elements(doc, *layer, strokes, texts, images, *pivot, *sx, *sy);
        }
        Command::Rotate { layer, strokes, texts, images, pivot, angle } => {
            rotate_elements(doc, *layer, strokes, texts, images, *pivot, *angle);
        }
        Command::AddLayer { index, layer } => {
            let at = (*index).min(doc.layers.len());
            doc.layers.insert(at, (**layer).clone());
            doc.active_layer = at;
        }
        Command::RemoveLayer { index, .. } => {
            if *index < doc.layers.len() && doc.layers.len() > 1 {
                doc.layers.remove(*index);
                if doc.active_layer >= doc.layers.len() {
                    doc.active_layer = doc.layers.len() - 1;
                }
            }
        }
        Command::SetLayers { after, after_active, .. } => {
            doc.layers = after.clone();
            doc.active_layer = (*after_active).min(doc.layers.len().saturating_sub(1));
        }
        Command::SetDoc { after, .. } => {
            *doc = (**after).clone();
        }
        Command::EditPenPath { layer, id, after_path, after_points, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                if let Some(s) = l.strokes.iter_mut().find(|s| s.id == *id) {
                    s.anchors = Some(after_path.clone());
                    s.points = after_points.clone();
                }
            }
        }
        Command::PaintRaster { layer, target, tiles, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let raster = match target {
                    RasterTarget::Content => &mut l.raster,
                    RasterTarget::Mask => l.mask.get_or_insert_with(Default::default),
                };
                for (key, _, after) in tiles {
                    match after {
                        Some(t) => raster.tiles.insert(*key, t.clone()),
                        None => raster.tiles.remove(key),
                    };
                }
            }
        }
    }
}

fn revert(doc: &mut Document, cmd: &Command) {
    match cmd {
        Command::AddStroke { layer, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes.pop();
            }
        }
        Command::AddMany { layer, strokes } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes.truncate(l.strokes.len().saturating_sub(strokes.len()));
            }
        }
        Command::Move { layer, strokes, texts, images, delta } => {
            translate(doc, *layer, strokes, texts, images, (-delta.0, -delta.1));
        }
        Command::MoveEach { layer, moves } => {
            for (id, d) in moves {
                translate_one(doc, *layer, *id, (-d.0, -d.1));
            }
        }
        Command::SetZMany { layer, changes } => {
            if let Some(l) = layer_mut(doc, *layer) {
                for (id, before, _) in changes {
                    l.set_elem_z(*id, *before);
                }
            }
        }
        Command::Erase { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                // Réinsertion dans l'ordre croissant des index d'origine.
                let mut items = removed.clone();
                items.sort_by_key(|(i, _)| *i);
                for (i, s) in items {
                    let at = i.min(l.strokes.len());
                    l.strokes.insert(at, s);
                }
            }
        }
        Command::SplitStrokes { layer, removed, added } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let added_ids: std::collections::HashSet<u64> = added.iter().map(|s| s.id).collect();
                l.strokes.retain(|s| !added_ids.contains(&s.id));
                let mut items = removed.clone();
                items.sort_by_key(|(i, _)| *i);
                for (i, s) in items {
                    let at = i.min(l.strokes.len());
                    l.strokes.insert(at, s);
                }
            }
        }
        Command::Clear { layer, previous, previous_raster } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes = previous.clone();
                l.raster = previous_raster.clone();
            }
        }
        Command::AddText { layer, text } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.texts.retain(|t| t.id != text.id);
            }
        }
        Command::DeleteText { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let mut items = removed.clone();
                items.sort_by_key(|(i, _)| *i);
                for (i, t) in items {
                    let at = i.min(l.texts.len());
                    l.texts.insert(at, t);
                }
            }
        }
        Command::AddImage { layer, image } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.images.retain(|im| im.id != image.id);
            }
        }
        Command::DeleteImage { layer, removed } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let mut items = removed.clone();
                items.sort_by_key(|(i, _)| *i);
                for (i, im) in items {
                    let at = i.min(l.images.len());
                    l.images.insert(at, im);
                }
            }
        }
        Command::ReplaceImage { layer, id, before, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                if let Some(im) = l.images.iter_mut().find(|im| im.id == *id) {
                    *im = (**before).clone();
                }
            }
        }
        Command::Scale { layer, strokes, texts, images, pivot, sx, sy } => {
            let (ix, iy) = (1.0 / sx, 1.0 / sy);
            scale_elements(doc, *layer, strokes, texts, images, *pivot, ix, iy);
        }
        Command::Rotate { layer, strokes, texts, images, pivot, angle } => {
            rotate_elements(doc, *layer, strokes, texts, images, *pivot, -angle);
        }
        Command::AddLayer { index, .. } => {
            if *index < doc.layers.len() && doc.layers.len() > 1 {
                doc.layers.remove(*index);
                if doc.active_layer >= doc.layers.len() {
                    doc.active_layer = doc.layers.len() - 1;
                }
            }
        }
        Command::RemoveLayer { index, layer } => {
            let at = (*index).min(doc.layers.len());
            doc.layers.insert(at, (**layer).clone());
            doc.active_layer = at;
        }
        Command::SetLayers { before, before_active, .. } => {
            doc.layers = before.clone();
            doc.active_layer = (*before_active).min(doc.layers.len().saturating_sub(1));
        }
        Command::SetDoc { before, .. } => {
            *doc = (**before).clone();
        }
        Command::EditPenPath { layer, id, before_path, before_points, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                if let Some(s) = l.strokes.iter_mut().find(|s| s.id == *id) {
                    s.anchors = Some(before_path.clone());
                    s.points = before_points.clone();
                }
            }
        }
        Command::PaintRaster { layer, target, tiles, .. } => {
            if let Some(l) = layer_mut(doc, *layer) {
                let raster = match target {
                    RasterTarget::Content => &mut l.raster,
                    RasterTarget::Mask => l.mask.get_or_insert_with(Default::default),
                };
                for (key, before, _) in tiles {
                    match before {
                        Some(t) => raster.tiles.insert(*key, t.clone()),
                        None => raster.tiles.remove(key),
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Stroke, Tool};

    fn s() -> Stroke {
        Stroke::new([0, 0, 0, 255], 4.0, Tool::Brush)
    }

    #[test]
    fn add_then_undo_redo() {
        let mut doc = Document::new((100, 100));
        let id = doc.active_id();
        let mut h = History::new();
        h.push(&mut doc, Command::AddStroke { layer: id, stroke: s() });
        assert_eq!(doc.layers[0].strokes.len(), 1);
        h.undo(&mut doc);
        assert_eq!(doc.layers[0].strokes.len(), 0);
        h.redo(&mut doc);
        assert_eq!(doc.layers[0].strokes.len(), 1);
    }

    #[test]
    fn erase_then_undo_restores_order() {
        let mut doc = Document::new((100, 100));
        let id = doc.active_id();
        let mut h = History::new();
        for _ in 0..3 {
            h.push(&mut doc, Command::AddStroke { layer: id, stroke: s() });
        }
        // Efface le trait du milieu (index 1).
        let removed = vec![(1, doc.layers[0].strokes[1].clone())];
        h.push(&mut doc, Command::Erase { layer: id, removed });
        assert_eq!(doc.layers[0].strokes.len(), 2);
        h.undo(&mut doc);
        assert_eq!(doc.layers[0].strokes.len(), 3);
    }

    #[test]
    fn set_doc_undo_restores_size_and_content() {
        let mut doc = Document::new((100, 100));
        let id = doc.active_id();
        let mut h = History::new();
        h.push(&mut doc, Command::AddStroke { layer: id, stroke: s() });
        // Redimensionnement ×2 : snapshot avant/après.
        let before = Box::new(doc.clone());
        let mut after = doc.clone();
        after.scale_content(2.0, 2.0);
        after.size = (200, 200);
        h.push(&mut doc, Command::SetDoc { before, after: Box::new(after), label: "Redimensionner" });
        assert_eq!(doc.size, (200, 200));
        h.undo(&mut doc);
        assert_eq!(doc.size, (100, 100));
        assert_eq!(doc.layers[0].strokes.len(), 1);
        h.redo(&mut doc);
        assert_eq!(doc.size, (200, 200));
    }

    #[test]
    fn paint_raster_undo_redo_restores_tiles() {
        let mut doc = Document::new((100, 100));
        let id = doc.active_id();
        let mut h = History::new();
        let key = (0, 0);
        // Snapshot "avant" (aucune tuile), peint directement (comme le fait
        // l'app pendant le geste), puis "après" = tuile peinte.
        let before = doc.layers[0].raster.tiles.get(&key).cloned();
        doc.layers[0].raster.set_pixel(5, 5, [255, 0, 0, 255]);
        let after = doc.layers[0].raster.tiles.get(&key).cloned();
        h.push(
            &mut doc,
            Command::PaintRaster {
                layer: id,
                op: RasterOp::Brush,
                target: RasterTarget::Content,
                tiles: vec![(key, before, after)],
            },
        );
        assert_eq!(doc.layers[0].raster.get_pixel(5, 5), [255, 0, 0, 255]);
        h.undo(&mut doc);
        assert_eq!(doc.layers[0].raster.get_pixel(5, 5), [0, 0, 0, 0]);
        assert!(doc.layers[0].raster.is_empty());
        h.redo(&mut doc);
        assert_eq!(doc.layers[0].raster.get_pixel(5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn edit_pen_path_undo_redo_restores_anchor_position() {
        use crate::tools::pen::{Anchor, PenPath};
        let mut doc = Document::new((100, 100));
        let layer = doc.active_id();
        let mut h = History::new();

        let before_path = PenPath { anchors: vec![Anchor::corner((0.0, 0.0)), Anchor::corner((10.0, 0.0))], closed: false };
        let mut stroke = s();
        stroke.id = 1;
        stroke.anchors = Some(before_path.clone());
        stroke.points = before_path
            .sample()
            .into_iter()
            .map(|pos| crate::model::StrokePoint { pos, width: 4.0 })
            .collect();
        h.push(&mut doc, Command::AddStroke { layer, stroke });

        let before_points = doc.layers[0].strokes[0].points.clone();
        let mut after_path = before_path.clone();
        after_path.anchors[1].pos = (20.0, 5.0); // ancre déplacée
        let after_points = after_path
            .sample()
            .into_iter()
            .map(|pos| crate::model::StrokePoint { pos, width: 4.0 })
            .collect();

        h.push(
            &mut doc,
            Command::EditPenPath { layer, id: 1, before_path, before_points, after_path, after_points },
        );
        assert_eq!(doc.layers[0].strokes[0].anchors.as_ref().unwrap().anchors[1].pos, (20.0, 5.0));

        h.undo(&mut doc);
        assert_eq!(doc.layers[0].strokes[0].anchors.as_ref().unwrap().anchors[1].pos, (10.0, 0.0));

        h.redo(&mut doc);
        assert_eq!(doc.layers[0].strokes[0].anchors.as_ref().unwrap().anchors[1].pos, (20.0, 5.0));
    }

    #[test]
    fn paint_raster_on_mask_creates_and_restores_mask() {
        let mut doc = Document::new((100, 100));
        let id = doc.active_id();
        let mut h = History::new();
        assert!(doc.layers[0].mask.is_none());
        let key = (0, 0);
        // Le masque n'existe pas encore : "avant" est None des deux côtés
        // (pas de tuile), le calque `mask` lui-même naît de la commande.
        doc.layers[0].add_mask();
        let before: Option<crate::model::raster::Tile> = None;
        doc.layers[0].mask.as_mut().unwrap().set_pixel(5, 5, [0, 0, 0, 255]); // peint noir = masqué
        let after = doc.layers[0].mask.as_ref().unwrap().tiles.get(&key).cloned();
        h.push(
            &mut doc,
            Command::PaintRaster {
                layer: id,
                op: RasterOp::Brush,
                target: RasterTarget::Mask,
                tiles: vec![(key, before, after)],
            },
        );
        assert_eq!(doc.layers[0].mask.as_ref().unwrap().mask_coverage(5, 5), 0);
        h.undo(&mut doc);
        // La tuile redevient absente : le pixel revient "visible" par défaut.
        assert_eq!(doc.layers[0].mask.as_ref().unwrap().mask_coverage(5, 5), 255);
        h.redo(&mut doc);
        assert_eq!(doc.layers[0].mask.as_ref().unwrap().mask_coverage(5, 5), 0);
    }

    #[test]
    fn command_on_missing_layer_is_noop() {
        let mut doc = Document::new((100, 100));
        let mut h = History::new();
        h.push(&mut doc, Command::AddStroke { layer: 999, stroke: s() });
        assert_eq!(doc.layers[0].strokes.len(), 0);
    }
}
