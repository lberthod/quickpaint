//! Pot de peinture et détourage en un clic : les deux outils inondent la
//! composition **affichée** (capture d'écran différée, `handle_screenshot`)
//! depuis le point cliqué, plutôt que le contenu du calque actif — seul
//! moyen de raisonner sur les pixels réellement vus (fusion de calques,
//! modes de fusion compris) sous le clic. Extrait de `app` en sous-module
//! (sprint.md T3.9, suite de T3.1-T3.8).

use super::{t, Command, PaintApp, RasterOp, RasterTarget};
use egui::Pos2;

impl PaintApp {
    /// Pot de peinture : inonde la composition affichée depuis le point cliqué,
    /// puis dépose le remplissage comme image sur le calque actif (roadmap #6).
    pub(super) fn do_bucket_fill(&mut self, ctx: &egui::Context, image: &egui::ColorImage, click: Pos2) {
        let ppp = ctx.pixels_per_point();
        let [iw, ih] = image.size;
        // Région physique du document visible.
        let r = self.last_doc_rect.intersect(self.last_canvas_rect);
        let x0 = (r.min.x * ppp).round().max(0.0) as usize;
        let y0 = (r.min.y * ppp).round().max(0.0) as usize;
        let rw = ((r.width() * ppp).round() as usize).min(iw.saturating_sub(x0));
        let rh = ((r.height() * ppp).round() as usize).min(ih.saturating_sub(y0));
        if rw == 0 || rh == 0 {
            return;
        }
        // Pixel cliqué dans la région.
        let cx = (click.x * ppp).round() as i64 - x0 as i64;
        let cy = (click.y * ppp).round() as i64 - y0 as i64;
        if cx < 0 || cy < 0 || cx as usize >= rw || cy as usize >= rh {
            return;
        }

        // Extrait la région en RGBA.
        let mut region = vec![0u8; rw * rh * 4];
        for y in 0..rh {
            for x in 0..rw {
                let px = image[(x0 + x, y0 + y)].to_srgba_unmultiplied();
                let i = (y * rw + x) * 4;
                region[i..i + 4].copy_from_slice(&px);
            }
        }

        let mask = crate::tools::bucket::flood(&region, rw, rh, cx as usize, cy as usize, 36);
        if !mask.iter().any(|&m| m) {
            return;
        }

        // Convertit le masque (résolution écran physique) en pixels document,
        // un par un dans l'espace document plutôt qu'écran : au zoom réel, un
        // pixel document peut couvrir plusieurs pixels écran (ou l'inverse) —
        // itérer côté document évite trous (zoom arrière) et travail redondant
        // (zoom avant), sans dépendre de la résolution de la capture d'écran.
        let view = self.current_view();
        let dp0 = view.screen_to_doc(r.min);
        let dp1 = view.screen_to_doc(egui::pos2(r.min.x + rw as f32 / ppp, r.min.y + rh as f32 / ppp));
        let (doc_w, doc_h) = (self.doc.size.0 as i32, self.doc.size.1 as i32);
        let dx0 = (dp0.0.floor() as i32).max(0);
        let dy0 = (dp0.1.floor() as i32).max(0);
        let dx1 = (dp1.0.ceil() as i32).min(doc_w);
        let dy1 = (dp1.1.ceil() as i32).min(doc_h);

        let fill = self.brush.color;
        let layer_id = self.doc.active_id();
        let mut hit: Vec<(i32, i32)> = Vec::new();
        for dy in dy0..dy1 {
            for dx in dx0..dx1 {
                let sp = view.doc_to_screen((dx as f32 + 0.5, dy as f32 + 0.5));
                let sx = ((sp.x * ppp).round() as i64) - x0 as i64;
                let sy = ((sp.y * ppp).round() as i64) - y0 as i64;
                if sx < 0 || sy < 0 || sx as usize >= rw || sy as usize >= rh {
                    continue;
                }
                if mask[sy as usize * rw + sx as usize] {
                    hit.push((dx, dy));
                }
            }
        }
        if hit.is_empty() {
            return;
        }

        let Some(layer) = self.doc.layers.iter().find(|l| l.id == layer_id) else { return };
        let mut before: std::collections::HashMap<crate::model::raster::TileKey, Option<crate::model::raster::Tile>> =
            Default::default();
        for &(dx, dy) in &hit {
            for key in crate::model::RasterLayer::tiles_touched(dx as f32, dy as f32, 0.5) {
                before.entry(key).or_insert_with(|| layer.raster.tiles.get(&key).cloned());
            }
        }
        let count = hit.len();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            for (dx, dy) in hit {
                layer.raster.set_pixel(dx, dy, [fill[0], fill[1], fill[2], 255]);
            }
        }
        let Some(layer) = self.doc.layers.iter().find(|l| l.id == layer_id) else { return };
        let tiles: Vec<_> = before
            .into_iter()
            .map(|(key, b)| (key, b, layer.raster.tiles.get(&key).cloned()))
            .collect();
        self.history.push(
            &mut self.doc,
            Command::PaintRaster { layer: layer_id, op: RasterOp::Bucket, target: RasterTarget::Content, tiles },
        );
        self.info(format!("{} ({count} px).", t("Zone remplie", "Area filled")));
    }

    /// Détourage en un clic (Sprint 9.1) : flood-fill depuis le point cliqué
    /// sur la composition affichée (comme le pot de peinture), bord dégradé
    /// par proximité de couleur (`bucket::soft_edge`), puis écrit comme
    /// masque de calque peint — 100 % local, aucun modèle ni réseau. Le
    /// résultat reste éditable ensuite au
    /// pinceau/gomme pixel via « Éditer le masque » (Sprint 9.3).
    ///
    /// `restore` (⌥+clic) inverse le geste : redonne de la visibilité au lieu
    /// d'en retirer, pour rattraper une zone trop agressivement détourée sans
    /// perdre le reste du masque. Les clics successifs sont cumulatifs dans
    /// les deux sens : `cutout_global` sélectionne toute la couleur proche de
    /// la zone visible plutôt que la seule région connexe au clic (fond
    /// visible par bouts, feuillage, grillage…).
    pub(super) fn do_cutout(&mut self, ctx: &egui::Context, image: &egui::ColorImage, click: Pos2, restore: bool) {
        let ppp = ctx.pixels_per_point();
        let [iw, ih] = image.size;
        let r = self.last_doc_rect.intersect(self.last_canvas_rect);
        let x0 = (r.min.x * ppp).round().max(0.0) as usize;
        let y0 = (r.min.y * ppp).round().max(0.0) as usize;
        let rw = ((r.width() * ppp).round() as usize).min(iw.saturating_sub(x0));
        let rh = ((r.height() * ppp).round() as usize).min(ih.saturating_sub(y0));
        if rw == 0 || rh == 0 {
            return;
        }
        let cx = (click.x * ppp).round() as i64 - x0 as i64;
        let cy = (click.y * ppp).round() as i64 - y0 as i64;
        if cx < 0 || cy < 0 || cx as usize >= rw || cy as usize >= rh {
            return;
        }
        let layer_id = self.doc.active_id();
        if restore && self.doc.layers.iter().find(|l| l.id == layer_id).is_none_or(|l| l.mask.is_none()) {
            self.info(t("Détourage : rien à restaurer (pas de masque).", "Cutout: nothing to restore (no mask yet)."));
            return;
        }

        let mut region = vec![0u8; rw * rh * 4];
        for y in 0..rh {
            for x in 0..rw {
                let px = image[(x0 + x, y0 + y)].to_srgba_unmultiplied();
                let i = (y * rw + x) * 4;
                region[i..i + 4].copy_from_slice(&px);
            }
        }

        let tolerance = self.cutout_tolerance as i32;
        let flooded = if self.cutout_global {
            crate::tools::bucket::flood_global(&region, rw, rh, cx as usize, cy as usize, tolerance)
        } else {
            crate::tools::bucket::flood(&region, rw, rh, cx as usize, cy as usize, tolerance)
        };
        if !flooded.iter().any(|&f| f) {
            self.info(t("Détourage : rien à retirer ici.", "Cutout: nothing to remove here."));
            return;
        }
        // Degré d'appartenance au fond dégradé par proximité de couleur
        // (Sprint 9.1, bords plus fins qu'un flou uniforme — voir
        // `bucket::soft_edge`) : 255 = pleinement fond, 0 = pleinement sujet.
        let mut membership = crate::tools::bucket::soft_edge(&region, rw, rh, cx as usize, cy as usize, tolerance, &flooded);
        if self.cutout_refine_edges {
            membership = crate::tools::bucket::refine_edges(&region, rw, rh, &membership, 2);
        }
        // En retrait, la visibilité est l'inverse de l'appartenance au fond
        // (fond franc → invisible) ; en restauration, elle la suit directement
        // (zone repeinte franchement fond → pleinement restaurée).
        let feathered: Vec<u8> = if restore {
            membership
        } else {
            membership.iter().map(|&m| 255 - m).collect()
        };

        let view = self.current_view();
        let dp0 = view.screen_to_doc(r.min);
        let dp1 = view.screen_to_doc(egui::pos2(r.min.x + rw as f32 / ppp, r.min.y + rh as f32 / ppp));
        let (doc_w, doc_h) = (self.doc.size.0 as i32, self.doc.size.1 as i32);
        let dx0 = (dp0.0.floor() as i32).max(0);
        let dy0 = (dp0.1.floor() as i32).max(0);
        let dx1 = (dp1.0.ceil() as i32).min(doc_w);
        let dy1 = (dp1.1.ceil() as i32).min(doc_h);

        let mask_ref = self.doc.layers.iter().find(|l| l.id == layer_id).and_then(|l| l.mask.as_ref());
        let mut before: std::collections::HashMap<crate::model::raster::TileKey, Option<crate::model::raster::Tile>> =
            Default::default();
        let mut writes: Vec<(i32, i32, u8)> = Vec::new();
        for dy in dy0..dy1 {
            for dx in dx0..dx1 {
                let sp = view.doc_to_screen((dx as f32 + 0.5, dy as f32 + 0.5));
                let sx = ((sp.x * ppp).round() as i64) - x0 as i64;
                let sy = ((sp.y * ppp).round() as i64) - y0 as i64;
                if sx < 0 || sy < 0 || sx as usize >= rw || sy as usize >= rh {
                    continue;
                }
                let cov = feathered[sy as usize * rw + sx as usize];
                let existing = mask_ref.map(|m| m.mask_coverage(dx, dy)).unwrap_or(255);
                // Cumulatif sans jamais reculer : en retrait, la visibilité ne
                // peut que baisser (min) ; en restauration, que remonter (max).
                let new_cov = if restore { cov.max(existing) } else { cov.min(existing) };
                if new_cov == existing {
                    continue; // rien à changer ici
                }
                for key in crate::model::RasterLayer::tiles_touched(dx as f32, dy as f32, 0.5) {
                    before.entry(key).or_insert_with(|| mask_ref.and_then(|m| m.tiles.get(&key).cloned()));
                }
                writes.push((dx, dy, new_cov));
            }
        }
        if writes.is_empty() {
            return;
        }
        let count = writes.len();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            let mask = layer.mask.get_or_insert_with(Default::default);
            for (dx, dy, cov) in writes {
                mask.set_pixel(dx, dy, [cov, cov, cov, 255]);
            }
        }
        let Some(layer) = self.doc.layers.iter().find(|l| l.id == layer_id) else { return };
        let tiles: Vec<_> = before
            .into_iter()
            .map(|(key, b)| (key, b, layer.mask.as_ref().and_then(|m| m.tiles.get(&key).cloned())))
            .collect();
        self.history.push(
            &mut self.doc,
            Command::PaintRaster { layer: layer_id, op: RasterOp::Cutout, target: RasterTarget::Mask, tiles },
        );
        self.info(format!("{} ({count} px).", t("Détourage appliqué", "Cutout applied")));
    }
}
