//! État global de l'application + boucle de frame (sections 1, 6).
//!
//! `PaintApp` relie : modèle (`Document`), capture du geste, historique,
//! outils et rendu. La boucle `update` suit la séquence de la section 6 :
//! lire les évènements → mettre à jour le trait → UI → rendre.

// Édition de nœuds Bézier après coup (roadmap P2 #12) — extrait en sous-module
// (ANALYSE.md §12.5) : sous-système autonome (état + geste + rendu) qui ne
// partage que `Document`/`Stroke` avec le reste de `app`.
mod pen_edit;
// Transformation interactive de la sélection (échelle/rotation) — extrait en
// sous-module (SPRINTS.md 13.8, suite d'ANALYSE.md §12.5) pour la même raison
// que `pen_edit` : état + geste + rendu autonomes, undo dédié.
mod transform;

use crate::history::{Command, History, RasterOp, RasterTarget};
use crate::i18n::t;
use crate::input::GestureCapture;
use crate::model::{Document, Stroke, Tool};
use crate::render::canvas::{self, ActiveStroke, StrokeCache, ViewTransform};
use crate::tools::guides::GuideLine;
use crate::tools::{eyedropper, hit, shape, ActiveTool, Brush, Eraser, SelectMode};
use crate::ui::{footer, layers, toolbar};
use egui::{Color32, Margin, Pos2, Rect, Sense, Vec2};
use pen_edit::PenNodeTarget;
use std::collections::HashSet;
use transform::TransformDrag;

/// Borne des dimensions de document à `model::image::MAX_IMAGE_SIDE`
/// (ANALYSE.md §8.2) — nouveau document, redimensionnement d'image ou de
/// canevas : aucune de ces entrées utilisateur ne doit pouvoir déclencher une
/// allocation sans limite.
fn clamp_doc_dims(w: u32, h: u32) -> (u32, u32) {
    let max = crate::model::image::MAX_IMAGE_SIDE;
    (w.clamp(1, max), h.clamp(1, max))
}

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

pub struct PaintApp {
    pub doc: Document,
    pub history: History,
    pub capture: GestureCapture,
    pub active_tool: ActiveTool,
    pub brush: Brush,
    pub eraser: Eraser,
    pub bg: Color32,
    pub capture_pressure_strength: f32,
    /// Stabilisation du tracé (Sprint 3.2), 0 = brut, 1 = très lissé —
    /// au-delà du lissage EMA fixe déjà en place par défaut.
    pub stroke_stabilization: f32,
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
    /// Préréglages de pinceau enregistrés par l'utilisateur (Sprint 3.4),
    /// persistés localement. Les préréglages fournis (`BrushPreset::builtins`)
    /// s'affichent en plus, jamais dans cette liste.
    pub brush_presets: Vec<crate::model::BrushPreset>,
    /// Panneau de la bibliothèque de brosses ouvert ?
    pub show_brush_library: bool,
    /// Panneau d'histogramme ouvert (Sprint 4.1) ?
    pub show_histogram: bool,
    /// Aperçu « avant » actif (Sprint 4.1) : la dernière action est
    /// temporairement annulée pour comparaison — voir
    /// [`Self::begin_compare_before`]/[`Self::end_compare_before`].
    pub comparing_before: bool,
    /// LUT `.cube` importée (Sprint 5.3), gardée en mémoire pour être
    /// réappliquée avec différentes intensités sans reparser le fichier.
    /// `Rc` : évite de cloner la table (potentiellement volumineuse) à
    /// chaque frame où l'UI y accède en lecture.
    pub loaded_lut: Option<(String, std::rc::Rc<crate::tools::lut::Lut3D>)>,
    /// Intensité de mélange de la LUT chargée, 0..=1.
    pub lut_intensity: f32,
    /// Panneau d'import/application de LUT ouvert (Sprint 5.3) ?
    pub show_lut_panel: bool,
    /// Panneau de transformation en perspective ouvert (Sprint 7.2) ?
    pub show_perspective_panel: bool,
    /// Qualité d'encodage JPEG à l'export (Sprint 8.2), 1..=100 — s'applique
    /// aussi au JPEG embarqué dans l'export PDF. Sans effet sur PNG (sans
    /// perte) ni WebP (toujours sans perte avec la crate `image`, voir
    /// `export::save_dialog`).
    pub jpeg_quality: u8,
    /// Décalage de chaque coin (haut-gauche, haut-droit, bas-droit,
    /// bas-gauche), en fraction de la largeur/hauteur de l'image (-0.5..=0.5)
    /// — indépendant de l'échelle, plus simple à régler que des pixels bruts.
    pub perspective_offsets: [(f32, f32); 4],
    /// Nom en cours de saisie pour enregistrer le pinceau actuel.
    pub brush_preset_name: String,
    /// Groupes de la barre d'outils repliés (UX-2.1), clés stables — voir
    /// `ui::toolbar::tool_group_key`. Persisté localement.
    pub collapsed_toolbar_groups: HashSet<String>,
    /// Largeur du panneau des calques (UX-3.2), persistée localement. Avant,
    /// le panneau était figé à 170px (`resizable(false)`) — un nom de calque
    /// long était tronqué sans recours (constat C5, UX_SPRINTS.md).
    pub layers_panel_width: f32,
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
    /// Sévérité du message courant (UX-1.2) : `true` = échec (rouge dans le
    /// footer), `false` = information/succès (vert). Mis à jour uniquement
    /// via [`Self::info`]/[`Self::fail`], jamais en écrivant `status`
    /// directement, pour que les deux restent toujours synchronisés.
    pub status_error: bool,
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
    /// Saisie du nom en cours pour l'enregistrement d'une sélection nommée
    /// (Sprint 1.2) — état d'UI éphémère, jamais persisté.
    pub named_selection_field: String,
    /// Rectangle de sélection en cours (coin de départ, coin courant).
    marquee: Option<((f32, f32), (f32, f32))>,
    /// Tracé du lasso en cours (échantillons monde).
    lasso: Vec<(f32, f32)>,
    /// Tolérance de la baguette magique (distance couleur par canal, 0–255).
    pub wand_tol: i32,
    /// Portée de la baguette magique (Sprint 2.2) : `true` = toute couleur
    /// proche sur le calque (comportement historique) ; `false` = seulement
    /// la région connexe autour de l'élément cliqué (chaîne de boîtes
    /// englobantes qui se recoupent), à la manière d'un flood-fill contigu.
    pub wand_global: bool,
    clip: ClipBoard,
    // Transformation interactive de la sélection (échelle / rotation).
    xform: Option<TransformDrag>,
    // Recadrage d'image : mode actif + rectangle en cours (coords doc).
    crop_mode: bool,
    crop_rect: Option<((f32, f32), (f32, f32))>,
    /// Retouche destructive d'image par glissé de rectangle (Sprint 4.3/4.4) :
    /// suppression d'objet, yeux rouges, retouche peau — même geste que le
    /// recadrage, traitement différent au relâchement (voir `RetouchKind`).
    retouch_mode: Option<crate::tools::RetouchKind>,
    retouch_rect: Option<((f32, f32), (f32, f32))>,
    /// Redressement d'horizon (Sprint 2.3), en radians : incline le
    /// rectangle de recadrage plutôt que l'image, la rotation est ensuite
    /// « rendue droite » en rééchantillonnant l'image source à l'envers de
    /// cet angle au moment d'appliquer le recadrage.
    pub crop_angle: f32,
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
    /// Ombre portée du texte (Sprint 7.1), `None` = désactivée — mêmes
    /// réglages « en attente » que `text_outline_w`, poussés au texte ciblé
    /// par `sync_text_style`.
    pub text_shadow: Option<crate::model::text::TextShadow>,
    /// Texte sur courbe (Sprint 7.1), même principe que `text_shadow`.
    pub text_arc: Option<crate::model::text::TextArc>,
    editing_text: Option<u64>,
    text_focus_pending: bool,
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
    /// Renommage inline en cours dans le panneau des calques (UX-3.3) :
    /// (id du calque, texte en cours d'édition). `None` = pas de renommage
    /// actif ; double-clic sur un nom de calque le démarre.
    pub layer_rename: Option<(u64, String)>,
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
    /// Récupération après crash (Sprint 1.1) : révision d'historique déjà
    /// autosauvegardée, pour ne réécrire le fichier de récupération que
    /// lorsque le document a réellement changé depuis le dernier tick.
    autosave_last_rev: u64,
    autosave_last_at: std::time::Instant,
    /// Un fichier de récupération d'une session précédente a été détecté au
    /// démarrage : propose à l'utilisateur de le restaurer ou de l'ignorer.
    pub show_recovery_prompt: bool,
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
            stroke_stabilization: 0.5,
            fill_shapes: false,
            poly_sides: 6,
            recent_colors: Vec::new(),
            custom_palette: crate::i18n::load_custom_palette(),
            keybindings: crate::keybindings::KeyBindings::load(),
            style_presets: crate::i18n::load_style_presets(),
            brush_presets: crate::i18n::load_brush_presets(),
            show_brush_library: false,
            brush_preset_name: String::new(),
            show_histogram: false,
            comparing_before: false,
            loaded_lut: None,
            lut_intensity: 1.0,
            show_lut_panel: false,
            show_perspective_panel: false,
            jpeg_quality: 90,
            perspective_offsets: [(0.0, 0.0); 4],
            collapsed_toolbar_groups: crate::i18n::load_collapsed_toolbar_groups().into_iter().collect(),
            layers_panel_width: crate::i18n::load_layers_panel_width(),
            show_style_presets: false,
            style_preset_name: String::new(),
            show_shortcuts_prefs: false,
            capturing_shortcut: None,
            status: None,
            status_error: false,
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
            named_selection_field: String::new(),
            marquee: None,
            lasso: Vec::new(),
            wand_tol: 32,
            wand_global: true,
            clip: ClipBoard::default(),
            xform: None,
            crop_mode: false,
            crop_rect: None,
            retouch_mode: None,
            retouch_rect: None,
            crop_angle: 0.0,
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
            text_shadow: None,
            text_arc: None,
            editing_text: None,
            text_focus_pending: false,
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
            // Premier lancement (pas encore de settings.json) : ouvre la
            // galerie de modèles plutôt qu'un canevas vide et muet (UX-5.1).
            show_template_gallery: crate::i18n::is_first_launch(),
            show_asset_library: false,
            style_clipboard: None,
            editing_mask: false,
            layer_rename: None,
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
            autosave_last_rev: 0,
            autosave_last_at: std::time::Instant::now(),
            show_recovery_prompt: false,
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
        let mut app = Self::default();
        // Détecté une seule fois, avant toute écriture de la session
        // courante : la présence du fichier signifie que la session
        // précédente ne s'est pas terminée proprement (crash, kill -9).
        app.show_recovery_prompt = crate::project::has_recovery();
        app
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
        self.info(t("Nouveau document.", "New document."));
    }

    /// Nouveau document vierge à une taille donnée (roadmap P1 #9, galerie
    /// de modèles) — contrairement à `set_canvas_size`, repart de zéro.
    pub fn new_document_sized(&mut self, w: u32, h: u32) {
        let (w, h) = clamp_doc_dims(w, h);
        self.apply_loaded(Document::new((w, h)));
        self.info(format!("{} {w}×{h}.", t("Nouveau document", "New document")));
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
        self.info(t("Modèle chargé avec du contenu à personnaliser.", "Template loaded with content to customize."));
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
        self.info(format!("{} « {} » {}", t("Élément", "Element"), asset.label(), t("ajouté.", "added.")));
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
    pub(crate) fn single_image_idx(&self) -> Option<usize> {
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

    /// Sélectionne les éléments dont le centre tombe dans l'ellipse inscrite
    /// dans le rectangle glissé (Sprint 2.1) — même test « centre » que le
    /// lasso, cohérent pour l'utilisateur.
    fn select_in_ellipse(&mut self, a: (f32, f32), b: (f32, f32), additive: bool) {
        let rect = ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)));
        if !additive {
            self.selection.clear();
        }
        for (id, _, center) in self.active_elements_geom() {
            if hit::point_in_ellipse(rect, center) {
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
    /// Portée pilotée par `wand_global` : toute l'image, ou seulement la
    /// région connexe (voir [`Self::wand_region_ids`]).
    fn magic_wand(&mut self, d: (f32, f32), additive: bool) {
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
        if !additive {
            self.selection.clear();
        }
        let ids: Vec<u64> = if self.wand_global {
            let l = &self.doc.layers[self.doc.active_layer];
            l.strokes
                .iter()
                .filter(|s| close(s.color))
                .map(|s| s.id)
                .chain(l.texts.iter().filter(|t| close(t.color)).map(|t| t.id))
                .collect()
        } else {
            self.wand_region_ids(clicked, close)
        };
        for id in ids {
            self.selection.insert(id);
        }
        self.report_selection();
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
        if let Some(t) = l.texts.iter().rev().find(|t| in_bounds(d, t.approx_bounds())) {
            return Some(t.color);
        }
        l.strokes.iter().rev().find(|s| hit::point_on_stroke(s, d)).map(|s| s.color)
    }

    /// Met à jour le footer après une sélection par région.
    fn report_selection(&mut self) {
        let n = self.selection.len();
        self.info(match n {
            0 => t("Aucun élément sélectionné.", "No element selected.").into(),
            1 => t("1 élément sélectionné.", "1 element selected.").into(),
            _ => format!("{n} {}", t("éléments sélectionnés.", "elements selected.")),
        });
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
            t.shadow = self.text_shadow;
            t.arc = self.text_arc;
            t.color = color;
            self.history.touch();
        }
    }

    /// Active le mode recadrage si une seule image est sélectionnée.
    pub fn start_crop(&mut self) {
        if self.single_image_idx().is_some() {
            self.crop_mode = true;
            self.crop_rect = None;
            self.crop_angle = 0.0;
            self.active_tool = ActiveTool::Select;
            self.info(t("Recadrage : glissez la zone à garder.", "Crop: drag the area to keep."));
        } else {
            self.info(t("Sélectionne d'abord une image.", "Select an image first."));
        }
    }

    /// Applique le recadrage du rectangle courant à l'image sélectionnée.
    /// Si `crop_angle` n'est pas nul (redressement d'horizon, Sprint 2.3),
    /// rééchantillonne l'image en tournant dans le sens inverse de l'angle
    /// plutôt que de faire une simple copie de sous-rectangle : le contenu
    /// ressort « droit » dans le rectangle de sortie, toujours axé sur les
    /// axes de l'image finale.
    fn apply_crop(&mut self) {
        let Some((a, b)) = self.crop_rect.take() else {
            self.crop_mode = false;
            self.crop_angle = 0.0;
            return;
        };
        self.crop_mode = false;
        let angle = std::mem::replace(&mut self.crop_angle, 0.0);
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

        let (nw, nh, out) = if angle.abs() < 1e-4 {
            let px0 = (((cx0 - im.pos.0) * sx) as i64).clamp(0, im.w as i64) as u32;
            let py0 = (((cy0 - im.pos.1) * sy) as i64).clamp(0, im.h as i64) as u32;
            let px1 = (((cx1 - im.pos.0) * sx).ceil() as i64).clamp(0, im.w as i64) as u32;
            let py1 = (((cy1 - im.pos.1) * sy).ceil() as i64).clamp(0, im.h as i64) as u32;
            let (nw, nh) = (px1.saturating_sub(px0), py1.saturating_sub(py0));
            if nw == 0 || nh == 0 {
                return;
            }
            // Extraction directe du sous-rectangle de pixels (chemin rapide,
            // exact — pas de rééchantillonnage nécessaire sans rotation).
            let mut out = Vec::with_capacity((nw * nh * 4) as usize);
            for y in py0..py1 {
                let row = ((y * im.w + px0) * 4) as usize;
                out.extend_from_slice(&im.rgba[row..row + (nw * 4) as usize]);
            }
            (nw, nh, out)
        } else {
            straighten_and_crop(im, (cx0, cy0, cx1, cy1), sx, sy, angle)
        };
        if nw == 0 || nh == 0 {
            return;
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
        self.info(t("Image recadrée.", "Image cropped."));
    }

    /// Active un mode de retouche par glissé de rectangle (Sprint 4.3/4.4) si
    /// une seule image est sélectionnée — même prérequis que le recadrage.
    pub fn start_retouch(&mut self, kind: crate::tools::RetouchKind) {
        use crate::tools::RetouchKind;
        if self.single_image_idx().is_some() {
            self.retouch_mode = Some(kind);
            self.retouch_rect = None;
            self.active_tool = ActiveTool::Select;
            self.info(match kind {
                RetouchKind::Remove => t(
                    "Suppression d'objet : glissez un rectangle sur l'objet à effacer.",
                    "Object removal: drag a rectangle over the object to erase.",
                ),
                RetouchKind::RedEye => t(
                    "Yeux rouges : glissez un rectangle sur l'œil à corriger.",
                    "Red eye: drag a rectangle over the eye to fix.",
                ),
                RetouchKind::SkinSmooth => t(
                    "Retouche peau : glissez un rectangle sur la zone à adoucir.",
                    "Skin smoothing: drag a rectangle over the area to soften.",
                ),
            });
        } else {
            self.info(t("Sélectionne d'abord une image.", "Select an image first."));
        }
    }

    /// Applique le traitement du rectangle courant (Sprint 4.3/4.4) à
    /// l'image sélectionnée : la taille/position de l'image ne change pas,
    /// seul son contenu à l'intérieur du rectangle est modifié.
    fn apply_retouch(&mut self) {
        use crate::tools::RetouchKind;
        let Some(kind) = self.retouch_mode.take() else { return };
        let Some((a, b)) = self.retouch_rect.take() else { return };
        let Some(idx) = self.single_image_idx() else { return };
        let layer = self.doc.active_id();
        let im = &self.doc.layers[self.doc.active_layer].images[idx];
        let (mn, mx) = im.bounds();
        let rx0 = a.0.min(b.0).max(mn.0);
        let ry0 = a.1.min(b.1).max(mn.1);
        let rx1 = a.0.max(b.0).min(mx.0);
        let ry1 = a.1.max(b.1).min(mx.1);
        if rx1 - rx0 < 2.0 || ry1 - ry0 < 2.0 {
            return; // trop petit
        }
        let sx = im.w as f32 / im.size.0.max(1.0);
        let sy = im.h as f32 / im.size.1.max(1.0);
        let px0 = (((rx0 - im.pos.0) * sx) as i64).clamp(0, im.w as i64) as usize;
        let py0 = (((ry0 - im.pos.1) * sy) as i64).clamp(0, im.h as i64) as usize;
        let px1 = (((rx1 - im.pos.0) * sx).ceil() as i64).clamp(0, im.w as i64) as usize;
        let py1 = (((ry1 - im.pos.1) * sy).ceil() as i64).clamp(0, im.h as i64) as usize;
        if px1 <= px0 || py1 <= py0 {
            return;
        }
        let (w, h) = (im.w as usize, im.h as usize);
        let before = im.clone();
        let mut rgba = before.rgba.clone();
        let label = match kind {
            RetouchKind::Remove => {
                let mut mask = vec![false; w * h];
                for y in py0..py1 {
                    for x in px0..px1 {
                        mask[y * w + x] = true;
                    }
                }
                crate::tools::inpaint::inpaint(&mut rgba, w, h, &mask);
                t("Objet supprimé.", "Object removed.")
            }
            RetouchKind::RedEye => {
                let mask = ellipse_pixel_mask(w, h, px0, py0, px1, py1);
                crate::tools::filter::reduce_red_eye(&mut rgba, w, h, &mask);
                t("Yeux rouges corrigés.", "Red eye fixed.")
            }
            RetouchKind::SkinSmooth => {
                let mut mask = vec![false; w * h];
                for y in py0..py1 {
                    for x in px0..px1 {
                        mask[y * w + x] = true;
                    }
                }
                crate::tools::filter::smooth_skin(&mut rgba, w, h, &mask, 0.7);
                t("Peau adoucie.", "Skin smoothed.")
            }
        };
        let id = before.id;
        let mut after = crate::model::ImageItem::from_rgba(id, before.pos, before.w, before.h, rgba);
        after.size = before.size;
        self.history.push(
            &mut self.doc,
            Command::ReplaceImage { layer, id, before: Box::new(before), after: Box::new(after) },
        );
        self.image_textures.remove(&id);
        self.info(label);
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
            item.shadow = self.text_shadow;
            item.arc = self.text_arc;
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
        match crate::project::import_image_dialog() {
            Some(Ok((w, h, rgba))) => {
                self.place_image(w, h, rgba);
                self.info(t("Image importée — déplacez-la (outil Sélection).", "Image imported — move it (Select tool)."));
            }
            Some(Err(msg)) => {
                self.fail(format!("{} : {msg}", t("Image refusée", "Image rejected")));
            }
            None => {}
        }
    }

    /// Importe un fichier `.psd` (Sprint 8.3) : contrairement à
    /// `import_image` (une image posée dans le document courant), un PSD
    /// devient un **nouveau document** multi-calques, comme `open_project`.
    pub fn import_psd(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Photoshop (.psd)", &["psd"]).pick_file() else {
            return;
        };
        match crate::psd_import::import_psd(&path) {
            Ok(doc) => {
                self.apply_loaded(doc);
                self.info(t("Fichier PSD importé.", "PSD file imported."));
            }
            Err(msg) => {
                self.fail(format!("{} : {msg}", t("Impossible d'importer le PSD", "Couldn't import the PSD")));
            }
        }
    }

    /// Colle une image depuis le presse-papiers (⌘V) — cœur du cas « comparer ».
    pub fn paste_image(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                // Bornage (ANALYSE.md §8.2) : le presse-papiers est une entrée
                // externe comme un fichier, à ne pas allouer sans limite.
                if let Err(e) = crate::model::image::check_dims(w, h) {
                    self.fail(format!("{} : {e}", t("Image du presse-papiers refusée", "Clipboard image rejected")));
                    return;
                }
                self.place_image(w, h, img.bytes.into_owned());
                self.info(t("Image collée depuis le presse-papiers.", "Image pasted from clipboard."));
            }
            Err(_) => {
                self.info(t("Aucune image dans le presse-papiers.", "No image in the clipboard."));
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
        self.info(t("Images alignées côte à côte.", "Images aligned side by side."));
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
            self.info(t("Sélectionne une image (outil Sélection).", "Select an image (Select tool)."));
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
        self.info(format!("{} {}", t("Filtre appliqué :", "Filter applied:"), filter.label()));
    }

    /// Suréchantillonnage haute qualité (Sprint 9.2, sans réseau de neurones :
    /// noyau Lanczos3, comme l'export par lots) des images sélectionnées —
    /// augmente la résolution **native** (`w`/`h`) sans changer la taille
    /// affichée (`size`), pour un rendu plus net à l'export/zoom sans
    /// ré-échantillonner un contenu déjà dégradé.
    pub fn upscale_selection(&mut self, factor: u32) {
        let idx = self.doc.active_layer;
        let layer = self.doc.active_id();
        let ids: Vec<u64> = self.doc.layers[idx]
            .images
            .iter()
            .filter(|im| self.selection.contains(&im.id))
            .map(|im| im.id)
            .collect();
        if ids.is_empty() {
            self.info(t("Sélectionne une image (outil Sélection).", "Select an image (Select tool)."));
            return;
        }
        for id in ids {
            let Some(before) = self.doc.layers[idx].images.iter().find(|im| im.id == id).cloned() else {
                continue;
            };
            let Some(src) = image::RgbaImage::from_raw(before.w, before.h, before.rgba.clone()) else {
                continue;
            };
            let (nw, nh) = (before.w.saturating_mul(factor).max(1), before.h.saturating_mul(factor).max(1));
            if crate::model::image::check_dims(nw, nh).is_err() {
                self.fail(t("Suréchantillonnage refusé : image résultante trop grande.", "Upscale rejected: resulting image too large."));
                continue;
            }
            let resized = image::imageops::resize(&src, nw, nh, image::imageops::FilterType::Lanczos3);
            let mut after = crate::model::ImageItem::from_rgba(id, before.pos, nw, nh, resized.into_raw());
            after.size = before.size; // même taille affichée, juste plus de détail natif
            self.history.push(
                &mut self.doc,
                Command::ReplaceImage { layer, id, before: Box::new(before), after: Box::new(after) },
            );
            self.image_textures.remove(&id);
        }
        self.info(format!("{} {factor}×.", t("Suréchantillonné à", "Upscaled to")));
    }

    // --- Import de LUT .cube (Sprint 5.3) -----------------------------------

    /// Ouvre un sélecteur de fichier `.cube`, le parse et le garde en mémoire
    /// (pas encore appliqué) — l'intensité se règle ensuite avant d'appliquer.
    pub fn import_lut(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("LUT (.cube)", &["cube"]).pick_file() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            self.fail(t("Fichier illisible.", "Unreadable file."));
            return;
        };
        match crate::tools::lut::parse_cube(&text) {
            Ok(lut) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.info(format!("{} « {name} ».", t("LUT chargée", "LUT loaded")));
                self.loaded_lut = Some((name, std::rc::Rc::new(lut)));
            }
            Err(msg) => self.fail(format!("{} : {msg}", t("LUT invalide", "Invalid LUT"))),
        }
    }

    /// Applique la LUT chargée (avec `lut_intensity`) aux images
    /// sélectionnées du calque actif — même schéma undo que `filter_selection`.
    pub fn apply_loaded_lut(&mut self) {
        let Some((name, lut)) = self.loaded_lut.clone() else {
            self.info(t("Importe d'abord une LUT (.cube).", "Import a LUT (.cube) first."));
            return;
        };
        let idx = self.doc.active_layer;
        let layer = self.doc.active_id();
        let ids: Vec<u64> = self.doc.layers[idx]
            .images
            .iter()
            .filter(|im| self.selection.contains(&im.id))
            .map(|im| im.id)
            .collect();
        if ids.is_empty() {
            self.info(t("Sélectionne une image (outil Sélection).", "Select an image (Select tool)."));
            return;
        }
        let intensity = self.lut_intensity;
        for id in ids {
            let Some(before) = self.doc.layers[idx].images.iter().find(|im| im.id == id).cloned() else {
                continue;
            };
            let mut rgba = before.rgba.clone();
            crate::tools::lut::apply_lut(&mut rgba, &lut, intensity);
            let mut after = crate::model::ImageItem::from_rgba(id, before.pos, before.w, before.h, rgba);
            after.size = before.size;
            self.history.push(
                &mut self.doc,
                Command::ReplaceImage { layer, id, before: Box::new(before), after: Box::new(after) },
            );
            self.image_textures.remove(&id);
        }
        self.info(format!("{} « {name} ».", t("LUT appliquée", "LUT applied")));
    }

    // --- Transformation en perspective (Sprint 7.2) -------------------------

    /// Applique `perspective_offsets` (4 coins réglés dans le panneau) à
    /// l'image sélectionnée : reprojette son contenu dans le quadrilatère
    /// résultant (homographie, `tools::perspective`) et redimensionne
    /// l'image à la boîte englobante de ce quadrilatère.
    pub fn apply_perspective_to_selection(&mut self) {
        let idx_layer = self.doc.active_layer;
        let Some(idx) = self.single_image_idx() else {
            self.info(t("Sélectionne une image (outil Sélection).", "Select an image (Select tool)."));
            return;
        };
        let layer = self.doc.active_id();
        let before = self.doc.layers[idx_layer].images[idx].clone();
        if before.w == 0 || before.h == 0 {
            return;
        }
        let (w, h) = (before.w as f32, before.h as f32);
        let base = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
        let corners: [(f32, f32); 4] = std::array::from_fn(|i| {
            let (ox, oy) = self.perspective_offsets[i];
            (base[i].0 + ox * w, base[i].1 + oy * h)
        });
        let minx = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
        let miny = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
        let maxx = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max);
        let maxy = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max);
        let (out_w, out_h) = ((maxx - minx).round().max(1.0) as usize, (maxy - miny).round().max(1.0) as usize);
        let relative: [(f32, f32); 4] = corners.map(|(x, y)| (x - minx, y - miny));
        let rgba = crate::tools::perspective::apply_perspective(&before.rgba, before.w as usize, before.h as usize, relative, out_w, out_h);
        // Même échelle affichage/natif qu'avant, appliquée au nouveau canevas
        // natif (out_w×out_h) — voir `ImageItem` (pos/size indépendants de w/h).
        let (scale_x, scale_y) = (before.size.0 / w, before.size.1 / h);
        let new_pos = (before.pos.0 + minx * scale_x, before.pos.1 + miny * scale_y);
        let new_size = (out_w as f32 * scale_x, out_h as f32 * scale_y);
        let id = before.id;
        let mut after = crate::model::ImageItem::from_rgba(id, new_pos, out_w as u32, out_h as u32, rgba);
        after.size = new_size;
        self.history.push(
            &mut self.doc,
            Command::ReplaceImage { layer, id, before: Box::new(before), after: Box::new(after) },
        );
        self.image_textures.remove(&id);
        self.perspective_offsets = [(0.0, 0.0); 4];
        self.show_perspective_panel = false;
        self.info(t("Perspective appliquée.", "Perspective applied."));
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
        self.info(t("Calque fusionné vers le bas.", "Layer merged down."));
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
        self.info(t("Calque dupliqué.", "Layer duplicated."));
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
            self.info(t("Sélectionne au moins 2 éléments.", "Select at least 2 elements."));
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
                    self.info(t("Répartir : au moins 3 éléments.", "Distribute: at least 3 elements."));
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
        self.info(t("Éléments alignés.", "Elements aligned."));
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
        self.info(t("Calques aplatis.", "Layers flattened."));
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
                    self.info(t(
                        "Avancer/Reculer : sélectionne un seul élément.",
                        "Forward/Backward: select a single element.",
                    ));
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

    // --- Statut (UX-1.2) -----------------------------------------------------
    //
    // Seuls points d'écriture de `status`/`status_error` — un message
    // d'échec (fichier corrompu, dimensions refusées, export impossible…)
    // s'affiche désormais en rouge dans le footer au lieu du même vert
    // qu'un succès (constat C2, UX_SPRINTS.md).

    /// Message neutre/succès (vert dans le footer).
    pub fn info(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
        self.status_error = false;
    }

    /// Message d'échec (rouge dans le footer).
    pub fn fail(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
        self.status_error = true;
    }

    /// Replie/déplie un groupe de la barre d'outils (UX-2.1), persisté
    /// immédiatement — même mécanique que la palette personnalisable.
    pub fn toggle_toolbar_group(&mut self, key: &str) {
        if !self.collapsed_toolbar_groups.remove(key) {
            self.collapsed_toolbar_groups.insert(key.to_string());
        }
        let sorted: Vec<String> = self.collapsed_toolbar_groups.iter().cloned().collect();
        crate::i18n::save_collapsed_toolbar_groups(&sorted);
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

    // --- Comparaison avant/après (Sprint 4.1) -------------------------------
    //
    // S'appuie sur undo/redo plutôt que sur un état parallèle : annule
    // temporairement la dernière action pour montrer l'« avant », la
    // réapplique à la fin du geste. Piloté par un bouton « maintenir »
    // (`is_pointer_button_down_on`) plutôt qu'un bouton à bascule libre : le
    // document reste annulé le temps le plus court possible, ce qui évite
    // qu'une autre action (dessin, etc.) ne s'empile par-dessus et n'efface
    // définitivement le « redo » — `History::push` vide la pile de redo à
    // chaque nouvelle commande.

    /// Annule temporairement la dernière action pour afficher l'état
    /// précédent. No-op si déjà en cours de comparaison ou si rien à annuler.
    pub fn begin_compare_before(&mut self) {
        if self.comparing_before || !self.history.can_undo() {
            return;
        }
        self.history.undo(&mut self.doc);
        self.comparing_before = true;
        self.cache.clear();
        self.image_textures.clear();
    }

    /// Réapplique l'action mise de côté par [`Self::begin_compare_before`].
    pub fn end_compare_before(&mut self) {
        if !self.comparing_before {
            return;
        }
        self.history.redo(&mut self.doc);
        self.comparing_before = false;
        self.cache.clear();
        self.image_textures.clear();
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
            self.info(t("Copié.", "Copied."));
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
            self.info(format!("{} {n} {}", t("Dégradé appliqué à", "Gradient applied to"), t("forme(s).", "shape(s).")));
        } else {
            self.info(t("Sélectionne au moins une forme pleine (Rempli).", "Select at least one filled shape (Filled)."));
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
            self.info(format!("{} {n} {}", t("Dégradé retiré de", "Gradient removed from"), t("forme(s).", "shape(s).")));
        }
    }

    // --- Booléens de chemins (roadmap P2 #13) --------------------------------

    /// Union/soustraction/intersection des deux formes pleines sélectionnées.
    /// `subject` = trait le plus profond (z le plus petit), `clip` = l'autre —
    /// pertinent pour la soustraction (« retire clip de subject »).
    pub fn boolean_op(&mut self, kind: crate::tools::boolean::BooleanKind) {
        if self.selection.len() != 2 {
            self.info(t("Sélectionne exactement 2 formes pleines.", "Select exactly 2 filled shapes."));
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
            self.info(t(
                "Sélectionne exactement 2 formes pleines (option « Rempli »).",
                "Select exactly 2 filled shapes (\"Filled\" option).",
            ));
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
        self.info(match self.selection.is_empty() {
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
            self.info(t("Donne un nom au preset.", "Give the preset a name."));
            return;
        }
        let l = &self.doc.layers[self.doc.active_layer];
        let Some(id) = self.selection.iter().next().copied() else {
            self.info(t("Sélectionne d'abord un élément.", "Select an element first."));
            return;
        };
        let Some(s) = l.strokes.iter().find(|s| s.id == id) else {
            self.info(t("Cet élément n'a pas de style enregistrable.", "This element has no savable style."));
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
        self.info(t("Preset de style enregistré.", "Style preset saved."));
    }

    pub fn delete_style_preset(&mut self, name: &str) {
        self.style_presets.retain(|p| p.name != name);
        crate::i18n::save_style_presets(&self.style_presets);
    }

    // --- Bibliothèque de brosses (Sprint 3.4) -------------------------------

    /// Enregistre les réglages de dessin courants (épaisseur, dureté du
    /// pinceau pixel, stabilisation, pression) sous un nom.
    pub fn save_brush_preset(&mut self, name: String) {
        if name.trim().is_empty() {
            self.info(t("Donne un nom au pinceau.", "Give the brush a name."));
            return;
        }
        let preset = crate::model::BrushPreset {
            name: name.trim().to_string(),
            width: self.brush.width,
            hardness: self.pixel_hardness,
            stabilization: self.stroke_stabilization,
            pressure_strength: self.capture_pressure_strength,
        };
        self.brush_presets.retain(|p| p.name != preset.name);
        self.brush_presets.push(preset);
        crate::i18n::save_brush_presets(&self.brush_presets);
        self.info(t("Pinceau enregistré.", "Brush saved."));
    }

    pub fn delete_brush_preset(&mut self, name: &str) {
        self.brush_presets.retain(|p| p.name != name);
        crate::i18n::save_brush_presets(&self.brush_presets);
    }

    /// Applique un préréglage de pinceau aux réglages de dessin courants.
    pub fn apply_brush_preset(&mut self, preset: &crate::model::BrushPreset) {
        self.brush.width = preset.width;
        self.pixel_hardness = preset.hardness;
        self.stroke_stabilization = preset.stabilization;
        self.capture_pressure_strength = preset.pressure_strength;
        self.info(format!("{} « {} ».", t("Pinceau appliqué", "Brush applied"), preset.name));
    }

    /// Importe un ou plusieurs préréglages depuis un fichier `.json` (un objet
    /// `BrushPreset` seul, ou un tableau) — format sérialisé identique à celui
    /// utilisé en interne, donc les fichiers exportés/partagés entre
    /// installations se rechargent tels quels. Écrase un préréglage existant
    /// du même nom (mêmes règles que l'enregistrement manuel).
    pub fn import_brush_presets(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("Pinceaux QuickPaint", "QuickPaint brushes"), &["json"])
            .pick_file()
        else {
            return;
        };
        match load_brush_presets_from_path(&path) {
            Ok(imported) => {
                let n = imported.len();
                for preset in imported {
                    self.brush_presets.retain(|p| p.name != preset.name);
                    self.brush_presets.push(preset);
                }
                crate::i18n::save_brush_presets(&self.brush_presets);
                self.info(format!("{n} {}", t("pinceau(x) importé(s).", "brush(es) imported.")));
            }
            Err(msg) => self.fail(msg),
        }
    }

    /// Applique un preset de style à tous les éléments sélectionnés (même
    /// logique que `paste_style`, plus le dégradé s'il y en a un).
    pub fn apply_style_preset(&mut self, preset: &crate::model::StylePreset) {
        if self.selection.is_empty() {
            self.info(t("Sélectionne au moins un élément.", "Select at least one element."));
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
            self.info(format!("{} {n} {}", t("Preset appliqué à", "Preset applied to"), t("élément(s).", "element(s).")));
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
                self.info(t("Sélectionne d'abord un élément.", "Select an element first."));
                return;
            }
        };
        if let Some(s) = l.strokes.iter().find(|s| s.id == id) {
            self.style_clipboard =
                Some(StyleClipboard { color: s.color, width: s.base_width, fill: s.fill, text: None });
            self.info(t("Style copié.", "Style copied."));
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
            self.info(crate::i18n::t("Style copié.", "Style copied."));
        } else {
            self.info(t("Cet élément n'a pas de style copiable.", "This element has no copyable style."));
        }
    }

    /// Applique le style copié à tous les éléments sélectionnés, chacun
    /// selon son propre type (un trait garde sa forme, seul le style change).
    pub fn paste_style(&mut self) {
        let Some(style) = self.style_clipboard.clone() else {
            self.info(t("Copie d'abord un style (⌥⌘C).", "Copy a style first (⌥⌘C)."));
            return;
        };
        if self.selection.is_empty() {
            self.info(t("Sélectionne au moins un élément.", "Select at least one element."));
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
            self.info(format!("{} {n} {}", t("Style appliqué à", "Style applied to"), t("élément(s).", "element(s).")));
        } else {
            self.info(t("Aucun trait ni texte dans la sélection.", "No stroke or text in the selection."));
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
        self.info(t("Collé.", "Pasted."));
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
        self.info(format!("{} « {label} » {}", t("Calque d'ajustement", "Adjustment layer"), t("ajouté.", "added.")));
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
        let (w, h) = clamp_doc_dims(w, h);
        let (ow, oh) = self.doc.size;
        if (w, h) == (ow, oh) {
            return;
        }
        let mut after = self.doc.clone();
        after.scale_content(w as f32 / ow as f32, h as f32 / oh as f32);
        after.size = (w, h);
        self.push_doc_snapshot(after, t("Redimensionner l'image", "Resize image"));
        self.info(format!("{} : {w}×{h}", t("Image redimensionnée", "Image resized")));
    }

    /// Change la taille du canevas sans mettre le contenu à l'échelle :
    /// l'ancre (colonne, ligne ∈ 0..=2) fixe le côté du document conservé.
    pub fn resize_canvas(&mut self, w: u32, h: u32, anchor: (u8, u8)) {
        let (w, h) = clamp_doc_dims(w, h);
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
        self.info(format!("{} : {w}×{h}", t("Canevas", "Canvas")));
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

    // --- Récupération après crash (Sprint 1.1) ------------------------------

    /// Intervalle minimal entre deux écritures du fichier de récupération :
    /// assez court pour ne pas perdre grand-chose en cas de crash, assez
    /// long pour ne pas faire de l'I/O disque à chaque frame.
    const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

    /// À appeler à chaque frame (`update`) : écrit le fichier de récupération
    /// si l'intervalle est écoulé **et** que le document a changé depuis le
    /// dernier autosave (comparaison de révision d'historique — pas de coût
    /// disque si l'utilisateur n'a rien fait).
    fn autosave_tick(&mut self) {
        if self.autosave_last_at.elapsed() < Self::AUTOSAVE_INTERVAL {
            return;
        }
        self.autosave_last_at = std::time::Instant::now();
        let rev = self.history.revision();
        if rev == self.autosave_last_rev {
            return; // rien de nouveau à sauvegarder
        }
        self.encode_all_images();
        crate::project::autosave(&self.doc);
        self.autosave_last_rev = rev;
    }

    /// Fenêtre modale affichée une fois au démarrage si une session
    /// précédente s'est terminée sans nettoyer son fichier de récupération.
    fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_recovery_prompt {
            return;
        }
        let mut restore = false;
        let mut discard = false;
        egui::Window::new(t("Récupération de session", "Session recovery"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(t(
                    "QuickPaint ne s'est pas fermé normalement. Restaurer le document en cours au moment de l'interruption ?",
                    "QuickPaint didn't close normally. Restore the document as it was at the time of the interruption?",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t("Ignorer", "Discard")).clicked() {
                        discard = true;
                    }
                    if ui.button(t("Restaurer", "Restore")).clicked() {
                        restore = true;
                    }
                });
            });
        if restore {
            match crate::project::load_recovery() {
                Ok(doc) => {
                    self.apply_loaded(doc);
                    self.info(t("Document restauré.", "Document restored."));
                }
                Err(msg) => {
                    self.fail(format!("{} : {msg}", t("Impossible de restaurer", "Couldn't restore")));
                }
            }
            crate::project::clear_recovery();
            self.show_recovery_prompt = false;
        } else if discard {
            crate::project::clear_recovery();
            self.show_recovery_prompt = false;
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
            crate::i18n::push_recent_project(&p.display().to_string());
            self.info(format!("{} : {}", t("Projet enregistré", "Project saved"), p.display()));
            // Le travail est maintenant en sécurité dans le fichier de
            // projet choisi par l'utilisateur : le brouillon de récupération
            // n'a plus lieu d'être.
            crate::project::clear_recovery();
        }
    }

    pub fn open_project(&mut self) {
        match crate::project::open_dialog() {
            Some((path, Ok(doc))) => {
                self.apply_loaded(doc);
                crate::i18n::push_recent_project(&path.display().to_string());
                self.info(t("Projet ouvert.", "Project opened."));
            }
            Some((_, Err(msg))) => {
                self.fail(format!("{} : {msg}", t("Impossible d'ouvrir le projet", "Couldn't open the project")));
            }
            None => {}
        }
    }

    /// Ouvre un projet depuis un chemin déjà connu (UX-4.3, menu **Fichier ›
    /// Ouvrir récent**) — pas de dialogue.
    pub fn open_recent_project(&mut self, path: &str) {
        match crate::project::open_path(std::path::Path::new(path)) {
            Ok(doc) => {
                self.apply_loaded(doc);
                crate::i18n::push_recent_project(path);
                self.info(t("Projet ouvert.", "Project opened."));
            }
            Err(msg) => {
                self.fail(format!("{} : {msg}", t("Impossible d'ouvrir le projet", "Couldn't open the project")));
            }
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
        // La révision d'historique repart de 0 avec le nouvel historique :
        // resynchronise le compteur d'autosave pour ne pas rater le premier
        // changement réel de cette nouvelle session de document.
        self.autosave_last_rev = 0;
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

    // --- Export bitmap -------------------------------------------------------

    /// Rendu du document à sa résolution native pour l'export (roadmap
    /// ANALYSE.md §12.2) — via le compositeur, jamais via une capture d'écran
    /// du viewport : la résolution exportée est toujours `doc.size`, quels
    /// que soient le zoom et la taille de la fenêtre à l'écran.
    fn render_for_export(&mut self, ctx: &egui::Context) -> Option<(u32, u32, Vec<u8>)> {
        self.compositor.render_to_rgba(ctx, &self.doc, self.bg)
    }

    /// Exporte le document au format `format`, à sa résolution native.
    pub fn request_export(&mut self, ctx: &egui::Context, format: crate::export::ExportFormat) {
        let Some((w, h, rgba)) = self.render_for_export(ctx) else {
            self.fail(t("Échec du rendu à l'export.", "Export render failed."));
            return;
        };
        match crate::export::save_dialog(w, h, &rgba, format, self.jpeg_quality) {
            Ok(p) => self.info(format!("{} {} : {}", format.label(), t("enregistré", "saved"), p.display())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export", "Export failed"))),
        }
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

    /// Déclenche l'export par lots (Sprint 7.3) : rendu natif unique (§12.2),
    /// puis redimensionné (Lanczos3) vers chaque taille cochée — la base 1×
    /// n'est plus une capture d'écran, donc les tailles 2×/3× ne ré-agrandissent
    /// plus une image déjà sous-échantillonnée.
    pub fn request_batch_export(&mut self, ctx: &egui::Context) {
        let sizes = self.batch_export_target_sizes();
        if sizes.is_empty() {
            self.info(t("Aucune taille sélectionnée.", "No size selected."));
            return;
        }
        self.show_batch_export = false;
        let format = self.batch_export.format;
        let Some((w, h, rgba)) = self.render_for_export(ctx) else {
            self.fail(t("Échec du rendu à l'export.", "Export render failed."));
            return;
        };
        match crate::export::save_batch(w, h, &rgba, format, &sizes, self.jpeg_quality) {
            Ok(n) => self.info(format!("{n} {} ({}).", t("fichiers exportés", "files exported"), format.label())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export", "Export failed"))),
        }
    }

    /// Export SVG vectoriel (opacité de calque correcte via `<g opacity>`).
    pub fn export_svg(&mut self) {
        self.encode_all_images();
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        match crate::svg::save_to_desktop(&self.doc, bg) {
            Ok(p) => self.info(format!("{} : {}", t("SVG enregistré", "SVG saved"), p.display())),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export SVG", "SVG export failed"))),
        }
    }

    /// Capture d'écran différée : ne sert plus qu'au pot de peinture et au
    /// détourage (qui échantillonnent les pixels *affichés*, y compris les
    /// modes de fusion, sous le clic) — l'export bitmap ne dépend plus d'une
    /// capture d'écran depuis §12.2, voir `render_for_export`.
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
        } else if let Some((click, restore)) = self.cutout_click.take() {
            self.do_cutout(ctx, &image, click, restore);
        }
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
        let membership = crate::tools::bucket::soft_edge(&region, rw, rh, cx as usize, cy as usize, tolerance, &flooded);
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

    fn paint_selection(&self, painter: &egui::Painter, view: &ViewTransform, moving: bool) {
        if self.active_tool != ActiveTool::Select
            || self.selection.is_empty()
            || self.crop_mode
            || self.retouch_mode.is_some()
        {
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
        if let Some(mut poly) = self.xform_preview(view) {
            poly.push(poly[0]);
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
            if self.crop_angle.abs() < 1e-4 {
                let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, orange));
            } else {
                // Redressement d'horizon (Sprint 2.3) : montre la région de
                // l'image source (encore affichée droite) qui finira droite
                // dans le résultat — c'est ce rectangle tourné qui est
                // échantillonné, cf. `straighten_and_crop`.
                let (cx0, cy0) = (a.0.min(b.0), a.1.min(b.1));
                let (cx1, cy1) = (a.0.max(b.0), a.1.max(b.1));
                let center = ((cx0 + cx1) * 0.5, (cy0 + cy1) * 0.5);
                let (cos_a, sin_a) = (self.crop_angle.cos(), self.crop_angle.sin());
                let rotate = |p: (f32, f32)| {
                    let (rx, ry) = (p.0 - center.0, p.1 - center.1);
                    view.doc_to_screen((center.0 + rx * cos_a - ry * sin_a, center.1 + rx * sin_a + ry * cos_a))
                };
                let corners = [rotate((cx0, cy0)), rotate((cx1, cy0)), rotate((cx1, cy1)), rotate((cx0, cy1))];
                painter.add(egui::Shape::closed_line(corners.to_vec(), egui::Stroke::new(1.5, orange)));
            }
        } else if let Some((_, corners)) = self.selected_image_corners() {
            // Avant le glissé : souligne l'image à recadrer.
            let r = Rect::from_two_pos(view.doc_to_screen(corners[0]), view.doc_to_screen(corners[2]));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, orange));
        }
    }

    /// Overlay des modes de retouche par rectangle (Sprint 4.3/4.4) — une
    /// couleur par type, pour ne pas prêter à confusion avec le recadrage
    /// (orange, garde le contenu) sur ce qui va se passer.
    fn paint_retouch(&self, painter: &egui::Painter, view: &ViewTransform) {
        use crate::tools::RetouchKind;
        let Some(kind) = self.retouch_mode else { return };
        let color = match kind {
            RetouchKind::Remove => Color32::from_rgb(230, 60, 60),
            RetouchKind::RedEye => Color32::from_rgb(230, 60, 230),
            RetouchKind::SkinSmooth => Color32::from_rgb(60, 200, 200),
        };
        if let Some((a, b)) = self.retouch_rect {
            let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, color));
        } else if let Some((_, corners)) = self.selected_image_corners() {
            let r = Rect::from_two_pos(view.doc_to_screen(corners[0]), view.doc_to_screen(corners[2]));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5, color));
        }
    }

    /// Overlay de la sélection par région : rectangle (marquee) ou tracé lasso,
    /// en bleu translucide tant que le geste est en cours.
    fn paint_marquee(&self, painter: &egui::Painter, view: &ViewTransform) {
        let blue = Color32::from_rgb(40, 110, 240);
        let fill = Color32::from_rgba_unmultiplied(40, 110, 240, 28);
        if let Some((a, b)) = self.marquee {
            let r = Rect::from_two_pos(view.doc_to_screen(a), view.doc_to_screen(b));
            if self.select_mode == SelectMode::Ellipse {
                painter.add(egui::Shape::Path(egui::epaint::PathShape::convex_polygon(
                    ellipse_points(r, 48),
                    fill,
                    egui::Stroke::new(1.0, blue),
                )));
            } else {
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, blue));
            }
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

    /// Menu contextuel (clic droit) sur le canevas (UX-3.5) : avant, aucune
    /// des actions ci-dessous n'était accessible autrement qu'en mémorisant
    /// un raccourci clavier ou en ouvrant le menu du haut (constat C7,
    /// UX_SPRINTS.md), alors qu'elles existent déjà comme fonctions. Un clic
    /// droit sur un élément non sélectionné le sélectionne d'abord (comme le
    /// clic gauche), pour que le menu porte toujours sur ce qu'on vient de
    /// désigner plutôt que sur une sélection potentiellement obsolète.
    fn canvas_context_menu(&mut self, response: &egui::Response, view: &ViewTransform) {
        if self.active_tool != ActiveTool::Select {
            return;
        }
        if response.secondary_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if let Some(id) = self.topmost_at(d) {
                    if !self.selection.contains(&id) {
                        self.selection.clear();
                        self.selection.insert(id);
                    }
                }
            }
        }
        response.context_menu(|ui| {
            if self.selection.is_empty() {
                ui.label(t("Aucun élément sélectionné.", "No element selected."));
                return;
            }
            if ui.button(t("Dupliquer (⌘D)", "Duplicate (⌘D)")).clicked() {
                self.duplicate_selection();
                ui.close_menu();
            }
            if ui.button(t("Supprimer (Suppr)", "Delete (Del)")).clicked() {
                self.delete_selection();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(t("Copier le style (⌥⌘C)", "Copy style (⌥⌘C)")).clicked() {
                self.copy_style();
                ui.close_menu();
            }
            if ui.button(t("Coller le style (⌥⌘V)", "Paste style (⌥⌘V)")).clicked() {
                self.paste_style();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(t("Premier plan (⌘⇧])", "Bring to front (⌘⇧])")).clicked() {
                self.reorder(ZMove::Front);
                ui.close_menu();
            }
            if ui.button(t("Avancer (⌘])", "Bring forward (⌘])")).clicked() {
                self.reorder(ZMove::Forward);
                ui.close_menu();
            }
            if ui.button(t("Reculer (⌘[)", "Send backward (⌘[)")).clicked() {
                self.reorder(ZMove::Backward);
                ui.close_menu();
            }
            if ui.button(t("Arrière-plan (⌘⇧[)", "Send to back (⌘⇧[)")).clicked() {
                self.reorder(ZMove::Back);
                ui.close_menu();
            }
        });
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
                // Modes de retouche par rectangle (Sprint 4.3/4.4) : même
                // geste que le recadrage, sans contrainte de ratio.
                if self.retouch_mode.is_some() {
                    if response.drag_started() {
                        if let Some(p) = response.interact_pointer_pos() {
                            let d = view.screen_to_doc(p);
                            self.retouch_rect = Some((d, d));
                        }
                    }
                    if response.dragged() {
                        if let (Some((s, _)), Some(p)) =
                            (self.retouch_rect, response.interact_pointer_pos())
                        {
                            self.retouch_rect = Some((s, view.screen_to_doc(p)));
                        }
                    }
                    if response.drag_stopped() {
                        self.apply_retouch();
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.retouch_mode = None;
                        self.retouch_rect = None;
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
                        if self.select_mode == SelectMode::Ellipse {
                            self.select_in_ellipse(a, b, shift);
                        } else {
                            self.select_in_rect(a, b, shift);
                        }
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
                        match eyedropper::pick(&self.doc, d) {
                            Some(rgb) => {
                                self.brush.color = [rgb[0], rgb[1], rgb[2], self.brush.color[3]];
                                self.info(t("Couleur prélevée.", "Color picked."));
                            }
                            None => self.info(t("Pas de trait ici (fond).", "No stroke here (background).")),
                        }
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
                    self.capture.set_stabilization(self.stroke_stabilization);
                    self.capture
                        .begin(d, self.brush.color, self.brush.width, Tool::Brush, now, real_pressure(ctx));
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
                    _ => self.capture.extend(d, now, real_pressure(ctx)),
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
        self.autosave_tick();
        // Sans repaint périodique, une session restée inactive (aucune
        // interaction) ne redéclenche jamais `update` et l'autosave ne
        // tourne plus — un crash après une longue pause perdrait tout.
        ctx.request_repaint_after(Self::AUTOSAVE_INTERVAL);
        self.show_recovery_dialog(ctx);
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

        // Panneau redimensionnable (UX-3.2) — était figé à 170px, un nom de
        // calque long était tronqué sans recours (constat C5). `default_width`
        // ne s'applique qu'à la toute première frame d'une session egui : au
        // relancement de l'app, elle restaure donc la largeur persistée.
        let layers_resp = egui::SidePanel::right("layers")
            .resizable(true)
            .default_width(self.layers_panel_width)
            .width_range(140.0..=320.0)
            .show(ctx, |ui| layers::show(ui, self));
        let new_width = layers_resp.response.rect.width();
        if (new_width - self.layers_panel_width).abs() > 0.5 {
            self.layers_panel_width = new_width;
            // Écrit seulement une fois le glissé terminé (bouton relâché) :
            // évite une écriture disque à chaque frame pendant le drag.
            if !ctx.input(|i| i.pointer.any_down()) {
                crate::i18n::save_layers_panel_width(new_width);
            }
        }

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
            self.canvas_context_menu(&response, &view);

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
            self.paint_retouch(&painter, &view);
            self.paint_marquee(&painter, &view);
            self.paint_measure(&painter, &view);
            self.paint_cursor(&painter, &response);
            if self.show_rulers {
                self.paint_rulers(&painter, &view);
            }

            self.text_editor(ctx, &view);
        });
    }

    /// Une fermeture propre (Cmd+Q, croix de fenêtre) n'a pas besoin du
    /// filet de sécurité : le fichier de récupération ne doit signaler
    /// qu'une session interrompue anormalement (crash, kill -9), qui ne
    /// passe jamais par ce chemin.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        crate::project::clear_recovery();
    }
}

/// Lit un fichier `.json` de préréglages de pinceau (Sprint 3.4) : accepte
/// aussi bien un objet `BrushPreset` seul qu'un tableau, pour rester
/// pratique à écrire/partager à la main. Séparée d'`import_brush_presets`
/// pour rester testable sans dialogue de fichier natif.
fn load_brush_presets_from_path(path: &std::path::Path) -> Result<Vec<crate::model::BrushPreset>, String> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("{} : {e}", t("fichier illisible", "unreadable file")))?;
    if let Ok(one) = serde_json::from_str(&data) {
        return Ok(vec![one]);
    }
    serde_json::from_str(&data).map_err(|e| format!("{} : {e}", t("fichier de pinceau JSON invalide", "invalid brush JSON file")))
}

/// Pression réelle du geste courant (Sprint 3.1), si le périphérique en
/// fournit une cette frame : egui relaie la force d'un stylet/tablette
/// compatible via `Event::Touch { force: Some(_), .. } — "en addition" des
/// évènements souris habituels (voir egui `input.rs`), donc jamais garanti :
/// `None` fait retomber `GestureCapture` sur la simulation vitesse→épaisseur
/// déjà en place, sans changement de comportement pour une souris/trackpad.
fn real_pressure(ctx: &egui::Context) -> Option<f32> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), .. } => Some(*f),
            _ => None,
        })
    })
}

/// Rééchantillonne `im` par échantillonnage inverse (nearest-neighbor) : pour
/// chaque pixel du rectangle de sortie `(cx0, cy0, cx1, cy1)` (coords doc),
/// tourne la position autour du centre du rectangle par `angle` (radians)
/// pour retrouver la position correspondante dans l'image source non
/// tournée. Hors de l'image source → pixel transparent. Renvoie
/// `(largeur, hauteur, pixels RGBA)`.
fn straighten_and_crop(
    im: &crate::model::ImageItem,
    (cx0, cy0, cx1, cy1): (f32, f32, f32, f32),
    sx: f32,
    sy: f32,
    angle: f32,
) -> (u32, u32, Vec<u8>) {
    let nw = (((cx1 - cx0) * sx).round().max(1.0)) as u32;
    let nh = (((cy1 - cy0) * sy).round().max(1.0)) as u32;
    let center = ((cx0 + cx1) * 0.5, (cy0 + cy1) * 0.5);
    let (cos_a, sin_a) = (angle.cos(), angle.sin());
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for j in 0..nh {
        for i in 0..nw {
            // Pixel de sortie → coords doc (centre du pixel).
            let dx = cx0 + (i as f32 + 0.5) / sx;
            let dy = cy0 + (j as f32 + 0.5) / sy;
            // Tourne autour du centre du rectangle : retrouve la position
            // dans l'image source avant redressement.
            let (rx, ry) = (dx - center.0, dy - center.1);
            let sx_doc = center.0 + rx * cos_a - ry * sin_a;
            let sy_doc = center.1 + rx * sin_a + ry * cos_a;
            // Vers pixels source de l'image.
            let px = ((sx_doc - im.pos.0) * sx) as i64;
            let py = ((sy_doc - im.pos.1) * sy) as i64;
            if px >= 0 && py >= 0 && (px as u32) < im.w && (py as u32) < im.h {
                let sidx = (((py as u32) * im.w + (px as u32)) * 4) as usize;
                let didx = ((j * nw + i) * 4) as usize;
                out[didx..didx + 4].copy_from_slice(&im.rgba[sidx..sidx + 4]);
            }
        }
    }
    (nw, nh, out)
}

/// Masque booléen (w×h) vrai pour les pixels dans l'ellipse inscrite dans le
/// rectangle `[px0,px1) × [py0,py1)` (Sprint 4.4, yeux rouges) — forme plus
/// fidèle qu'un rectangle pour une correction centrée sur l'œil.
fn ellipse_pixel_mask(w: usize, h: usize, px0: usize, py0: usize, px1: usize, py1: usize) -> Vec<bool> {
    let rect = ((px0 as f32, py0 as f32), (px1 as f32, py1 as f32));
    let mut mask = vec![false; w * h];
    for y in py0..py1.min(h) {
        for x in px0..px1.min(w) {
            if crate::tools::hit::point_in_ellipse(rect, (x as f32 + 0.5, y as f32 + 0.5)) {
                mask[y * w + x] = true;
            }
        }
    }
    mask
}

/// Approxime l'ellipse inscrite dans `r` par un polygone à `segments` côtés,
/// pour le rendu de l'overlay de sélection (Sprint 2.1) — `egui` n'a pas de
/// primitive ellipse remplie directement utilisable ici.
fn ellipse_points(r: Rect, segments: usize) -> Vec<Pos2> {
    let center = r.center();
    let (rx, ry) = (r.width() * 0.5, r.height() * 0.5);
    (0..segments)
        .map(|i| {
            let a = i as f32 / segments as f32 * std::f32::consts::TAU;
            Pos2::new(center.x + rx * a.cos(), center.y + ry * a.sin())
        })
        .collect()
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
    if let Some(arc) = &t.arc {
        draw_text_arc(painter, t, arc, view, opacity);
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

/// Dessine un texte sur courbe (Sprint 7.1) : chaque caractère est mis en
/// page et posé individuellement, plutôt qu'un seul galley global tourné en
/// bloc — nécessaire puisque chaque lettre a sa propre orientation le long du
/// cercle. `t.rot` est ignoré en mode arc (voir doc de `TextArc`) : c'est
/// `start_angle_deg` qui pilote l'orientation d'ensemble.
fn draw_text_arc(
    painter: &egui::Painter,
    t: &crate::model::TextItem,
    arc: &crate::model::text::TextArc,
    view: &ViewTransform,
    opacity: f32,
) {
    let center = view.doc_to_screen(t.pos);
    for ac in crate::render::text::arc_chars(t, arc) {
        let mut single = t.clone();
        single.text = ac.ch.to_string();
        single.rot = 0.0;
        single.arc = None; // évite une récursion infinie si jamais réutilisé
        let galley = crate::render::text::layout(painter.ctx(), &single, view.scale);
        let (gw, gh) = (galley.rect.width(), galley.rect.height());
        let (c, s) = (ac.angle.cos(), ac.angle.sin());
        let char_center = egui::pos2(center.x + ac.offset.0 * view.scale, center.y + ac.offset.1 * view.scale);
        for (poff, col) in crate::render::text::passes(&single) {
            let lx = -gw * 0.5 + poff.0 * view.scale;
            let ly = -gh * 0.5 + poff.1 * view.scale;
            let pos = egui::pos2(char_center.x + lx * c - ly * s, char_center.y + lx * s + ly * c);
            let color = Color32::from_rgba_unmultiplied(col[0], col[1], col[2], col[3]).gamma_multiply(opacity);
            let mut shape = egui::epaint::TextShape::new(pos, galley.clone(), color);
            shape.override_text_color = Some(color);
            shape.angle = ac.angle;
            painter.add(shape);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UX-3.1 : glisser-déposer un calque vers l'avant (index croissant) ou
    /// vers l'arrière (index décroissant) doit produire le même ordre que le
    /// modèle mental « ce calque prend la place de la cible » — et le calque
    /// actif doit suivre son propre contenu, pas rester au même index brut.
    fn layer_ids(app: &PaintApp) -> Vec<u64> {
        app.doc.layers.iter().map(|l| l.id).collect()
    }

    fn app_with_layers(n: usize) -> PaintApp {
        let mut app = PaintApp::default();
        app.doc.layers.clear();
        for i in 0..n {
            app.doc.layers.push(crate::model::Layer::new(i as u64 + 1, format!("L{}", i + 1)));
        }
        app.doc.active_layer = 0;
        app
    }

    #[test]
    fn reorder_layer_moves_backward_in_the_stack() {
        let mut app = app_with_layers(4); // ids 1,2,3,4 = L1..L4
        app.doc.active_layer = 2; // L3
        app.reorder_layer(4, 1); // déplace L4 à la position de L1
        assert_eq!(layer_ids(&app), vec![4, 1, 2, 3]);
        // L3 (id 3) était à l'index 2, se retrouve à l'index 3.
        assert_eq!(app.doc.active_layer, 3);
    }

    #[test]
    fn reorder_layer_moves_forward_in_the_stack() {
        let mut app = app_with_layers(4);
        app.doc.active_layer = 1; // L2
        app.reorder_layer(1, 3); // déplace L1 à la position de L3
        assert_eq!(layer_ids(&app), vec![2, 3, 1, 4]);
        // L2 (id 2) était à l'index 1, se retrouve à l'index 0.
        assert_eq!(app.doc.active_layer, 0);
    }

    #[test]
    fn reorder_layer_active_follows_the_moved_layer() {
        let mut app = app_with_layers(3);
        app.doc.active_layer = 0; // L1, celui qu'on déplace
        app.reorder_layer(1, 3); // L1 -> position de L3
        assert_eq!(layer_ids(&app), vec![2, 3, 1]);
        assert_eq!(app.doc.active_layer, 2);
    }

    #[test]
    fn reorder_layer_is_a_noop_for_unknown_ids() {
        let mut app = app_with_layers(2);
        let before = layer_ids(&app);
        app.reorder_layer(999, 1);
        assert_eq!(layer_ids(&app), before);
    }

    fn add_stroke_at(app: &mut PaintApp, layer: usize, id: u64, pos: (f32, f32)) {
        let mut s = Stroke::new([0, 0, 0, 255], 4.0, crate::model::stroke::Tool::Brush);
        s.id = id;
        s.points.push(crate::model::stroke::StrokePoint { pos, width: 4.0 });
        app.doc.layers[layer].strokes.push(s);
    }

    #[test]
    fn upscale_selection_increases_native_resolution_but_not_displayed_size() {
        let mut app = app_with_layers(1);
        let mut im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 4, 3, vec![200u8; 4 * 3 * 4]);
        im.size = (40.0, 30.0); // taille affichée arbitraire, indépendante de w/h
        app.doc.layers[0].images.push(im);
        app.selection = [1].into_iter().collect();

        app.upscale_selection(2);

        let after = &app.doc.layers[0].images[0];
        assert_eq!((after.w, after.h), (8, 6), "résolution native doublée");
        assert_eq!(after.size, (40.0, 30.0), "taille affichée inchangée");
        assert_eq!(after.rgba.len(), 8 * 6 * 4);
    }

    #[test]
    fn upscale_selection_is_a_noop_without_a_selected_image() {
        let mut app = app_with_layers(1);
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 4, 3, vec![200u8; 4 * 3 * 4]);
        app.doc.layers[0].images.push(im);
        // Pas de sélection : aucune image ciblée.
        app.upscale_selection(2);
        let after = &app.doc.layers[0].images[0];
        assert_eq!((after.w, after.h), (4, 3));
    }

    #[test]
    fn compare_before_undoes_and_redoes_the_last_action() {
        let mut app = app_with_layers(1);
        let layer = app.doc.active_id();
        let mut s = Stroke::new([0, 0, 0, 255], 4.0, crate::model::stroke::Tool::Brush);
        s.points.push(crate::model::stroke::StrokePoint { pos: (0.0, 0.0), width: 4.0 });
        app.history.push(&mut app.doc, Command::AddStroke { layer, stroke: s });
        assert_eq!(app.doc.layers[0].strokes.len(), 1);

        app.begin_compare_before();
        assert!(app.comparing_before);
        assert_eq!(app.doc.layers[0].strokes.len(), 0, "l'action doit être temporairement annulée");

        app.end_compare_before();
        assert!(!app.comparing_before);
        assert_eq!(app.doc.layers[0].strokes.len(), 1, "l'action doit être réappliquée");
    }

    #[test]
    fn compare_before_is_a_noop_with_nothing_to_undo() {
        let mut app = app_with_layers(1);
        app.begin_compare_before();
        assert!(!app.comparing_before);
    }

    #[test]
    fn named_selection_round_trips_on_the_same_layer() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 10, (0.0, 0.0));
        add_stroke_at(&mut app, 0, 11, (5.0, 5.0));
        app.selection = [10, 11].into_iter().collect();
        app.save_named_selection("zone".into());
        assert_eq!(app.doc.named_selections.len(), 1);

        app.selection.clear();
        app.load_named_selection("zone");
        assert_eq!(app.selection, [10, 11].into_iter().collect());
    }

    #[test]
    fn named_selection_switches_to_its_owning_layer() {
        let mut app = app_with_layers(2);
        app.doc.active_layer = 1; // L2
        add_stroke_at(&mut app, 1, 20, (0.0, 0.0));
        app.selection = [20].into_iter().collect();
        app.save_named_selection("l2".into());

        app.doc.active_layer = 0; // L1
        app.selection.clear();
        app.load_named_selection("l2");
        assert_eq!(app.doc.active_layer, 1);
        assert_eq!(app.selection, [20].into_iter().collect());
    }

    #[test]
    fn named_selection_drops_ids_of_elements_removed_since() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 30, (0.0, 0.0));
        add_stroke_at(&mut app, 0, 31, (1.0, 1.0));
        app.selection = [30, 31].into_iter().collect();
        app.save_named_selection("both".into());

        app.doc.layers[0].strokes.retain(|s| s.id != 31);
        app.selection.clear();
        app.load_named_selection("both");
        assert_eq!(app.selection, [30].into_iter().collect());
    }

    #[test]
    fn select_in_ellipse_keeps_center_drops_corner() {
        let mut app = app_with_layers(1);
        // Boîte englobante (0,0)-(20,10) : le centre (10,5) est dans
        // l'ellipse inscrite, le coin (0,0) n'y est pas.
        add_stroke_at(&mut app, 0, 1, (10.0, 5.0));
        add_stroke_at(&mut app, 0, 2, (0.0, 0.0));
        app.select_mode = SelectMode::Ellipse;
        app.select_in_ellipse((0.0, 0.0), (20.0, 10.0), false);
        assert_eq!(app.selection, [1].into_iter().collect());
    }

    fn add_stroke_bbox(app: &mut PaintApp, layer: usize, id: u64, color: [u8; 4], a: (f32, f32), b: (f32, f32)) {
        let mut s = Stroke::new(color, 2.0, crate::model::stroke::Tool::Brush);
        s.id = id;
        s.points.push(crate::model::stroke::StrokePoint { pos: a, width: 2.0 });
        s.points.push(crate::model::stroke::StrokePoint { pos: b, width: 2.0 });
        app.doc.layers[layer].strokes.push(s);
    }

    #[test]
    fn magic_wand_global_selects_every_matching_color_on_the_layer() {
        let mut app = app_with_layers(1);
        let red = [255, 0, 0, 255];
        // Deux traits rouges disjoints + un trait bleu.
        add_stroke_bbox(&mut app, 0, 1, red, (0.0, 0.0), (2.0, 2.0));
        add_stroke_bbox(&mut app, 0, 2, red, (100.0, 100.0), (102.0, 102.0));
        add_stroke_bbox(&mut app, 0, 3, [0, 0, 255, 255], (50.0, 50.0), (52.0, 52.0));
        app.wand_global = true;
        app.magic_wand((1.0, 1.0), false);
        assert_eq!(app.selection, [1, 2].into_iter().collect());
    }

    #[test]
    fn magic_wand_contiguous_stops_at_the_disconnected_region() {
        let mut app = app_with_layers(1);
        let red = [255, 0, 0, 255];
        // Chaîne connexe : trait 1 touche trait 2 (boîtes qui se recoupent),
        // trait 3 est de la même couleur mais isolé, loin des deux autres.
        add_stroke_bbox(&mut app, 0, 1, red, (0.0, 0.0), (5.0, 5.0));
        add_stroke_bbox(&mut app, 0, 2, red, (4.0, 4.0), (9.0, 9.0));
        add_stroke_bbox(&mut app, 0, 3, red, (500.0, 500.0), (505.0, 505.0));
        app.wand_global = false;
        app.magic_wand((1.0, 1.0), false);
        assert_eq!(app.selection, [1, 2].into_iter().collect());
    }

    #[test]
    fn straighten_and_crop_rotates_90_degrees() {
        // Grille 2×2 : A rouge, B vert, C bleu, D jaune, lignes de gauche à
        // droite puis de haut en bas.
        let (a, b, c, d) = ([255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]);
        let rgba: Vec<u8> = [a, b, c, d].concat();
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 2, 2, rgba);
        let (nw, nh, out) = straighten_and_crop(&im, (0.0, 0.0, 2.0, 2.0), 1.0, 1.0, std::f32::consts::FRAC_PI_2);
        assert_eq!((nw, nh), (2, 2));
        let px = |i: u32, j: u32| out[((j * nw + i) * 4) as usize..][..4].to_vec();
        // Rotation à 90° : voir dérivation dans la description du Sprint 2.3.
        assert_eq!(px(0, 0), b);
        assert_eq!(px(1, 0), d);
        assert_eq!(px(0, 1), a);
        assert_eq!(px(1, 1), c);
    }

    /// `save_brush_preset` persiste via `i18n::save_brush_presets`, qui écrit
    /// dans le vrai `settings.json` du poste (dérivé de `$HOME`, comme
    /// `project::recovery_path`). On redirige `$HOME` vers un dossier
    /// temporaire le temps du test pour ne rien polluer sur la machine —
    /// même précaution que `project::tests::recovery_round_trip_via_temporary_home`.
    fn with_temp_home(f: impl FnOnce()) {
        // `$HOME` est un état global au process : verrou partagé avec
        // `project::tests` pour empêcher deux tests de le rediriger en même
        // temps (l'un pourrait restaurer `$HOME` pendant que l'autre écrit
        // encore, polluant le vrai dossier utilisateur — déjà arrivé une
        // fois avant l'ajout de ce verrou).
        let _guard = crate::project::home_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var("HOME").ok();
        let tmp = std::env::temp_dir().join("quickpaint-test-brush-preset-home");
        std::env::set_var("HOME", &tmp);
        f();
        if let Some(home) = real_home {
            std::env::set_var("HOME", home);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_and_apply_brush_preset_round_trips_the_settings() {
        with_temp_home(|| {
            let mut app = PaintApp::default();
            app.brush.width = 12.0;
            app.pixel_hardness = 0.4;
            app.stroke_stabilization = 0.7;
            app.capture_pressure_strength = 0.2;
            app.save_brush_preset("perso".into());
            assert_eq!(app.brush_presets.len(), 1);

            app.brush.width = 1.0;
            app.pixel_hardness = 0.0;
            app.stroke_stabilization = 0.0;
            app.capture_pressure_strength = 0.0;
            let preset = app.brush_presets[0].clone();
            app.apply_brush_preset(&preset);
            assert_eq!(app.brush.width, 12.0);
            assert_eq!(app.pixel_hardness, 0.4);
            assert_eq!(app.stroke_stabilization, 0.7);
            assert_eq!(app.capture_pressure_strength, 0.2);
        });
    }

    #[test]
    fn saving_a_brush_preset_with_the_same_name_overwrites_it() {
        with_temp_home(|| {
            let mut app = PaintApp::default();
            app.brush.width = 5.0;
            app.save_brush_preset("x".into());
            app.brush.width = 9.0;
            app.save_brush_preset("x".into());
            assert_eq!(app.brush_presets.len(), 1);
            assert_eq!(app.brush_presets[0].width, 9.0);
        });
    }

    #[test]
    fn load_brush_presets_from_path_accepts_a_single_object_or_an_array() {
        let dir = std::env::temp_dir();
        let one = crate::model::BrushPreset { name: "solo".into(), width: 3.0, hardness: 0.5, stabilization: 0.1, pressure_strength: 0.9 };
        let single_path = dir.join("quickpaint-test-brush-single.json");
        std::fs::write(&single_path, serde_json::to_string(&one).unwrap()).unwrap();
        let loaded = load_brush_presets_from_path(&single_path).expect("single object should load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "solo");
        let _ = std::fs::remove_file(&single_path);

        let many = vec![one.clone(), crate::model::BrushPreset { name: "duo".into(), ..one }];
        let array_path = dir.join("quickpaint-test-brush-array.json");
        std::fs::write(&array_path, serde_json::to_string(&many).unwrap()).unwrap();
        let loaded = load_brush_presets_from_path(&array_path).expect("array should load");
        assert_eq!(loaded.len(), 2);
        let _ = std::fs::remove_file(&array_path);
    }

    #[test]
    fn load_brush_presets_from_path_rejects_garbage() {
        let path = std::env::temp_dir().join("quickpaint-test-brush-garbage.json");
        std::fs::write(&path, b"not json").unwrap();
        assert!(load_brush_presets_from_path(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn delete_named_selection_removes_it() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 40, (0.0, 0.0));
        app.selection = [40].into_iter().collect();
        app.save_named_selection("temp".into());
        app.delete_named_selection("temp");
        assert!(app.doc.named_selections.is_empty());
    }
}
