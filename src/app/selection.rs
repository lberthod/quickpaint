//! Sélection d'éléments (marquee/lasso/baguette) et masque de sélection en
//! pixels (Sprint H) — extrait de `app` en sous-module (sprint.md T3.1,
//! suite du découpage commencé en ANALYSE.md §12.5) : les méthodes ne
//! partagent avec le reste de `app` que `Document`/`ElemGeom`/`SelectionCombine`
//! et les fonctions libres `hit::*`/`in_bounds`.
//!
//! Les méthodes appelées uniquement depuis `app` restent `pub(super)`
//! (comme `pen_edit`/`transform`) ; celles utilisées par `ui::toolbar`
//! (feather/dilate/contract, sélections nommées, invert) restent `pub`,
//! exactement comme avant l'extraction — pas un élargissement de l'API.

use super::{t, ElemGeom, PaintApp, SelectionCombine};
use crate::tools::hit;
use std::collections::HashSet;

impl PaintApp {
    /// Coins (coords doc) de l'image sélectionnée seule (pour le recadrage).
    pub(super) fn selected_image_corners(&self) -> Option<(u64, [(f32, f32); 4])> {
        let idx = self.single_image_idx()?;
        let im = &self.doc.layers[self.doc.active_layer].images[idx];
        let (mn, mx) = im.bounds();
        Some((im.id, [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)]))
    }

    /// Boîte englobante de toute la sélection (coords doc).
    pub(super) fn selection_bounds(&self) -> Option<((f32, f32), (f32, f32))> {
        let l = &self.doc.layers[self.doc.active_layer];
        let sel = &self.selection;
        let mut b = hit::bounds_of(l.strokes.iter().filter(|s| sel.contains(&s.id)));
        let extra = l
            .texts
            .iter()
            .filter(|t| sel.contains(&t.id))
            .map(|t| t.approx_bounds())
            .chain(l.images.iter().filter(|im| sel.contains(&im.id)).map(|im| im.bounds()));
        for (lo, hi) in extra {
            b = Some(match b {
                Some((mn, mx)) => {
                    ((mn.0.min(lo.0), mn.1.min(lo.1)), (mx.0.max(hi.0), mx.1.max(hi.1)))
                }
                None => (lo, hi),
            });
        }
        b
    }

    /// Ids sélectionnés par type (calque actif).
    pub(super) fn selection_ids(&self) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
        let l = &self.doc.layers[self.doc.active_layer];
        let sel = &self.selection;
        (
            l.strokes.iter().filter(|s| sel.contains(&s.id)).map(|s| s.id).collect(),
            l.texts.iter().filter(|t| sel.contains(&t.id)).map(|t| t.id).collect(),
            l.images.iter().filter(|im| sel.contains(&im.id)).map(|im| im.id).collect(),
        )
    }

    // --- Sélection par région : marquee / lasso / baguette (Sprint 1) --------

    /// Pour chaque élément du calque actif : (id, boîte englobante, centre).
    /// Sert au marquee (recouvrement de boîte) et au lasso (centre dans tracé).
    pub(super) fn active_elements_geom(&self) -> Vec<ElemGeom> {
        let l = &self.doc.layers[self.doc.active_layer];
        let mut out = Vec::new();
        for s in &l.strokes {
            if let Some(bb) = hit::bounds_of(std::iter::once(s)) {
                let center = ((bb.0 .0 + bb.1 .0) * 0.5, (bb.0 .1 + bb.1 .1) * 0.5);
                out.push((s.id, bb, center));
            }
        }
        for t in &l.texts {
            let bb = t.approx_bounds();
            let center = ((bb.0 .0 + bb.1 .0) * 0.5, (bb.0 .1 + bb.1 .1) * 0.5);
            out.push((t.id, bb, center));
        }
        for im in &l.images {
            let bb = im.bounds();
            let center = ((bb.0 .0 + bb.1 .0) * 0.5, (bb.0 .1 + bb.1 .1) * 0.5);
            out.push((im.id, bb, center));
        }
        out
    }

    /// Applique un ensemble d'ID retenus par un geste à `self.selection`
    /// selon le mode de combinaison (Sprint G.1) : Replace/Add se comportent
    /// comme l'ancien booléen `additive`, Subtract/Intersect opèrent sur les
    /// ensembles sans toucher aux ID non concernés par le geste.
    pub(super) fn apply_selection_combine(&mut self, combine: SelectionCombine, hit_ids: &HashSet<u64>) {
        match combine {
            SelectionCombine::Replace => {
                self.selection = hit_ids.clone();
            }
            SelectionCombine::Add => {
                self.selection.extend(hit_ids.iter().copied());
            }
            SelectionCombine::Subtract => {
                for id in hit_ids {
                    self.selection.remove(id);
                }
            }
            SelectionCombine::Intersect => {
                self.selection.retain(|id| hit_ids.contains(id));
            }
        }
        self.report_selection();
    }

    /// Peuple/combine le masque de sélection en pixels (Sprint H) depuis la
    /// géométrie d'un geste de sélection (rectangle/ellipse/lasso) — voir
    /// `tools::selection_mask`. `bounds` borne la zone parcourue,
    /// `coverage_at` teste chaque pixel (coords document, centre du pixel).
    pub(super) fn paint_selection_mask(&mut self, combine: SelectionCombine, bounds: ((f32, f32), (f32, f32)), coverage_at: impl Fn(f32, f32) -> f32) {
        let (w, h) = self.doc.size;
        let mut mask = self.selection_mask.take().unwrap_or_default();
        crate::tools::selection_mask::paint_mask_region(&mut mask, combine, w, h, bounds, coverage_at);
        self.selection_mask = Some(mask);
    }

    /// Sélectionne les éléments dont la boîte recoupe le rectangle (coords doc).
    /// `combine` pilote comment le résultat se combine à la sélection
    /// existante (Sprint G.1).
    pub(super) fn select_in_rect(&mut self, a: (f32, f32), b: (f32, f32), combine: SelectionCombine) {
        let rect = ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)));
        let hit_ids: HashSet<u64> = self
            .active_elements_geom()
            .into_iter()
            .filter(|(_, bb, _)| hit::bbox_intersects(rect, *bb))
            .map(|(id, _, _)| id)
            .collect();
        self.apply_selection_combine(combine, &hit_ids);
        self.paint_selection_mask(combine, rect, |_, _| 1.0);
    }

    /// Sélectionne les éléments dont le centre tombe dans l'ellipse inscrite
    /// dans le rectangle glissé (Sprint 2.1) — même test « centre » que le
    /// lasso, cohérent pour l'utilisateur.
    pub(super) fn select_in_ellipse(&mut self, a: (f32, f32), b: (f32, f32), combine: SelectionCombine) {
        let rect = ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)));
        let hit_ids: HashSet<u64> = self
            .active_elements_geom()
            .into_iter()
            .filter(|(_, _, center)| hit::point_in_ellipse(rect, *center))
            .map(|(id, _, _)| id)
            .collect();
        self.apply_selection_combine(combine, &hit_ids);
        self.paint_selection_mask(combine, rect, |x, y| if hit::point_in_ellipse(rect, (x, y)) { 1.0 } else { 0.0 });
    }

    /// Sélectionne les éléments dont le centre tombe dans le tracé du lasso.
    pub(super) fn select_in_lasso(&mut self, poly: &[(f32, f32)], combine: SelectionCombine) {
        let hit_ids: HashSet<u64> = self
            .active_elements_geom()
            .into_iter()
            .filter(|(_, _, center)| hit::point_in_polygon(poly, *center))
            .map(|(id, _, _)| id)
            .collect();
        self.apply_selection_combine(combine, &hit_ids);
        if let Some(bounds) = hit::bounds_of_points(poly) {
            self.paint_selection_mask(combine, bounds, |x, y| if hit::point_in_polygon(poly, (x, y)) { 1.0 } else { 0.0 });
        }
    }

    /// Baguette magique : sélectionne les traits et textes du calque actif dont
    /// la couleur est proche (par canal, ≤ `wand_tol`) de l'élément cliqué.
    /// Portée pilotée par `wand_global` : toute l'image, ou seulement la
    /// région connexe (voir [`Self::wand_region_ids`]).
    pub(super) fn magic_wand(&mut self, d: (f32, f32), combine: SelectionCombine) {
        let Some(clicked) = self.topmost_at(d) else {
            self.info(t("Baguette : aucun élément coloré ici.", "Wand: no colored element here."));
            return;
        };
        let Some(target) = self.color_at_active(d) else {
            self.info(t("Baguette : aucun élément coloré ici.", "Wand: no colored element here."));
            return;
        };
        let tol = self.wand_tol;
        let close = |c: [u8; 4]| {
            (0..4).all(|i| (c[i] as i32 - target[i] as i32).abs() <= tol)
        };
        let hit_ids: HashSet<u64> = if self.wand_global {
            let l = &self.doc.layers[self.doc.active_layer];
            l.strokes
                .iter()
                .filter(|s| close(s.color))
                .map(|s| s.id)
                .chain(l.texts.iter().filter(|t| close(t.color)).map(|t| t.id))
                .collect()
        } else {
            self.wand_region_ids(clicked, close).into_iter().collect()
        };
        self.apply_selection_combine(combine, &hit_ids);
        // Masque de sélection (Sprint H) : approximation par union des
        // boîtes englobantes des éléments retenus — la baguette n'a pas de
        // silhouette de geste unique comme rectangle/ellipse/lasso à
        // rasteriser directement (documenté comme limite connue).
        let geoms: Vec<((f32, f32), (f32, f32))> =
            self.active_elements_geom().into_iter().filter(|(id, _, _)| hit_ids.contains(id)).map(|(_, bb, _)| bb).collect();
        if let Some((mn, mx)) = geoms.iter().fold(None, |acc: Option<((f32, f32), (f32, f32))>, &(bmn, bmx)| match acc {
            Some((amn, amx)) => Some(((amn.0.min(bmn.0), amn.1.min(bmn.1)), (amx.0.max(bmx.0), amx.1.max(bmx.1)))),
            None => Some((bmn, bmx)),
        }) {
            self.paint_selection_mask(combine, (mn, mx), move |x, y| {
                if geoms.iter().any(|&(bmn, bmx)| x >= bmn.0 && x <= bmx.0 && y >= bmn.1 && y <= bmx.1) {
                    1.0
                } else {
                    0.0
                }
            });
        }
    }

    /// Région connexe pour la baguette en mode « Contigu » : élargit depuis
    /// `start` de proche en proche (boîtes englobantes qui se recoupent),
    /// en ne retenant que les éléments de couleur proche — pas de notion de
    /// pixel adjacent en modèle vectoriel, donc l'adjacence est celle des
    /// boîtes englobantes plutôt qu'une grille de pixels.
    fn wand_region_ids(&self, start: u64, close: impl Fn([u8; 4]) -> bool) -> Vec<u64> {
        let l = &self.doc.layers[self.doc.active_layer];
        let colors: std::collections::HashMap<u64, [u8; 4]> = l
            .strokes
            .iter()
            .map(|s| (s.id, s.color))
            .chain(l.texts.iter().map(|t| (t.id, t.color)))
            .collect();
        let geoms = self.active_elements_geom();
        let mut visited = HashSet::new();
        let mut queue = vec![start];
        while let Some(id) = queue.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some((_, bb, _)) = geoms.iter().find(|(gid, _, _)| *gid == id) else { continue };
            for (nid, nbb, _) in &geoms {
                if visited.contains(nid) {
                    continue;
                }
                if let Some(&c) = colors.get(nid) {
                    if close(c) && hit::bbox_intersects(*bb, *nbb) {
                        queue.push(*nid);
                    }
                }
            }
        }
        visited.into_iter().collect()
    }

    /// Couleur de l'élément (trait/texte) le plus haut sous `d` sur le calque actif.
    fn color_at_active(&self, d: (f32, f32)) -> Option<[u8; 4]> {
        let l = &self.doc.layers[self.doc.active_layer];
        if let Some(t) = l.texts.iter().rev().find(|t| super::in_bounds(d, t.approx_bounds())) {
            return Some(t.color);
        }
        l.strokes.iter().rev().find(|s| hit::point_on_stroke(s, d)).map(|s| s.color)
    }

    /// Met à jour le footer après une sélection par région.
    pub(super) fn report_selection(&mut self) {
        let n = self.selection.len();
        self.info(match n {
            0 => t("Aucun élément sélectionné.", "No element selected.").into(),
            1 => t("1 élément sélectionné.", "1 element selected.").into(),
            _ => format!("{n} {}", t("éléments sélectionnés.", "elements selected.")),
        });
    }

    /// Inverse la sélection (Sprint G.2) : garde tous les ID du calque actif
    /// qui n'étaient PAS sélectionnés, et retire ceux qui l'étaient.
    pub fn invert_selection(&mut self) {
        let all_ids: HashSet<u64> =
            self.active_elements_geom().into_iter().map(|(id, _, _)| id).collect();
        self.selection = all_ids.difference(&self.selection).copied().collect();
        self.report_selection();
    }

    // --- Masque de sélection en pixels (Sprint H) ----------------------------

    /// Contour progressif (feather) : adoucit le bord du masque de sélection
    /// sur environ `radius` pixels de large (flou du canal de couverture).
    /// Sans effet s'il n'y a pas de sélection par région active.
    pub fn feather_selection(&mut self, radius: f32) {
        let Some(mask) = &self.selection_mask else {
            self.info(t(
                "Aucune sélection par région (rectangle/ellipse/lasso) à adoucir.",
                "No region selection (rectangle/ellipse/lasso) to feather.",
            ));
            return;
        };
        let (w, h) = self.doc.size;
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        let feathered = crate::tools::selection_mask::feather(&dense, w as usize, h as usize, radius);
        self.selection_mask = Some(crate::tools::selection_mask::dense_to_mask(&feathered, w, h));
        self.info(t("Contour de la sélection adouci.", "Selection edge feathered."));
    }

    /// Dilate (grossit) la sélection en pixels de `amount` pixels.
    pub fn dilate_selection(&mut self, amount: i32) {
        let Some(mask) = &self.selection_mask else {
            self.info(t(
                "Aucune sélection par région (rectangle/ellipse/lasso) à dilater.",
                "No region selection (rectangle/ellipse/lasso) to dilate.",
            ));
            return;
        };
        let (w, h) = self.doc.size;
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        let grown = crate::tools::selection_mask::dilate(&dense, w as usize, h as usize, amount);
        self.selection_mask = Some(crate::tools::selection_mask::dense_to_mask(&grown, w, h));
        self.info(t("Sélection dilatée.", "Selection dilated."));
    }

    /// Amélioration des bords (audit_100_features.md #38, façon Refine
    /// Edge/Select and Mask) : affine le bord du masque de sélection selon
    /// la texture locale de l'image composée — les zones plates gardent un
    /// dégradé doux, les zones texturées (feuillage, cheveux, herbe…) sont
    /// durcies vers 0/255 pour coller aux détails fins plutôt que les
    /// moyenner. Réutilise `bucket::refine_edges`, jusqu'ici câblé
    /// uniquement au détourage en un clic (`bucket_cutout.rs`) — même
    /// algorithme, généralisé à n'importe quelle sélection par région.
    pub fn refine_selection_edges(&mut self, ctx: &egui::Context, radius: i32) {
        let Some(mask) = &self.selection_mask else {
            self.info(t(
                "Aucune sélection par région (rectangle/ellipse/lasso) à affiner.",
                "No region selection (rectangle/ellipse/lasso) to refine.",
            ));
            return;
        };
        let (w, h) = self.doc.size;
        let Some((rw, rh, rgba)) = self.compositor.render_to_rgba(ctx, &self.doc, self.bg) else {
            return;
        };
        if (rw, rh) != (w, h) {
            return;
        }
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        let refined = crate::tools::bucket::refine_edges(&rgba, w as usize, h as usize, &dense, radius);
        self.selection_mask = Some(crate::tools::selection_mask::dense_to_mask(&refined, w, h));
        self.info(t("Bords de la sélection affinés.", "Selection edges refined."));
    }

    /// Contracte (rétrécit) la sélection en pixels de `amount` pixels.
    pub fn contract_selection(&mut self, amount: i32) {
        let Some(mask) = &self.selection_mask else {
            self.info(t(
                "Aucune sélection par région (rectangle/ellipse/lasso) à contracter.",
                "No region selection (rectangle/ellipse/lasso) to contract.",
            ));
            return;
        };
        let (w, h) = self.doc.size;
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        let shrunk = crate::tools::selection_mask::erode(&dense, w as usize, h as usize, amount);
        self.selection_mask = Some(crate::tools::selection_mask::dense_to_mask(&shrunk, w, h));
        self.info(t("Sélection contractée.", "Selection contracted."));
    }

    // --- Sélections nommées (Sprint 1.2) ------------------------------------

    /// Enregistre la sélection courante du calque actif sous `name`. Écrase
    /// silencieusement une entrée existante du même nom **sur le même
    /// calque** (deux calques peuvent avoir chacun une sélection nommée
    /// identique sans se marcher dessus).
    pub fn save_named_selection(&mut self, name: String) {
        if name.trim().is_empty() || self.selection.is_empty() {
            return;
        }
        let layer = self.doc.active_id();
        let mut ids: Vec<u64> = self.selection.iter().copied().collect();
        ids.sort_unstable();
        let name = name.trim().to_string();
        if let Some(existing) = self.doc.named_selections.iter_mut().find(|s| s.name == name && s.layer == layer) {
            existing.ids = ids;
        } else {
            self.doc.named_selections.push(crate::model::NamedSelection { name: name.clone(), layer, ids });
        }
        self.history.touch();
        self.info(format!("{} « {name} ».", t("Sélection enregistrée", "Selection saved")));
    }

    /// Recharge une sélection nommée : bascule sur le calque qui la possède
    /// (si nécessaire) et ne restaure que les ids encore présents dans ce
    /// calque — un élément supprimé depuis l'enregistrement est simplement
    /// omis plutôt que de faire échouer tout le rechargement.
    pub fn load_named_selection(&mut self, name: &str) {
        let Some(saved) = self.doc.named_selections.iter().find(|s| s.name == name) else { return };
        let layer = saved.layer;
        let ids = saved.ids.clone();
        if let Some(idx) = self.doc.layers.iter().position(|l| l.id == layer) {
            self.doc.active_layer = idx;
        }
        let existing: HashSet<u64> = self.active_elements_geom().into_iter().map(|(id, _, _)| id).collect();
        self.selection = ids.into_iter().filter(|id| existing.contains(id)).collect();
        self.report_selection();
    }

    /// Supprime une sélection nommée (toutes celles portant ce nom, tous
    /// calques confondus — un nom identifie la sélection pour l'utilisateur,
    /// peu importe sur quel calque il l'avait enregistrée).
    pub fn delete_named_selection(&mut self, name: &str) {
        self.doc.named_selections.retain(|s| s.name != name);
        self.history.touch();
    }
}
