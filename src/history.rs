//! Undo / Redo via pattern Command (section 5).
//!
//! Chaque action est une commande réversible. Les calques sont référencés par
//! **id stable** (pas par index) : suppression / réordonnancement ne corrompent
//! plus la pile. Une commande sur un calque disparu devient un no-op.

use crate::model::{Document, ImageItem, Layer, Stroke, TextItem};

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
    Clear { layer: u64, previous: Vec<Stroke> },
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
        )
    }

    /// Libellé court pour le panneau d'historique.
    pub fn label(&self) -> &'static str {
        match self {
            Command::AddStroke { .. } => "Trait",
            Command::AddMany { .. } => "Coller / dupliquer",
            Command::Erase { .. } => "Effacer",
            Command::SplitStrokes { .. } => "Gomme partielle",
            Command::Move { .. } => "Déplacer",
            Command::MoveEach { .. } => "Aligner",
            Command::SetZMany { .. } => "Réordonner",
            Command::Scale { .. } => "Mise à l'échelle",
            Command::Rotate { .. } => "Rotation",
            Command::Clear { .. } => "Vider le calque",
            Command::AddText { .. } => "Texte",
            Command::DeleteText { .. } => "Suppr. texte",
            Command::AddImage { .. } => "Image",
            Command::DeleteImage { .. } => "Suppr. image",
            Command::ReplaceImage { .. } => "Filtre / recadrage",
            Command::AddLayer { .. } => "Ajouter calque",
            Command::RemoveLayer { .. } => "Suppr. calque",
            Command::SetLayers { .. } => "Fusion / aplatir",
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
        Command::Clear { layer, previous } => {
            if let Some(l) = layer_mut(doc, *layer) {
                l.strokes = previous.clone();
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
    fn command_on_missing_layer_is_noop() {
        let mut doc = Document::new((100, 100));
        let mut h = History::new();
        h.push(&mut doc, Command::AddStroke { layer: 999, stroke: s() });
        assert_eq!(doc.layers[0].strokes.len(), 0);
    }
}
