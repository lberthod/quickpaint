//! État global de l'application + boucle de frame (sections 1, 6).
//!
//! `PaintApp` relie : modèle (`Document`), capture du geste, historique,
//! outils et rendu. La boucle `update` suit la séquence de la section 6 :
//! lire les évènements → mettre à jour le trait → UI → rendre.

use crate::history::{Command, History, RasterOp, RasterTarget};
use crate::i18n::t;
use crate::input::GestureCapture;
use crate::model::{Document, Stroke, Tool};
use crate::render::canvas::{self, ActiveStroke, StrokeCache, ViewTransform};
use crate::tools::guides::GuideLine;
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

/// Style copié depuis un élément (roadmap P1 #10, pipette de style) :
/// couleur et épaisseur/remplissage partagés par traits et formes, plus les
/// attributs de texte si la source était un texte (`None` sinon — un collage
/// sur un texte gardera alors sa police actuelle).
#[derive(Clone)]
struct StyleClipboard {
    color: [u8; 4],
    width: f32,
    fill: bool,
    text: Option<TextStyleClip>,
}

#[derive(Clone)]
struct TextStyleClip {
    font: crate::model::text::TextFont,
    font_family: Option<String>,
    bold: bool,
    align: crate::model::text::TextAlign,
    outline_w: f32,
    outline_color: [u8; 4],
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

/// Dialogue « Redimensionner l'image » / « Taille du canevas » (roadmap P0 #4).
pub struct ResizeDialog {
    /// `false` = redimensionner l'image (le contenu est mis à l'échelle),
    /// `true` = taille du canevas (le contenu est décalé selon l'ancre).
    pub canvas_mode: bool,
    pub w: u32,
    pub h: u32,
    /// Conserver les proportions (mode image).
    pub keep_ratio: bool,
    /// Ancre 9 positions (mode canevas) : colonne 0..=2, ligne 0..=2.
    pub anchor: (u8, u8),
}

/// État du panneau d'export par lots (Sprint 7.3) : les tailles cochées sont
/// des multiples de `Document::size`, plus une largeur personnalisée
/// optionnelle (hauteur déduite du ratio du document).
pub struct BatchExportState {
    pub format: crate::export::ExportFormat,
    pub scale_half: bool,
    pub scale_1: bool,
    pub scale_2: bool,
    pub scale_3: bool,
    pub custom_enabled: bool,
    pub custom_width: String,
}

impl Default for BatchExportState {
    fn default() -> Self {
        Self {
            format: crate::export::ExportFormat::Png,
            scale_half: false,
            scale_1: true,
            scale_2: true,
            scale_3: false,
            custom_enabled: false,
            custom_width: String::new(),
        }
    }
}

/// Gabarits « riches » avec contenu pré-rempli (Sprint 10.2), au-delà de la
/// simple taille de document de la galerie de modèles existante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateContent {
    InstagramPromo,
    FacebookBanner,
}

/// Nœud ciblé par un glissé pendant l'édition de plume après coup (P2 #12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PenNodeTarget {
    Anchor(usize),
    HandleIn(usize),
    HandleOut(usize),
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
    /// Palette personnalisable de l'utilisateur, persistée localement
    /// (Sprint 7.1 — `settings.json`, jamais synchronisée).
    pub custom_palette: Vec<[u8; 3]>,
    /// Raccourcis clavier des outils, personnalisables (Sprint 7.2).
    pub keybindings: crate::keybindings::KeyBindings,
    /// Presets de style nommés (Sprint 10.3), persistés localement.
    pub style_presets: Vec<crate::model::StylePreset>,
    /// Panneau des presets de style ouvert ?
    pub show_style_presets: bool,
    /// Nom en cours de saisie pour enregistrer le style actuel.
    pub style_preset_name: String,
    /// Panneau de préférences des raccourcis ouvert ?
    pub show_shortcuts_prefs: bool,
    /// Action en attente d'une nouvelle touche (capture au prochain appui).
    pub capturing_shortcut: Option<crate::keybindings::ShortcutAction>,
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
    /// Guides actifs pendant le déplacement en cours (roadmap P1 #8) : lignes
    /// magenta affichées quand un bord/centre de la sélection s'accroche à un
    /// bord/centre d'un autre élément (ou du canevas).
    active_guides: Vec<GuideLine>,
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
    // Édition de nœuds après coup (roadmap P2 #12) : id du trait rouvert +
    // copie de travail de son chemin (mutée en direct pendant le glissé).
    editing_pen: Option<(u64, crate::tools::pen::PenPath)>,
    pen_drag: Option<PenNodeTarget>,
    /// Chemin avant le geste en cours (pour l'entrée d'annulation).
    pen_edit_before: Option<crate::tools::pen::PenPath>,
    // Grille / magnétisme (roadmap #10).
    pub show_grid: bool,
    pub snap_enabled: bool,
    pub grid_size: f32,
    /// Règles graduées le long du canvas (Sprint 2).
    pub show_rulers: bool,
    // Texte (roadmap #2) : taille courante + élément en cours d'édition.
    pub text_size: f32,
    // Style de texte courant (Sprint 3) : appliqué aux nouveaux textes, et au
    // texte édité/sélectionné lorsqu'on modifie ces réglages.
    pub text_font: crate::model::text::TextFont,
    /// Police système sélectionnée (roadmap P1 #7) ; prioritaire sur
    /// `text_font` si présente. `None` = polices intégrées Sans/Mono.
    pub text_font_family: Option<String>,
    /// Filtre de recherche du sélecteur de polices système (UI seulement).
    pub font_search: String,
    pub font_manager: crate::fonts::FontManager,
    pub text_bold: bool,
    pub text_align: crate::model::text::TextAlign,
    pub text_outline_w: f32,
    pub text_outline_color: [u8; 4],
    editing_text: Option<u64>,
    text_focus_pending: bool,
    // Export bitmap (capture d'écran différée d'une frame) + format demandé.
    export_requested: bool,
    export_format: crate::export::ExportFormat,
    // Export par lots / tailles multiples (Sprint 7.3) : capture différée
    // d'une frame, comme l'export simple, mais écrit N fichiers au lieu d'un.
    batch_export_requested: bool,
    batch_export_format: crate::export::ExportFormat,
    batch_export_sizes: Vec<(u32, u32)>,
    /// Panneau « Exporter en plusieurs tailles » ouvert ?
    pub show_batch_export: bool,
    /// État des cases à cocher du panneau d'export par lots.
    pub batch_export: BatchExportState,
    // Pot de peinture : point cliqué (écran) en attente de la capture.
    bucket_click: Option<Pos2>,
    // Détourage en un clic (Sprint 9.1) : point cliqué (écran) + modificateur
    // (⌥ = restaurer) en attente de la capture, comme le pot de peinture.
    cutout_click: Option<(Pos2, bool)>,
    /// Tolérance de couleur du détourage (0..=100, comparable au pot de
    /// peinture mais plus permissive par défaut — un fond photo est rarement
    /// parfaitement uni).
    pub cutout_tolerance: u8,
    /// Détourage non contigu (Sprint 9.1, renforcement) : sélectionne toute
    /// la couleur proche dans la zone visible, pas seulement la région
    /// connectée au clic — utile pour un fond visible par bouts (feuillage…).
    pub cutout_global: bool,
    last_canvas_rect: Rect,
    // Document à taille fixe (roadmap #3).
    last_doc_rect: Rect,
    view_initialized: bool,
    /// Dialogue de redimensionnement image / canevas (roadmap P0 #4).
    pub resize_dialog: Option<ResizeDialog>,
    /// Galerie « Nouveau depuis un modèle » ouverte (roadmap P1 #9).
    pub show_template_gallery: bool,
    /// Bibliothèque d'éléments réutilisables ouverte (Sprint 10.1) ?
    pub show_asset_library: bool,
    /// Style copié (roadmap P1 #10, pipette de style).
    style_clipboard: Option<StyleClipboard>,
    /// Le pinceau/gomme pixel peint dans le masque du calque actif plutôt
    /// que dans son contenu (roadmap P2 #14).
    pub editing_mask: bool,
    /// Saisie hexadécimale de la couleur courante (roadmap P0 #6).
    pub hex_field: String,
    // Pinceau / gomme pixel (roadmap F1) : dureté du tampon (0 = dégradé
    // complet, 1 = bord net) + état du geste en cours.
    pub pixel_hardness: f32,
    raster_stroke_last: Option<(f32, f32)>,
    /// Tuiles touchées pendant le geste en cours, snapshotées **avant**
    /// modification (undo par tuile, cf. `history::Command::PaintRaster`).
    raster_touch: std::collections::HashMap<crate::model::raster::TileKey, Option<crate::model::raster::Tile>>,
    // Tampon de clonage (roadmap P0 #5) : point source (Alt+clic) et décalage
    // figé au début du geste courant (source suit la destination en parallèle).
    pub clone_source: Option<(f32, f32)>,
    clone_offset: Option<(f32, f32)>,
    // --- Retouche locale (Sprint 11) : densité +/-, éponge, flou, netteté,
    // estompe — partagent l'intensité par coup de pinceau (0..=1).
    pub effect_strength: f32,
    /// Miroir/symétrie (Sprint 11) : nombre d'axes (copies rotées autour du
    /// centre du document).
    pub symmetry_axes: u32,
    /// Dégradé interactif (Sprint 11) : type posé par défaut sur les formes
    /// qui n'ont pas encore de dégradé.
    pub gradient_kind: crate::model::GradientKind,
    gradient_drag_start: Option<(f32, f32)>,
    /// Règle / mesure (Sprint 11) : segment affiché pendant le glissé
    /// (distance px + angle), jamais écrit dans le document.
    pub measure: Option<((f32, f32), (f32, f32))>,
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
            custom_palette: crate::i18n::load_custom_palette(),
            keybindings: crate::keybindings::KeyBindings::load(),
            style_presets: crate::i18n::load_style_presets(),
            show_style_presets: false,
            style_preset_name: String::new(),
            show_shortcuts_prefs: false,
            capturing_shortcut: None,
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
            editing_pen: None,
            pen_drag: None,
            pen_edit_before: None,
            show_grid: false,
            snap_enabled: false,
            grid_size: 25.0,
            show_rulers: false,
            selection: HashSet::new(),
            move_origin: None,
            move_delta: (0.0, 0.0),
            active_guides: Vec::new(),
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
            text_font: crate::model::text::TextFont::Proportional,
            text_font_family: None,
            font_search: String::new(),
            font_manager: crate::fonts::FontManager::new(),
            text_bold: false,
            text_align: crate::model::text::TextAlign::Left,
            text_outline_w: 0.0,
            text_outline_color: [255, 255, 255, 255],
            editing_text: None,
            text_focus_pending: false,
            export_requested: false,
            export_format: crate::export::ExportFormat::Png,
            batch_export_requested: false,
            batch_export_format: crate::export::ExportFormat::Png,
            batch_export_sizes: Vec::new(),
            show_batch_export: false,
            batch_export: BatchExportState::default(),
            bucket_click: None,
            cutout_click: None,
            cutout_tolerance: 32,
            cutout_global: false,
            last_canvas_rect: Rect::ZERO,
            last_doc_rect: Rect::ZERO,
            view_initialized: false,
            resize_dialog: None,
            show_template_gallery: false,
            show_asset_library: false,
            style_clipboard: None,
            editing_mask: false,
            hex_field: String::new(),
            pixel_hardness: 0.8,
            raster_stroke_last: None,
            raster_touch: std::collections::HashMap::new(),
            clone_source: None,
            clone_offset: None,
            effect_strength: 0.5,
            symmetry_axes: 4,
            gradient_kind: crate::model::GradientKind::Linear,
            gradient_drag_start: None,
            measure: None,
        }
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        // Variante "Fill" (silhouettes pleines) enregistrée à part sous un nom
        // de famille dédié : utilisée pour les icônes d'outils (tuiles
        // colorées, style boîte à outils), plus contrastée que Regular qui
        // reste la famille par défaut pour les menus/texte.
        fonts.font_data.insert("phosphor-fill".into(), egui_phosphor::Variant::Fill.font_data());
        fonts
            .families
            .insert(egui::FontFamily::Name("phosphor-fill".into()), vec!["phosphor-fill".into()]);
        cc.egui_ctx.set_fonts(fonts);
        Self::default()
    }

    pub fn clear_active_layer(&mut self) {
        let layer = self.doc.active_id();
        let active = &self.doc.layers[self.doc.active_layer];
        let previous = active.strokes.clone();
        let previous_raster = active.raster.clone();
        if !previous.is_empty() || !previous_raster.is_empty() {
            self.history.push(&mut self.doc, Command::Clear { layer, previous, previous_raster });
        }
    }

    pub fn new_document(&mut self) {
        self.apply_loaded(Document::new(self.doc.size));
        self.status = Some(t("Nouveau document.", "New document.").into());
    }

    /// Nouveau document vierge à une taille donnée (roadmap P1 #9, galerie
    /// de modèles) — contrairement à `set_canvas_size`, repart de zéro.
    pub fn new_document_sized(&mut self, w: u32, h: u32) {
        self.apply_loaded(Document::new((w.max(1), h.max(1))));
        self.status = Some(format!("{} {w}×{h}.", t("Nouveau document", "New document")));
    }

    /// Profondeur monotone pour qu'un nouvel élément passe au-dessus des autres.
    fn bump_z(&mut self) -> f64 {
        let z = self.doc.next_z;
        self.doc.next_z += 1.0;
        z
    }

    /// Peuple un document tout juste créé avec un contenu de départ
    /// (Sprint 10.2) : fond coloré + textes/éléments substituables — plutôt
    /// qu'un document vide avec juste la bonne taille. Chaque élément reste
    /// un objet éditable normal (texte, forme…), pas un aplat figé ; chacun
    /// est annulable individuellement puisqu'on repart d'un historique vide
    /// (document tout juste créé par `new_document_sized`).
    pub fn seed_template_content(&mut self, content: TemplateContent) {
        let (dw, dh) = self.doc.size;
        let (w, h) = (dw as f32, dh as f32);
        match content {
            TemplateContent::InstagramPromo => {
                self.add_template_rect((0.0, 0.0), (w, h), [30, 41, 59, 255]);
                self.add_template_text((w * 0.08, h * 0.28), t("Votre titre ici", "Your title here"), h * 0.09, [255, 255, 255, 255]);
                self.add_template_text((w * 0.08, h * 0.44), t("Sous-titre ou accroche", "Subtitle or tagline"), h * 0.04, [214, 219, 230, 255]);
                self.add_template_asset(
                    crate::tools::assets::Asset::Banner,
                    (w * 0.28, h * 0.75),
                    w * 0.32,
                    [234, 88, 12, 255],
                );
                self.add_template_text((w * 0.19, h * 0.72), "PROMO", h * 0.035, [255, 255, 255, 255]);
            }
            TemplateContent::FacebookBanner => {
                self.add_template_rect((0.0, 0.0), (w, h), [15, 76, 129, 255]);
                self.add_template_text((w * 0.06, h * 0.28), t("Nom de la marque", "Brand name"), h * 0.16, [255, 255, 255, 255]);
                self.add_template_text((w * 0.06, h * 0.58), t("Votre slogan ici", "Your tagline here"), h * 0.07, [212, 226, 240, 255]);
            }
        }
        self.status = Some(t("Modèle chargé avec du contenu à personnaliser.", "Template loaded with content to customize.").into());
    }

    /// Rectangle plein (fond de gabarit) — mêmes conventions que l'outil Forme.
    fn add_template_rect(&mut self, min: (f32, f32), max: (f32, f32), color: [u8; 4]) {
        let mut stroke = crate::tools::shape::build(crate::tools::shape::Shape::Rectangle, min, max, color, 0.0, true, 4);
        stroke.id = self.next_id;
        self.next_id += 1;
        stroke.z = self.bump_z();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddStroke { layer, stroke });
    }

    /// Élément de la bibliothèque (Sprint 10.1), pour les gabarits riches.
    fn add_template_asset(&mut self, asset: crate::tools::assets::Asset, center: (f32, f32), size: f32, color: [u8; 4]) {
        let mut stroke = crate::tools::assets::build(asset, center, size, color, 0.0, true);
        stroke.id = self.next_id;
        self.next_id += 1;
        stroke.z = self.bump_z();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddStroke { layer, stroke });
    }

    /// Bloc de texte substituable (Sprint 10.2), gras, aligné à gauche.
    fn add_template_text(&mut self, pos: (f32, f32), text: &str, size: f32, color: [u8; 4]) {
        let id = self.next_id;
        self.next_id += 1;
        let mut item = crate::model::TextItem::new(id, pos, size, color);
        item.text = text.to_string();
        item.bold = true;
        item.z = self.bump_z();
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddText { layer, text: item });
    }

    /// Insère un élément de la bibliothèque (Sprint 10.1) au centre du
    /// document, éditable ensuite comme n'importe quelle forme (nœuds,
    /// couleur, dégradé…) — pas une image bitmap figée.
    pub fn insert_asset(&mut self, asset: crate::tools::assets::Asset) {
        let (dw, dh) = self.doc.size;
        let center = (dw as f32 * 0.5, dh as f32 * 0.5);
        let size = (dw.min(dh) as f32 * 0.35).max(20.0);
        let stroke = crate::tools::assets::build(asset, center, size, self.brush.color, self.brush.width, self.fill_shapes);
        self.commit_stroke(stroke);
        self.status = Some(format!("{} « {} » {}", t("Élément", "Element"), asset.label(), t("ajouté.", "added.")));
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
        self.active_guides.clear();
        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            return;
        }
        self.push_move(dx, dy);
    }

    /// Calcule le déplacement brut (curseur − origine) puis l'accroche aux
    /// bords/centres des autres éléments ou du canevas si assez proche
    /// (roadmap P1 #8, guides intelligents). Met à jour `move_delta` et
    /// `active_guides` (lignes magenta affichées pendant le glissé).
    fn apply_move_with_snap(&mut self, origin: (f32, f32), current: (f32, f32)) {
        let raw = (current.0 - origin.0, current.1 - origin.1);
        let elems = self.selected_elements_bounds();
        if elems.is_empty() {
            self.move_delta = raw;
            self.active_guides.clear();
            return;
        }
        let mn = (
            elems.iter().map(|(_, (mn, _))| mn.0).fold(f32::INFINITY, f32::min),
            elems.iter().map(|(_, (mn, _))| mn.1).fold(f32::INFINITY, f32::min),
        );
        let mx = (
            elems.iter().map(|(_, (_, mx))| mx.0).fold(f32::NEG_INFINITY, f32::max),
            elems.iter().map(|(_, (_, mx))| mx.1).fold(f32::NEG_INFINITY, f32::max),
        );
        let threshold = 6.0 / self.zoom.max(0.01);
        let targets = self.guide_targets();
        let (snapped, guides) = crate::tools::guides::snap((mn, mx), &targets, threshold, raw);
        self.move_delta = snapped;
        self.active_guides = guides;
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
            self.status = Some(t("Baguette : aucun élément coloré ici.", "Wand: no colored element here.").into());
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
            0 => t("Aucun élément sélectionné.", "No element selected.").into(),
            1 => t("1 élément sélectionné.", "1 element selected.").into(),
            _ => format!("{n} {}", t("éléments sélectionnés.", "elements selected.")),
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

    /// Texte ciblé par les réglages de style : celui en cours d'édition, sinon
    /// l'unique texte sélectionné. Sert à l'édition live depuis la barre d'outils.
    fn styled_text_id(&self) -> Option<u64> {
        if let Some(id) = self.editing_text {
            return Some(id);
        }
        if self.selection.len() == 1 {
            let id = *self.selection.iter().next()?;
            if self.doc.layers[self.doc.active_layer].texts.iter().any(|t| t.id == id) {
                return Some(id);
            }
        }
        None
    }

    /// Recopie le style courant dans le texte ciblé (édité/sélectionné), le cas
    /// échéant. Appelé quand l'utilisateur change un réglage de style.
    pub fn sync_text_style(&mut self) {
        let Some(id) = self.styled_text_id() else { return };
        let active = self.doc.active_layer;
        let color = [self.brush.color[0], self.brush.color[1], self.brush.color[2], 255];
        if let Some(t) = self.doc.layers[active].texts.iter_mut().find(|t| t.id == id) {
            t.font = self.text_font;
            t.font_family = self.text_font_family.clone();
            t.bold = self.text_bold;
            t.align = self.text_align;
            t.outline_w = self.text_outline_w;
            t.outline_color = self.text_outline_color;
            t.color = color;
            self.history.touch();
        }
    }

    /// Active le mode recadrage si une seule image est sélectionnée.
    pub fn start_crop(&mut self) {
        if self.single_image_idx().is_some() {
            self.crop_mode = true;
            self.crop_rect = None;
            self.active_tool = ActiveTool::Select;
            self.status = Some(t("Recadrage : glissez la zone à garder.", "Crop: drag the area to keep.").into());
        } else {
            self.status = Some(t("Sélectionne d'abord une image.", "Select an image first.").into());
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
        self.status = Some(t("Image recadrée.", "Image cropped.").into());
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
            item.font = self.text_font;
            item.font_family = self.text_font_family.clone();
            item.bold = self.text_bold;
            item.align = self.text_align;
            item.outline_w = self.text_outline_w;
            item.outline_color = self.text_outline_color;
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
        // Le texte qui vient d'être saisi reste la cible des réglages de
        // style (police, gras…) tant qu'aucune autre sélection n'est faite :
        // sinon, cliquer sur un widget qui vole le focus (ex. un champ de
        // recherche dans la barre d'options) termine silencieusement
        // l'édition, et le prochain réglage changé ne s'applique à rien.
        if self.doc.layers[self.doc.active_layer].texts.iter().any(|t| t.id == id) {
            self.selection.clear();
            self.selection.insert(id);
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
        self.status = Some(t("Image importée — déplacez-la (outil Sélection).", "Image imported — move it (Select tool).").into());
    }

    /// Colle une image depuis le presse-papiers (⌘V) — cœur du cas « comparer ».
    pub fn paste_image(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                self.place_image(w, h, img.bytes.into_owned());
                self.status = Some(t("Image collée depuis le presse-papiers.", "Image pasted from clipboard.").into());
            }
            Err(_) => {
                self.status = Some(t("Aucune image dans le presse-papiers.", "No image in the clipboard.").into());
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
        self.status = Some(t("Images alignées côte à côte.", "Images aligned side by side.").into());
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
            self.status = Some(t("Sélectionne une image (outil Sélection).", "Select an image (Select tool).").into());
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
        self.status = Some(format!("{} {}", t("Filtre appliqué :", "Filter applied:"), filter.label()));
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
        self.status = Some(t("Calque fusionné vers le bas.", "Layer merged down.").into());
    }

    /// Duplique le calque actif (nouveaux ids), inséré au-dessus. Annulable.
    pub fn duplicate_layer(&mut self) {
        let i = self.doc.active_layer;
        let mut dup = self.doc.layers[i].clone();
        dup.id = self.doc.next_layer_id;
        self.doc.next_layer_id += 1;
        dup.name = format!("{} {}", dup.name, t("copie", "copy"));
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
        self.status = Some(t("Calque dupliqué.", "Layer duplicated.").into());
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

    /// Boîtes des éléments **non** sélectionnés du calque actif, plus les
    /// bords/centre du canevas — cibles d'accrochage des guides intelligents
    /// (roadmap P1 #8).
    fn guide_targets(&self) -> Vec<((f32, f32), (f32, f32))> {
        let l = &self.doc.layers[self.doc.active_layer];
        let sel = &self.selection;
        let mut v = Vec::new();
        for s in &l.strokes {
            if !sel.contains(&s.id) {
                if let Some(b) = hit::bounds_of(std::iter::once(s)) {
                    v.push(b);
                }
            }
        }
        for t in &l.texts {
            if !sel.contains(&t.id) {
                v.push(t.approx_bounds());
            }
        }
        for im in &l.images {
            if !sel.contains(&im.id) {
                v.push(im.bounds());
            }
        }
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        v.push(((0.0, 0.0), (w, h)));
        v
    }

    pub fn align(&mut self, mode: AlignMode) {
        let elems = self.selected_elements_bounds();
        if elems.len() < 2 {
            self.status = Some(t("Sélectionne au moins 2 éléments.", "Select at least 2 elements.").into());
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
                    self.status = Some(t("Répartir : au moins 3 éléments.", "Distribute: at least 3 elements.").into());
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
        self.status = Some(t("Éléments alignés.", "Elements aligned.").into());
    }

    /// Aplatit tous les calques (visibles) en un seul. Annulable.
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
        }
        self.selection.clear();
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active, after: vec![base], after_active: 0 },
        );
        self.status = Some(t("Calques aplatis.", "Layers flattened.").into());
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
                    self.status = Some(t(
                        "Avancer/Reculer : sélectionne un seul élément.",
                        "Forward/Backward: select a single element.",
                    ).into());
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
        self.editing_pen = None;
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.cache.clear();
        }
        self.image_textures.clear();
        self.selection.clear();
        self.editing_pen = None;
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
            self.status = Some(t("Copié.", "Copied.").into());
        }
    }

    pub fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    // --- Dégradés de remplissage (roadmap P2 #11, fait partie de F2) --------

    /// Applique un dégradé (couleur courante → blanc) aux formes pleines
    /// sélectionnées, dimensionné sur la boîte englobante de chaque forme
    /// (pas de la sélection entière — chaque forme garde son propre dégradé).
    pub fn apply_gradient(&mut self, kind: crate::model::GradientKind) {
        let active = self.doc.active_layer;
        let sel = self.selection.clone();
        let color_a = self.brush.color;
        let color_b = [255, 255, 255, color_a[3]];
        let l = &mut self.doc.layers[active];
        let mut n = 0;
        for s in &mut l.strokes {
            if sel.contains(&s.id) && s.fill {
                if let Some(bounds) = crate::tools::hit::bounds_of(std::iter::once(&*s)) {
                    s.gradient = Some(crate::model::Gradient::two_stop(kind, bounds, color_a, color_b));
                    n += 1;
                }
            }
        }
        if n > 0 {
            self.history.touch();
            self.cache.clear();
            self.status = Some(format!("{} {n} {}", t("Dégradé appliqué à", "Gradient applied to"), t("forme(s).", "shape(s).")));
        } else {
            self.status = Some(t("Sélectionne au moins une forme pleine (Rempli).", "Select at least one filled shape (Filled).").into());
        }
    }

    pub fn remove_gradient(&mut self) {
        let active = self.doc.active_layer;
        let sel = &self.selection;
        let l = &mut self.doc.layers[active];
        let mut n = 0;
        for s in &mut l.strokes {
            if sel.contains(&s.id) && s.gradient.take().is_some() {
                n += 1;
            }
        }
        if n > 0 {
            self.history.touch();
            self.cache.clear();
            self.status = Some(format!("{} {n} {}", t("Dégradé retiré de", "Gradient removed from"), t("forme(s).", "shape(s).")));
        }
    }

    // --- Booléens de chemins (roadmap P2 #13) --------------------------------

    /// Union/soustraction/intersection des deux formes pleines sélectionnées.
    /// `subject` = trait le plus profond (z le plus petit), `clip` = l'autre —
    /// pertinent pour la soustraction (« retire clip de subject »).
    pub fn boolean_op(&mut self, kind: crate::tools::boolean::BooleanKind) {
        if self.selection.len() != 2 {
            self.status = Some(t("Sélectionne exactement 2 formes pleines.", "Select exactly 2 filled shapes.").into());
            return;
        }
        let active = self.doc.active_layer;
        let layer_id = self.doc.active_id();
        let l = &self.doc.layers[active];
        let mut picked: Vec<(usize, &Stroke)> = l
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| self.selection.contains(&s.id) && s.fill)
            .collect();
        if picked.len() != 2 {
            self.status = Some(t(
                "Sélectionne exactement 2 formes pleines (option « Rempli »).",
                "Select exactly 2 filled shapes (\"Filled\" option).",
            ).into());
            return;
        }
        picked.sort_by(|a, b| a.1.z.partial_cmp(&b.1.z).unwrap());
        let (idx_a, subject) = picked[0];
        let (idx_b, clip) = picked[1];
        let result = crate::tools::boolean::apply(subject, clip, kind);
        let removed = vec![(idx_a, subject.clone()), (idx_b, clip.clone())];
        let z = subject.z;
        let mut added: Vec<Stroke> = Vec::new();
        if let Some(polys) = result {
            for pts in polys {
                let mut s = crate::tools::boolean::stroke_from_points(subject, pts);
                s.id = self.next_id;
                self.next_id += 1;
                s.z = z;
                added.push(s);
            }
        }
        let new_ids: Vec<u64> = added.iter().map(|s| s.id).collect();
        self.history.push(&mut self.doc, Command::SplitStrokes { layer: layer_id, removed, added });
        self.selection = new_ids.into_iter().collect();
        self.cache.clear();
        self.status = Some(match self.selection.is_empty() {
            true => format!("{} : {}", kind.label(), t("résultat vide.", "empty result.")),
            false => format!("{} {}", kind.label(), t("appliquée.", "applied.")),
        });
    }

    // --- Presets de style nommés (Sprint 10.3) ------------------------------

    /// Enregistre le style de l'élément sélectionné (couleur, épaisseur,
    /// remplissage, dégradé si présent) sous `name`, persisté immédiatement.
    /// Écrase un preset existant du même nom plutôt que d'en empiler un
    /// doublon.
    pub fn save_style_preset(&mut self, name: String) {
        if name.trim().is_empty() {
            self.status = Some(t("Donne un nom au preset.", "Give the preset a name.").into());
            return;
        }
        let l = &self.doc.layers[self.doc.active_layer];
        let Some(id) = self.selection.iter().next().copied() else {
            self.status = Some(t("Sélectionne d'abord un élément.", "Select an element first.").into());
            return;
        };
        let Some(s) = l.strokes.iter().find(|s| s.id == id) else {
            self.status = Some(t("Cet élément n'a pas de style enregistrable.", "This element has no savable style.").into());
            return;
        };
        let preset = crate::model::StylePreset {
            name: name.trim().to_string(),
            color: s.color,
            width: s.base_width,
            fill: s.fill,
            gradient: s.gradient.clone(),
        };
        self.style_presets.retain(|p| p.name != preset.name);
        self.style_presets.push(preset);
        crate::i18n::save_style_presets(&self.style_presets);
        self.status = Some(t("Preset de style enregistré.", "Style preset saved.").into());
    }

    pub fn delete_style_preset(&mut self, name: &str) {
        self.style_presets.retain(|p| p.name != name);
        crate::i18n::save_style_presets(&self.style_presets);
    }

    /// Applique un preset de style à tous les éléments sélectionnés (même
    /// logique que `paste_style`, plus le dégradé s'il y en a un).
    pub fn apply_style_preset(&mut self, preset: &crate::model::StylePreset) {
        if self.selection.is_empty() {
            self.status = Some(t("Sélectionne au moins un élément.", "Select at least one element.").into());
            return;
        }
        let active = self.doc.active_layer;
        let l = &mut self.doc.layers[active];
        let mut n = 0;
        for s in &mut l.strokes {
            if self.selection.contains(&s.id) {
                s.color = preset.color;
                s.fill = preset.fill;
                if preset.width > 0.0 {
                    let ratio = preset.width / s.base_width.max(0.01);
                    for p in &mut s.points {
                        p.width *= ratio;
                    }
                    s.base_width = preset.width;
                }
                s.gradient = preset.gradient.clone();
                n += 1;
            }
        }
        for t in &mut l.texts {
            if self.selection.contains(&t.id) {
                t.color = preset.color;
                n += 1;
            }
        }
        if n > 0 {
            self.history.touch();
            self.cache.clear();
            self.status = Some(format!("{} {n} {}", t("Preset appliqué à", "Preset applied to"), t("élément(s).", "element(s).")));
        }
    }

    // --- Pipette de style (roadmap P1 #10) -----------------------------------

    /// Copie le style du premier élément sélectionné (couleur/épaisseur/
    /// remplissage, plus police/gras/alignement/contour si c'est un texte).
    pub fn copy_style(&mut self) {
        let l = &self.doc.layers[self.doc.active_layer];
        let id = match self.selection.iter().next() {
            Some(id) => *id,
            None => {
                self.status = Some(t("Sélectionne d'abord un élément.", "Select an element first.").into());
                return;
            }
        };
        if let Some(s) = l.strokes.iter().find(|s| s.id == id) {
            self.style_clipboard =
                Some(StyleClipboard { color: s.color, width: s.base_width, fill: s.fill, text: None });
            self.status = Some(t("Style copié.", "Style copied.").into());
        } else if let Some(t) = l.texts.iter().find(|t| t.id == id) {
            self.style_clipboard = Some(StyleClipboard {
                color: t.color,
                width: 0.0,
                fill: false,
                text: Some(TextStyleClip {
                    font: t.font,
                    font_family: t.font_family.clone(),
                    bold: t.bold,
                    align: t.align,
                    outline_w: t.outline_w,
                    outline_color: t.outline_color,
                }),
            });
            self.status = Some(crate::i18n::t("Style copié.", "Style copied.").into());
        } else {
            self.status = Some(t("Cet élément n'a pas de style copiable.", "This element has no copyable style.").into());
        }
    }

    /// Applique le style copié à tous les éléments sélectionnés, chacun
    /// selon son propre type (un trait garde sa forme, seul le style change).
    pub fn paste_style(&mut self) {
        let Some(style) = self.style_clipboard.clone() else {
            self.status = Some(t("Copie d'abord un style (⌥⌘C).", "Copy a style first (⌥⌘C).").into());
            return;
        };
        if self.selection.is_empty() {
            self.status = Some(t("Sélectionne au moins un élément.", "Select at least one element.").into());
            return;
        }
        let active = self.doc.active_layer;
        let l = &mut self.doc.layers[active];
        let mut n = 0;
        for s in &mut l.strokes {
            if self.selection.contains(&s.id) {
                s.color = style.color;
                s.fill = style.fill;
                if style.width > 0.0 {
                    let ratio = style.width / s.base_width.max(0.01);
                    for p in &mut s.points {
                        p.width *= ratio;
                    }
                    s.base_width = style.width;
                }
                n += 1;
            }
        }
        for t in &mut l.texts {
            if self.selection.contains(&t.id) {
                t.color = style.color;
                if let Some(ts) = &style.text {
                    t.font = ts.font;
                    t.font_family = ts.font_family.clone();
                    t.bold = ts.bold;
                    t.align = ts.align;
                    t.outline_w = ts.outline_w;
                    t.outline_color = ts.outline_color;
                }
                n += 1;
            }
        }
        if n > 0 {
            self.history.touch();
            self.cache.clear();
            self.status = Some(format!("{} {n} {}", t("Style appliqué à", "Style applied to"), t("élément(s).", "element(s).")));
        } else {
            self.status = Some(t("Aucun trait ni texte dans la sélection.", "No stroke or text in the selection.").into());
        }
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
        self.status = Some(t("Collé.", "Pasted.").into());
        true
    }

    pub fn push_recent_color(&mut self, rgba: [u8; 4]) {
        let rgb = [rgba[0], rgba[1], rgba[2]];
        self.recent_colors.retain(|c| *c != rgb);
        self.recent_colors.insert(0, rgb);
        self.recent_colors.truncate(8);
    }

    /// Ajoute une nuance à la palette personnalisable (Sprint 7.1) et
    /// persiste immédiatement — pas de bouton « enregistrer » séparé.
    pub fn add_to_palette(&mut self, rgb: [u8; 3]) {
        if !self.custom_palette.contains(&rgb) {
            self.custom_palette.push(rgb);
            crate::i18n::save_custom_palette(&self.custom_palette);
        }
    }

    /// Retire une nuance de la palette personnalisable (clic droit sur la
    /// pastille) et persiste immédiatement.
    pub fn remove_from_palette(&mut self, idx: usize) {
        if idx < self.custom_palette.len() {
            self.custom_palette.remove(idx);
            crate::i18n::save_custom_palette(&self.custom_palette);
        }
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
        self.status = Some(format!("{} « {label} » {}", t("Calque d'ajustement", "Adjustment layer"), t("ajouté.", "added.")));
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
        self.status = Some(t("Calques groupés.", "Layers grouped.").into());
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

    /// Change la taille du document (presets) : canevas centré, sans déformer
    /// le contenu. Annulable.
    pub fn set_canvas_size(&mut self, w: u32, h: u32) {
        self.resize_canvas(w, h, (1, 1));
    }

    /// Ouvre le dialogue « Redimensionner l'image » (`canvas_mode = false`) ou
    /// « Taille du canevas » (`canvas_mode = true`), pré-rempli avec la taille
    /// actuelle.
    pub fn open_resize_dialog(&mut self, canvas_mode: bool) {
        self.resize_dialog = Some(ResizeDialog {
            canvas_mode,
            w: self.doc.size.0,
            h: self.doc.size.1,
            keep_ratio: true,
            anchor: (1, 1),
        });
    }

    /// Pousse un remplacement complet du document dans l'historique (annulable)
    /// et invalide tous les caches de rendu.
    fn push_doc_snapshot(&mut self, after: Document, label: &'static str) {
        let before = Box::new(self.doc.clone());
        self.history.push(&mut self.doc, Command::SetDoc { before, after: Box::new(after), label });
        self.cache.clear();
        self.image_textures.clear();
        self.selection.clear();
        self.fit_view();
    }

    /// Redimensionne l'image (façon PhotoFiltre) : le document ET son contenu
    /// sont mis à l'échelle. Non destructif pour les images (leur bitmap source
    /// est conservé, seule la taille affichée change).
    pub fn resize_document(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        let (ow, oh) = self.doc.size;
        if (w, h) == (ow, oh) {
            return;
        }
        let mut after = self.doc.clone();
        after.scale_content(w as f32 / ow as f32, h as f32 / oh as f32);
        after.size = (w, h);
        self.push_doc_snapshot(after, t("Redimensionner l'image", "Resize image"));
        self.status = Some(format!("{} : {w}×{h}", t("Image redimensionnée", "Image resized")));
    }

    /// Change la taille du canevas sans mettre le contenu à l'échelle :
    /// l'ancre (colonne, ligne ∈ 0..=2) fixe le côté du document conservé.
    pub fn resize_canvas(&mut self, w: u32, h: u32, anchor: (u8, u8)) {
        let (w, h) = (w.max(1), h.max(1));
        let (ow, oh) = self.doc.size;
        if (w, h) == (ow, oh) {
            return;
        }
        let dx = (w as f32 - ow as f32) * anchor.0.min(2) as f32 / 2.0;
        let dy = (h as f32 - oh as f32) * anchor.1.min(2) as f32 / 2.0;
        let mut after = self.doc.clone();
        after.translate_content(dx, dy);
        after.size = (w, h);
        self.push_doc_snapshot(after, t("Taille du canevas", "Canvas size"));
        self.status = Some(format!("{} : {w}×{h}", t("Canevas", "Canvas")));
    }

    /// Fenêtre modale du redimensionnement (rendue à chaque frame si ouverte).
    fn show_resize_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.resize_dialog.take() else { return };
        let (ow, oh) = self.doc.size;
        let title = if d.canvas_mode {
            t("Taille du canevas", "Canvas size")
        } else {
            t("Redimensionner l'image", "Resize image")
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!("{} : {ow}×{oh}", t("Taille actuelle", "Current size")));
                ui.horizontal(|ui| {
                    ui.label(t("Largeur :", "Width:"));
                    let rw = ui.add(egui::DragValue::new(&mut d.w).range(1..=16384).suffix(" px"));
                    ui.label(t("Hauteur :", "Height:"));
                    let rh = ui.add(egui::DragValue::new(&mut d.h).range(1..=16384).suffix(" px"));
                    // Proportions liées (mode image) : le champ modifié pilote l'autre.
                    if !d.canvas_mode && d.keep_ratio && oh > 0 && ow > 0 {
                        if rw.changed() {
                            d.h = ((d.w as f64 * oh as f64 / ow as f64).round() as u32).max(1);
                        } else if rh.changed() {
                            d.w = ((d.h as f64 * ow as f64 / oh as f64).round() as u32).max(1);
                        }
                    }
                });
                if d.canvas_mode {
                    ui.label(t("Ancrage du contenu :", "Content anchor:"));
                    // Grille 3×3 : la case cochée est le côté où reste le contenu.
                    for row in 0..3u8 {
                        ui.horizontal(|ui| {
                            for col in 0..3u8 {
                                let sel = d.anchor == (col, row);
                                if ui.selectable_label(sel, if sel { "◉" } else { "○" }).clicked() {
                                    d.anchor = (col, row);
                                }
                            }
                        });
                    }
                } else {
                    ui.checkbox(&mut d.keep_ratio, t("Conserver les proportions", "Keep proportions"));
                    ui.horizontal(|ui| {
                        ui.label(t("Échelle :", "Scale:"));
                        for pct in [25u32, 50, 200] {
                            if ui.button(format!("{pct} %")).clicked() {
                                d.w = (ow.saturating_mul(pct) / 100).max(1);
                                d.h = (oh.saturating_mul(pct) / 100).max(1);
                            }
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t("Annuler", "Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui.button(t("Appliquer", "Apply")).clicked() {
                        apply = true;
                    }
                });
            });
        if apply {
            if d.canvas_mode {
                self.resize_canvas(d.w, d.h, d.anchor);
            } else {
                self.resize_document(d.w, d.h);
            }
        } else if open && !cancel {
            self.resize_dialog = Some(d); // toujours ouvert
        }
    }

    // --- Projet : sauvegarde / ouverture (idée 6) ---------------------------

    /// Encode (paresseusement) le PNG de toutes les images et du raster peint
    /// avant un export nécessitant les données encodées (projet, SVG).
    fn encode_all_images(&mut self) {
        for layer in &mut self.doc.layers {
            for im in &mut layer.images {
                im.ensure_encoded();
            }
            layer.ensure_raster_encoded();
            layer.ensure_mask_encoded();
        }
    }

    /// Ajoute/retire le masque de calque peint (roadmap P2 #14) et bascule
    /// directement en mode édition de masque quand on vient d'en créer un.
    pub fn toggle_active_layer_mask(&mut self) {
        let active = self.doc.active_layer;
        let layer = &mut self.doc.layers[active];
        if layer.mask.is_some() {
            layer.remove_mask();
            self.editing_mask = false;
        } else {
            layer.add_mask();
            self.editing_mask = true;
        }
        self.history.touch();
        self.cache.clear();
    }

    pub fn save_project(&mut self) {
        self.encode_all_images();
        if let Some(p) = crate::project::save_dialog(&self.doc) {
            self.status = Some(format!("{} : {}", t("Projet enregistré", "Project saved"), p.display()));
        }
    }

    pub fn open_project(&mut self) {
        if let Some(doc) = crate::project::open_dialog() {
            self.apply_loaded(doc);
            self.status = Some(t("Projet ouvert.", "Project opened.").into());
        }
    }

    fn apply_loaded(&mut self, mut doc: Document) {
        doc.normalize_ids(); // répare les anciens projets (id manquants)
        // Reconstruit les pixels des images depuis leur PNG base64.
        for layer in &mut doc.layers {
            for im in &mut layer.images {
                im.decode();
            }
            layer.decode_raster();
            layer.decode_mask();
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

    /// Tailles cochées dans le panneau d'export par lots, en pixels, dérivées
    /// de `Document::size` (Sprint 7.3). Vide si rien n'est coché / saisi.
    fn batch_export_target_sizes(&self) -> Vec<(u32, u32)> {
        let (dw, dh) = self.doc.size;
        let mut sizes = Vec::new();
        let mut push = |w: u32, h: u32| {
            if w > 0 && h > 0 && !sizes.contains(&(w, h)) {
                sizes.push((w, h));
            }
        };
        let scale = |m: f32| {
            (((dw as f32) * m).round() as u32, ((dh as f32) * m).round() as u32)
        };
        if self.batch_export.scale_half {
            let (w, h) = scale(0.5);
            push(w, h);
        }
        if self.batch_export.scale_1 {
            push(dw, dh);
        }
        if self.batch_export.scale_2 {
            let (w, h) = scale(2.0);
            push(w, h);
        }
        if self.batch_export.scale_3 {
            let (w, h) = scale(3.0);
            push(w, h);
        }
        if self.batch_export.custom_enabled {
            if let Ok(w) = self.batch_export.custom_width.trim().parse::<u32>() {
                if w > 0 && dw > 0 {
                    let h = ((w as f32) * (dh as f32 / dw as f32)).round() as u32;
                    push(w, h);
                }
            }
        }
        sizes
    }

    /// Déclenche l'export par lots (Sprint 7.3) : une capture d'écran comme
    /// l'export simple, mais écrite en N fichiers (un dossier choisi une
    /// seule fois) dans `handle_screenshot`.
    pub fn request_batch_export(&mut self, ctx: &egui::Context) {
        let sizes = self.batch_export_target_sizes();
        if sizes.is_empty() {
            self.status = Some(t("Aucune taille sélectionnée.", "No size selected.").into());
            return;
        }
        self.batch_export_sizes = sizes;
        self.batch_export_format = self.batch_export.format;
        self.batch_export_requested = true;
        self.show_batch_export = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }

    /// Export SVG vectoriel (opacité de calque correcte via `<g opacity>`).
    pub fn export_svg(&mut self) {
        self.encode_all_images();
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        self.status = Some(match crate::svg::save_to_desktop(&self.doc, bg) {
            Ok(p) => format!("{} : {}", t("SVG enregistré", "SVG saved"), p.display()),
            Err(e) => format!("{} : {e}", t("Échec de l'export SVG", "SVG export failed")),
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
        if let Some((click, restore)) = self.cutout_click.take() {
            self.do_cutout(ctx, &image, click, restore);
            return;
        }
        let ppp = ctx.pixels_per_point();
        // On exporte la zone du document, bornée à la partie visible.
        let r = self.last_doc_rect.intersect(self.last_canvas_rect);
        let crop = (
            (r.min.x * ppp).round().max(0.0) as usize,
            (r.min.y * ppp).round().max(0.0) as usize,
            (r.width() * ppp).round().max(0.0) as usize,
            (r.height() * ppp).round().max(0.0) as usize,
        );

        if self.batch_export_requested {
            self.batch_export_requested = false;
            let format = self.batch_export_format;
            let sizes = std::mem::take(&mut self.batch_export_sizes);
            self.status = Some(match crate::export::save_batch(&image, crop, format, &sizes) {
                Ok(n) => format!("{n} {} ({}).", t("fichiers exportés", "files exported"), format.label()),
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => t("Export annulé.", "Export cancelled.").into(),
                Err(e) => format!("{} : {e}", t("Échec de l'export", "Export failed")),
            });
            return;
        }

        if !self.export_requested {
            return;
        }
        self.export_requested = false;

        let format = self.export_format;
        self.status = Some(match crate::export::save_dialog(&image, crop, format) {
            Ok(p) => format!("{} {} : {}", format.label(), t("enregistré", "saved"), p.display()),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => t("Export annulé.", "Export cancelled.").into(),
            Err(e) => format!("{} : {e}", t("Échec de l'export", "Export failed")),
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
        // Le chemin est déjà échantillonné (courbes de Bézier + angles nets
        // aux sommets anguleux) : un second lissage Catmull-Rom au rendu
        // arrondirait les angles voulus.
        stroke.smooth = false;
        for p in &pts {
            stroke.points.push(crate::model::StrokePoint { pos: *p, width: self.brush.width });
        }
        // Conserve les ancres (roadmap P2 #12) : un double-clic ultérieur
        // avec l'outil Sélection rouvre l'édition des poignées.
        stroke.anchors = Some(crate::tools::pen::PenPath { anchors: std::mem::take(&mut self.pen), closed });
        self.commit_stroke(stroke);
    }

    // --- Édition de nœuds après coup (roadmap P2 #12, F2) -------------------

    /// Double-clic sur un trait de plume : rouvre ses ancres/poignées.
    /// `false` si le trait ciblé n'a pas d'ancres (ex. formes, pinceau libre).
    fn try_start_pen_edit(&mut self, id: u64) -> bool {
        let l = &self.doc.layers[self.doc.active_layer];
        let Some(s) = l.strokes.iter().find(|s| s.id == id) else { return false };
        let Some(path) = &s.anchors else { return false };
        self.editing_pen = Some((id, path.clone()));
        self.selection.clear();
        self.selection.insert(id);
        self.status = Some(t(
            "Édition du chemin : glisse une ancre/poignée ; Échap ou double-clic ailleurs pour terminer.",
            "Editing path: drag an anchor/handle; Esc or double-click elsewhere to finish.",
        ).into());
        true
    }

    fn hit_test_pen_node(&self, d: (f32, f32)) -> Option<PenNodeTarget> {
        let (_, path) = self.editing_pen.as_ref()?;
        let thresh = 8.0 / self.zoom.max(0.01);
        let mut best: Option<(f32, PenNodeTarget)> = None;
        let consider = |pos: (f32, f32), target: PenNodeTarget, best: &mut Option<(f32, PenNodeTarget)>| {
            let dd = ((pos.0 - d.0).powi(2) + (pos.1 - d.1).powi(2)).sqrt();
            if dd <= thresh && best.map(|(bd, _)| dd < bd).unwrap_or(true) {
                *best = Some((dd, target));
            }
        };
        for (i, a) in path.anchors.iter().enumerate() {
            consider(a.pos, PenNodeTarget::Anchor(i), &mut best);
            if a.h_in != a.pos {
                consider(a.h_in, PenNodeTarget::HandleIn(i), &mut best);
            }
            if a.h_out != a.pos {
                consider(a.h_out, PenNodeTarget::HandleOut(i), &mut best);
            }
        }
        best.map(|(_, t)| t)
    }

    /// Déplace le nœud ciblé dans la copie de travail, puis ré-échantillonne
    /// immédiatement le trait pour un aperçu live (même geste que la peinture
    /// raster : mutation directe + `history.touch()`, undo à la fin du geste).
    fn apply_pen_drag(&mut self, target: PenNodeTarget, d: (f32, f32)) {
        let Some((id, path)) = &mut self.editing_pen else { return };
        match target {
            PenNodeTarget::Anchor(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    let delta = (d.0 - a.pos.0, d.1 - a.pos.1);
                    a.pos = d;
                    a.h_in = (a.h_in.0 + delta.0, a.h_in.1 + delta.1);
                    a.h_out = (a.h_out.0 + delta.0, a.h_out.1 + delta.1);
                }
            }
            PenNodeTarget::HandleIn(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    a.h_in = d;
                }
            }
            PenNodeTarget::HandleOut(i) => {
                if let Some(a) = path.anchors.get_mut(i) {
                    a.h_out = d;
                }
            }
        }
        let id = *id;
        let pts = path.sample();
        let active = self.doc.active_layer;
        if let Some(s) = self.doc.layers[active].strokes.iter_mut().find(|s| s.id == id) {
            let w = s.base_width;
            s.points = pts.into_iter().map(|pos| crate::model::StrokePoint { pos, width: w }).collect();
        }
        self.cache.invalidate(std::iter::once(&id));
        self.history.touch();
    }

    /// Fin du glissé d'un nœud : pousse une commande d'undo (avant/après).
    fn commit_pen_edit(&mut self, id: u64) {
        let Some(before_path) = self.pen_edit_before.take() else { return };
        let Some((_, after_path)) = &self.editing_pen else { return };
        let after_path = after_path.clone();
        let active = self.doc.active_layer;
        let layer = self.doc.active_id();
        let Some(s) = self.doc.layers[active].strokes.iter().find(|s| s.id == id) else { return };
        let width = s.base_width;
        let after_points = s.points.clone();
        let before_points: Vec<_> = before_path
            .sample()
            .into_iter()
            .map(|pos| crate::model::StrokePoint { pos, width })
            .collect();
        if before_points.len() == after_points.len()
            && before_points.iter().zip(&after_points).all(|(a, b)| a.pos == b.pos)
        {
            return; // pas de déplacement net (clic sans glissé réel)
        }
        self.history.push(
            &mut self.doc,
            Command::EditPenPath { layer, id, before_path, before_points, after_path, after_points },
        );
    }

    /// Gère le glissé des ancres/poignées tant qu'un chemin est en édition
    /// (roadmap P2 #12) ; appelé en priorité par l'outil Sélection.
    fn handle_pen_node_edit(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let Some((id, _)) = self.editing_pen else { return };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.editing_pen = None;
            self.pen_drag = None;
            self.status = Some(t("Édition du chemin terminée.", "Path editing finished.").into());
            return;
        }
        if response.drag_started() {
            // `interact_pointer_pos()` peut déjà refléter une position avancée
            // dans le glissé (plusieurs évènements souris fusionnés avant la
            // première frame où `drag_started()` devient vrai) : on teste donc
            // le nœud visé à partir du point de pression réel, pas de la
            // position courante du pointeur.
            let press = ctx.input(|i| i.pointer.press_origin());
            if let Some(p) = press {
                let d = view.screen_to_doc(p);
                self.pen_drag = self.hit_test_pen_node(d);
                if self.pen_drag.is_some() {
                    self.pen_edit_before = self.editing_pen.as_ref().map(|(_, p)| p.clone());
                }
            }
        }
        if response.dragged() {
            if let (Some(target), Some(p)) = (self.pen_drag, response.interact_pointer_pos()) {
                let d = self.snap(view.screen_to_doc(p));
                self.apply_pen_drag(target, d);
            }
        }
        if response.drag_stopped() && self.pen_drag.take().is_some() {
            self.commit_pen_edit(id);
        }
        // Double-clic hors de tout nœud : referme l'édition.
        if response.double_clicked() && self.pen_drag.is_none() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if self.hit_test_pen_node(d).is_none() {
                    self.editing_pen = None;
                    self.status = Some(t("Édition du chemin terminée.", "Path editing finished.").into());
                }
            }
        }
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
            l.visible
                && (l.blend != crate::model::BlendMode::Normal
                    || l.opacity < 0.999
                    || l.clip
                    || !l.raster.is_empty()
                    || l.adjustment.is_some()
                    || l.mask.is_some()
                    || l.strokes.iter().any(|s| s.gradient.is_some()))
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
            h = h.wrapping_mul(31).wrapping_add(l.clip as u64);
            h = h.wrapping_mul(31).wrapping_add(l.adjustment.map(|a| a.hash_key() + 1).unwrap_or(0));
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
        self.status = Some(format!("{} ({count} px).", t("Zone remplie", "Area filled")));
    }

    /// Détourage en un clic (Sprint 9.1) : flood-fill depuis le point cliqué
    /// sur la composition affichée (comme le pot de peinture), adouci
    /// (`bucket::feather`), puis écrit comme masque de calque peint — 100 %
    /// local, aucun modèle ni réseau. Le résultat reste éditable ensuite au
    /// pinceau/gomme pixel via « Éditer le masque » (Sprint 9.3).
    ///
    /// `restore` (⌥+clic) inverse le geste : redonne de la visibilité au lieu
    /// d'en retirer, pour rattraper une zone trop agressivement détourée sans
    /// perdre le reste du masque. Les clics successifs sont cumulatifs dans
    /// les deux sens : `cutout_global` sélectionne toute la couleur proche de
    /// la zone visible plutôt que la seule région connexe au clic (fond
    /// visible par bouts, feuillage, grillage…).
    fn do_cutout(&mut self, ctx: &egui::Context, image: &egui::ColorImage, click: Pos2, restore: bool) {
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
            self.status = Some(t("Détourage : rien à restaurer (pas de masque).", "Cutout: nothing to restore (no mask yet).").into());
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
            self.status = Some(t("Détourage : rien à retirer ici.", "Cutout: nothing to remove here.").into());
            return;
        }
        // Masque brut, puis adouci pour un contour progressif plutôt qu'un
        // découpage à l'emporte-pièce : en retrait, 0 = fond à cacher ; en
        // restauration, 255 = zone à rendre visible.
        let raw: Vec<u8> = flooded.iter().map(|&f| if restore == f { 255 } else { 0 }).collect();
        let feathered = crate::tools::bucket::feather(&raw, rw, rh, 2);

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
        self.status = Some(format!("{} ({count} px).", t("Détourage appliqué", "Cutout applied")));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Capture d'un nouveau raccourci en cours (panneau de préférences,
        // Sprint 7.2) : la prochaine touche pressée devient le raccourci de
        // l'action visée, prioritaire sur tout le reste.
        if let Some(action) = self.capturing_shortcut {
            let captured = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, .. } => Some(*key),
                _ => None,
            }));
            if let Some(key) = captured {
                if key == egui::Key::Escape {
                    // Échap annule la capture sans rien changer.
                } else {
                    self.keybindings.set(action, key);
                }
                self.capturing_shortcut = None;
            }
            return;
        }
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
            if cmd && i.modifiers.alt && i.key_pressed(egui::Key::C) {
                self.copy_style();
            } else if cmd && i.key_pressed(egui::Key::C) {
                self.copy_selection();
            }
            if cmd && i.modifiers.alt && i.key_pressed(egui::Key::V) {
                self.paste_style();
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
            if cmd && !i.modifiers.alt && i.key_pressed(egui::Key::V) {
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
                // Changement d'outil : raccourcis personnalisables
                // (Sprint 7.2, `crate::keybindings`) plutôt que câblés en dur.
                for action in crate::keybindings::ShortcutAction::ALL {
                    if self.keybindings.action_pressed(action, i) {
                        self.active_tool = action.tool();
                    }
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
                let font = egui::FontId::new(t.size.clamp(12.0, 48.0), crate::render::text::family(t));
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut t.text)
                        .desired_width(260.0)
                        .hint_text(crate::i18n::t("Tapez votre texte…", "Type your text…"))
                        .font(font),
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

    /// Édition de nœuds après coup (roadmap P2 #12) : ancres/poignées d'un
    /// trait de plume déjà posé, rouvertes par double-clic — même style
    /// visuel que le tracé en cours (`paint_pen`), en orange pour distinguer
    /// « en édition » de « en train de tracer ».
    fn paint_pen_edit(&self, painter: &egui::Painter, view: &ViewTransform) {
        let Some((_, path)) = &self.editing_pen else { return };
        let orange = Color32::from_rgb(230, 140, 20);
        let pts = path.sample();
        let screen: Vec<egui::Pos2> = pts.iter().map(|p| view.doc_to_screen(*p)).collect();
        if screen.len() >= 2 {
            painter.add(egui::Shape::line(screen, egui::Stroke::new(1.5, orange)));
        }
        for a in &path.anchors {
            let c = view.doc_to_screen(a.pos);
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(7.0)), 1.0, orange);
            painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(7.0)), 1.0, egui::Stroke::new(1.0, Color32::WHITE));
            for h in [a.h_in, a.h_out] {
                if h != a.pos {
                    let hp = view.doc_to_screen(h);
                    painter.line_segment([c, hp], egui::Stroke::new(1.0, Color32::from_gray(150)));
                    painter.circle_filled(hp, 3.5, Color32::WHITE);
                    painter.circle_stroke(hp, 3.5, egui::Stroke::new(1.0, orange));
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

    /// Segment de mesure (Sprint 11, outil Règle) : ligne + étiquette
    /// distance/angle, jamais écrit dans le document — cf. `handle_measure`.
    fn paint_measure(&self, painter: &egui::Painter, view: &ViewTransform) {
        let Some((a, b)) = self.measure else { return };
        let (sa, sb) = (view.doc_to_screen(a), view.doc_to_screen(b));
        let col = Color32::from_rgb(40, 200, 160);
        painter.line_segment([sa, sb], egui::Stroke::new(1.5, col));
        painter.circle_filled(sa, 3.0, col);
        painter.circle_filled(sb, 3.0, col);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx).to_degrees();
        let mid = sa + (sb - sa) * 0.5;
        painter.text(
            mid + Vec2::new(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.0} px · {:.1}°", dist, angle),
            egui::FontId::proportional(13.0),
            col,
        );
    }

    /// Anneau de prévisualisation de la taille de l'outil sous le curseur
    /// (repère ergonomique « vrai Paint »). Bichromie pour rester visible sur
    /// tout fond.
    fn paint_cursor(&self, painter: &egui::Painter, response: &egui::Response) {
        let Some(p) = response.hover_pos() else { return };
        let radius = match self.active_tool {
            ActiveTool::Eraser => self.eraser.width * 0.5 * self.zoom,
            ActiveTool::Brush
            | ActiveTool::Line
            | ActiveTool::Rectangle
            | ActiveTool::Ellipse
            | ActiveTool::Dodge
            | ActiveTool::Burn
            | ActiveTool::Saturate
            | ActiveTool::Desaturate
            | ActiveTool::Blur
            | ActiveTool::Sharpen
            | ActiveTool::Smudge => self.brush.width * 0.5 * self.zoom,
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
                // Édition de nœuds (roadmap P2 #12) : prioritaire tant qu'un
                // chemin de plume est rouvert.
                if self.editing_pen.is_some() {
                    self.handle_pen_node_edit(ctx, response, view);
                    return;
                }
                if response.double_clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        if let Some(id) = self.topmost_at(d) {
                            if self.try_start_pen_edit(id) {
                                return;
                            }
                        }
                    }
                }
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
                            self.apply_move_with_snap(o, d);
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
            ActiveTool::Cutout => {
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        // Détourage différé : même mécanisme que le pot de peinture.
                        // ⌥+clic restaure la visibilité au lieu de la retirer —
                        // corrige une zone trop agressivement détourée sans
                        // repasser par « Éditer le masque ».
                        let restore = ctx.input(|i| i.modifiers.alt);
                        self.cutout_click = Some((p, restore));
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
                                t("Couleur prélevée.", "Color picked.").into()
                            }
                            None => t("Pas de trait ici (fond).", "No stroke here (background).").into(),
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
            ActiveTool::PixelBrush | ActiveTool::PixelEraser => {
                let erase = self.active_tool == ActiveTool::PixelEraser;
                if response.drag_started() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        self.raster_touch.clear();
                        self.paint_raster_point(d, erase);
                        self.raster_stroke_last = Some(d);
                    }
                }
                if response.dragged() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let d = view.screen_to_doc(p);
                        match self.raster_stroke_last {
                            Some(last) => self.paint_raster_segment(last, d, erase),
                            None => self.paint_raster_point(d, erase),
                        }
                        self.raster_stroke_last = Some(d);
                    }
                } else if self.raster_stroke_last.is_some() {
                    // Le geste s'est arrêté sans `drag_stopped` propre (perte
                    // de focus de la fenêtre, clic intercepté par l'OS…) :
                    // on referme quand même le trait, sinon la peinture déjà
                    // appliquée au document resterait sans entrée d'annulation.
                    self.raster_stroke_last = None;
                    self.commit_raster_stroke(if erase { RasterOp::Eraser } else { RasterOp::Brush }, self.editing_mask);
                }
                if response.drag_stopped() {
                    self.raster_stroke_last = None;
                    self.commit_raster_stroke(if erase { RasterOp::Eraser } else { RasterOp::Brush }, self.editing_mask);
                }
            }
            ActiveTool::CloneStamp => self.handle_clone_stamp(ctx, response, view, false),
            ActiveTool::Healing => self.handle_clone_stamp(ctx, response, view, true),
            ActiveTool::Dodge => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Lighten, RasterOp::Dodge),
            ActiveTool::Burn => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Darken, RasterOp::Burn),
            ActiveTool::Saturate => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Saturate, RasterOp::Saturate),
            ActiveTool::Desaturate => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Desaturate, RasterOp::Desaturate),
            ActiveTool::Blur => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Blur, RasterOp::Blur),
            ActiveTool::Sharpen => self.handle_pixel_effect(response, view, crate::model::PixelEffect::Sharpen, RasterOp::Sharpen),
            ActiveTool::Smudge => self.handle_smudge(response, view),
            ActiveTool::Measure => self.handle_measure(ctx, response, view),
            ActiveTool::Gradient => self.handle_gradient_drag(response, view),
            _ => self.handle_draw(ctx, response, view),
        }
    }

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
    fn touch_raster_tiles(&mut self, cx: f32, cy: f32, radius: f32, mask: bool) {
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

    fn paint_raster_point(&mut self, d: (f32, f32), erase: bool) {
        let radius = self.pixel_radius(erase);
        self.touch_raster_tiles(d.0, d.1, radius, self.editing_mask);
        let color = self.brush.color;
        let hardness = self.pixel_hardness;
        let layer_id = self.doc.active_id();
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            Self::active_raster_mut(layer, self.editing_mask).stamp(d.0, d.1, radius, hardness, color, erase);
        }
        self.history.touch();
    }

    fn paint_raster_segment(&mut self, from: (f32, f32), to: (f32, f32), erase: bool) {
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
        if let Some(layer) = self.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            Self::active_raster_mut(layer, self.editing_mask)
                .stroke_segment(from, to, radius, hardness, color, erase);
        }
        self.history.touch();
    }

    /// Fin du geste : pousse UNE commande d'undo couvrant toutes les tuiles
    /// touchées (comme Photoshop/GIMP — un trait = un cran d'annulation).
    /// `mask` doit être la même surface que celle passée à `touch_raster_tiles`
    /// pour ce geste (voir sa doc).
    fn commit_raster_stroke(&mut self, op: RasterOp, mask: bool) {
        if self.raster_touch.is_empty() {
            return;
        }
        let layer_id = self.doc.active_id();
        let target = if mask { RasterTarget::Mask } else { RasterTarget::Content };
        let before = std::mem::take(&mut self.raster_touch);
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
    fn handle_clone_stamp(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform, heal: bool) {
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
                self.status = Some(t("Source du tampon définie (glisser pour peindre).", "Stamp source set (drag to paint).").into());
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
                self.status = Some(t("Alt+clic pour définir la source du tampon.", "Alt+click to set the stamp source.").into());
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

    fn handle_pixel_effect(
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
    fn handle_smudge(&mut self, response: &egui::Response, view: &ViewTransform) {
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

    /// Règle / mesure (Sprint 11) : pur survol, ne touche jamais au document
    /// ni à l'historique — seul `self.measure` est mis à jour pour l'aperçu
    /// peint par `paint_measure_overlay`.
    fn handle_measure(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                self.measure = Some((d, d));
            }
        }
        if response.dragged() {
            if let (Some((s, _)), Some(p)) = (self.measure, response.interact_pointer_pos()) {
                self.measure = Some((s, view.screen_to_doc(p)));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.measure = None;
        }
    }

    /// Dégradé interactif (Sprint 11) : glisser sur le canevas pose les deux
    /// points du dégradé directement sur chaque forme pleine sélectionnée —
    /// alternative au menu **Édition › Dégradé** qui ne propose que des
    /// valeurs par défaut calculées depuis la boîte englobante.
    fn handle_gradient_drag(&mut self, response: &egui::Response, view: &ViewTransform) {
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                self.gradient_drag_start = Some(view.screen_to_doc(p));
            }
        }
        if response.dragged() {
            if let (Some(from), Some(p)) = (self.gradient_drag_start, response.interact_pointer_pos()) {
                let to = view.screen_to_doc(p);
                let (kind, color_a) = (self.gradient_kind, self.brush.color);
                let color_b = [255, 255, 255, color_a[3]];
                let active = self.doc.active_layer;
                let sel = self.selection.clone();
                for s in &mut self.doc.layers[active].strokes {
                    if sel.contains(&s.id) && s.fill {
                        match &mut s.gradient {
                            Some(g) => {
                                g.from = from;
                                g.to = to;
                            }
                            None => {
                                s.gradient = Some(crate::model::Gradient {
                                    kind,
                                    from,
                                    to,
                                    stops: vec![(0.0, color_a), (1.0, color_b)],
                                });
                            }
                        }
                    }
                }
                // Invalidation ciblée (pas `cache.clear()`) : ce bloc tourne à
                // chaque frame du glissé, vider tout le cache de maillages à
                // chaque frame serait coûteux sur un document avec beaucoup
                // de traits — cf. le même choix pour `align` (`MoveEach`).
                self.cache.invalidate(sel.iter());
            }
        }
        if response.drag_stopped() {
            self.gradient_drag_start = None;
            if !self.selection.is_empty() {
                self.history.touch();
            }
        }
    }

    /// Pousse une copie miroir/symétrie (Sprint 11) : `symmetry_axes` copies
    /// du trait, réparties par rotation régulière autour du centre du
    /// document, en une seule commande d'undo (comme `duplicate_selection`).
    fn commit_symmetry_stroke(&mut self, stroke: Stroke) {
        if stroke.points.is_empty() {
            return;
        }
        let axes = self.symmetry_axes.max(1);
        let center = (self.doc.size.0 as f32 / 2.0, self.doc.size.1 as f32 / 2.0);
        let mut strokes = Vec::with_capacity(axes as usize);
        for k in 0..axes {
            let angle = k as f32 * std::f32::consts::TAU / axes as f32;
            let (ca, sa) = (angle.cos(), angle.sin());
            let mut c = stroke.clone();
            c.id = self.next_id;
            self.next_id += 1;
            c.z = self.bump_z();
            for p in &mut c.points {
                let (dx, dy) = (p.pos.0 - center.0, p.pos.1 - center.1);
                p.pos = (center.0 + dx * ca - dy * sa, center.1 + dx * sa + dy * ca);
            }
            strokes.push(c);
        }
        self.push_recent_color(stroke.color);
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddMany { layer, strokes });
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
                if self.active_tool == ActiveTool::Symmetry {
                    self.commit_symmetry_stroke(stroke);
                } else {
                    self.commit_stroke(stroke);
                }
            }
        }
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_screenshot(ctx);
        self.handle_shortcuts(ctx);
        self.show_resize_dialog(ctx);
        // Quitter l'édition de texte si on change d'outil.
        if self.active_tool != ActiveTool::Text && self.editing_text.is_some() {
            self.finish_text_editing();
        }
        // Efface l'aperçu de mesure en changeant d'outil, sinon le segment
        // resterait affiché indéfiniment par-dessus un autre outil.
        if self.active_tool != ActiveTool::Measure && self.measure.is_some() {
            self.measure = None;
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
                // Guides intelligents (roadmap P1 #8) : lignes magenta pleine
                // largeur/hauteur là où la sélection accroche un bord/centre
                // d'un autre élément ou du canevas.
                let guide_stroke = egui::Stroke::new(1.0, Color32::from_rgb(255, 0, 200));
                for g in &self.active_guides {
                    match *g {
                        GuideLine::Vertical(x) => {
                            let top = view.doc_to_screen((x, 0.0));
                            let bot = view.doc_to_screen((x, self.doc.size.1 as f32));
                            content.line_segment([top, bot], guide_stroke);
                        }
                        GuideLine::Horizontal(y) => {
                            let l = view.doc_to_screen((0.0, y));
                            let r = view.doc_to_screen((self.doc.size.0 as f32, y));
                            content.line_segment([l, r], guide_stroke);
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
            self.paint_pen_edit(&content, &view);
            self.paint_crop(&painter, &view);
            self.paint_marquee(&painter, &view);
            self.paint_measure(&painter, &view);
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

/// Dessine un texte riche via le painter, en coords écran (taille ∝ zoom),
/// tourné. Police + alignement via le helper partagé ; contour et faux-bold via
/// les passes (cohérent avec le compositeur CPU).
fn draw_text(painter: &egui::Painter, t: &crate::model::TextItem, view: &ViewTransform, opacity: f32) {
    if t.text.is_empty() {
        return;
    }
    let galley = crate::render::text::layout(painter.ctx(), t, view.scale);
    let anchor = view.doc_to_screen(t.pos);
    let (c, s) = (t.rot.cos(), t.rot.sin());
    for (off, col) in crate::render::text::passes(t) {
        // Décalage de passe (unités document) → écran, tourné comme le texte.
        let (ox, oy) = (off.0 * view.scale, off.1 * view.scale);
        let pos = egui::pos2(anchor.x + ox * c - oy * s, anchor.y + ox * s + oy * c);
        let color = Color32::from_rgba_unmultiplied(col[0], col[1], col[2], col[3])
            .gamma_multiply(opacity);
        let mut shape = egui::epaint::TextShape::new(pos, galley.clone(), color);
        shape.override_text_color = Some(color);
        shape.angle = t.rot;
        painter.add(shape);
    }
}
