//! Outils de peinture raster au pixel près : pinceau/gomme, aérographe,
//! tampon de clonage/correcteur, retouche locale (densité +/-, éponge, flou,
//! netteté) et estompe — extrait de `app` en sous-module (sprint.md T3.6,
//! suite de T3.1-T3.5) : partage l'undo par tuile (`touch_raster_tiles`/
//! `commit_raster_stroke`) et ne diffère que par la fonction de peinture
//! pixel appelée (`RasterLayer::stamp`/`clone_stamp`/`effect_segment`/…).

use super::{t, Command, PaintApp, RasterOp, RasterTarget, ViewTransform};

impl PaintApp {
    // --- Pinceau / gomme pixel (roadmap F1, ciblage masque en P2 #14) -------

    /// Vue immuable de la surface raster ciblée par le geste en cours :
    /// le contenu du calque, ou son masque si `editing_mask` est actif.
    fn active_raster(layer: &crate::model::Layer, mask: bool) -> Option<&crate::model::RasterLayer> {
        if mask {
            layer.mask.as_ref()
        } else {
            Some(&layer.raster)
        }
    }

    /// Vue mutable — crée le masque à la volée si nécessaire (premier trait).
    fn active_raster_mut(layer: &mut crate::model::Layer, mask: bool) -> &mut crate::model::RasterLayer {
        if mask {
            layer.mask.get_or_insert_with(Default::default)
        } else {
            &mut layer.raster
        }
    }

    /// Snapshotte (une seule fois par geste) l'état "avant" des tuiles
    /// recoupées par un tampon, pour l'undo par tuile. `mask` doit refléter
    /// **la surface réellement peinte** par l'appelant (pas forcément
    /// `self.editing_mask` — les outils de retouche locale, Sprint 11,
    /// n'écrivent jamais dans le masque et doivent donc toujours passer
    /// `false`, sans quoi le snapshot et l'écriture cibleraient deux
    /// surfaces différentes et l'undo perdrait silencieusement le geste).
    pub(super) fn touch_raster_tiles(&mut self, cx: f32, cy: f32, radius: f32, mask: bool) {
        let layer_id = self.doc.active_id();
        let Some(layer) = self.doc.layers.iter().find(|l| l.id == layer_id) else { return };
        let existing = Self::active_raster(layer, mask);
        for key in crate::model::RasterLayer::tiles_touched(cx, cy, radius) {
            self.raster_touch
                .entry(key)
                .or_insert_with(|| existing.and_then(|r| r.tiles.get(&key).cloned()));
        }
    }

    fn pixel_radius(&self, erase: bool) -> f32 {
        (if erase { self.eraser.width } else { self.brush.width }) * 0.5
    }

    pub(super) fn paint_raster_point(&mut self, d: (f32, f32), erase: bool) {
        let radius = self.pixel_radius(erase);
        self.touch_raster_tiles(d.0, d.1, radius, self.editing_mask);
        let color = self.brush.color;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        // Tampon personnalisé (Sprint J.2) : uniquement pour le pinceau (pas
        // la gomme), échantillonné à la place de la formule circulaire.
        let stamp = (!erase).then(|| self.brush.custom_stamp.clone()).flatten();
        // Masque de sélection en pixels (Sprint H) : restreint la peinture à
        // la zone sélectionnée quand il y en a une.
        let mask = self.selection_mask.as_ref();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            let raster = Self::active_raster_mut(layer, self.editing_mask);
            match &stamp {
                Some(s) => raster.stamp_custom(d.0, d.1, radius * 2.0, color, s, erase, mask),
                None => raster.stamp(d.0, d.1, radius, hardness, color, erase, mask),
            }
        }
        self.history.touch();
    }

    pub(super) fn paint_raster_segment(&mut self, from: (f32, f32), to: (f32, f32), erase: bool) {
        let radius = self.pixel_radius(erase);
        // Échantillonne le long du segment pour toucher toutes les tuiles
        // traversées (pas seulement les deux bouts).
        let dist = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = (dist / radius.max(1.0)).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.touch_raster_tiles(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t, radius, self.editing_mask);
        }
        let color = self.brush.color;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        let stamp = (!erase).then(|| self.brush.custom_stamp.clone()).flatten();
        let mask = self.selection_mask.as_ref();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            let raster = Self::active_raster_mut(layer, self.editing_mask);
            match &stamp {
                // Pas de version « segment » dédiée pour le tampon
                // personnalisé : on ré-échantillonne au même pas que le
                // sondage de tuiles ci-dessus, cohérent avec un trait continu.
                Some(s) => {
                    for i in 0..=steps {
                        let t = i as f32 / steps as f32;
                        let p = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                        raster.stamp_custom(p.0, p.1, radius * 2.0, color, s, erase, mask);
                    }
                }
                None => raster.stroke_segment(from, to, radius, hardness, color, erase, mask),
            }
        }
        self.history.touch();
    }

    /// Intervalle entre deux dépôts d'aérographe (Sprint J.1), en secondes.
    const AIRBRUSH_INTERVAL: f64 = 0.03;

    /// Aérographe (Sprint J.1) : contrairement au pinceau pixel qui ne dépose
    /// qu'au fil du glissé (`drag_started`/`dragged`), l'aérographe dépose en
    /// continu tant que le clic est maintenu, même sans déplacement — piloté
    /// par un accumulateur de temps écoulé plutôt qu'un événement de geste.
    pub(super) fn handle_airbrush(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let held = response.is_pointer_button_down_on();
        if !held {
            if self.airbrush_last_dab.is_some() {
                self.airbrush_last_dab = None;
                self.raster_stroke_last = None;
                self.commit_raster_stroke(RasterOp::Brush, self.editing_mask);
            }
            return;
        }
        let Some(p) = response.interact_pointer_pos().or_else(|| response.hover_pos()) else { return };
        let d = view.screen_to_doc(p);
        let now = ctx.input(|i| i.time);
        let should_dab = self.airbrush_last_dab.is_none_or(|last| now - last >= Self::AIRBRUSH_INTERVAL);
        if should_dab {
            self.paint_airbrush_dab(d);
            self.airbrush_last_dab = Some(now);
        }
        // Continu même sans déplacement : force une repaint proche de
        // l'intervalle de dépôt, sinon egui n'appellerait `update()` de
        // nouveau qu'au prochain événement d'entrée (mouvement, clic...).
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(Self::AIRBRUSH_INTERVAL));
    }

    /// Un dépôt d'aérographe : même tampon que le pinceau pixel
    /// (`RasterLayer::stamp`), mais à une opacité par dépôt plus faible — la
    /// couverture s'accumule à chaque dépôt successif au même endroit
    /// (blend source-over répété), plutôt que d'atteindre l'opacité pleine
    /// dès le premier contact comme un pinceau normal.
    pub(super) fn paint_airbrush_dab(&mut self, d: (f32, f32)) {
        let radius = self.pixel_radius(false);
        self.touch_raster_tiles(d.0, d.1, radius, self.editing_mask);
        let mut color = self.brush.color;
        color[3] = ((color[3] as f32) * 0.25).round().clamp(0.0, 255.0) as u8;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        let mask = self.selection_mask.as_ref();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            Self::active_raster_mut(layer, self.editing_mask).stamp(d.0, d.1, radius, hardness, color, false, mask);
        }
        self.history.touch();
    }

    /// Fin du geste : pousse UNE commande d'undo couvrant toutes les tuiles
    /// touchées (comme Photoshop/GIMP — un trait = un cran d'annulation).
    /// `mask` doit être la même surface que celle passée à `touch_raster_tiles`
    /// pour ce geste (voir sa doc).
    pub(super) fn commit_raster_stroke(&mut self, op: RasterOp, mask: bool) {
        if self.raster_touch.is_empty() {
            return;
        }
        let layer_id = self.doc.active_id();
        let target = if mask { RasterTarget::Mask } else { RasterTarget::Content };
        let before = std::mem::take(&mut self.raster_touch);

        // Verrouillage granulaire — transparence (audit point 28) : ne
        // s'applique qu'au contenu (pas au masque de calque peint, qui n'a
        // pas de notion de « transparence » comparable). Restaure l'alpha
        // d'origine de chaque pixel des tuiles touchées par ce geste (la
        // couleur du nouveau tampon est conservée) directement sur les
        // tuiles vivantes, avant de calculer l'« après » ci-dessous — donc
        // avant l'écran ET avant l'historique voient la même chose. Une
        // tuile absente avant le geste (entièrement transparente) reste
        // ainsi entièrement transparente : peindre ne peut pas rendre
        // opaque un pixel qui ne l'était pas.
        if !mask && self.doc.layers.iter().find(|l| l.id == layer_id).is_some_and(|l| l.lock_alpha) {
            if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
                for (key, b) in &before {
                    if let Some(after_tile) = layer.raster.tiles.get_mut(key) {
                        let n = after_tile.px.len() / 4;
                        match b {
                            Some(before_tile) => {
                                for i in 0..n {
                                    after_tile.px[i * 4 + 3] = before_tile.px[i * 4 + 3];
                                }
                            }
                            None => {
                                for i in 0..n {
                                    after_tile.px[i * 4 + 3] = 0;
                                }
                            }
                        }
                    }
                }
            }
        }

        let Some(layer) = self.doc.layers.iter().find(|l| l.id == layer_id) else { return };
        let current = Self::active_raster(layer, mask);
        let mut tiles = Vec::with_capacity(before.len());
        let mut changed = false;
        for (key, b) in before {
            let a = current.and_then(|r| r.tiles.get(&key).cloned());
            let same = match (&a, &b) {
                (Some(x), Some(y)) => x.px == y.px,
                (None, None) => true,
                _ => false,
            };
            if !same {
                changed = true;
            }
            tiles.push((key, b, a));
        }
        if !changed {
            return;
        }
        self.history.push(&mut self.doc, Command::PaintRaster { layer: layer_id, op, target, tiles });
    }

    // --- Tampon de clonage (roadmap P0 #5) ----------------------------------
    //
    // Alt+clic définit le point source ; le glissé peint en échantillonnant
    // depuis ce point avec un **décalage constant** (calculé une fois au
    // début du geste), comme dans GIMP/Photoshop : la source suit la
    // destination en parallèle pendant tout le trait.

    /// Geste partagé par le tampon de clonage et le correcteur (Sprint 8.3) :
    /// Alt+clic définit la source, glisser peint. Seule la fonction de
    /// peinture pixel diffère (`heal` bascule vers `heal_stamp*`, qui recale
    /// la couleur recopiée sur la zone cible au lieu de la recopier telle
    /// quelle).
    pub(super) fn handle_clone_stamp(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform, heal: bool) {
        let alt = ctx.input(|i| i.modifiers.alt);
        if alt {
            // `clicked()` seul rate parfois un clic quasi-immobile interprété
            // comme un micro-glissé par egui (jitter d'un pilote/tablette ou
            // d'un clic automatisé) : `drag_started()` couvre ce cas aussi.
            let pos = if response.clicked() || response.drag_started() {
                response.interact_pointer_pos()
            } else {
                None
            };
            if let Some(p) = pos {
                self.clone_source = Some(view.screen_to_doc(p));
                self.info(t("Source du tampon définie (glisser pour peindre).", "Stamp source set (drag to paint)."));
            }
            return;
        }
        let op = if heal { RasterOp::Heal } else { RasterOp::Clone };
        if response.drag_started() {
            if let (Some(p), Some(src)) = (response.interact_pointer_pos(), self.clone_source) {
                let d = view.screen_to_doc(p);
                self.clone_offset = Some((src.0 - d.0, src.1 - d.1));
                self.paint_clone_point(d, heal);
                self.raster_stroke_last = Some(d);
            } else if self.clone_source.is_none() {
                self.info(t("Alt+clic pour définir la source du tampon.", "Alt+click to set the stamp source."));
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                match self.raster_stroke_last {
                    Some(last) => self.paint_clone_segment(last, d, heal),
                    None => self.paint_clone_point(d, heal),
                }
                self.raster_stroke_last = Some(d);
            }
        } else if self.raster_stroke_last.is_some() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(op, false);
        }
        if response.drag_stopped() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(op, false);
        }
    }

    fn paint_clone_point(&mut self, d: (f32, f32), heal: bool) {
        let radius = self.brush.width * 0.5;
        self.touch_raster_tiles(d.0, d.1, radius, false);
        let offset = self.clone_offset.unwrap_or((0.0, 0.0));
        let opacity = self.brush.color[3] as f32 / 255.0;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            if heal {
                layer.raster.heal_stamp(d.0, d.1, radius, hardness, offset, opacity);
            } else {
                layer.raster.clone_stamp(d.0, d.1, radius, hardness, offset, opacity);
            }
        }
        self.history.touch();
    }

    fn paint_clone_segment(&mut self, from: (f32, f32), to: (f32, f32), heal: bool) {
        let radius = self.brush.width * 0.5;
        let dist = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = (dist / radius.max(1.0)).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.touch_raster_tiles(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t, radius, false);
        }
        let offset = self.clone_offset.unwrap_or((0.0, 0.0));
        let opacity = self.brush.color[3] as f32 / 255.0;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            if heal {
                layer.raster.heal_stamp_segment(from, to, radius, hardness, offset, opacity);
            } else {
                layer.raster.clone_stamp_segment(from, to, radius, hardness, offset, opacity);
            }
        }
        self.history.touch();
    }

    // --- Retouche locale : densité +/-, éponge, flou, netteté (Sprint 11) ---
    //
    // Les six outils partagent un seul geste (glisser sur la couche raster,
    // intensité = `effect_strength`) et une seule fonction pixel
    // (`RasterLayer::effect_segment` / `PixelEffect`) — seul l'enum passé en
    // paramètre change le résultat.

    pub(super) fn handle_pixel_effect(
        &mut self,
        response: &egui::Response,
        view: &ViewTransform,
        effect: crate::model::PixelEffect,
        op: RasterOp,
    ) {
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                self.paint_effect_point(d, effect);
                self.raster_stroke_last = Some(d);
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                match self.raster_stroke_last {
                    Some(last) => self.paint_effect_segment(last, d, effect),
                    None => self.paint_effect_point(d, effect),
                }
                self.raster_stroke_last = Some(d);
            }
        } else if self.raster_stroke_last.is_some() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(op, false);
        }
        if response.drag_stopped() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(op, false);
        }
    }

    fn paint_effect_point(&mut self, d: (f32, f32), effect: crate::model::PixelEffect) {
        let radius = self.brush.width * 0.5;
        self.touch_raster_tiles(d.0, d.1, radius, false);
        let strength = self.effect_strength;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.raster.effect_segment(d, d, radius, hardness, strength, effect);
        }
        self.history.touch();
    }

    fn paint_effect_segment(&mut self, from: (f32, f32), to: (f32, f32), effect: crate::model::PixelEffect) {
        let radius = self.brush.width * 0.5;
        let dist = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = (dist / radius.max(1.0)).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.touch_raster_tiles(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t, radius, false);
        }
        let strength = self.effect_strength;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.raster.effect_segment(from, to, radius, hardness, strength, effect);
        }
        self.history.touch();
    }

    /// Estompe (smudge, Sprint 11) : même geste, mais l'algorithme "pousse" la
    /// couleur plutôt que de la mélanger à une cible fixe — fonction dédiée
    /// dans `RasterLayer::smudge_segment`.
    pub(super) fn handle_smudge(&mut self, response: &egui::Response, view: &ViewTransform) {
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                self.touch_raster_tiles(d.0, d.1, self.brush.width * 0.5, false);
                self.raster_stroke_last = Some(d);
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if let Some(last) = self.raster_stroke_last {
                    let radius = self.brush.width * 0.5;
                    // Échantillonne le long du segment (pas seulement le point
                    // d'arrivée) : un glissé rapide entre deux frames doit
                    // snapshoter toutes les tuiles traversées, sinon l'undo
                    // manquerait certaines tuiles modifiées par `smudge_segment`.
                    let dist = ((d.0 - last.0).powi(2) + (d.1 - last.1).powi(2)).sqrt();
                    let steps = (dist / radius.max(1.0)).ceil().max(1.0) as i32;
                    for i in 0..=steps {
                        let t = i as f32 / steps as f32;
                        self.touch_raster_tiles(last.0 + (d.0 - last.0) * t, last.1 + (d.1 - last.1) * t, radius, false);
                    }
                    let strength = self.effect_strength;
                    let hardness = self.pixel_hardness;
                    let layer_id = self.doc.active_id();
                    if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
                        layer.raster.smudge_segment(last, d, radius, hardness, strength);
                    }
                    self.history.touch();
                }
                self.raster_stroke_last = Some(d);
            }
        } else if self.raster_stroke_last.is_some() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(RasterOp::Smudge, false);
        }
        if response.drag_stopped() {
            self.raster_stroke_last = None;
            self.commit_raster_stroke(RasterOp::Smudge, false);
        }
    }
}
