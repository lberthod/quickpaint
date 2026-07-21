//! Opérations sur les calques : alignement/répartition, aplatissement, et
//! gestion de la pile (ajout, suppression, groupes, réordonnancement) —
//! extrait de `app` en sous-module (sprint.md T3.2, suite de T3.1) : ne
//! partage avec le reste de `app` que `Document`/`Command::*`/`AlignMode`.
//!
//! `pub fn` là où le code existant était déjà `pub` (appelé depuis
//! `ui::toolbar`/`ui::layers`) — pas un élargissement de l'API, même
//! convention que `selection.rs`/`transform.rs`.

use super::{t, AlignMode, Command, LayerBounds, PaintApp};
use crate::tools::hit;

impl PaintApp {
    /// Aligne le contenu vectoriel du calque actif (traits ∪ textes ∪
    /// images) par rapport au document (Sprint I.2) — différent de
    /// `align()`, qui n'agit que sur les éléments sélectionnés. Les modes
    /// Distribute n'ont pas de sens pour un seul calque (répartir suppose
    /// plusieurs calques) — voir [`Self::distribute_layers`], qui s'appuie
    /// sur `layer_multi_select` (point 36 de l'audit).
    pub fn align_layer_to_document(&mut self, mode: AlignMode) {
        if matches!(mode, AlignMode::DistributeH | AlignMode::DistributeV) {
            self.info(t(
                "Répartir : sélectionne plusieurs calques dans le panneau (⇧/⌘+clic).",
                "Distribute: select multiple layers in the panel (Shift/Cmd+click).",
            ));
            return;
        }
        let geoms = self.active_elements_geom();
        if geoms.is_empty() {
            self.info(t("Calque vide.", "Empty layer."));
            return;
        }
        let mn_x = geoms.iter().map(|(_, (mn, _), _)| mn.0).fold(f32::INFINITY, f32::min);
        let mx_x = geoms.iter().map(|(_, (_, mx), _)| mx.0).fold(f32::NEG_INFINITY, f32::max);
        let mn_y = geoms.iter().map(|(_, (mn, _), _)| mn.1).fold(f32::INFINITY, f32::min);
        let mx_y = geoms.iter().map(|(_, (_, mx), _)| mx.1).fold(f32::NEG_INFINITY, f32::max);
        let (dw, dh) = self.doc.size;
        let (dw, dh) = (dw as f32, dh as f32);
        let (dx, dy) = match mode {
            AlignMode::Left => (-mn_x, 0.0),
            AlignMode::Right => (dw - mx_x, 0.0),
            AlignMode::CenterH => (dw * 0.5 - (mn_x + mx_x) * 0.5, 0.0),
            AlignMode::Top => (0.0, -mn_y),
            AlignMode::Bottom => (0.0, dh - mx_y),
            AlignMode::MiddleV => (0.0, dh * 0.5 - (mn_y + mx_y) * 0.5),
            AlignMode::DistributeH | AlignMode::DistributeV => unreachable!("filtré au-dessus"),
        };
        if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
            return;
        }
        let moves: Vec<(u64, (f32, f32))> = geoms.iter().map(|(id, _, _)| (*id, (dx, dy))).collect();
        let ids: Vec<u64> = moves.iter().map(|(id, _)| *id).collect();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::MoveEach { layer, moves });
        self.cache.invalidate(ids.iter());
        self.info(t("Calque aligné.", "Layer aligned."));
    }

    /// Boîte englobante de tout le contenu vectoriel d'un calque (traits ∪
    /// textes ∪ images) — même périmètre que `active_elements_geom` (pas le
    /// raster peint), généralisé à un calque arbitraire pour la distribution
    /// multi-calque. `None` si le calque n'a aucun de ces éléments.
    fn layer_content_bounds(l: &crate::model::Layer) -> Option<((f32, f32), (f32, f32))> {
        let mut mn = (f32::INFINITY, f32::INFINITY);
        let mut mx = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        let mut feed = |b: ((f32, f32), (f32, f32))| {
            mn.0 = mn.0.min(b.0 .0);
            mn.1 = mn.1.min(b.0 .1);
            mx.0 = mx.0.max(b.1 .0);
            mx.1 = mx.1.max(b.1 .1);
            any = true;
        };
        for s in &l.strokes {
            if let Some(b) = hit::bounds_of(std::iter::once(s)) {
                feed(b);
            }
        }
        for t in &l.texts {
            feed(t.approx_bounds());
        }
        for im in &l.images {
            feed(im.bounds());
        }
        any.then_some((mn, mx))
    }

    /// Décale tout le contenu vectoriel d'un calque de `(dx, dy)` — même
    /// périmètre que ci-dessus, pas le raster peint (limite déjà assumée par
    /// `align_layer_to_document`).
    fn shift_layer_content(l: &mut crate::model::Layer, dx: f32, dy: f32) {
        for s in &mut l.strokes {
            for p in &mut s.points {
                p.pos.0 += dx;
                p.pos.1 += dy;
            }
        }
        for t in &mut l.texts {
            t.pos.0 += dx;
            t.pos.1 += dy;
        }
        for im in &mut l.images {
            im.pos.0 += dx;
            im.pos.1 += dy;
        }
    }

    /// Répartit à espacement égal les calques sélectionnés dans le panneau
    /// (`layer_multi_select`, point 36 de l'audit) — les deux calques aux
    /// extrémités (par centre de leur boîte englobante, le long de l'axe)
    /// restent fixes, les calques intermédiaires sont espacés uniformément
    /// entre eux, comme la répartition d'éléments déjà existante (`align`
    /// avec `AlignMode::DistributeH/V`). Un seul pas d'annulation
    /// (`SetDoc`) : plusieurs calques bougent à la fois, pas de commande
    /// existante qui couvre un déplacement inter-calques.
    pub fn distribute_layers(&mut self, horizontal: bool) {
        let ids = self.layer_multi_select.clone();
        let mut items: Vec<LayerBounds> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| ids.contains(&l.id))
            .filter_map(|(i, l)| Self::layer_content_bounds(l).map(|b| (i, b)))
            .collect();
        if items.len() < 3 {
            self.info(t(
                "Répartir : sélectionne au moins 3 calques non vides.",
                "Distribute: select at least 3 non-empty layers.",
            ));
            return;
        }
        let center = |b: &((f32, f32), (f32, f32))| {
            if horizontal {
                (b.0 .0 + b.1 .0) * 0.5
            } else {
                (b.0 .1 + b.1 .1) * 0.5
            }
        };
        items.sort_by(|a, b| center(&a.1).total_cmp(&center(&b.1)));
        let n = items.len();
        let first_c = center(&items[0].1);
        let last_c = center(&items[n - 1].1);
        let step = (last_c - first_c) / (n as f32 - 1.0);

        let mut after = self.doc.clone();
        for (k, (li, b)) in items.iter().enumerate().skip(1).take(n - 2) {
            let target = first_c + step * k as f32;
            let delta = target - center(b);
            if horizontal {
                Self::shift_layer_content(&mut after.layers[*li], delta, 0.0);
            } else {
                Self::shift_layer_content(&mut after.layers[*li], 0.0, delta);
            }
        }
        self.push_doc_snapshot(
            after,
            if horizontal { "Calques répartis horizontalement" } else { "Calques répartis verticalement" },
        );
    }

    /// Aplatit tous les calques en un seul. Annulable. Sprint P (point 30) :
    /// le contenu peint (raster) de chaque calque est composé de bas en haut
    /// (opacité et masque cuits dans les pixels, comme `merge_down`) — il
    /// était silencieusement perdu avant. Les éléments vectoriels restent
    /// transférés tels quels, éditables (leur masquage éventuel n'est pas
    /// cuit — même limite documentée que `merge_down`).
    pub fn flatten(&mut self) {
        if self.doc.layers.len() <= 1 {
            return;
        }
        let before = self.doc.layers.clone();
        let before_active = self.doc.active_layer;
        let mut base = crate::model::Layer::new(1, t("Calque 1", "Layer 1"));
        for l in &before {
            base.strokes.extend(l.strokes.iter().cloned());
            base.texts.extend(l.texts.iter().cloned());
            base.images.extend(l.images.iter().cloned());
            if !l.raster.is_empty() {
                base.raster.composite_over(&l.raster, l.opacity, l.mask.as_ref());
            }
        }
        self.selection.clear();
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active, after: vec![base], after_active: 0 },
        );
        self.cache.clear();
        self.info(t("Calques aplatis.", "Layers flattened."));
    }

    // --- Calques (idée 1) ----------------------------------------------------

    pub fn add_layer(&mut self) {
        let id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        let n = self.doc.layers.len() + 1;
        let layer = crate::model::Layer::new(id, format!("{} {n}", t("Calque", "Layer")));
        let index = self.doc.layers.len();
        self.history.push(&mut self.doc, Command::AddLayer { index, layer: Box::new(layer) });
    }

    /// Ajoute un calque d'ajustement (roadmap F3) au-dessus du calque actif :
    /// non destructif, réversible, re-réglable en changeant simplement son
    /// filtre depuis le panneau de calques.
    pub fn add_adjustment_layer(&mut self, adjustment: crate::tools::filter::Adjustment) {
        let id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        let label = adjustment.label();
        let layer = crate::model::Layer::new_adjustment(id, format!("{} : {label}", t("Réglage", "Adjustment")), adjustment);
        let index = self.doc.active_layer + 1;
        self.history.push(&mut self.doc, Command::AddLayer { index, layer: Box::new(layer) });
        self.info(format!("{} « {label} » {}", t("Calque d'ajustement", "Adjustment layer"), t("ajouté.", "added.")));
    }

    /// Ajoute un calque de remplissage (Sprint I.1) au-dessus du calque actif :
    /// aucun contenu propre, peint tout son rectangle (uni/dégradé).
    pub fn add_fill_layer(&mut self, fill: crate::model::FillKind) {
        let id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        let label = fill.label();
        let layer = crate::model::Layer::new_fill(id, format!("{} : {label}", t("Remplissage", "Fill")), fill);
        let index = self.doc.active_layer + 1;
        self.history.push(&mut self.doc, Command::AddLayer { index, layer: Box::new(layer) });
        self.info(format!("{} « {label} » {}", t("Calque de remplissage", "Fill layer"), t("ajouté.", "added.")));
    }

    pub fn delete_active_layer(&mut self) {
        if self.doc.layers.len() <= 1 {
            return;
        }
        let i = self.doc.active_layer;
        let layer = self.doc.layers[i].clone();
        self.selection.clear();
        self.history.push(&mut self.doc, Command::RemoveLayer { index: i, layer: Box::new(layer) });
        self.cache.prune(&self.doc);
    }

    /// Regroupe le calque actif avec celui du dessous (dossier).
    pub fn group_with_below(&mut self) {
        let i = self.doc.active_layer;
        if i == 0 {
            return;
        }
        let name = self.doc.layers[i - 1]
            .group
            .clone()
            .unwrap_or_else(|| format!("{} {}", t("Groupe", "Group"), self.doc.next_layer_id));
        self.doc.next_layer_id += 1;
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        after[i].group = Some(name.clone());
        after[i - 1].group = Some(name);
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i },
        );
        self.info(t("Calques groupés.", "Layers grouped."));
    }

    /// Retire le calque actif de son groupe.
    pub fn ungroup_active(&mut self) {
        let i = self.doc.active_layer;
        if self.doc.layers[i].group.is_none() {
            return;
        }
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        after[i].group = None;
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i },
        );
    }

    /// Affiche / masque tous les calques d'un groupe.
    pub fn toggle_group(&mut self, name: &str) {
        let any_visible = self.doc.layers.iter().any(|l| l.group.as_deref() == Some(name) && l.visible);
        for l in &mut self.doc.layers {
            if l.group.as_deref() == Some(name) {
                l.visible = !any_visible;
            }
        }
        self.history.touch();
    }

    /// Déplace le calque actif vers le haut (`+1`) ou le bas (`-1`) de la pile.
    pub fn move_active_layer(&mut self, dir: i32) {
        let i = self.doc.active_layer;
        let j = i as i32 + dir;
        if j < 0 || j as usize >= self.doc.layers.len() {
            return;
        }
        let j = j as usize;
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        after.swap(i, j);
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: j },
        );
    }

    /// Déplace le calque `from_id` à la position qu'occupe `to_id` (UX-3.1,
    /// glisser-déposer dans le panneau) — même mécanisme que
    /// `move_active_layer` (`Command::SetLayers`), généralisé à une distance
    /// arbitraire. Identifie les calques par id, pas par index : robuste
    /// même si l'index affiché par l'UI datait d'une frame précédente
    /// (cohérent avec le reste de l'historique, qui référence toujours les
    /// calques par id — voir `history.rs`).
    pub fn reorder_layer(&mut self, from_id: u64, to_id: u64) {
        if from_id == to_id {
            return;
        }
        let Some(from) = self.doc.layers.iter().position(|l| l.id == from_id) else { return };
        let Some(to) = self.doc.layers.iter().position(|l| l.id == to_id) else { return };
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        let moved = after.remove(from);
        after.insert(to, moved);
        let before_active = self.doc.active_layer;
        let after_active = if before_active == from {
            to
        } else if from < before_active && before_active <= to {
            before_active - 1
        } else if to <= before_active && before_active < from {
            before_active + 1
        } else {
            before_active
        };
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active, after, after_active },
        );
    }
}
