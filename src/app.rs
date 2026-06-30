//! État global de l'application + boucle de frame (sections 1, 6).
//!
//! `PaintApp` relie : modèle (`Document`), capture du geste, historique,
//! outils et rendu. La boucle `update` suit la séquence de la section 6 :
//! lire les évènements → mettre à jour le trait → UI → rendre.

use crate::history::{Command, History};
use crate::input::GestureCapture;
use crate::model::{Document, Stroke, Tool};
use crate::render::canvas::{self, ActiveStroke, StrokeCache, ViewTransform};
use crate::tools::{eyedropper, hit, shape, ActiveTool, Brush, Eraser, SelectMode};
use crate::ui::{footer, layers, toolbar};
use egui::{Color32, Margin, Pos2, Rect, Sense, Vec2};
use std::collections::HashSet;

/// (id d'élément, boîte englobante (min, max)) en coordonnées document.
type ElemBounds = (u64, ((f32, f32), (f32, f32)));

/// (id, boîte englobante (min, max), centre) — géométrie de sélection (Sprint 1).
type ElemGeom = (u64, ((f32, f32), (f32, f32)), (f32, f32));

/// Mouvements de profondeur (z-order) de la sélection.
#[derive(Clone, Copy)]
pub enum ZMove {
    Front,
    Forward,
    Backward,
    Back,
}

/// Modes d'alignement / répartition de la sélection.
#[derive(Clone, Copy)]
pub enum AlignMode {
    Left,
    CenterH,
    Right,
    Top,
    MiddleV,
    Bottom,
    DistributeH,
    DistributeV,
}

/// Presse-papiers interne (copier/coller d'éléments du document).
#[derive(Default)]
struct ClipBoard {
    strokes: Vec<Stroke>,
    texts: Vec<crate::model::TextItem>,
    images: Vec<crate::model::ImageItem>,
}

impl ClipBoard {
    fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.texts.is_empty() && self.images.is_empty()
    }
}

/// Transformation interactive de la sélection (échelle ou rotation).
enum XformKind {
    Scale { anchor: (f32, f32) },           // coin opposé fixe
    Rotate { center: (f32, f32), start: f32 }, // pivot + angle initial du pointeur
}

struct TransformDrag {
    kind: XformKind,
    bbox: ((f32, f32), (f32, f32)), // boîte de sélection au départ (doc)
    sx: f32,
    sy: f32,
    angle: f32,
}

pub struct PaintApp {
    pub doc: Document,
    pub history: History,
    pub capture: GestureCapture,
    pub active_tool: ActiveTool,
    pub brush: Brush,
    pub eraser: Eraser,
    pub bg: Color32,
    pub capture_pressure_strength: f32,
    /// Remplir les formes fermées au lieu du contour.
    pub fill_shapes: bool,
    /// Nombre de côtés des polygones / branches des étoiles.
    pub poly_sides: usize,
    /// Dernières couleurs utilisées (accès rapide, palette tactile).
    pub recent_colors: Vec<[u8; 3]>,
    /// Message éphémère affiché dans le footer (export, sauvegarde, etc.).
    pub status: Option<String>,
    /// Zoom et décalage (pan) de la vue (idée 2).
    pub zoom: f32,
    pub pan: Vec2,
    cache: StrokeCache,
    active_stroke: ActiveStroke,
    /// Compositeur CPU par calque (modes de fusion / opacité réelle, #8).
    compositor: crate::render::compositor::Compositor,
    /// Textures GPU des images, par id (non sérialisé, reconstruit au besoin).
    image_textures: std::collections::HashMap<u64, egui::TextureHandle>,
    next_id: u64,
    // Tracé de forme en cours (ligne / rectangle / ellipse).
    shape_start: Option<(f32, f32)>,
    shape_preview: Option<Stroke>,
    // Gomme vectorielle : ids des éléments survolés (effacés à la fin du geste).
    erase_pending: HashSet<u64>,
    /// Mode gomme : `false` = objet (supprime l'élément entier), `true` =
    /// partielle (n'efface que la portion touchée des traits).
    pub eraser_partial: bool,
    /// Trajet de la gomme (échantillons monde) pour la gomme partielle.
    erase_path: Vec<(f32, f32)>,
    // Sélection (outil flèche) : ids d'éléments du calque actif + déplacement.
    pub selection: HashSet<u64>,
    move_origin: Option<(f32, f32)>,
    move_delta: (f32, f32),
    // Sélection par région (Sprint 1) : sous-mode + tracés en cours (coords doc).
    pub select_mode: SelectMode,
    /// Rectangle de sélection en cours (coin de départ, coin courant).
    marquee: Option<((f32, f32), (f32, f32))>,
    /// Tracé du lasso en cours (échantillons monde).
    lasso: Vec<(f32, f32)>,
    /// Tolérance de la baguette magique (distance couleur par canal, 0–255).
    pub wand_tol: i32,
    clip: ClipBoard,
    // Transformation interactive de la sélection (échelle / rotation).
    xform: Option<TransformDrag>,
    // Recadrage d'image : mode actif + rectangle en cours (coords doc).
    crop_mode: bool,
    crop_rect: Option<((f32, f32), (f32, f32))>,
    /// Contrainte de ratio largeur/hauteur du recadrage (`None` = libre).
    pub crop_ratio: Option<f32>,
    // Plume (roadmap #9) : ancres du chemin en cours.
    pen: Vec<crate::tools::pen::Anchor>,
    // Grille / magnétisme (roadmap #10).
    pub show_grid: bool,
    pub snap_enabled: bool,
    pub grid_size: f32,
    /// Règles graduées le long du canvas (Sprint 2).
    pub show_rulers: bool,
    // Texte (roadmap #2) : taille courante + élément en cours d'édition.
    pub text_size: f32,
    editing_text: Option<u64>,
    text_focus_pending: bool,
    // Export bitmap (capture d'écran différée d'une frame) + format demandé.
    export_requested: bool,
    export_format: crate::export::ExportFormat,
    // Pot de peinture : point cliqué (écran) en attente de la capture.
    bucket_click: Option<Pos2>,
    last_canvas_rect: Rect,
    // Document à taille fixe (roadmap #3).
    last_doc_rect: Rect,
    view_initialized: bool,
}

impl Default for PaintApp {
    fn default() -> Self {
        Self {
            doc: Document::new((1280, 800)),
            history: History::new(),
            capture: GestureCapture::new(),
            active_tool: ActiveTool::Brush,
            brush: Brush::default(),
            eraser: Eraser::default(),
            bg: Color32::from_rgb(250, 250, 252),
            capture_pressure_strength: 0.8,
            fill_shapes: false,
            poly_sides: 6,
            recent_colors: Vec::new(),
            status: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            cache: StrokeCache::new(),
            active_stroke: ActiveStroke::default(),
            compositor: crate::render::compositor::Compositor::new(),
            image_textures: std::collections::HashMap::new(),
            next_id: 1, // 0 est réservé au trait en cours
            shape_start: None,
            shape_preview: None,
            erase_pending: HashSet::new(),
            eraser_partial: false,
            erase_path: Vec::new(),
            pen: Vec::new(),
            show_grid: false,
            snap_enabled: false,
            grid_size: 25.0,
            show_rulers: false,
            selection: HashSet::new(),
            move_origin: None,
            move_delta: (0.0, 0.0),
            select_mode: SelectMode::Rect,
            marquee: None,
            lasso: Vec::new(),
            wand_tol: 32,
            clip: ClipBoard::default(),
            xform: None,
            crop_mode: false,
            crop_rect: None,
            crop_ratio: None,
            text_size: 28.0,
            editing_text: None,
            text_focus_pending: false,
            export_requested: false,
            export_format: crate::export::ExportFormat::Png,
            bucket_click: None,
            last_canvas_rect: Rect::ZERO,
            last_doc_rect: Rect::ZERO,
            view_initialized: false,
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        Self::default()
    }

    pub fn clear_active_layer(&mut self) {
        let layer = self.doc.active_id();
        let previous = self.doc.layers[self.doc.active_layer].strokes.clone();
        if !previous.is_empty() {
            self.history.push(&mut self.doc, Command::Clear { layer, previous });
        }
    }

    pub fn new_document(&mut self) {
        self.apply_loaded(Document::new(self.doc.size));
        self.status = Some("Nouveau document.".into());
    }

    /// Profondeur monotone pour qu'un nouvel élément passe au-dessus des autres.
    fn bump_z(&mut self) -> f64 {
        let z = self.doc.next_z;
        self.doc.next_z += 1.0;
        z
    }

    fn commit_stroke(&mut self, mut stroke: Stroke) {
        if stroke.points.is_empty() {
            return;
        }
        stroke.id = self.next_id;
        self.next_id += 1;
        stroke.z = self.bump_z();
        self.push_recent_color(stroke.color);
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddStroke { layer, stroke });
    }

    /// Survol de la gomme. Mode partiel : mémorise le trajet ; mode objet :
    /// marque les éléments (traits/textes/images) touchés.
    fn erase_at(&mut self, world: (f32, f32)) {
        let radius = self.eraser.width * 0.5;
        if self.eraser_partial {
            self.erase_path.push(world);
            return;
        }
        let layer = &self.doc.layers[self.doc.active_layer];
        for s in &layer.strokes {
            if !self.erase_pending.contains(&s.id) && hit::stroke_near(s, world, radius) {
                self.erase_pending.insert(s.id);
            }
        }
        for t in &layer.texts {
            if !self.erase_pending.contains(&t.id) && in_bounds(world, expand_bounds(t.approx_bounds(), radius)) {
                self.erase_pending.insert(t.id);
            }
        }
        for im in &layer.images {
            if !self.erase_pending.contains(&im.id) && in_bounds(world, expand_bounds(im.bounds(), radius)) {
                self.erase_pending.insert(im.id);
            }
        }
    }

    /// Mode objet : retire les éléments entiers marqués (traits/textes/images).
    fn commit_erase(&mut self) {
        if self.erase_pending.is_empty() {
            return;
        }
        let layer_id = self.doc.active_id();
        let idx = self.doc.active_layer;
        let pending = &self.erase_pending;
        let l = &self.doc.layers[idx];
        let mut strokes: Vec<(usize, Stroke)> = l
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| pending.contains(&s.id))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        strokes.sort_by_key(|(i, _)| *i);
        let mut texts: Vec<(usize, crate::model::TextItem)> = l
            .texts
            .iter()
            .enumerate()
            .filter(|(_, t)| pending.contains(&t.id))
            .map(|(i, t)| (i, t.clone()))
            .collect();
        texts.sort_by_key(|(i, _)| *i);
        let mut images: Vec<(usize, crate::model::ImageItem)> = l
            .images
            .iter()
            .enumerate()
            .filter(|(_, im)| pending.contains(&im.id))
            .map(|(i, im)| (i, im.clone()))
            .collect();
        images.sort_by_key(|(i, _)| *i);
        self.erase_pending.clear();
        if !strokes.is_empty() {
            self.history.push(&mut self.doc, Command::Erase { layer: layer_id, removed: strokes });
        }
        if !texts.is_empty() {
            self.history.push(&mut self.doc, Command::DeleteText { layer: layer_id, removed: texts });
        }
        if !images.is_empty() {
            self.history.push(&mut self.doc, Command::DeleteImage { layer: layer_id, removed: images });
        }
    }

    /// Mode partiel : découpe les traits touchés par le trajet de la gomme,
    /// ne gardant que les portions hors de la gomme (annulable).
    fn commit_partial_erase(&mut self) {
        let path = std::mem::take(&mut self.erase_path);
        if path.is_empty() {
            return;
        }
        let radius = self.eraser.width * 0.5;
        let layer_id = self.doc.active_id();
        let idx = self.doc.active_layer;
        let mut removed: Vec<(usize, Stroke)> = Vec::new();
        let mut added: Vec<Stroke> = Vec::new();
        let mut next_id = self.next_id;

        for (i, s) in self.doc.layers[idx].strokes.iter().enumerate() {
            // Points effacés = à portée du trajet de la gomme.
            let erased: Vec<bool> = s
                .points
                .iter()
                .map(|p| {
                    let r = radius + p.width * 0.5;
                    path.iter().any(|q| {
                        (p.pos.0 - q.0).powi(2) + (p.pos.1 - q.1).powi(2) <= r * r
                    })
                })
                .collect();
            if !erased.iter().any(|&e| e) {
                continue; // trait non touché
            }
            removed.push((i, s.clone()));
            // Fragments = suites de points consécutifs non effacés.
            let mut run: Vec<crate::model::StrokePoint> = Vec::new();
            let flush = |run: &mut Vec<crate::model::StrokePoint>,
                         added: &mut Vec<Stroke>,
                         next_id: &mut u64| {
                if run.len() >= 2 {
                    let mut frag = Stroke::new(s.color, s.base_width, s.tool);
                    frag.id = *next_id;
                    *next_id += 1;
                    frag.z = s.z;
                    frag.fill = false; // un fragment n'est plus une forme pleine
                    frag.points = std::mem::take(run);
                    added.push(frag);
                } else {
                    run.clear();
                }
            };
            for (pt, &er) in s.points.iter().zip(&erased) {
                if er {
                    flush(&mut run, &mut added, &mut next_id);
                } else {
                    run.push(*pt);
                }
            }
            flush(&mut run, &mut added, &mut next_id);
        }
        self.next_id = next_id;
        if !removed.is_empty() {
            self.cache.invalidate(removed.iter().map(|(_, s)| &s.id));
            self.history.push(
                &mut self.doc,
                Command::SplitStrokes { layer: layer_id, removed, added },
            );
        }
    }

    // --- Sélection / déplacement (roadmap #1) -------------------------------

    /// Id de l'élément le plus en avant sous `d` (texte d'abord, puis trait).
    fn topmost_at(&self, d: (f32, f32)) -> Option<u64> {
        let layer = &self.doc.layers[self.doc.active_layer];
        if !layer.visible {
            return None;
        }
        if let Some(t) = layer.texts.iter().rev().find(|t| text_contains(t, d)) {
            return Some(t.id);
        }
        if let Some(im) = layer.images.iter().rev().find(|im| image_contains(im, d)) {
            return Some(im.id);
        }
        layer
            .strokes
            .iter()
            .rev()
            .find(|s| hit::point_on_stroke(s, d))
            .map(|s| s.id)
    }

    /// Id du texte sous `d` (pour ré-éditer avec l'outil Texte).
    fn text_at(&self, d: (f32, f32)) -> Option<u64> {
        let layer = &self.doc.layers[self.doc.active_layer];
        layer.texts.iter().rev().find(|t| text_contains(t, d)).map(|t| t.id)
    }

    /// Valide le déplacement de la sélection (annulable).
    fn commit_move(&mut self) {
        let (dx, dy) = self.move_delta;
        self.move_origin = None;
        self.move_delta = (0.0, 0.0);
        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            return;
        }
        self.push_move(dx, dy);
    }

    /// Enregistre un déplacement (dx, dy) de la sélection comme commande.
    fn push_move(&mut self, dx: f32, dy: f32) {
        if self.selection.is_empty() {
            return;
        }
        let layer = self.doc.active_id();
        let l = &self.doc.layers[self.doc.active_layer];
        let strokes: Vec<u64> =
            l.strokes.iter().filter(|s| self.selection.contains(&s.id)).map(|s| s.id).collect();
        let texts: Vec<u64> =
            l.texts.iter().filter(|t| self.selection.contains(&t.id)).map(|t| t.id).collect();
        let images: Vec<u64> =
            l.images.iter().filter(|im| self.selection.contains(&im.id)).map(|im| im.id).collect();
        self.cache.invalidate(strokes.iter());
        self.history
            .push(&mut self.doc, Command::Move { layer, strokes, texts, images, delta: (dx, dy) });
    }

    // --- Transformation de sélection (échelle / rotation) -------------------

    /// Index de l'image dans le calque actif si elle est la seule sélectionnée.
    fn single_image_idx(&self) -> Option<usize> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.iter().next()?;
        self.doc.layers[self.doc.active_layer].images.iter().position(|im| im.id == id)
    }

    /// Coins (coords doc) de l'image sélectionnée seule (pour le recadrage).
    fn selected_image_corners(&self) -> Option<(u64, [(f32, f32); 4])> {
        let idx = self.single_image_idx()?;
        let im = &self.doc.layers[self.doc.active_layer].images[idx];
        let (mn, mx) = im.bounds();
        Some((im.id, [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)]))
    }

    /// Boîte englobante de toute la sélection (coords doc).
    fn selection_bounds(&self) -> Option<((f32, f32), (f32, f32))> {
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
    fn selection_ids(&self) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
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
    fn active_elements_geom(&self) -> Vec<ElemGeom> {
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

    /// Sélectionne les éléments dont la boîte recoupe le rectangle (coords doc).
    /// `additive` (Maj) conserve la sélection existante.
    fn select_in_rect(&mut self, a: (f32, f32), b: (f32, f32), additive: bool) {
        let rect = ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)));
        if !additive {
            self.selection.clear();
        }
        for (id, bb, _) in self.active_elements_geom() {
            if hit::bbox_intersects(rect, bb) {
                self.selection.insert(id);
            }
        }
        self.report_selection();
    }

    /// Sélectionne les éléments dont le centre tombe dans le tracé du lasso.
    fn select_in_lasso(&mut self, poly: &[(f32, f32)], additive: bool) {
        if !additive {
            self.selection.clear();
        }
        for (id, _, center) in self.active_elements_geom() {
            if hit::point_in_polygon(poly, center) {
                self.selection.insert(id);
            }
        }
        self.report_selection();
    }

    /// Baguette magique : sélectionne les traits et textes du calque actif dont
    /// la couleur est proche (par canal, ≤ `wand_tol`) de l'élément cliqué.
    fn magic_wand(&mut self, d: (f32, f32), additive: bool) {
        let Some(target) = self.color_at_active(d) else {
            self.status = Some("Baguette : aucun élément coloré ici.".into());
            return;
        };
        let tol = self.wand_tol;
        let close = |c: [u8; 4]| {
            (0..4).all(|i| (c[i] as i32 - target[i] as i32).abs() <= tol)
        };
        if !additive {
            self.selection.clear();
        }
        let l = &self.doc.layers[self.doc.active_layer];
        let ids: Vec<u64> = l
            .strokes
            .iter()
            .filter(|s| close(s.color))
            .map(|s| s.id)
            .chain(l.texts.iter().filter(|t| close(t.color)).map(|t| t.id))
            .collect();
        for id in ids {
            self.selection.insert(id);
        }
        self.report_selection();
    }

    /// Couleur de l'élément (trait/texte) le plus haut sous `d` sur le calque actif.
    fn color_at_active(&self, d: (f32, f32)) -> Option<[u8; 4]> {
        let l = &self.doc.layers[self.doc.active_layer];
        if let Some(t) = l.texts.iter().rev().find(|t| in_bounds(d, t.approx_bounds())) {
            return Some(t.color);
        }
        l.strokes.iter().rev().find(|s| hit::point_on_stroke(s, d)).map(|s| s.color)
    }

    /// Met à jour le footer après une sélection par région.
    fn report_selection(&mut self) {
        let n = self.selection.len();
        self.status = Some(match n {
            0 => "Aucun élément sélectionné.".into(),
            1 => "1 élément sélectionné.".into(),
            _ => format!("{n} éléments sélectionnés."),
        });
    }

    /// 4 coins + poignée de rotation de la boîte de sélection (écran).
    fn transform_handles(&self, view: &ViewTransform) -> Option<([Pos2; 4], Pos2)> {
        let (mn, mx) = self.selection_bounds()?;
        let corners = [
            view.doc_to_screen((mn.0, mn.1)),
            view.doc_to_screen((mx.0, mn.1)),
            view.doc_to_screen((mx.0, mx.1)),
            view.doc_to_screen((mn.0, mx.1)),
        ];
        let top = view.doc_to_screen(((mn.0 + mx.0) * 0.5, mn.1));
        let rot = Pos2::new(top.x, top.y - 22.0);
        Some((corners, rot))
    }

    /// Démarre une transformation si le clic tombe sur une poignée.
    fn start_transform_if_handle(&mut self, p: Pos2, view: &ViewTransform) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let Some((corners, rot_handle)) = self.transform_handles(view) else { return false };
        let Some((mn, mx)) = self.selection_bounds() else { return false };
        let center = ((mn.0 + mx.0) * 0.5, (mn.1 + mx.1) * 0.5);
        // Rotation en priorité.
        if (rot_handle - p).length() <= 10.0 {
            let pc = view.doc_to_screen(center);
            let start = (p.y - pc.y).atan2(p.x - pc.x);
            self.xform = Some(TransformDrag {
                kind: XformKind::Rotate { center, start },
                bbox: (mn, mx),
                sx: 1.0,
                sy: 1.0,
                angle: 0.0,
            });
            return true;
        }
        // Coins → échelle.
        let doc_corners = [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)];
        for (ci, cs) in corners.iter().enumerate() {
            if (*cs - p).length() <= 10.0 {
                let anchor = doc_corners[(ci + 2) % 4];
                self.xform = Some(TransformDrag {
                    kind: XformKind::Scale { anchor },
                    bbox: (mn, mx),
                    sx: 1.0,
                    sy: 1.0,
                    angle: 0.0,
                });
                return true;
            }
        }
        false
    }

    /// Met à jour les paramètres de la transformation pendant le glissé.
    fn update_transform(&mut self, p: Pos2, view: &ViewTransform, uniform: bool) {
        let Some(x) = &mut self.xform else { return };
        match x.kind {
            XformKind::Scale { anchor } => {
                let d = view.screen_to_doc(p);
                let (w, h) = (x.bbox.1 .0 - anchor.0, x.bbox.1 .1 - anchor.1);
                // Largeur/hauteur signées entre l'ancre et le coin tiré.
                let denom_x = if w.abs() < 1e-3 { x.bbox.0 .0 - anchor.0 } else { w };
                let denom_y = if h.abs() < 1e-3 { x.bbox.0 .1 - anchor.1 } else { h };
                let mut sx = if denom_x.abs() > 1e-3 { (d.0 - anchor.0) / denom_x } else { 1.0 };
                let mut sy = if denom_y.abs() > 1e-3 { (d.1 - anchor.1) / denom_y } else { 1.0 };
                if uniform {
                    let s = sx.abs().max(sy.abs());
                    sx = s * sx.signum();
                    sy = s * sy.signum();
                }
                x.sx = sx.clamp(-20.0, 20.0);
                x.sy = sy.clamp(-20.0, 20.0);
            }
            XformKind::Rotate { center, start } => {
                let pc = view.doc_to_screen(center);
                let a = (p.y - pc.y).atan2(p.x - pc.x);
                let mut da = a - start;
                if uniform {
                    // Maj : par pas de 15°.
                    let step = std::f32::consts::FRAC_PI_8 * 0.5;
                    da = (da / step).round() * step;
                }
                x.angle = da;
            }
        }
    }

    /// Valide la transformation en cours (annulable).
    fn commit_transform(&mut self) {
        let Some(x) = self.xform.take() else { return };
        let (strokes, texts, images) = self.selection_ids();
        let layer = self.doc.active_id();
        match x.kind {
            XformKind::Scale { anchor } => {
                if (x.sx - 1.0).abs() < 1e-3 && (x.sy - 1.0).abs() < 1e-3 {
                    return;
                }
                if x.sx.abs() < 1e-2 || x.sy.abs() < 1e-2 {
                    return;
                }
                self.history.push(
                    &mut self.doc,
                    Command::Scale { layer, strokes: strokes.clone(), texts, images, pivot: anchor, sx: x.sx, sy: x.sy },
                );
            }
            XformKind::Rotate { center, .. } => {
                if x.angle.abs() < 1e-3 {
                    return;
                }
                self.history.push(
                    &mut self.doc,
                    Command::Rotate { layer, strokes: strokes.clone(), texts, images, pivot: center, angle: x.angle },
                );
            }
        }
        self.cache.invalidate(strokes.iter());
    }

    // --- Recadrage d'image --------------------------------------------------

    /// Applique la contrainte de ratio au coin courant `e` du recadrage (à
    /// partir du coin d'ancrage `s`). Sans contrainte, renvoie `e` tel quel.
    fn constrain_crop(&self, s: (f32, f32), e: (f32, f32)) -> (f32, f32) {
        let Some(ratio) = self.crop_ratio else { return e };
        if ratio <= 0.0 {
            return e;
        }
        // Signe de la direction de glissé (0 → +1, pour rester déterministe).
        let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
        let (dx, dy) = (e.0 - s.0, e.1 - s.1);
        // La dimension la plus « ample » impose la taille ; l'autre suit le ratio.
        let h = (dx.abs() / ratio).max(dy.abs());
        let w = h * ratio;
        (s.0 + w * sign(dx), s.1 + h * sign(dy))
    }

    /// `true` si le mode recadrage est actif (UI : sélecteur de ratio).
    pub fn is_cropping(&self) -> bool {
        self.crop_mode
    }

    /// Active le mode recadrage si une seule image est sélectionnée.
    pub fn start_crop(&mut self) {
        if self.single_image_idx().is_some() {
            self.crop_mode = true;
            self.crop_rect = None;
            self.active_tool = ActiveTool::Select;
            self.status = Some("Recadrage : glissez la zone à garder.".into());
        } else {
            self.status = Some("Sélectionne d'abord une image.".into());
        }
    }

    /// Applique le recadrage du rectangle courant à l'image sélectionnée.
    fn apply_crop(&mut self) {
        let Some((a, b)) = self.crop_rect.take() else {
            self.crop_mode = false;
            return;
        };
        self.crop_mode = false;
        let Some(idx) = self.single_image_idx() else { return };
        let layer = self.doc.active_id();
        let im = &self.doc.layers[self.doc.active_layer].images[idx];
        // Rectangle de recadrage borné à l'image (coords doc).
        let (mn, mx) = im.bounds();
        let cx0 = a.0.min(b.0).max(mn.0);
        let cy0 = a.1.min(b.1).max(mn.1);
        let cx1 = a.0.max(b.0).min(mx.0);
        let cy1 = a.1.max(b.1).min(mx.1);
        if cx1 - cx0 < 4.0 || cy1 - cy0 < 4.0 {
            return; // trop petit
        }
        // Conversion doc → pixels source de l'image.
        let sx = im.w as f32 / im.size.0.max(1.0);
        let sy = im.h as f32 / im.size.1.max(1.0);
        let px0 = (((cx0 - im.pos.0) * sx) as i64).clamp(0, im.w as i64) as u32;
        let py0 = (((cy0 - im.pos.1) * sy) as i64).clamp(0, im.h as i64) as u32;
        let px1 = (((cx1 - im.pos.0) * sx).ceil() as i64).clamp(0, im.w as i64) as u32;
        let py1 = (((cy1 - im.pos.1) * sy).ceil() as i64).clamp(0, im.h as i64) as u32;
        let (nw, nh) = (px1.saturating_sub(px0), py1.saturating_sub(py0));
        if nw == 0 || nh == 0 {
            return;
        }
        // Extraction du sous-rectangle de pixels.
        let mut out = Vec::with_capacity((nw * nh * 4) as usize);
        for y in py0..py1 {
            let row = ((y * im.w + px0) * 4) as usize;
            out.extend_from_slice(&im.rgba[row..row + (nw * 4) as usize]);
        }
        let before = im.clone();
        let id = before.id;
        let mut after = crate::model::ImageItem::from_rgba(id, (cx0, cy0), nw, nh, out);
        after.size = (cx1 - cx0, cy1 - cy0);
        self.history.push(
            &mut self.doc,
            Command::ReplaceImage { layer, id, before: Box::new(before), after: Box::new(after) },
        );
        self.image_textures.remove(&id);
        self.status = Some("Image recadrée.".into());
    }

    /// Supprime les éléments sélectionnés (Suppr) : traits et textes.
    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let layer = self.doc.active_id();
        let idx = self.doc.active_layer;
        let mut removed: Vec<(usize, Stroke)> = self.doc.layers[idx]
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| self.selection.contains(&s.id))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        removed.sort_by_key(|(i, _)| *i);
        let mut removed_text: Vec<(usize, crate::model::TextItem)> = self.doc.layers[idx]
            .texts
            .iter()
            .enumerate()
            .filter(|(_, t)| self.selection.contains(&t.id))
            .map(|(i, t)| (i, t.clone()))
            .collect();
        removed_text.sort_by_key(|(i, _)| *i);
        let mut removed_img: Vec<(usize, crate::model::ImageItem)> = self.doc.layers[idx]
            .images
            .iter()
            .enumerate()
            .filter(|(_, im)| self.selection.contains(&im.id))
            .map(|(i, im)| (i, im.clone()))
            .collect();
        removed_img.sort_by_key(|(i, _)| *i);
        if !removed.is_empty() {
            self.history.push(&mut self.doc, Command::Erase { layer, removed });
        }
        if !removed_text.is_empty() {
            self.history.push(&mut self.doc, Command::DeleteText { layer, removed: removed_text });
        }
        if !removed_img.is_empty() {
            self.history.push(&mut self.doc, Command::DeleteImage { layer, removed: removed_img });
        }
        self.selection.clear();
    }

    // --- Texte (roadmap #2) -------------------------------------------------

    /// Clic outil Texte : édite le texte sous le curseur, sinon en crée un.
    fn create_or_edit_text(&mut self, d: (f32, f32)) {
        self.finish_text_editing();
        if let Some(id) = self.text_at(d) {
            self.editing_text = Some(id);
        } else {
            let id = self.next_id;
            self.next_id += 1;
            let color = [self.brush.color[0], self.brush.color[1], self.brush.color[2], 255];
            let mut item = crate::model::TextItem::new(id, d, self.text_size, color);
            item.z = self.bump_z();
            let layer = self.doc.active_id();
            self.history.push(&mut self.doc, Command::AddText { layer, text: item });
            self.editing_text = Some(id);
        }
        self.text_focus_pending = true;
    }

    /// Termine l'édition en cours ; supprime le texte s'il est resté vide.
    fn finish_text_editing(&mut self) {
        let Some(id) = self.editing_text.take() else { return };
        for layer in &mut self.doc.layers {
            layer.texts.retain(|t| !(t.id == id && t.text.trim().is_empty()));
        }
    }

    // --- Image (roadmap #7) -------------------------------------------------

    /// Place une image (pixels RGBA bruts) centrée et ajustée, et la sélectionne.
    fn place_image(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        let (dw, dh) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        let scale = ((dw * 0.8 / w.max(1) as f32).min(dh * 0.8 / h.max(1) as f32)).min(1.0);
        let (sw, sh) = (w as f32 * scale, h as f32 * scale);
        let pos = ((dw - sw) * 0.5, (dh - sh) * 0.5);
        let id = self.next_id;
        self.next_id += 1;
        let mut item = crate::model::ImageItem::from_rgba(id, pos, w, h, rgba);
        item.size = (sw, sh);
        item.z = self.bump_z();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddImage { layer, image: item });
        self.selection.clear();
        self.selection.insert(id);
        self.active_tool = ActiveTool::Select;
    }

    /// Importe une image depuis un fichier.
    pub fn import_image(&mut self) {
        let Some((w, h, rgba)) = crate::project::import_image_dialog() else { return };
        self.place_image(w, h, rgba);
        self.status = Some("Image importée — déplacez-la (outil Sélection).".into());
    }

    /// Colle une image depuis le presse-papiers (⌘V) — cœur du cas « comparer ».
    pub fn paste_image(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                self.place_image(w, h, img.bytes.into_owned());
                self.status = Some("Image collée depuis le presse-papiers.".into());
            }
            Err(_) => {
                self.status = Some("Aucune image dans le presse-papiers.".into());
            }
        }
    }

    /// Aligne toutes les images du calque actif côte à côte, à hauteur égale
    /// (comparaison d'images façon Freeboard).
    pub fn align_images_row(&mut self) {
        let i = self.doc.active_layer;
        if self.doc.layers[i].images.is_empty() {
            return;
        }
        let target_h = self
            .doc
            .layers[i]
            .images
            .iter()
            .map(|im| im.size.1)
            .fold(0.0_f32, f32::max)
            .max(1.0);
        // Annulable : on mute une copie de la pile puis on enregistre SetLayers.
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        let (gap, mut x, y) = (16.0, 16.0, 16.0);
        for im in &mut after[i].images {
            let s = target_h / im.size.1.max(1.0);
            im.size = (im.size.0 * s, target_h);
            im.pos = (x, y);
            x += im.size.0 + gap;
        }
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i },
        );
        self.status = Some("Images alignées côte à côte.".into());
    }

    /// Applique un filtre aux images sélectionnées du calque actif (annulable).
    pub fn filter_selection(&mut self, filter: crate::tools::filter::Filter) {
        let idx = self.doc.active_layer;
        let layer = self.doc.active_id();
        let ids: Vec<u64> = self.doc.layers[idx]
            .images
            .iter()
            .filter(|im| self.selection.contains(&im.id))
            .map(|im| im.id)
            .collect();
        if ids.is_empty() {
            self.status = Some("Sélectionne une image (outil Sélection).".into());
            return;
        }
        for id in ids {
            let Some(before) =
                self.doc.layers[idx].images.iter().find(|im| im.id == id).cloned()
            else {
                continue;
            };
            let mut rgba = before.rgba.clone();
            crate::tools::filter::apply(filter, &mut rgba, before.w, before.h);
            let mut after = crate::model::ImageItem::from_rgba(id, before.pos, before.w, before.h, rgba);
            after.size = before.size;
            self.history.push(
                &mut self.doc,
                Command::ReplaceImage {
                    layer,
                    id,
                    before: Box::new(before),
                    after: Box::new(after),
                },
            );
            self.image_textures.remove(&id);
        }
        self.status = Some(format!("Filtre appliqué : {}", filter.label()));
    }

    // --- Fusion de calques --------------------------------------------------

    /// Fusionne le calque actif dans celui du dessous (Merge Down). Annulable.
    pub fn merge_down(&mut self) {
        let i = self.doc.active_layer;
        if i == 0 {
            return;
        }
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        let upper = after.remove(i);
        let lower = &mut after[i - 1];
        lower.strokes.extend(upper.strokes);
        lower.texts.extend(upper.texts);
        lower.images.extend(upper.images);
        self.selection.clear();
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i - 1 },
        );
        self.status = Some("Calque fusionné vers le bas.".into());
    }

    /// Duplique le calque actif (nouveaux ids), inséré au-dessus. Annulable.
    pub fn duplicate_layer(&mut self) {
        let i = self.doc.active_layer;
        let mut dup = self.doc.layers[i].clone();
        dup.id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        dup.name = format!("{} copie", dup.name);
        for s in &mut dup.strokes {
            s.id = self.next_id;
            self.next_id += 1;
        }
        for t in &mut dup.texts {
            t.id = self.next_id;
            self.next_id += 1;
        }
        for im in &mut dup.images {
            im.id = self.next_id;
            self.next_id += 1;
        }
        self.selection.clear();
        self.history.push(&mut self.doc, Command::AddLayer { index: i + 1, layer: Box::new(dup) });
        self.status = Some("Calque dupliqué.".into());
    }

    // --- Alignement / répartition (backlog) ---------------------------------

    /// Boîtes (id, (min, max)) des éléments sélectionnés du calque actif.
    fn selected_elements_bounds(&self) -> Vec<ElemBounds> {
        let l = &self.doc.layers[self.doc.active_layer];
        let sel = &self.selection;
        let mut v = Vec::new();
        for s in &l.strokes {
            if sel.contains(&s.id) {
                if let Some(b) = hit::bounds_of(std::iter::once(s)) {
                    v.push((s.id, b));
                }
            }
        }
        for t in &l.texts {
            if sel.contains(&t.id) {
                v.push((t.id, t.approx_bounds()));
            }
        }
        for im in &l.images {
            if sel.contains(&im.id) {
                v.push((im.id, im.bounds()));
            }
        }
        v
    }

    pub fn align(&mut self, mode: AlignMode) {
        let elems = self.selected_elements_bounds();
        if elems.len() < 2 {
            self.status = Some("Sélectionne au moins 2 éléments.".into());
            return;
        }
        let gmin_x = elems.iter().map(|(_, (mn, _))| mn.0).fold(f32::INFINITY, f32::min);
        let gmax_x = elems.iter().map(|(_, (_, mx))| mx.0).fold(f32::NEG_INFINITY, f32::max);
        let gmin_y = elems.iter().map(|(_, (mn, _))| mn.1).fold(f32::INFINITY, f32::min);
        let gmax_y = elems.iter().map(|(_, (_, mx))| mx.1).fold(f32::NEG_INFINITY, f32::max);
        let gc_x = (gmin_x + gmax_x) * 0.5;
        let gc_y = (gmin_y + gmax_y) * 0.5;

        let mut moves: Vec<(u64, (f32, f32))> = Vec::new();
        match mode {
            AlignMode::Left => for (id, (mn, _)) in &elems { moves.push((*id, (gmin_x - mn.0, 0.0))); },
            AlignMode::Right => for (id, (_, mx)) in &elems { moves.push((*id, (gmax_x - mx.0, 0.0))); },
            AlignMode::CenterH => for (id, (mn, mx)) in &elems {
                moves.push((*id, (gc_x - (mn.0 + mx.0) * 0.5, 0.0)));
            },
            AlignMode::Top => for (id, (mn, _)) in &elems { moves.push((*id, (0.0, gmin_y - mn.1))); },
            AlignMode::Bottom => for (id, (_, mx)) in &elems { moves.push((*id, (0.0, gmax_y - mx.1))); },
            AlignMode::MiddleV => for (id, (mn, mx)) in &elems {
                moves.push((*id, (0.0, gc_y - (mn.1 + mx.1) * 0.5)));
            },
            AlignMode::DistributeH | AlignMode::DistributeV => {
                if elems.len() < 3 {
                    self.status = Some("Répartir : au moins 3 éléments.".into());
                    return;
                }
                let horiz = matches!(mode, AlignMode::DistributeH);
                let center = |b: &((f32, f32), (f32, f32))| {
                    if horiz { (b.0 .0 + b.1 .0) * 0.5 } else { (b.0 .1 + b.1 .1) * 0.5 }
                };
                let mut sorted = elems.clone();
                sorted.sort_by(|a, b| center(&a.1).total_cmp(&center(&b.1)));
                let first = center(&sorted.first().unwrap().1);
                let last = center(&sorted.last().unwrap().1);
                let step = (last - first) / (sorted.len() as f32 - 1.0);
                for (i, (id, b)) in sorted.iter().enumerate() {
                    let target = first + step * i as f32;
                    let d = target - center(b);
                    moves.push((*id, if horiz { (d, 0.0) } else { (0.0, d) }));
                }
            }
        }
        moves.retain(|(_, d)| d.0.abs() > 1e-3 || d.1.abs() > 1e-3);
        if moves.is_empty() {
            return;
        }
        let ids: Vec<u64> = moves.iter().map(|(id, _)| *id).collect();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::MoveEach { layer, moves });
        self.cache.invalidate(ids.iter());
        self.status = Some("Éléments alignés.".into());
    }

    /// Aplatit tous les calques (visibles) en un seul. Annulable.
    pub fn flatten(&mut self) {
        if self.doc.layers.len() <= 1 {
            return;
        }
        let before = self.doc.layers.clone();
        let before_active = self.doc.active_layer;
        let mut base = crate::model::Layer::new(1, "Calque 1");
        for l in &before {
            base.strokes.extend(l.strokes.iter().cloned());
            base.texts.extend(l.texts.iter().cloned());
            base.images.extend(l.images.iter().cloned());
        }
        self.selection.clear();
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active, after: vec![base], after_active: 0 },
        );
        self.status = Some("Calques aplatis.".into());
    }

    /// Duplique la sélection avec un léger décalage (⌘D).
    pub fn duplicate_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let idx = self.doc.active_layer;
        let layer = self.doc.active_id();
        let mut clones = Vec::new();
        let mut new_ids = HashSet::new();
        let mut next_id = self.next_id;
        let mut z = self.doc.next_z;
        for s in &self.doc.layers[idx].strokes {
            if self.selection.contains(&s.id) {
                let mut c = s.clone();
                c.id = next_id;
                next_id += 1;
                c.z = z; // au-dessus
                z += 1.0;
                for p in &mut c.points {
                    p.pos.0 += 12.0;
                    p.pos.1 += 12.0;
                }
                new_ids.insert(c.id);
                clones.push(c);
            }
        }
        self.next_id = next_id;
        self.doc.next_z = z;
        if !clones.is_empty() {
            self.history.push(&mut self.doc, Command::AddMany { layer, strokes: clones });
            self.selection = new_ids;
        }
    }

    // --- Ordre de superposition (z-order) -----------------------------------

    /// Met la sélection au premier plan / l'avance / la recule / l'arrière-plan.
    pub fn reorder(&mut self, mode: ZMove) {
        if self.selection.is_empty() {
            return;
        }
        let layer_id = self.doc.active_id();
        let zs = self.doc.layers[self.doc.active_layer].each_z();
        if zs.is_empty() {
            return;
        }
        let maxz = zs.iter().map(|(_, z)| *z).fold(f64::MIN, f64::max);
        let minz = zs.iter().map(|(_, z)| *z).fold(f64::MAX, f64::min);
        let sel: Vec<(u64, f64)> =
            zs.iter().filter(|(id, _)| self.selection.contains(id)).cloned().collect();
        let mut changes: Vec<(u64, f64, f64)> = Vec::new();

        match mode {
            ZMove::Front => {
                let mut s = sel.clone();
                s.sort_by(|a, b| a.1.total_cmp(&b.1));
                let mut nz = maxz + 1.0;
                for (id, before) in s {
                    changes.push((id, before, nz));
                    nz += 1.0;
                }
            }
            ZMove::Back => {
                let mut s = sel.clone();
                s.sort_by(|a, b| b.1.total_cmp(&a.1));
                let mut nz = minz - 1.0;
                for (id, before) in s {
                    changes.push((id, before, nz));
                    nz -= 1.0;
                }
            }
            ZMove::Forward | ZMove::Backward => {
                if sel.len() != 1 {
                    self.status = Some("Avancer/Reculer : sélectionne un seul élément.".into());
                    return;
                }
                let (id, zc) = sel[0];
                let neighbor = if matches!(mode, ZMove::Forward) {
                    zs.iter().filter(|(_, z)| *z > zc).min_by(|a, b| a.1.total_cmp(&b.1))
                } else {
                    zs.iter().filter(|(_, z)| *z < zc).max_by(|a, b| a.1.total_cmp(&b.1))
                };
                if let Some((oid, oz)) = neighbor.cloned() {
                    changes.push((id, zc, oz));
                    changes.push((oid, oz, zc));
                }
            }
        }
        if !changes.is_empty() {
            self.history.push(&mut self.doc, Command::SetZMany { layer: layer_id, changes });
        }
    }

    /// Undo/redo centralisés : invalident les caches si nécessaire.
    pub fn undo(&mut self) {
        if self.history.undo(&mut self.doc) {
            self.cache.clear();
        }
        self.image_textures.clear();
        self.selection.clear();
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.cache.clear();
        }
        self.image_textures.clear();
        self.selection.clear();
    }

    /// Saut direct dans la frise d'historique (panneau d'historique).
    pub fn history_goto(&mut self, target: usize) {
        self.history.goto(&mut self.doc, target);
        self.cache.clear();
        self.image_textures.clear();
        self.selection.clear();
        if self.doc.active_layer >= self.doc.layers.len() {
            self.doc.active_layer = self.doc.layers.len().saturating_sub(1);
        }
    }

    // --- Copier / coller (backlog) ------------------------------------------

    /// Copie les éléments sélectionnés dans le presse-papiers interne.
    pub fn copy_selection(&mut self) {
        let l = &self.doc.layers[self.doc.active_layer];
        let mut c = ClipBoard::default();
        for s in &l.strokes {
            if self.selection.contains(&s.id) {
                c.strokes.push(s.clone());
            }
        }
        for t in &l.texts {
            if self.selection.contains(&t.id) {
                c.texts.push(t.clone());
            }
        }
        for im in &l.images {
            if self.selection.contains(&im.id) {
                c.images.push(im.clone());
            }
        }
        if !c.is_empty() {
            self.clip = c;
            self.status = Some("Copié.".into());
        }
    }

    pub fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    /// Colle le presse-papiers interne (décalé) sur le calque actif.
    /// Renvoie `false` s'il est vide (l'appelant tente alors le presse-papiers
    /// système / image).
    pub fn paste_clipboard(&mut self) -> bool {
        if self.clip.is_empty() {
            return false;
        }
        let layer = self.doc.active_id();
        let mut newsel = HashSet::new();
        let mut strokes = Vec::new();
        for s in &self.clip.strokes {
            let mut c = s.clone();
            c.id = self.next_id;
            self.next_id += 1;
            c.z = self.doc.next_z;
            self.doc.next_z += 1.0;
            for p in &mut c.points {
                p.pos.0 += 16.0;
                p.pos.1 += 16.0;
            }
            newsel.insert(c.id);
            strokes.push(c);
        }
        if !strokes.is_empty() {
            self.history.push(&mut self.doc, Command::AddMany { layer, strokes });
        }
        // Textes et images : clonés avec un nouvel id, au-dessus.
        let texts: Vec<_> = self.clip.texts.clone();
        for mut t in texts {
            t.id = self.next_id;
            self.next_id += 1;
            t.z = self.bump_z();
            t.pos.0 += 16.0;
            t.pos.1 += 16.0;
            newsel.insert(t.id);
            self.history.push(&mut self.doc, Command::AddText { layer, text: t });
        }
        let images: Vec<_> = self.clip.images.clone();
        for mut im in images {
            im.id = self.next_id;
            self.next_id += 1;
            im.z = self.bump_z();
            im.pos.0 += 16.0;
            im.pos.1 += 16.0;
            newsel.insert(im.id);
            self.history.push(&mut self.doc, Command::AddImage { layer, image: im });
        }
        self.selection = newsel;
        self.active_tool = ActiveTool::Select;
        self.status = Some("Collé.".into());
        true
    }

    pub fn push_recent_color(&mut self, rgba: [u8; 4]) {
        let rgb = [rgba[0], rgba[1], rgba[2]];
        self.recent_colors.retain(|c| *c != rgb);
        self.recent_colors.insert(0, rgb);
        self.recent_colors.truncate(8);
    }

    fn adjust_size(&mut self, delta: f32) {
        match self.active_tool {
            ActiveTool::Eraser => {
                self.eraser.width = (self.eraser.width + delta * 2.0).clamp(4.0, 80.0)
            }
            _ => self.brush.width = (self.brush.width + delta).clamp(1.0, 40.0),
        }
    }

    // --- Calques (idée 1) ----------------------------------------------------

    pub fn add_layer(&mut self) {
        let id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        let n = self.doc.layers.len() + 1;
        let layer = crate::model::Layer::new(id, format!("Calque {n}"));
        let index = self.doc.layers.len();
        self.history.push(&mut self.doc, Command::AddLayer { index, layer: Box::new(layer) });
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
            .unwrap_or_else(|| format!("Groupe {}", self.doc.next_layer_id));
        self.doc.next_layer_id += 1;
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        after[i].group = Some(name.clone());
        after[i - 1].group = Some(name);
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i },
        );
        self.status = Some("Calques groupés.".into());
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

    // --- Vue : zoom / pan (idée 2) ------------------------------------------

    /// Zoome autour d'un point écran en gardant le point document sous le curseur.
    fn zoom_about(&mut self, ptr: Pos2, origin_base: Pos2, factor: f32) {
        let local = ptr - (origin_base + self.pan);
        let world = local / self.zoom;
        let new_zoom = (self.zoom * factor).clamp(0.1, 16.0);
        self.pan = (ptr - origin_base) - world * new_zoom;
        self.zoom = new_zoom;
    }

    pub fn zoom_in(&mut self) {
        let c = self.last_canvas_rect.center();
        self.zoom_about(c, self.last_canvas_rect.min, 1.2);
    }

    pub fn zoom_out(&mut self) {
        let c = self.last_canvas_rect.center();
        self.zoom_about(c, self.last_canvas_rect.min, 1.0 / 1.2);
    }

    /// Remet à 100 % et centre le document dans la zone de travail.
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.center_document();
    }

    /// Ajuste le zoom pour que tout le document tienne dans la zone (puis centre).
    pub fn fit_view(&mut self) {
        let cr = self.last_canvas_rect;
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        if w <= 0.0 || h <= 0.0 || cr.width() <= 0.0 {
            return;
        }
        self.zoom = (cr.width() / w).min(cr.height() / h).clamp(0.05, 16.0) * 0.95;
        self.center_document();
    }

    fn center_document(&mut self) {
        let cr = self.last_canvas_rect;
        let (w, h) = (self.doc.size.0 as f32 * self.zoom, self.doc.size.1 as f32 * self.zoom);
        self.pan = Vec2::new((cr.width() - w) * 0.5, (cr.height() - h) * 0.5);
    }

    /// Change la taille du document (presets) en conservant les traits.
    pub fn set_canvas_size(&mut self, w: u32, h: u32) {
        self.doc.size = (w.max(1), h.max(1));
        self.fit_view();
        self.status = Some(format!("Document : {w}×{h}"));
    }

    // --- Projet : sauvegarde / ouverture (idée 6) ---------------------------

    /// Encode (paresseusement) le PNG de toutes les images avant un export
    /// nécessitant les données encodées (projet, SVG).
    fn encode_all_images(&mut self) {
        for layer in &mut self.doc.layers {
            for im in &mut layer.images {
                im.ensure_encoded();
            }
        }
    }

    pub fn save_project(&mut self) {
        self.encode_all_images();
        if let Some(p) = crate::project::save_dialog(&self.doc) {
            self.status = Some(format!("Projet enregistré : {}", p.display()));
        }
    }

    pub fn open_project(&mut self) {
        if let Some(doc) = crate::project::open_dialog() {
            self.apply_loaded(doc);
            self.status = Some("Projet ouvert.".into());
        }
    }

    fn apply_loaded(&mut self, mut doc: Document) {
        doc.normalize_ids(); // répare les anciens projets (id manquants)
        // Reconstruit les pixels des images depuis leur PNG base64.
        for layer in &mut doc.layers {
            for im in &mut layer.images {
                im.decode();
            }
        }
        self.doc = doc;
        self.history = History::new();
        self.cache.clear();
        self.image_textures.clear();
        self.erase_pending.clear();
        self.selection.clear();
        self.editing_text = None;
        self.reset_view();
        // Inclut traits, images ET textes : sinon `next_id` peut retomber sous
        // l'id d'un texte/image existant et provoquer des collisions d'ids.
        let max = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.each_z())
            .map(|(id, _)| id)
            .max()
            .unwrap_or(0);
        self.next_id = max + 1;
        // next_z au-dessus de tout élément chargé.
        let maxz = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.each_z())
            .map(|(_, z)| z)
            .fold(0.0_f64, f64::max);
        self.doc.next_z = maxz + 1.0;
    }

    // --- Export PNG (idée précédente) ---------------------------------------

    /// Demande un export bitmap au format `format` : déclenche une capture
    /// d'écran différée, traitée par `handle_screenshot`.
    pub fn request_export(&mut self, ctx: &egui::Context, format: crate::export::ExportFormat) {
        self.export_requested = true;
        self.export_format = format;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }

    /// Export SVG vectoriel (opacité de calque correcte via `<g opacity>`).
    pub fn export_svg(&mut self) {
        self.encode_all_images();
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        self.status = Some(match crate::svg::save_to_desktop(&self.doc, bg) {
            Ok(p) => format!("SVG enregistré : {}", p.display()),
            Err(e) => format!("Échec de l'export SVG : {e}"),
        });
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = shot else { return };
        if let Some(click) = self.bucket_click.take() {
            self.do_bucket_fill(ctx, &image, click);
            return;
        }
        if !self.export_requested {
            return;
        }
        self.export_requested = false;

        let ppp = ctx.pixels_per_point();
        // On exporte la zone du document, bornée à la partie visible.
        let r = self.last_doc_rect.intersect(self.last_canvas_rect);
        let crop = (
            (r.min.x * ppp).round().max(0.0) as usize,
            (r.min.y * ppp).round().max(0.0) as usize,
            (r.width() * ppp).round().max(0.0) as usize,
            (r.height() * ppp).round().max(0.0) as usize,
        );
        let format = self.export_format;
        self.status = Some(match crate::export::save_dialog(&image, crop, format) {
            Ok(p) => format!("{} enregistré : {}", format.label(), p.display()),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => "Export annulé.".into(),
            Err(e) => format!("Échec de l'export : {e}"),
        });
    }

    /// Aligne un point document sur la grille si le magnétisme est actif.
    fn snap(&self, p: (f32, f32)) -> (f32, f32) {
        if !self.snap_enabled || self.grid_size <= 0.0 {
            return p;
        }
        let g = self.grid_size;
        ((p.0 / g).round() * g, (p.1 / g).round() * g)
    }

    // --- Plume (roadmap #9) -------------------------------------------------

    /// Ajoute une ancre, ou ferme le chemin si on clique près du départ.
    fn pen_press(&mut self, d: (f32, f32)) {
        let d = self.snap(d);
        if self.pen.len() >= 2 {
            let first = self.pen[0].pos;
            let near = ((d.0 - first.0).powi(2) + (d.1 - first.1).powi(2)).sqrt()
                < 8.0 / self.zoom.max(0.01);
            if near {
                self.commit_pen(true);
                return;
            }
        }
        self.pen.push(crate::tools::pen::Anchor::corner(d));
    }

    /// Valide le chemin en cours : échantillonne en `Stroke` (annulable).
    fn commit_pen(&mut self, closed: bool) {
        if self.pen.len() < 2 {
            self.pen.clear();
            return;
        }
        let pts = crate::tools::pen::sample(&self.pen, closed);
        let mut stroke = Stroke::new(self.brush.color, self.brush.width, Tool::Brush);
        stroke.fill = closed && self.fill_shapes;
        for p in &pts {
            stroke.points.push(crate::model::StrokePoint { pos: *p, width: self.brush.width });
        }
        self.pen.clear();
        self.commit_stroke(stroke);
    }

    fn current_view(&self) -> ViewTransform {
        ViewTransform { origin: self.last_canvas_rect.min + self.pan, scale: self.zoom }
    }

    /// Libère les textures d'images supprimées (évite une fuite mémoire).
    /// Court-circuit O(1) tant qu'il n'y a pas plus de textures que d'images.
    fn prune_image_textures(&mut self) {
        let total: usize = self.doc.layers.iter().map(|l| l.images.len()).sum();
        if self.image_textures.len() <= total {
            return;
        }
        let live: HashSet<u64> =
            self.doc.layers.iter().flat_map(|l| l.images.iter().map(|im| im.id)).collect();
        self.image_textures.retain(|id, _| live.contains(id));
    }

    /// Le compositing CPU n'est utile que si un calque a un mode ≠ Normal ou une
    /// opacité < 100 % (sinon le rendu vectoriel egui reste net à tout zoom).
    fn needs_composite(&self) -> bool {
        self.doc.layers.iter().any(|l| {
            l.visible && (l.blend != crate::model::BlendMode::Normal || l.opacity < 0.999)
        })
    }

    /// Signature d'invalidation du compositeur (contenu + apparence des calques).
    fn composite_signature(&self) -> u64 {
        let mut h = self.history.revision();
        for l in &self.doc.layers {
            h = h.wrapping_mul(1099511628211).wrapping_add(l.id);
            h = h.wrapping_mul(31).wrapping_add(l.visible as u64);
            h = h.wrapping_mul(31).wrapping_add((l.opacity * 1000.0) as u64);
            h = h.wrapping_mul(31).wrapping_add(l.blend as u64);
        }
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.doc.size.0 as u64)
            .wrapping_mul(31)
            .wrapping_add(self.doc.size.1 as u64);
        // Rebâtir quand on commence / finit d'éditer un texte (exclusion).
        h.wrapping_mul(31).wrapping_add(self.editing_text.unwrap_or(0))
    }

    /// Pot de peinture : inonde la composition affichée depuis le point cliqué,
    /// puis dépose le remplissage comme image sur le calque actif (roadmap #6).
    fn do_bucket_fill(&mut self, ctx: &egui::Context, image: &egui::ColorImage, click: Pos2) {
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
        let count = mask.iter().filter(|&&m| m).count();
        if count == 0 {
            return;
        }
        // Buffer de sortie : couleur de remplissage là où c'est rempli.
        let fill = self.brush.color;
        let mut out = vec![0u8; rw * rh * 4];
        for (k, &m) in mask.iter().enumerate() {
            if m {
                out[k * 4..k * 4 + 4].copy_from_slice(&[fill[0], fill[1], fill[2], 255]);
            }
        }

        // Place l'image en coords document (la région correspond à un sous-rect).
        let view = self.current_view();
        let pos = view.screen_to_doc(r.min);
        let size = ((rw as f32 / ppp) / self.zoom, (rh as f32 / ppp) / self.zoom);
        let id = self.next_id;
        self.next_id += 1;
        let mut item = crate::model::ImageItem::from_rgba(id, pos, rw as u32, rh as u32, out);
        item.size = size;
        item.z = self.bump_z();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddImage { layer, image: item });
        self.status = Some(format!("Zone remplie ({count} px)."));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        // Les actions ouvrant une boîte de dialogue native sont exécutées
        // APRÈS la fermeture du verrou d'entrée (évite tout blocage modal).
        let mut want_export = false;
        let mut want_new = false;
        let mut want_open = false;
        let mut want_save = false;
        let mut want_paste = false;
        ctx.input(|i| {
            let cmd = i.modifiers.command || i.modifiers.ctrl;
            if cmd && i.key_pressed(egui::Key::Z) {
                if i.modifiers.shift {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if cmd && i.key_pressed(egui::Key::D) {
                self.duplicate_selection();
            }
            if cmd && i.key_pressed(egui::Key::C) {
                self.copy_selection();
            }
            if cmd && i.key_pressed(egui::Key::X) {
                self.cut_selection();
            }
            if !typing && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                self.delete_selection();
            }
            // Fichier (conventions macOS) : ⌘N / ⌘O / ⌘S / ⌘E.
            if cmd && i.key_pressed(egui::Key::N) {
                want_new = true;
            }
            if cmd && i.key_pressed(egui::Key::O) {
                want_open = true;
            }
            if cmd && i.key_pressed(egui::Key::S) {
                want_save = true;
            }
            if cmd && i.key_pressed(egui::Key::E) {
                want_export = true;
            }
            if cmd && i.key_pressed(egui::Key::V) {
                want_paste = true;
            }
            // Ordre de superposition : ⌘] avancer / ⌘⇧] premier plan, ⌘[ reculer / ⌘⇧[ arrière.
            if cmd && i.key_pressed(egui::Key::CloseBracket) {
                self.reorder(if i.modifiers.shift { ZMove::Front } else { ZMove::Forward });
            }
            if cmd && i.key_pressed(egui::Key::OpenBracket) {
                self.reorder(if i.modifiers.shift { ZMove::Back } else { ZMove::Backward });
            }
            // Zoom clavier (⌘0 = 100 %, ⌘+ / ⌘-).
            if cmd && i.key_pressed(egui::Key::Num0) {
                self.reset_view();
            }
            if cmd && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)) {
                self.zoom_in();
            }
            if cmd && i.key_pressed(egui::Key::Minus) {
                self.zoom_out();
            }
            if !cmd && !typing {
                use egui::Key;
                if i.key_pressed(Key::V) {
                    self.active_tool = ActiveTool::Select;
                }
                if i.key_pressed(Key::B) {
                    self.active_tool = ActiveTool::Brush;
                }
                if i.key_pressed(Key::E) {
                    self.active_tool = ActiveTool::Eraser;
                }
                if i.key_pressed(Key::L) {
                    self.active_tool = ActiveTool::Line;
                }
                if i.key_pressed(Key::A) {
                    self.active_tool = ActiveTool::Arrow;
                }
                if i.key_pressed(Key::R) {
                    self.active_tool = ActiveTool::Rectangle;
                }
                if i.key_pressed(Key::O) {
                    self.active_tool = ActiveTool::Ellipse;
                }
                if i.key_pressed(Key::T) {
                    self.active_tool = ActiveTool::Text;
                }
                if i.key_pressed(Key::G) {
                    self.active_tool = ActiveTool::Bucket;
                }
                if i.key_pressed(Key::P) {
                    self.active_tool = ActiveTool::Pen;
                }
                // Plume : Entrée valide, Échap annule le chemin en cours.
                if !self.pen.is_empty() {
                    if i.key_pressed(Key::Enter) {
                        self.commit_pen(false);
                    }
                    if i.key_pressed(Key::Escape) {
                        self.pen.clear();
                    }
                }
                // Nudge clavier de la sélection (flèches ; Maj = pas de 10).
                if !self.selection.is_empty() {
                    let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                    let mut nx = 0.0;
                    let mut ny = 0.0;
                    if i.key_pressed(Key::ArrowLeft) {
                        nx -= step;
                    }
                    if i.key_pressed(Key::ArrowRight) {
                        nx += step;
                    }
                    if i.key_pressed(Key::ArrowUp) {
                        ny -= step;
                    }
                    if i.key_pressed(Key::ArrowDown) {
                        ny += step;
                    }
                    if nx != 0.0 || ny != 0.0 {
                        self.push_move(nx, ny);
                    }
                }
                if i.key_pressed(Key::I) {
                    self.active_tool = ActiveTool::Eyedropper;
                }
                if i.key_pressed(Key::H) {
                    self.active_tool = ActiveTool::Pan;
                }
                if i.key_pressed(Key::OpenBracket) {
                    self.adjust_size(-1.0);
                }
                if i.key_pressed(Key::CloseBracket) {
                    self.adjust_size(1.0);
                }
            }
        });
        if want_new {
            self.new_document();
        }
        if want_open {
            self.open_project();
        }
        if want_save {
            self.save_project();
        }
        if want_paste {
            // Priorité au presse-papiers interne (éléments), sinon image système.
            if !self.paste_clipboard() {
                self.paste_image();
            }
        }
        if want_export {
            self.request_export(ctx, crate::export::ExportFormat::Png);
        }
    }

    /// Zone de saisie flottante pour le texte en cours d'édition.
    fn text_editor(&mut self, ctx: &egui::Context, view: &ViewTransform) {
        let Some(id) = self.editing_text else { return };
        let active = self.doc.active_layer;
        let Some(t) = self.doc.layers[active].texts.iter_mut().find(|t| t.id == id) else {
            self.editing_text = None;
            return;
        };
        let pos = view.doc_to_screen(t.pos);
        let mut finished = false;
        egui::Area::new(egui::Id::new(("text_edit", id)))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut t.text)
                        .desired_width(260.0)
                        .hint_text("Tapez votre texte…")
                        .font(egui::FontId::proportional(t.size.clamp(12.0, 48.0))),
                );
                if self.text_focus_pending {
                    resp.request_focus();
                    self.text_focus_pending = false;
                }
                if resp.changed() {
                    // Invalide la composition CPU pendant la frappe.
                    self.history.touch();
                }
                if resp.lost_focus() {
                    finished = true;
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            finished = true;
        }
        if finished {
            self.finish_text_editing();
        }
    }

    /// Cadre bleu autour de la sélection (outil flèche), suit le déplacement.
    /// Grille du document (roadmap #10), rognée au cadre.
    fn paint_grid(&self, painter: &egui::Painter, view: &ViewTransform, doc_rect: Rect) {
        let g = self.grid_size;
        if g <= 0.0 {
            return;
        }
        let p = painter.with_clip_rect(doc_rect.intersect(self.last_canvas_rect));
        let stroke = egui::Stroke::new(1.0, Color32::from_black_alpha(28));
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        let mut x = 0.0;
        while x <= w {
            let a = view.doc_to_screen((x, 0.0));
            let b = view.doc_to_screen((x, h));
            p.line_segment([a, b], stroke);
            x += g;
        }
        let mut y = 0.0;
        while y <= h {
            let a = view.doc_to_screen((0.0, y));
            let b = view.doc_to_screen((w, y));
            p.line_segment([a, b], stroke);
            y += g;
        }
    }

    /// Règles graduées (haut + gauche) en coordonnées document. Le pas est
    /// choisi pour rester lisible quel que soit le zoom.
    fn paint_rulers(&self, painter: &egui::Painter, view: &ViewTransform) {
        const TH: f32 = 18.0; // épaisseur de la règle (px écran)
        let cr = self.last_canvas_rect;
        let bg = Color32::from_gray(244);
        let line = Color32::from_gray(120);
        let text = Color32::from_gray(90);
        let top = Rect::from_min_max(cr.min, egui::pos2(cr.max.x, cr.min.y + TH));
        let left = Rect::from_min_max(cr.min, egui::pos2(cr.min.x + TH, cr.max.y));
        painter.rect_filled(top, 0.0, bg);
        painter.rect_filled(left, 0.0, bg);

        let step = ruler_step(self.zoom); // pas en unités document
        let font = egui::FontId::proportional(9.0);
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);

        // Graduations horizontales (axe X) sur la règle du haut.
        let mut x = 0.0;
        while x <= w {
            let sx = view.doc_to_screen((x, 0.0)).x;
            if sx >= left.max.x && sx <= cr.max.x {
                painter.line_segment(
                    [egui::pos2(sx, cr.min.y + TH * 0.45), egui::pos2(sx, cr.min.y + TH)],
                    egui::Stroke::new(1.0, line),
                );
                painter.text(
                    egui::pos2(sx + 2.0, cr.min.y + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", x as i32),
                    font.clone(),
                    text,
                );
            }
            x += step;
        }
        // Graduations verticales (axe Y) sur la règle de gauche.
        let mut y = 0.0;
        while y <= h {
            let sy = view.doc_to_screen((0.0, y)).y;
            if sy >= top.max.y && sy <= cr.max.y {
                painter.line_segment(
                    [egui::pos2(cr.min.x + TH * 0.45, sy), egui::pos2(cr.min.x + TH, sy)],
                    egui::Stroke::new(1.0, line),
                );
                painter.text(
                    egui::pos2(cr.min.x + 1.0, sy + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", y as i32),
                    font.clone(),
                    text,
                );
            }
            y += step;
        }
        // Coin + liserés de séparation.
        painter.rect_filled(Rect::from_min_size(cr.min, Vec2::splat(TH)), 0.0, bg);
        painter.line_segment([egui::pos2(cr.min.x, cr.min.y + TH), egui::pos2(cr.max.x, cr.min.y + TH)], egui::Stroke::new(1.0, line));
        painter.line_segment([egui::pos2(cr.min.x + TH, cr.min.y), egui::pos2(cr.min.x + TH, cr.max.y)], egui::Stroke::new(1.0, line));
    }

    /// Aperçu du chemin de plume en cours : courbe + ancres + poignées.
    fn paint_pen(&self, painter: &egui::Painter, view: &ViewTransform, response: &egui::Response) {
        if self.active_tool != ActiveTool::Pen || self.pen.is_empty() {
            return;
        }
        let blue = Color32::from_rgb(40, 110, 220);
        // Chemin échantillonné, avec un segment élastique vers le curseur.
        let mut anchors = self.pen.clone();
        if let Some(hp) = response.hover_pos() {
            anchors.push(crate::tools::pen::Anchor::corner(self.snap(view.screen_to_doc(hp))));
        }
        let pts = crate::tools::pen::sample(&anchors, false);
        let screen: Vec<egui::Pos2> = pts.iter().map(|p| view.doc_to_screen(*p)).collect();
        if screen.len() >= 2 {
            painter.add(egui::Shape::line(screen, egui::Stroke::new(1.5, blue)));
        }
        // Ancres (carrés) + poignées (lignes + cercles).
        for a in &self.pen {
            let c = view.doc_to_screen(a.pos);
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(6.0)), 1.0, blue);
            for h in [a.h_in, a.h_out] {
                if h != a.pos {
                    let hp = view.doc_to_screen(h);
                    painter.line_segment([c, hp], egui::Stroke::new(1.0, Color32::from_gray(150)));
                    painter.circle_filled(hp, 3.0, Color32::WHITE);
                    painter.circle_stroke(hp, 3.0, egui::Stroke::new(1.0, blue));
                }
            }
        }
    }

    fn paint_selection(&self, painter: &egui::Painter, view: &ViewTransform, moving: bool) {
        if self.active_tool != ActiveTool::Select || self.selection.is_empty() || self.crop_mode {
            return;
        }
        let active = self.doc.active_layer;
        let sel = &self.selection;
        let layer = &self.doc.layers[active];
        let mut bounds = hit::bounds_of(layer.strokes.iter().filter(|s| sel.contains(&s.id)));
        // Inclut aussi les textes et images sélectionnés.
        let extra = layer
            .texts
            .iter()
            .filter(|t| sel.contains(&t.id))
            .map(|t| t.approx_bounds())
            .chain(layer.images.iter().filter(|im| sel.contains(&im.id)).map(|im| im.bounds()));
        for (lo, hi) in extra {
            bounds = Some(match bounds {
                Some((mn, mx)) => (
                    (mn.0.min(lo.0), mn.1.min(lo.1)),
                    (mx.0.max(hi.0), mx.1.max(hi.1)),
                ),
                None => (lo, hi),
            });
        }
        let Some((min, max)) = bounds else { return };
        let blue = Color32::from_rgb(40, 110, 220);

        // En cours de transformation : aperçu de la boîte transformée.
        if let Some(x) = &self.xform {
            let (mn, mx) = x.bbox;
            let corners0 = [(mn.0, mn.1), (mx.0, mn.1), (mx.0, mx.1), (mn.0, mx.1)];
            let pts: Vec<Pos2> = corners0
                .iter()
                .map(|c| {
                    let t = match x.kind {
                        XformKind::Scale { anchor } => (
                            anchor.0 + (c.0 - anchor.0) * x.sx,
                            anchor.1 + (c.1 - anchor.1) * x.sy,
                        ),
                        XformKind::Rotate { center, .. } => {
                            let (co, si) = (x.angle.cos(), x.angle.sin());
                            let (dx, dy) = (c.0 - center.0, c.1 - center.1);
                            (center.0 + dx * co - dy * si, center.1 + dx * si + dy * co)
                        }
                    };
                    view.doc_to_screen(t)
                })
                .collect();
            let mut poly = pts.clone();
            poly.push(pts[0]);
            painter.add(egui::Shape::line(poly, egui::Stroke::new(1.5, blue)));
            return;
        }

        let d = if moving { self.move_delta } else { (0.0, 0.0) };
        let p0 = view.doc_to_screen((min.0 + d.0, min.1 + d.1));
        let p1 = view.doc_to_screen((max.0 + d.0, max.1 + d.1));
        let r = Rect::from_two_pos(p0, p1).expand(2.0);
        painter.rect_stroke(r, 2.0, egui::Stroke::new(1.5, blue));

        // Poignées d'échelle (coins) + rotation (au-dessus) — toute sélection.
        if !moving {
            if let Some((corners, rot)) = self.transform_handles(view) {
                let top = view.doc_to_screen(((min.0 + max.0) * 0.5, min.1));
                painter.line_segment([top, rot], egui::Stroke::new(1.0, blue));
                painter.circle_filled(rot, 5.0, Color32::WHITE);
                painter.circle_stroke(rot, 5.0, egui::Stroke::new(1.5, blue));
                for c in corners {
                    let hr = Rect::from_center_size(c, Vec2::splat(9.0));
                    painter.rect_filled(hr, 1.0, Color32::WHITE);
                    painter.rect_stroke(hr, 1.0, egui::Stroke::new(1.5, blue));
                }
            }
        }
    }

    /// Overlay du recadrage : rectangle orange (zone conservée).
    fn paint_crop(&self, painter: &egui::Painter, view: &ViewTransform) {
        if !self.crop_mode {
            return;
        }
        let orange = Color32::from_rgb(255, 170, 0);
        if let Some((a, b)) = self.crop_rect {
            let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, orange));
        } else if let Some((_, corners)) = self.selected_image_corners() {
            // Avant le glissé : souligne l'image à recadrer.
            let r = Rect::from_two_pos(view.doc_to_screen(corners[0]), view.doc_to_screen(corners[2]));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, orange));
        }
    }

    /// Overlay de la sélection par région : rectangle (marquee) ou tracé lasso,
    /// en bleu translucide tant que le geste est en cours.
    fn paint_marquee(&self, painter: &egui::Painter, view: &ViewTransform) {
        let blue = Color32::from_rgb(40, 110, 240);
        let fill = Color32::from_rgba_unmultiplied(40, 110, 240, 28);
        if let Some((a, b)) = self.marquee {
            let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
            painter.rect_filled(r, 0.0, fill);
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, blue));
        } else if self.lasso.len() >= 2 {
            let pts: Vec<Pos2> = self.lasso.iter().map(|&d| view.doc_to_screen(d)).collect();
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0, blue)));
            // Trait de fermeture (du dernier point au premier).
            painter.line_segment([pts[pts.len() - 1], pts[0]], egui::Stroke::new(1.0, fill));
        }
    }

    /// Anneau de prévisualisation de la taille de l'outil sous le curseur
    /// (repère ergonomique « vrai Paint »). Bichromie pour rester visible sur
    /// tout fond.
    fn paint_cursor(&self, painter: &egui::Painter, response: &egui::Response) {
        let Some(p) = response.hover_pos() else { return };
        let radius = match self.active_tool {
            ActiveTool::Eraser => self.eraser.width * 0.5 * self.zoom,
            ActiveTool::Brush | ActiveTool::Line | ActiveTool::Rectangle | ActiveTool::Ellipse => {
                self.brush.width * 0.5 * self.zoom
            }
            _ => return,
        };
        if radius < 1.0 {
            return;
        }
        painter.circle_stroke(p, radius, egui::Stroke::new(2.0, Color32::from_white_alpha(200)));
        painter.circle_stroke(p, radius, egui::Stroke::new(1.0, Color32::from_black_alpha(160)));
    }

    /// Gère le geste sur le canvas selon l'outil actif.
    fn handle_canvas(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let origin_base = response.rect.min;

        // Pan / zoom à la molette ou au pincé (uniquement au-dessus du canvas).
        if response.hovered() {
            let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if scroll != Vec2::ZERO {
                self.pan += scroll;
            }
            if zoom_delta != 1.0 {
                if let Some(ptr) = response.hover_pos() {
                    self.zoom_about(ptr, origin_base, zoom_delta);
                }
            }
        }

        // Espace maintenu = déplacement temporaire (réflexe « main » à la
        // Photoshop), quel que soit l'outil.
        let space_pan = ctx.input(|i| i.key_down(egui::Key::Space)) && !ctx.wants_keyboard_input();
        if space_pan {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            if response.dragged() {
                self.pan += response.drag_delta();
            }
            return;
        }

        match self.active_tool {
            ActiveTool::Select => {
                let shift = ctx.input(|i| i.modifiers.shift);
                // Mode recadrage : le glissé définit la zone à conserver.
                if self.crop_mode {
                    if response.drag_started() {
                        if let Some(p) = response.interact_pointer_pos() {
                            let d = view.screen_to_doc(p);
                            self.crop_rect = Some((d, d));
                        }
                    }
                    if response.dragged() {
                        if let (Some((s, _)), Some(p)) =
                            (self.crop_rect, response.interact_pointer_pos())
                        {
                            let e = self.constrain_crop(s, view.screen_to_doc(p));
                            self.crop_rect = Some((s, e));
                        }
                    }
                    if response.drag_stopped() {
                        self.apply_crop();
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.crop_mode = false;
                        self.crop_rect = None;
                    }
                    return;
                }
                // Échap annule une sélection par région en cours.
                if ctx.input(|i| i.key_pressed(egui::Key::Escape))
                    && (self.marquee.is_some() || !self.lasso.is_empty())
                {
                    self.marquee = None;
                    self.lasso.clear();
                }
                if response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        // Poignée d'échelle / rotation en priorité.
                        if self.start_transform_if_handle(p, view) {
                            return;
                        }
                        let d = view.screen_to_doc(p);
                        match self.topmost_at(d) {
                            // Sur un élément déjà sélectionné → on garde la sélection.
                            Some(id) if self.selection.contains(&id) => {
                                self.move_origin = Some(d);
                                self.move_delta = (0.0, 0.0);
                            }
                            Some(id) => {
                                if !shift {
                                    self.selection.clear();
                                }
                                self.selection.insert(id);
                                self.move_origin = Some(d);
                                self.move_delta = (0.0, 0.0);
                            }
                            // Glissé sur le vide → sélection par région (marquee/lasso).
                            None => match self.select_mode {
                                SelectMode::Lasso => self.lasso = vec![d],
                                _ => self.marquee = Some((d, d)),
                            },
                        }
                    }
                }
                if response.dragged() {
                    if self.xform.is_some() {
                        if let Some(p) = response.interact_pointer_pos() {
                            self.update_transform(p, view, shift);
                        }
                    } else if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some((s, _)) = self.marquee {
                            self.marquee = Some((s, d));
                        } else if !self.lasso.is_empty() {
                            self.lasso.push(d);
                        } else if let Some(o) = self.move_origin {
                            self.move_delta = (d.0 - o.0, d.1 - o.1);
                        }
                    }
                }
                if response.drag_stopped() {
                    if self.xform.is_some() {
                        self.commit_transform();
                    } else if let Some((a, b)) = self.marquee.take() {
                        self.select_in_rect(a, b, shift);
                    } else if !self.lasso.is_empty() {
                        let poly = std::mem::take(&mut self.lasso);
                        self.select_in_lasso(&poly, shift);
                    } else {
                        self.commit_move();
                    }
                }
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        // Baguette magique : sélection par couleur.
                        if self.select_mode == SelectMode::Wand {
                            self.magic_wand(d, shift);
                        } else {
                            match self.topmost_at(d) {
                                Some(id) => {
                                    if shift && self.selection.contains(&id) {
                                        self.selection.remove(&id);
                                    } else {
                                        if !shift {
                                            self.selection.clear();
                                        }
                                        self.selection.insert(id);
                                    }
                                }
                                None => {
                                    if !shift {
                                        self.selection.clear();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ActiveTool::Pan => {
                if response.dragged() {
                    self.pan += response.drag_delta();
                }
            }
            ActiveTool::Text => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        self.create_or_edit_text(d);
                    }
                }
            }
            ActiveTool::Bucket => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        // Remplissage différé : on capture la composition affichée.
                        self.bucket_click = Some(p);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    }
                }
            }
            ActiveTool::Pen => {
                // Clic = sommet anguleux ; clic-glissé = sommet lisse (poignées).
                if response.drag_started() || response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        self.pen_press(view.screen_to_doc(p));
                    }
                }
                if response.dragged() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some(last) = self.pen.last_mut() {
                            last.set_symmetric(d);
                        }
                    }
                }
                if response.double_clicked() {
                    self.commit_pen(false);
                }
            }
            ActiveTool::Eyedropper => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        self.status = Some(match eyedropper::pick(&self.doc, d) {
                            Some(rgb) => {
                                self.brush.color = [rgb[0], rgb[1], rgb[2], self.brush.color[3]];
                                "Couleur prélevée.".into()
                            }
                            None => "Pas de trait ici (fond).".into(),
                        });
                    }
                }
            }
            ActiveTool::Eraser => {
                if response.drag_started() {
                    self.erase_pending.clear();
                    self.erase_path.clear();
                }
                if response.dragged() || response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        self.erase_at(view.screen_to_doc(p));
                    }
                }
                if response.drag_stopped() {
                    if self.eraser_partial {
                        self.commit_partial_erase();
                    } else {
                        self.commit_erase();
                    }
                }
            }
            _ => self.handle_draw(ctx, response, view),
        }
    }

    fn handle_draw(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let now = ctx.input(|i| i.time);
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if let Some(sh) = self.active_tool.as_shape() {
                    let d = self.snap(d); // magnétisme grille pour les formes
                    self.shape_start = Some(d);
                    self.shape_preview = Some(shape::build(
                        sh, d, d, self.brush.color, self.brush.width, self.fill_shapes, self.poly_sides,
                    ));
                } else {
                    self.capture.set_pressure_strength(self.capture_pressure_strength);
                    self.capture
                        .begin(d, self.brush.color, self.brush.width, Tool::Brush, now);
                    self.active_stroke.reset();
                }
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                match (self.active_tool.as_shape(), self.shape_start) {
                    (Some(sh), Some(start)) => {
                        let shift = ctx.input(|i| i.modifiers.shift);
                        let d = constrain_shape(sh, start, self.snap(d), shift);
                        self.shape_preview = Some(shape::build(
                            sh, start, d, self.brush.color, self.brush.width, self.fill_shapes, self.poly_sides,
                        ));
                    }
                    _ => self.capture.extend(d, now),
                }
            }
        }
        if response.drag_stopped() {
            if self.shape_start.take().is_some() {
                if let Some(stroke) = self.shape_preview.take() {
                    self.commit_stroke(stroke);
                }
            } else if let Some(stroke) = self.capture.finish() {
                self.commit_stroke(stroke);
            }
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_screenshot(ctx);
        self.handle_shortcuts(ctx);
        // Quitter l'édition de texte si on change d'outil.
        if self.active_tool != ActiveTool::Text && self.editing_text.is_some() {
            self.finish_text_editing();
        }

        let panel_frame = egui::Frame::default()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(Margin::symmetric(10.0, 6.0));

        egui::TopBottomPanel::top("toolbar")
            .frame(panel_frame)
            .show(ctx, |ui| toolbar::show(ui, self, ctx));

        egui::SidePanel::right("layers")
            .resizable(false)
            .default_width(170.0)
            .show(ctx, |ui| layers::show(ui, self));

        egui::TopBottomPanel::bottom("footer")
            .frame(panel_frame)
            .show(ctx, |ui| footer::show(ui, self));

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let (response, painter) = ui.allocate_painter(avail, Sense::click_and_drag());

            let rect = response.rect;
            self.last_canvas_rect = rect;
            // Centrage initial du document dès que la taille de la zone est connue.
            if !self.view_initialized && rect.width() > 1.0 {
                self.reset_view();
                self.view_initialized = true;
            }

            // Plan de travail (pasteboard) sombre, puis le document à sa taille.
            painter.rect_filled(rect, 0.0, Color32::from_gray(70));
            let view = ViewTransform { origin: rect.min + self.pan, scale: self.zoom };
            let doc_rect = Rect::from_min_max(
                view.doc_to_screen((0.0, 0.0)),
                view.doc_to_screen((self.doc.size.0 as f32, self.doc.size.1 as f32)),
            );
            self.last_doc_rect = doc_rect;
            // Ombre portée + fond du document.
            painter.rect_filled(doc_rect.translate(Vec2::splat(3.0)), 0.0, Color32::from_black_alpha(60));
            painter.rect_filled(doc_rect, 0.0, self.bg);
            if self.show_grid {
                self.paint_grid(&painter, &view, doc_rect);
            }

            self.handle_canvas(ctx, &response, &view);

            // 4. Rendu : couches cachées + aperçu en cours. Le calque actif
            // masque les traits effacés (gomme) ou en cours de déplacement.
            self.cache.prune(&self.doc);
            self.prune_image_textures();
            let moving = self.move_origin.is_some()
                && (self.move_delta.0 != 0.0 || self.move_delta.1 != 0.0);
            let active = self.doc.active_layer;
            let empty: HashSet<u64> = HashSet::new();
            // En déplacement, on masque aussi la sélection ; sinon on réutilise
            // directement `erase_pending` sans le cloner (perf).
            let moving_hidden: HashSet<u64> = if moving {
                self.erase_pending.iter().chain(self.selection.iter()).copied().collect()
            } else {
                HashSet::new()
            };
            let active_hidden: &HashSet<u64> = if moving { &moving_hidden } else { &self.erase_pending };
            // Le contenu est rogné au document (ce qui dépasse est hors-cadre).
            let content = painter.with_clip_rect(doc_rect.intersect(rect));

            // #8 : si un calque a un mode/opacité non triviaux, on affiche la
            // composition CPU (traits + images + fusion) en une texture. Sinon,
            // rendu vectoriel net classique.
            let use_composite = self.needs_composite();
            if use_composite {
                let sig = self.composite_signature();
                if let Some(tex) = self.compositor.texture(ctx, &self.doc, sig, self.editing_text) {
                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    content.image(tex.id(), doc_rect, uv, Color32::WHITE);
                }
            }

            // Rendu unifié en z-order (un nouvel élément est au-dessus de tous).
            // En mode composite, tout est déjà dans la texture → on saute.
            if !use_composite {
                for (i, layer) in self.doc.layers.iter().enumerate() {
                    if !layer.visible || layer.opacity <= 0.0 {
                        continue;
                    }
                    let on_active = i == active;
                    let hidden = if on_active { active_hidden } else { &empty };
                    for r in layer.z_order() {
                        match r {
                            crate::model::ElemRef::Stroke(si) => {
                                let s = &layer.strokes[si];
                                if hidden.contains(&s.id) {
                                    continue;
                                }
                                self.cache.paint_one(&content, s, &view, self.bg, layer.opacity);
                            }
                            crate::model::ElemRef::Image(ii) => {
                                let im = &layer.images[ii];
                                if on_active && hidden.contains(&im.id) {
                                    continue; // aperçu de gomme objet
                                }
                                if on_active && moving && self.selection.contains(&im.id) {
                                    continue;
                                }
                                let tex = self.image_textures.entry(im.id).or_insert_with(|| {
                                    let ci = egui::ColorImage::from_rgba_unmultiplied(
                                        [im.w.max(1) as usize, im.h.max(1) as usize],
                                        &im.rgba,
                                    );
                                    ctx.load_texture(format!("img{}", im.id), ci, egui::TextureOptions::LINEAR)
                                });
                                draw_image(&content, im, tex, &view, layer.opacity);
                            }
                            crate::model::ElemRef::Text(ti) => {
                                let t = &layer.texts[ti];
                                if self.editing_text == Some(t.id) {
                                    continue;
                                }
                                if on_active && hidden.contains(&t.id) {
                                    continue; // aperçu de gomme objet
                                }
                                if on_active && moving && self.selection.contains(&t.id) {
                                    continue;
                                }
                                draw_text(&content, t, &view, layer.opacity);
                            }
                        }
                    }
                }
            }
            // Éléments en cours de déplacement : dessinés décalés (aperçu live).
            if moving {
                let off = ViewTransform {
                    origin: view.origin
                        + Vec2::new(self.move_delta.0 * self.zoom, self.move_delta.1 * self.zoom),
                    scale: self.zoom,
                };
                for s in &self.doc.layers[active].strokes {
                    if self.selection.contains(&s.id) {
                        canvas::paint_stroke(&content, s, &off, self.bg);
                    }
                }
                for t in &self.doc.layers[active].texts {
                    if self.selection.contains(&t.id) {
                        draw_text(&content, t, &off, 1.0);
                    }
                }
                for im in &self.doc.layers[active].images {
                    if self.selection.contains(&im.id) {
                        if let Some(tex) = self.image_textures.get(&im.id) {
                            draw_image(&content, im, tex, &off, 1.0);
                        }
                    }
                }
            }
            if let Some(stroke) = self.capture.current() {
                // Opaque → rendu incrémental (fluide) ; translucide → passe
                // unique (évite les coutures du double-blending).
                if stroke.color[3] == 255 {
                    self.active_stroke.paint(&content, stroke, &view, self.bg);
                } else {
                    canvas::paint_stroke(&content, stroke, &view, self.bg);
                }
            }
            if let Some(stroke) = &self.shape_preview {
                canvas::paint_stroke(&content, stroke, &view, self.bg);
            }

            // Bord du document (sur le plan de travail, non rogné).
            painter.rect_stroke(doc_rect, 0.0, egui::Stroke::new(1.0, Color32::from_gray(120)));
            self.paint_selection(&painter, &view, moving);
            self.paint_pen(&content, &view, &response);
            self.paint_crop(&painter, &view);
            self.paint_marquee(&painter, &view);
            self.paint_cursor(&painter, &response);
            if self.show_rulers {
                self.paint_rulers(&painter, &view);
            }

            self.text_editor(ctx, &view);
        });
    }
}

/// Pas de graduation des règles, en unités document, choisi pour qu'un cran
/// fasse ~50–120 px à l'écran (séquence 1-2-5 × puissances de 10).
fn ruler_step(zoom: f32) -> f32 {
    let target = 80.0; // px écran visés entre deux graduations
    let raw = target / zoom.max(0.01);
    let pow = 10f32.powf(raw.log10().floor());
    let n = raw / pow;
    let mult = if n < 2.0 { 2.0 } else if n < 5.0 { 5.0 } else { 10.0 };
    (mult * pow).max(1.0)
}

/// `true` si `p` est dans la boîte `(min, max)`.
fn in_bounds(p: (f32, f32), bounds: ((f32, f32), (f32, f32))) -> bool {
    let ((x0, y0), (x1, y1)) = bounds;
    p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1
}

/// Élargit une boîte de `m` dans toutes les directions.
fn expand_bounds(b: ((f32, f32), (f32, f32)), m: f32) -> ((f32, f32), (f32, f32)) {
    ((b.0 .0 - m, b.0 .1 - m), (b.1 .0 + m, b.1 .1 + m))
}

/// Ramène un point dans le repère local d'un élément tourné de `rot` autour de
/// `pivot` (test de sélection correct pour images/textes tournés).
fn unrotate(p: (f32, f32), pivot: (f32, f32), rot: f32) -> (f32, f32) {
    if rot.abs() < 1e-5 {
        return p;
    }
    let (c, s) = ((-rot).cos(), (-rot).sin());
    let (dx, dy) = (p.0 - pivot.0, p.1 - pivot.1);
    (pivot.0 + dx * c - dy * s, pivot.1 + dx * s + dy * c)
}

fn image_contains(im: &crate::model::ImageItem, d: (f32, f32)) -> bool {
    let center = (im.pos.0 + im.size.0 * 0.5, im.pos.1 + im.size.1 * 0.5);
    in_bounds(unrotate(d, center, im.rot), im.bounds())
}

fn text_contains(t: &crate::model::TextItem, d: (f32, f32)) -> bool {
    in_bounds(unrotate(d, t.pos, t.rot), t.approx_bounds())
}

/// Contrainte avec Maj : ligne horizontale/verticale, rectangle carré, ellipse
/// cercle (sert au tracé de lignes droites parfaites, cas « comparer 2 images »).
fn constrain_shape(
    sh: crate::tools::Shape,
    start: (f32, f32),
    end: (f32, f32),
    shift: bool,
) -> (f32, f32) {
    if !shift {
        return end;
    }
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    match sh {
        // Ligne / flèche : contraint à l'horizontale ou la verticale.
        crate::tools::Shape::Line | crate::tools::Shape::Arrow => {
            if dx.abs() >= dy.abs() {
                (end.0, start.1)
            } else {
                (start.0, end.1)
            }
        }
        // Formes fermées : côtés égaux (carré / cercle / régulier).
        _ => {
            let s = dx.abs().max(dy.abs());
            (start.0 + s * dx.signum(), start.1 + s * dy.signum())
        }
    }
}

/// Dessine une image (rectangle texturé) en coords écran, teintée par l'opacité.
fn draw_image(
    painter: &egui::Painter,
    im: &crate::model::ImageItem,
    tex: &egui::TextureHandle,
    view: &ViewTransform,
    opacity: f32,
) {
    let tint = Color32::WHITE.gamma_multiply(opacity);
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    if im.rot.abs() < 1e-4 {
        let rect = Rect::from_min_max(
            view.doc_to_screen(im.pos),
            view.doc_to_screen((im.pos.0 + im.size.0, im.pos.1 + im.size.1)),
        );
        painter.image(tex.id(), rect, uv, tint);
        return;
    }
    // Image tournée : maillage texturé à 4 sommets (rotation autour du centre).
    let center = (im.pos.0 + im.size.0 * 0.5, im.pos.1 + im.size.1 * 0.5);
    let (co, si) = (im.rot.cos(), im.rot.sin());
    let (hw, hh) = (im.size.0 * 0.5, im.size.1 * 0.5);
    let corner = |sx: f32, sy: f32| {
        let (dx, dy) = (sx * hw, sy * hh);
        view.doc_to_screen((center.0 + dx * co - dy * si, center.1 + dx * si + dy * co))
    };
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let pts = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
    let uvs = [egui::pos2(0.0, 0.0), egui::pos2(1.0, 0.0), egui::pos2(1.0, 1.0), egui::pos2(0.0, 1.0)];
    for (p, u) in pts.iter().zip(uvs.iter()) {
        mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv: *u, color: tint });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// Dessine un texte via le painter, en coords écran (taille ∝ zoom), tourné.
fn draw_text(painter: &egui::Painter, t: &crate::model::TextItem, view: &ViewTransform, opacity: f32) {
    let color = Color32::from_rgba_unmultiplied(t.color[0], t.color[1], t.color[2], t.color[3])
        .gamma_multiply(opacity);
    let galley = painter.layout_no_wrap(
        t.text.clone(),
        egui::FontId::proportional(t.size * view.scale),
        color,
    );
    let mut shape = egui::epaint::TextShape::new(view.doc_to_screen(t.pos), galley, color);
    shape.angle = t.rot;
    painter.add(shape);
}
