//! État global de l'application + boucle de frame (sections 1, 6).
//!
//! `PaintApp` relie : modèle (`Document`), capture du geste, historique,
//! outils et rendu. La boucle `update` suit la séquence de la section 6 :
//! lire les évènements → mettre à jour le trait → UI → rendre.

// Animation (Sprint L.6) : frames = instantanés complets de la pile de
// calques, gérées via l'undo général (`push_doc_snapshot`, `Command::SetDoc`)
// plutôt qu'un système de commandes dédié — même sous-module autonome que
// `pen_edit`/`transform`, ne partage que `Document`/`AnimationFrame`.
mod animation;
// Rendu des overlays du canevas : grille, règles graduées, aperçu du chemin
// de plume, cadre + poignées de sélection, recadrage, retouche par
// rectangle, marquee/lasso, segment de mesure, anneau de curseur — extrait
// en sous-module (sprint.md T3.8, suite de T3.1-T3.7) : fonctions de
// peinture pures (`&self`), aucune mutation.
mod canvas_overlay;
// Pot de peinture et détourage en un clic : inondent la composition
// affichée (capture d'écran différée) depuis le point cliqué — extrait en
// sous-module (sprint.md T3.9, suite de T3.1-T3.8).
mod bucket_cutout;
// Dispatch des évènements du canevas (pan/zoom, verrouillage de calque, puis
// un `match` par outil actif) vers les gestes déjà implémentés dans les
// sous-modules dédiés — extrait en sous-module (sprint.md T3.10, suite de
// T3.1-T3.9) : un seul point d'entrée pour l'interaction souris/trackpad.
mod canvas_input;
// Édition de nœuds Bézier après coup (roadmap P2 #12) — extrait en sous-module
// (ANALYSE.md §12.5) : sous-système autonome (état + geste + rendu) qui ne
// partage que `Document`/`Stroke` avec le reste de `app`.
// Import de fichiers (image/PSD/SVG/tampon de brosse), presse-papiers, et
// persistance de projet (sauvegarde/ouverture/récents) — extrait en
// sous-module (sprint.md T3.3, suite de T3.1/T3.2) : ne partage que
// Document/History avec le reste de `app`.
mod io;
mod pen_edit;
// Opérations sur les calques : alignement/répartition, aplatissement, pile
// (ajout/suppression/groupes/réordonnancement) — extrait en sous-module
// (sprint.md T3.2, suite de T3.1) : ne partage que Document/Command/AlignMode.
mod layers_ops;
// Sélection d'éléments (marquee/lasso/baguette) et masque de sélection en
// pixels (Sprint H) — extrait en sous-module (sprint.md T3.1) : plus gros
// contributeur à la taille de `app/mod.rs`, mais autonome (ne partage que
// `Document`/`ElemGeom`/`SelectionCombine` avec le reste de `app`).
mod selection;
// Pipeline d'export : rendu composite → encodage → écriture disque, aperçu/
// poids estimé, export par lots, profils nommés, export SVG/PDF vectoriels —
// extrait en sous-module (sprint.md T3.7, suite de T3.1-T3.6) : un seul
// rendu natif par export (`render_for_export`), tout le reste en dérive.
mod export_ops;
// Pinceau/gomme pixel, aérographe, tampon de clonage/correcteur, retouche
// locale (densité +/-, éponge, flou, netteté) et estompe — extrait en
// sous-module (sprint.md T3.6, suite de T3.1-T3.5) : partagent l'undo par
// tuile (`touch_raster_tiles`/`commit_raster_stroke`), autonomes du reste
// de `app` hormis `Document`/`RasterOp`/`RasterTarget`.
mod raster_paint;
// Dispatch clavier (raccourcis globaux + capture de raccourci personnalisé),
// menu Édition natif macOS, glisser-déposer de fichiers — extrait en
// sous-module (sprint.md T3.4, suite de T3.1/T3.2/T3.3) : le gros `match`
// d'évènements qui route vers des actions déjà définies ailleurs.
mod shortcuts;
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
use crate::tools::{eyedropper, hit, shape, ActiveTool, Brush, Eraser, SelectMode, SelectionCombine, SymmetryMode};
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

/// Un calque verrouillé (audit_sprint_xx.md B.1) bloque-t-il ce geste pour
/// `tool` ? Seuls les outils jamais destructifs restent autorisés (Main,
/// Pipette, Règle) — tout le reste modifierait potentiellement le contenu du
/// calque actif (peinture, formes, texte, déplacement/transformation…), donc
/// reste bloqué tant que le calque est verrouillé.
fn layer_lock_blocks_tool(tool: ActiveTool) -> bool {
    !matches!(tool, ActiveTool::Pan | ActiveTool::Eyedropper | ActiveTool::Measure)
}

/// Recadre un buffer RGBA8 `w×h` au rectangle doc-space `(mn, mx)`, borné aux
/// dimensions du buffer (Sprint L.1 — export d'une zone sélectionnée
/// uniquement). Renvoie le buffer d'origine inchangé si le rectangle borné
/// est vide (sélection hors cadre, ou dégénérée).
fn crop_rgba_to_bounds(w: u32, h: u32, rgba: &[u8], mn: (f32, f32), mx: (f32, f32)) -> (u32, u32, Vec<u8>) {
    let x0 = mn.0.floor().clamp(0.0, w as f32) as u32;
    let y0 = mn.1.floor().clamp(0.0, h as f32) as u32;
    let x1 = mx.0.ceil().clamp(0.0, w as f32) as u32;
    let y1 = mx.1.ceil().clamp(0.0, h as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return (w, h, rgba.to_vec());
    }
    let (rw, rh) = (x1 - x0, y1 - y0);
    let mut out = vec![0u8; (rw * rh * 4) as usize];
    for y in 0..rh {
        let src = (((y0 + y) * w + x0) * 4) as usize;
        let dst = (y * rw * 4) as usize;
        out[dst..dst + (rw * 4) as usize].copy_from_slice(&rgba[src..src + (rw * 4) as usize]);
    }
    (rw, rh, out)
}

/// (id d'élément, boîte englobante (min, max)) en coordonnées document.
type ElemBounds = (u64, ((f32, f32), (f32, f32)));

/// (id, boîte englobante (min, max), centre) — géométrie de sélection (Sprint 1).
type ElemGeom = (u64, ((f32, f32), (f32, f32)), (f32, f32));
/// (index de calque, boîte englobante) — utilisé par `distribute_layers`.
type LayerBounds = (usize, ((f32, f32), (f32, f32)));

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
    italic: bool,
    underline: bool,
    line_height: f32,
    letter_spacing: f32,
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

/// Aperçu et poids estimé avant export (Sprint L.2), avec case à cocher pour
/// n'exporter que la sélection (Sprint L.1) — un seul dialogue couvre les
/// deux : la sélection change la région rendue, donc l'aperçu/poids doit de
/// toute façon se recalculer quand elle change.
pub struct ExportPreviewDialog {
    pub format: crate::export::ExportFormat,
    /// Coché seulement si une sélection non vide existait à l'ouverture ;
    /// grisé sinon (rien à limiter).
    pub selection_only: bool,
    pub w: u32,
    pub h: u32,
    /// Octets déjà encodés (Sprint L.2) — réutilisés tels quels à l'export
    /// final, pas de second encodage.
    pub bytes: Vec<u8>,
    pub texture: egui::TextureHandle,
}

/// Gabarits « riches » avec contenu pré-rempli (Sprint 10.2), au-delà de la
/// simple taille de document de la galerie de modèles existante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateContent {
    InstagramPromo,
    FacebookBanner,
}

/// Action sur le masque de sélection en pixels en attente de validation
/// (Sprint H) — un seul dialogue partagé, le paramètre (rayon en pixels)
/// change de sens selon l'action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMaskAction {
    Feather,
    Dilate,
    Contract,
    /// Amélioration des bords (previous_audit.md #38) : généralise
    /// `bucket::refine_edges`, jusque-là câblé seulement au détourage en un
    /// clic, à n'importe quelle sélection par région.
    RefineEdges,
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
    /// Trait en pointillés pour les outils Forme (previous_audit.md
    /// #55) — motif fixe relatif à l'épaisseur du trait plutôt que deux
    /// champs numériques de plus dans une barre déjà dense.
    pub dashed_stroke: bool,
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
    /// Kits de marque nommés (previous_audit.md #92), persistés
    /// localement — extension du même mécanisme que `style_presets`/
    /// `brush_presets`.
    pub brand_kits: Vec<crate::model::BrandKit>,
    /// Panneau des kits de marque ouvert ?
    pub show_brand_kits: bool,
    /// Nom en cours de saisie pour enregistrer le kit de marque actuel.
    pub brand_kit_name: String,
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
    /// Coin en cours de glissé sur le canevas (previous_audit.md #87) :
    /// remplace les sliders X/Y du panneau perspective par une manipulation
    /// directe des 4 coins, comme les poignées d'échelle/rotation
    /// (`app/transform.rs`) — index dans `perspective_offsets`, `None` hors
    /// glissé.
    perspective_drag: Option<usize>,
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
    /// Fenêtre de documentation des outils (À propos ▸ Documentation) ouverte ?
    pub show_help: bool,
    /// Action en attente d'une nouvelle touche (capture au prochain appui).
    pub capturing_shortcut: Option<crate::keybindings::ShortcutAction>,
    /// Capture en cours d'une commande ⌘ rebindable (Sprint R, point 97).
    pub capturing_cmd_shortcut: Option<crate::keybindings::CommandAction>,
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
    /// Vignettes de calque (Sprint I.3), par id → (hash de contenu, texture) —
    /// recalculée seulement quand le hash change, pas à chaque frame.
    layer_thumbnails: std::collections::HashMap<u64, (u64, egui::TextureHandle)>,
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
    /// Masque de sélection en pixels (Sprint H), en plus des ID d'éléments
    /// ci-dessus — nécessaire pour contour progressif (feather) et dilater/
    /// contracter, des opérations où un pixel peut être « à moitié
    /// sélectionné ». Peuplé directement depuis la géométrie du geste de
    /// sélection (rectangle/ellipse/lasso), voir `tools::selection_mask`.
    /// `None` tant qu'aucune sélection par région n'a été faite.
    pub selection_mask: Option<crate::model::RasterLayer>,
    /// Dialogue de réglage feather/dilater/contracter en attente (Sprint H),
    /// `None` si fermé — (action choisie, rayon en pixels).
    pub selection_mask_dialog: Option<(SelectionMaskAction, f32)>,
    /// Texture d'aperçu du masque de sélection (Sprint H), mise en cache par
    /// hash de contenu (`RasterLayer::content_hash`) — recalculée seulement
    /// quand le masque change, pas à chaque frame.
    selection_mask_texture: Option<(u64, egui::TextureHandle)>,
    /// Contours du masque de sélection (Sprint O, point 60 : « fourmis en
    /// marche ») en coordonnées document — même cache par hash de contenu
    /// que la texture de teinte, recalculé au changement du masque seulement.
    selection_ants: Option<(u64, Vec<Vec<(f32, f32)>>)>,
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
    /// Redressement par ligne tracée (previous_audit.md #88), en plus du
    /// curseur d'angle : tant que vrai, le prochain glissé sur le canevas
    /// (en mode recadrage) trace une ligne de référence au lieu de redéfinir
    /// le rectangle — sa pente devient `crop_angle` au relâchement, geste
    /// unique qui se désactive de lui-même ensuite.
    pub straighten_line_mode: bool,
    /// Ligne en cours de tracé (coords document), `None` hors glissé.
    straighten_drag: Option<((f32, f32), (f32, f32))>,
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
    /// Italique (Sprint Q, point 82) — effectif pour les polices système
    /// disposant d'une vraie fonte italique (voir `TextItem::italic`).
    pub text_italic: bool,
    /// Soulignement (previous_audit.md #61).
    pub text_underline: bool,
    /// Interligne (Sprint Q, point 83), multiple de la taille.
    pub text_line_height: f32,
    /// Espacement entre caractères (Sprint Q, point 83), unités document.
    pub text_letter_spacing: f32,
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
    /// Dialogue d'aperçu/poids estimé avant export (Sprint L.1/L.2), `None`
    /// si fermé.
    pub export_dialog: Option<ExportPreviewDialog>,
    /// Profils d'export nommés (Sprint L.8) : format + qualité + tailles du
    /// batch export, persistés localement.
    pub export_profiles: Vec<crate::export::ExportProfile>,
    /// Saisie du nom pour « Enregistrer un profil d'export » (Sprint L.8).
    pub export_profile_name: String,
    /// Panneau « Animation » (Sprint L.6, frames) ouvert ?
    pub show_animation_panel: bool,
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
    /// Affiner les bords (audit_sprint_xx.md C.1) : après le dégradé de
    /// `soft_edge`, repousse la couverture vers 0/255 dans les zones à forte
    /// variance de luminance locale (mèches de cheveux, fourrure, grillage)
    /// pour éviter qu'un flou générique ne noie ces détails fins.
    pub cutout_refine_edges: bool,
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
    /// Filtre texte de la liste des calques (Sprint I.4) : n'affiche que les
    /// calques dont le nom correspond (insensible à la casse). Vide = pas de
    /// filtre. Le champ n'est révélé dans l'UI qu'au-delà d'un seuil de
    /// calques, pour ne pas alourdir les petits documents.
    pub layer_search: String,
    /// Ancre de sélection étendue (⇧) dans la liste « Éléments du calque »
    /// (index de ligne du dernier clic simple) — permet de sélectionner une
    /// plage sans redéfinir un concept de sélection propre à cette liste :
    /// réutilise `self.selection`, déjà partagé avec le canevas (aligner,
    /// rogner, ordre… dans `toolbar::selection_actions`).
    pub layer_elements_anchor: Option<usize>,
    /// Sélection multi-calque dans le panneau des calques (point 36 de
    /// l'audit : distribution entre plusieurs calques) — indépendante de
    /// `doc.active_layer` (qui reste un index unique, seul calque réellement
    /// « actif » pour la peinture/édition de contenu). ⇧/⌘+clic sur un nom de
    /// calque la peuple ; sert uniquement à `distribute_layers`.
    pub layer_multi_select: std::collections::HashSet<u64>,
    /// Ancre de sélection étendue (⇧) pour `layer_multi_select`, même principe
    /// que [`Self::layer_elements_anchor`].
    pub layer_select_anchor: Option<usize>,
    /// Saisie hexadécimale de la couleur courante (roadmap P0 #6).
    pub hex_field: String,
    // Pinceau / gomme pixel (roadmap F1) : dureté du tampon (0 = dégradé
    // complet, 1 = bord net) + état du geste en cours.
    pub pixel_hardness: f32,
    raster_stroke_last: Option<(f32, f32)>,
    /// Aérographe (Sprint J.1) : horodatage (`ctx.input(|i| i.time)`) du
    /// dernier dépôt — pilote l'accumulateur de temps écoulé qui dépose à
    /// intervalles réguliers tant que le clic est maintenu, même immobile.
    airbrush_last_dab: Option<f64>,
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
    /// Mode de symétrie (Sprint O, point 54 de l'audit) : radial (copies
    /// rotées) ou réflexion miroir autour d'un axe central.
    pub symmetry_mode: SymmetryMode,
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
    /// Identifiants du menu ⌘ natif (UIX_ANALYSE.md U1) installés par
    /// `PaintApp::new` — absent des instances de test (`Default`), qui ne
    /// tournent pas dans un vrai processus AppKit.
    native_edit_menu: Option<crate::native_menu::EditMenuIds>,
    /// Thème d'interface (Sprint R, point 96), persisté dans `settings.json`.
    pub ui_theme: UiTheme,
    /// Affiche le nom de chaque outil sous son icône dans la barre
    /// (previous_audit.md critique n°1) : un tooltip au survol ne se
    /// déclenche jamais au doigt sur écran tactile, ce qui rend les 32
    /// outils indiscoverables sans essai-erreur sur ce canal — ce réglage,
    /// désactivé par défaut pour ne pas alourdir l'usage souris/clavier
    /// habituel, restaure une identification visuelle directe.
    pub show_tool_labels: bool,
    /// Mode plein écran / sans distraction (previous_audit.md #17) :
    /// masque barre d'outils/panneau de calques/pied de page, ne garde que
    /// le canevas — gagne de l'espace sur un usage tactile où chaque
    /// centimètre compte. Pas persisté (redémarrer l'app en sort toujours).
    pub distraction_free: bool,
    /// Guide manuel en cours de glissé (Sprint R, point 95) : création
    /// depuis une règle ou déplacement d'un guide existant.
    guide_drag: Option<GuideDrag>,
    /// Rotation de la vue en radians (Sprint T, point 93) : affichage
    /// seulement, le document n'est jamais modifié. Les règles et les
    /// gestes qui en dépendent (guides manuels) sont désactivés hors 0°.
    pub view_angle: f32,
    /// Pelure d'oignon (Sprint U) : affiche les frames voisines en
    /// semi-transparence teintée sous la frame active (animation).
    pub onion_skin: bool,
    /// Cache des rendus de frames voisines pour la pelure d'oignon :
    /// index de frame → (révision d'historique au rendu, texture).
    onion_textures: std::collections::HashMap<usize, (u64, egui::TextureHandle)>,
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
            dashed_stroke: false,
            poly_sides: 6,
            recent_colors: Vec::new(),
            custom_palette: crate::i18n::load_custom_palette(),
            keybindings: crate::keybindings::KeyBindings::load(),
            style_presets: crate::i18n::load_style_presets(),
            brush_presets: crate::i18n::load_brush_presets(),
            brand_kits: crate::i18n::load_brand_kits(),
            show_brand_kits: false,
            brand_kit_name: String::new(),
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
            perspective_drag: None,
            collapsed_toolbar_groups: crate::i18n::load_collapsed_toolbar_groups().into_iter().collect(),
            layers_panel_width: crate::i18n::load_layers_panel_width(),
            show_style_presets: false,
            style_preset_name: String::new(),
            show_shortcuts_prefs: false,
            show_help: false,
            capturing_shortcut: None,
            capturing_cmd_shortcut: None,
            status: None,
            status_error: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            cache: StrokeCache::new(),
            active_stroke: ActiveStroke::default(),
            compositor: crate::render::compositor::Compositor::new(),
            image_textures: std::collections::HashMap::new(),
            layer_thumbnails: std::collections::HashMap::new(),
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
            selection_mask: None,
            selection_mask_dialog: None,
            selection_mask_texture: None,
            selection_ants: None,
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
            straighten_line_mode: false,
            straighten_drag: None,
            crop_ratio: None,
            text_size: 28.0,
            text_font: crate::model::text::TextFont::Proportional,
            text_font_family: None,
            font_search: String::new(),
            font_manager: crate::fonts::FontManager::new(),
            text_bold: false,
            text_italic: false,
            text_underline: false,
            text_line_height: 1.25,
            text_letter_spacing: 0.0,
            text_align: crate::model::text::TextAlign::Left,
            text_outline_w: 0.0,
            text_outline_color: [255, 255, 255, 255],
            text_shadow: None,
            text_arc: None,
            editing_text: None,
            text_focus_pending: false,
            show_batch_export: false,
            batch_export: BatchExportState::default(),
            export_dialog: None,
            export_profiles: crate::i18n::load_export_profiles(),
            export_profile_name: String::new(),
            show_animation_panel: false,
            bucket_click: None,
            cutout_click: None,
            cutout_tolerance: 32,
            cutout_global: false,
            cutout_refine_edges: false,
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
            layer_search: String::new(),
            layer_elements_anchor: None,
            layer_multi_select: std::collections::HashSet::new(),
            layer_select_anchor: None,
            hex_field: String::new(),
            pixel_hardness: 0.8,
            raster_stroke_last: None,
            airbrush_last_dab: None,
            raster_touch: std::collections::HashMap::new(),
            clone_source: None,
            clone_offset: None,
            effect_strength: 0.5,
            symmetry_axes: 4,
            symmetry_mode: SymmetryMode::default(),
            gradient_kind: crate::model::GradientKind::Linear,
            gradient_drag_start: None,
            measure: None,
            autosave_last_rev: 0,
            autosave_last_at: std::time::Instant::now(),
            show_recovery_prompt: false,
            native_edit_menu: None,
            ui_theme: UiTheme::load(),
            show_tool_labels: crate::i18n::load_show_tool_labels(),
            distraction_free: false,
            guide_drag: None,
            view_angle: 0.0,
            onion_skin: false,
            onion_textures: std::collections::HashMap::new(),
        }
    }
}

/// Glissé de guide manuel en cours (Sprint R, point 95).
struct GuideDrag {
    vertical: bool,
    pos: f32,
    /// `Some(i)` = déplacement du guide existant `doc.guides[i]` ;
    /// `None` = création depuis une règle.
    existing: Option<usize>,
}

/// Thème d'interface (Sprint R, point 96) : suit macOS par défaut, avec
/// bascule manuelle persistée dans `settings.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiTheme {
    #[default]
    System,
    Light,
    Dark,
}

impl UiTheme {
    pub const ALL: [UiTheme; 3] = [UiTheme::System, UiTheme::Light, UiTheme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            UiTheme::System => t("Système", "System"),
            UiTheme::Light => t("Clair", "Light"),
            UiTheme::Dark => t("Sombre", "Dark"),
        }
    }

    fn code(self) -> &'static str {
        match self {
            UiTheme::System => "system",
            UiTheme::Light => "light",
            UiTheme::Dark => "dark",
        }
    }

    fn load() -> Self {
        match crate::i18n::load_theme().as_deref() {
            Some("light") => UiTheme::Light,
            Some("dark") => UiTheme::Dark,
            _ => UiTheme::System,
        }
    }

    pub fn save(self) {
        crate::i18n::save_theme(self.code());
    }
}

impl PaintApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Thème (Sprint R, point 96) : plus de `Visuals::light()` forcé —
        // la préférence persistée (ou le thème système) est appliquée à
        // chaque frame par `apply_theme`.
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
        // Détecté une seule fois, avant toute écriture de la session
        // courante : la présence du fichier signifie que la session
        // précédente ne s'est pas terminée proprement (crash, kill -9).
        Self {
            show_recovery_prompt: crate::project::has_recovery(),
            native_edit_menu: Some(crate::native_menu::install()),
            ..Default::default()
        }
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
        // Guides manuels (Sprint R, point 95) : candidats d'accroche mono-axe.
        let gx: Vec<f32> = self.doc.guides.iter().filter(|g| g.vertical).map(|g| g.pos).collect();
        let gy: Vec<f32> = self.doc.guides.iter().filter(|g| !g.vertical).map(|g| g.pos).collect();
        let (snapped, guides) = crate::tools::guides::snap((mn, mx), &targets, &gx, &gy, threshold, raw);
        self.move_delta = snapped;
        self.active_guides = guides;
    }

    /// Enregistre un déplacement (dx, dy) de la sélection comme commande.
    fn push_move(&mut self, dx: f32, dy: f32) {
        if self.selection.is_empty() {
            return;
        }
        if self.doc.layers[self.doc.active_layer].lock_position {
            self.info(t(
                "Position verrouillée : déverrouille le calque pour le déplacer.",
                "Position locked: unlock the layer to move it.",
            ));
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
            t.italic = self.text_italic;
            t.underline = self.text_underline;
            t.line_height = self.text_line_height;
            t.letter_spacing = self.text_letter_spacing;
            t.align = self.text_align;
            t.outline_w = self.text_outline_w;
            t.outline_color = self.text_outline_color;
            t.shadow = self.text_shadow;
            t.arc = self.text_arc;
            t.color = color;
            self.history.touch();
        }
    }

    /// Démarre le tracé d'une ligne de référence pour le redressement
    /// d'horizon (previous_audit.md #88), en plus du curseur d'angle —
    /// geste à la Photoshop : tracer une ligne le long de l'horizon plutôt
    /// que deviner l'angle en degrés. Un seul glissé, se désactive de
    /// lui-même après (`update_straighten_line`).
    pub fn start_straighten_line(&mut self) {
        self.straighten_line_mode = true;
        self.info(t("Trace une ligne le long de l'horizon à redresser.", "Draw a line along the horizon to straighten."));
    }

    pub(super) fn update_straighten_line(&mut self, p: (f32, f32)) {
        match &mut self.straighten_drag {
            Some((_, end)) => *end = p,
            None => self.straighten_drag = Some((p, p)),
        }
    }

    /// Termine le tracé : la pente de la ligne devient `crop_angle`. Une
    /// ligne trop courte (clic sans glissé réel) est ignorée plutôt que de
    /// produire un angle bruité par imprécision du pointeur.
    pub(super) fn commit_straighten_line(&mut self) {
        self.straighten_line_mode = false;
        let Some((a, b)) = self.straighten_drag.take() else { return };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        if dx.hypot(dy) < 4.0 {
            return;
        }
        // Même sens que `straighten_and_crop` : le rééchantillonnage tourne
        // le contenu dans le sens *inverse* de `crop_angle` (voir sa doc) —
        // une ligne tracée avec une pente `angle` ressort donc horizontale.
        self.crop_angle = dy.atan2(dx);
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
            item.italic = self.text_italic;
            item.underline = self.text_underline;
            item.line_height = self.text_line_height;
            item.letter_spacing = self.text_letter_spacing;
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

    /// Texte → tracés vectoriels (previous_audit.md #64) : remplace le
    /// texte sélectionné par un `Stroke` non rempli par contour de glyphe
    /// (extraction via `tools::text_outline`, sur les octets réels de la
    /// police système). Snapshot avant/après du document entier
    /// (`Command::SetDoc`) plutôt qu'une commande dédiée : remplacement
    /// structurel (un texte devient N traits), pas une simple édition de
    /// champ — même choix que `resize_document`/`merge_selection_to_image`
    /// pour ce type de changement irrégulier.
    ///
    /// Limites assumées, documentées dans `tools::text_outline` : police
    /// intégrée Sans/Mono non convertible (pas dans `fontdb`, message
    /// d'erreur explicite) ; pas de crénage de paires (avance simple par
    /// glyphe) ; un contour rempli individuellement ne recrée pas
    /// correctement le trou d'une lettre comme « O » si l'utilisateur active
    /// « Rempli » après coup — seul le résultat non rempli (par défaut) est
    /// garanti visuellement correct.
    pub fn convert_text_to_outlines(&mut self) {
        let active = self.doc.active_layer;
        let Some(id) = self.single_text_idx() else {
            self.info(t("Sélectionne un texte (outil Sélection).", "Select a text (Select tool)."));
            return;
        };
        let text = self.doc.layers[active].texts[id].clone();
        let Some(family) = &text.font_family else {
            self.info(t(
                "Police système requise (pas Sans/Mono intégrées) pour convertir en tracés.",
                "A system font is required (not built-in Sans/Mono) to convert to paths.",
            ));
            return;
        };
        let Some(bytes) = self.font_manager.font_bytes(family, text.bold, text.italic) else {
            self.info(t("Police introuvable.", "Font not found."));
            return;
        };
        let Some(contours) = crate::tools::text_outline::glyph_contours(&bytes, &text.text, text.size) else {
            self.info(t("Police invalide.", "Invalid font."));
            return;
        };
        if contours.is_empty() {
            self.info(t("Rien à convertir (texte vide).", "Nothing to convert (empty text)."));
            return;
        }

        let before = Box::new(self.doc.clone());
        let mut after = self.doc.clone();
        let layer = &mut after.layers[active];
        layer.texts.retain(|t2| t2.id != text.id);
        for contour in contours {
            let mut s = crate::model::Stroke::new(text.color, 1.0, crate::model::Tool::Brush);
            s.id = self.next_id;
            self.next_id += 1;
            s.smooth = false;
            s.z = text.z;
            s.points = contour.into_iter().map(|(x, y)| crate::model::StrokePoint { pos: (text.pos.0 + x, text.pos.1 + y), width: 1.0 }).collect();
            layer.strokes.push(s);
        }
        self.selection.clear();
        self.history.push(&mut self.doc, Command::SetDoc { before, after: Box::new(after), label: "Texte → tracés" });
        self.info(t("Texte converti en tracés.", "Text converted to paths."));
    }

    /// Index du texte dans le calque actif si c'est le seul élément
    /// sélectionné (même schéma que `single_image_idx`).
    fn single_text_idx(&self) -> Option<usize> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.iter().next()?;
        self.doc.layers[self.doc.active_layer].texts.iter().position(|t| t.id == id)
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

    /// Position écran des 4 poignées de coin (previous_audit.md #87) :
    /// coin d'origine de l'image sélectionnée + décalage courant de
    /// `perspective_offsets`, dans cet ordre (haut-gauche, haut-droit,
    /// bas-droit, bas-gauche — même ordre que `perspective_offsets` et
    /// `selected_image_corners`). `None` si le panneau perspective n'est pas
    /// ouvert ou si la sélection n'est pas une image unique.
    pub(super) fn perspective_handles(&self, view: &crate::render::canvas::ViewTransform) -> Option<[egui::Pos2; 4]> {
        if !self.show_perspective_panel {
            return None;
        }
        let (_, corners) = self.selected_image_corners()?;
        let (w, h) = (corners[2].0 - corners[0].0, corners[2].1 - corners[0].1);
        Some(std::array::from_fn(|i| {
            let (ox, oy) = self.perspective_offsets[i];
            view.doc_to_screen((corners[i].0 + ox * w, corners[i].1 + oy * h))
        }))
    }

    /// Démarre le glissé d'un coin si `p` (écran) tombe sur l'une des 4
    /// poignées de `perspective_handles`.
    pub(super) fn start_perspective_drag_if_handle(&mut self, p: egui::Pos2, view: &crate::render::canvas::ViewTransform) -> bool {
        let Some(handles) = self.perspective_handles(view) else { return false };
        for (i, h) in handles.iter().enumerate() {
            if (*h - p).length() <= 10.0 {
                self.perspective_drag = Some(i);
                return true;
            }
        }
        false
    }

    /// Met à jour le décalage du coin en cours de glissé, en fraction de la
    /// largeur/hauteur de l'image (même unité que les anciens sliders X/Y,
    /// bornée un peu plus large — -1.0..=1.0 — puisqu'un glissé direct sur
    /// le canevas se prête mieux à une grande distorsion qu'un curseur).
    pub(super) fn update_perspective_drag(&mut self, p: egui::Pos2, view: &crate::render::canvas::ViewTransform) {
        let Some(i) = self.perspective_drag else { return };
        let Some((_, corners)) = self.selected_image_corners() else { return };
        let (w, h) = (corners[2].0 - corners[0].0, corners[2].1 - corners[0].1);
        if w.abs() < 1e-3 || h.abs() < 1e-3 {
            return;
        }
        let d = view.screen_to_doc(p);
        let ox = (d.0 - corners[i].0) / w;
        let oy = (d.1 - corners[i].1) / h;
        self.perspective_offsets[i] = (ox.clamp(-1.0, 1.0), oy.clamp(-1.0, 1.0));
    }

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
    /// Sprint P (point 30) : le contenu **peint** (raster) est composé
    /// par-dessus celui du calque du dessous — il disparaissait silencieusement
    /// avant. L'opacité et le masque du calque source sont « cuits » dans les
    /// pixels fusionnés (ces attributs disparaissent avec le calque) ; les
    /// éléments vectoriels restent transférés tels quels, éditables — leur
    /// éventuel masquage par le masque du calque source n'est pas cuit
    /// (limite documentée : il faudrait les rasteriser, on perdrait l'édition).
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
        if !upper.raster.is_empty() {
            lower.raster.composite_over(&upper.raster, upper.opacity, upper.mask.as_ref());
        }
        self.selection.clear();
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: i, after, after_active: i - 1 },
        );
        self.cache.clear();
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

    /// Fusionne les éléments sélectionnés (traits/formes/images/textes,
    /// mélangés) en une seule image bitmap, à leur place. Rendu isolé dans un
    /// document temporaire d'un seul calque ne contenant que la sélection —
    /// réutilise le compositeur existant (dégradés, styles…) plutôt que
    /// dupliquer sa logique, puis recadré à la boîte englobante de la
    /// sélection. Besoin d'au moins 2 éléments : à 1 seul, ce serait juste une
    /// conversion sans intérêt (et une image seule n'a rien à « fusionner »).
    pub fn merge_selection_to_image(&mut self, ctx: &egui::Context) {
        if self.selection.len() < 2 {
            return;
        }
        let bounds = self.selected_elements_bounds();
        if bounds.is_empty() {
            return;
        }
        let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for (_, (bmin, bmax)) in &bounds {
            min.0 = min.0.min(bmin.0);
            min.1 = min.1.min(bmin.1);
            max.0 = max.0.max(bmax.0);
            max.1 = max.1.max(bmax.1);
        }
        min = (min.0.max(0.0), min.1.max(0.0));
        max = (max.0.min(self.doc.size.0 as f32), max.1.min(self.doc.size.1 as f32));
        let (w, h) = ((max.0 - min.0).ceil().max(1.0) as u32, (max.1 - min.1).ceil().max(1.0) as u32);
        let (x0, y0) = (min.0.floor() as u32, min.1.floor() as u32);

        let idx = self.doc.active_layer;
        let sel = self.selection.clone();
        let src = &self.doc.layers[idx];
        let mut temp_layer = crate::model::Layer::new(1, String::new());
        temp_layer.strokes = src.strokes.iter().filter(|s| sel.contains(&s.id)).cloned().collect();
        temp_layer.texts = src.texts.iter().filter(|t| sel.contains(&t.id)).cloned().collect();
        temp_layer.images = src.images.iter().filter(|im| sel.contains(&im.id)).cloned().collect();
        let mut temp_doc = crate::model::Document::new(self.doc.size);
        temp_doc.layers = vec![temp_layer];

        let mut temp_compositor = crate::render::compositor::Compositor::new();
        let Some((full_w, full_h, rgba)) = temp_compositor.render_to_rgba(ctx, &temp_doc, Color32::TRANSPARENT) else {
            return;
        };
        let mut cropped = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let sy = y0 + y;
            if sy >= full_h {
                break;
            }
            for x in 0..w {
                let sx = x0 + x;
                if sx >= full_w {
                    break;
                }
                let si = ((sy * full_w + sx) * 4) as usize;
                let di = ((y * w + x) * 4) as usize;
                cropped[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
            }
        }

        let image = crate::model::ImageItem::from_rgba(self.next_id, (x0 as f32, y0 as f32), w, h, cropped);
        self.next_id += 1;

        let before = self.doc.layers.clone();
        let mut after = before.clone();
        after[idx].strokes.retain(|s| !sel.contains(&s.id));
        after[idx].texts.retain(|t| !sel.contains(&t.id));
        after[idx].images.retain(|im| !sel.contains(&im.id));
        let new_id = image.id;
        after[idx].images.push(image);
        self.selection.clear();
        self.selection.insert(new_id);
        self.history.push(&mut self.doc, Command::SetLayers { before, before_active: idx, after, after_active: idx });
        self.info(t("Éléments fusionnés en une image.", "Elements merged into an image."));
    }

    /// Réunit les éléments sélectionnés dans un nouveau calque dédié (Cmd+G
    /// façon Photoshop/GIMP, mais un vrai calque plutôt qu'un nouveau concept
    /// de groupe d'éléments — déplaçable/verrouillable/masquable comme
    /// n'importe quel calque, réutilise entièrement le système existant).
    /// Inséré juste au-dessus du calque source et activé.
    pub fn group_selection_into_layer(&mut self) {
        if self.selection.len() < 2 {
            return;
        }
        let idx = self.doc.active_layer;
        let sel = self.selection.clone();
        let before = self.doc.layers.clone();
        let mut after = before.clone();
        let mut group = crate::model::Layer::new(self.doc.next_layer_id, t("Groupe", "Group"));
        after[idx].strokes.retain(|s| {
            let keep = !sel.contains(&s.id);
            if !keep {
                group.strokes.push(s.clone());
            }
            keep
        });
        after[idx].texts.retain(|tx| {
            let keep = !sel.contains(&tx.id);
            if !keep {
                group.texts.push(tx.clone());
            }
            keep
        });
        after[idx].images.retain(|im| {
            let keep = !sel.contains(&im.id);
            if !keep {
                group.images.push(im.clone());
            }
            keep
        });
        after.insert(idx + 1, group);
        self.doc.next_layer_id += 1;
        self.history.push(
            &mut self.doc,
            Command::SetLayers { before, before_active: idx, after, after_active: idx + 1 },
        );
        self.info(t("Éléments réunis dans un nouveau calque.", "Elements grouped into a new layer."));
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
                // `elems.len() < 3` a déjà retourné plus haut : first/last existent forcément.
                let (Some(first_el), Some(last_el)) = (sorted.first(), sorted.last()) else { return };
                let first = center(&first_el.1);
                let last = center(&last_el.1);
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

    /// Bascule le mode plein écran / sans distraction (previous_audit.md
    /// #17) : plein écran natif macOS (`ViewportCommand::Fullscreen`) et
    /// masquage des panneaux vont toujours ensemble — un plein écran natif
    /// qui garderait la barre d'outils n'apporterait pas grand-chose de
    /// plus qu'agrandir la fenêtre.
    pub fn toggle_distraction_free(&mut self, ctx: &egui::Context) {
        self.distraction_free = !self.distraction_free;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.distraction_free));
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
        picked.sort_by(|a, b| a.1.z.total_cmp(&b.1.z));
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

    // --- Kit de marque (previous_audit.md #92) --------------------------

    /// Enregistre la palette personnalisée et la police système courantes
    /// sous `name` — écrase un kit existant du même nom (mêmes règles que
    /// les autres presets). Le logo se règle séparément (`set_brand_kit_logo`),
    /// jamais écrasé par cet appel.
    pub fn save_brand_kit(&mut self, name: String) {
        if name.trim().is_empty() {
            return;
        }
        let logo = self.brand_kits.iter().find(|k| k.name == name).and_then(|k| k.logo_png_b64.clone());
        self.brand_kits.retain(|k| k.name != name);
        let fonts = self.text_font_family.clone().into_iter().collect();
        self.brand_kits.push(crate::model::BrandKit { name: name.clone(), colors: self.custom_palette.clone(), fonts, logo_png_b64: logo });
        crate::i18n::save_brand_kits(&self.brand_kits);
        self.info(format!("{} « {name} ».", t("Kit de marque enregistré", "Brand kit saved")));
    }

    pub fn delete_brand_kit(&mut self, name: &str) {
        self.brand_kits.retain(|k| k.name != name);
        crate::i18n::save_brand_kits(&self.brand_kits);
    }

    /// Applique un kit : remplace la palette personnalisée, règle la police
    /// système du texte courant sur la première du kit s'il y en a une.
    /// Le logo n'est **pas** posé automatiquement (poser une image est un
    /// geste voulu, pas un effet de bord d'un clic « appliquer ») — voir
    /// `place_brand_kit_logo`.
    pub fn apply_brand_kit(&mut self, kit: &crate::model::BrandKit) {
        self.custom_palette = kit.colors.clone();
        crate::i18n::save_custom_palette(&self.custom_palette);
        if let Some(font) = kit.fonts.first() {
            self.text_font_family = Some(font.clone());
            self.sync_text_style();
        }
        self.info(format!("{} « {} ».", t("Kit de marque appliqué", "Brand kit applied"), kit.name));
    }

    /// Choisit un fichier image comme logo du kit `name` (remplace le
    /// précédent s'il y en avait un).
    pub fn set_brand_kit_logo(&mut self, name: &str) {
        let Some(Ok((w, h, rgba))) = crate::project::import_image_dialog() else { return };
        let Some(kit) = self.brand_kits.iter_mut().find(|k| k.name == name) else { return };
        kit.set_logo(w, h, &rgba);
        crate::i18n::save_brand_kits(&self.brand_kits);
        self.info(t("Logo enregistré.", "Logo saved."));
    }

    /// Pose le logo du kit `name` comme nouvelle image sur le canevas
    /// (même mécanisme que `import_image`), geste explicite plutôt qu'un
    /// effet de bord d'`apply_brand_kit`.
    pub fn place_brand_kit_logo(&mut self, name: &str) {
        let Some(kit) = self.brand_kits.iter().find(|k| k.name == name) else { return };
        let Some((w, h, rgba)) = kit.decode_logo() else {
            self.info(t("Ce kit n'a pas de logo.", "This kit has no logo."));
            return;
        };
        self.place_image(w, h, rgba);
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
                    italic: t.italic,
                    underline: t.underline,
                    line_height: t.line_height,
                    letter_spacing: t.letter_spacing,
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
                    t.italic = ts.italic;
                    t.underline = ts.underline;
                    t.line_height = ts.line_height;
                    t.letter_spacing = ts.letter_spacing;
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

    /// Extrait 5-8 couleurs dominantes de l'image sélectionnée (Sprint M.1)
    /// et les ajoute en un clic à la palette personnalisée.
    pub fn extract_palette_from_selection(&mut self) {
        let Some(idx) = self.single_image_idx() else {
            self.info(t("Sélectionne d'abord une image.", "Select an image first."));
            return;
        };
        let im = &self.doc.layers[self.doc.active_layer].images[idx];
        let colors = crate::tools::palette::extract_palette(&im.rgba, 8);
        if colors.is_empty() {
            self.info(t("Aucune couleur extraite (image entièrement transparente ?).", "No color extracted (fully transparent image?)."));
            return;
        }
        let n = colors.len();
        for c in colors {
            self.add_to_palette(c);
        }
        self.info(format!("{n} {}", t("couleurs ajoutées à la palette.", "colors added to the palette.")));
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

    /// Retourne toute l'image en miroir (point 66 de l'audit) — menu Image,
    /// annulable en un pas comme le redimensionnement.
    pub fn flip_document(&mut self, horizontal: bool) {
        let mut after = self.doc.clone();
        after.flip_content(horizontal);
        let label = if horizontal {
            t("Retourner horizontalement", "Flip horizontal")
        } else {
            t("Retourner verticalement", "Flip vertical")
        };
        self.push_doc_snapshot(after, label);
        self.info(label);
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

    // --- Textures de prévisualisation (vignettes de calque, aperçu du masque
    // de sélection) — le pipeline d'export lui-même vit dans `export_ops`.

    /// Vignette de calque (Sprint I.3), mise en cache par id + hash de
    /// contenu — recalculée seulement quand le calque change (même logique
    /// d'invalidation que le compositeur principal), pas à chaque frame.
    /// `None` si le calque n'a pas encore de pixmap en cache (masqué, ou pas
    /// encore de frame de composition).
    pub fn layer_thumbnail(&mut self, ctx: &egui::Context, layer_id: u64) -> Option<egui::TextureHandle> {
        let hash = self.compositor.layer_content_hash(layer_id)?;
        if let Some((h, tex)) = self.layer_thumbnails.get(&layer_id) {
            if *h == hash {
                return Some(tex.clone());
            }
        }
        let image = self.compositor.layer_thumbnail(layer_id, 32)?;
        let tex = ctx.load_texture(format!("layer_thumb_{layer_id}"), image, egui::TextureOptions::LINEAR);
        self.layer_thumbnails.insert(layer_id, (hash, tex.clone()));
        Some(tex)
    }

    /// Texture d'aperçu du masque de sélection en pixels (Sprint H) : teinte
    /// semi-transparente sur les zones **hors** sélection (option la moins
    /// coûteuse évoquée dans l'audit, plutôt qu'une vraie animation de
    /// contour en pointillés). Mise en cache par hash de contenu, recalculée
    /// seulement quand le masque change. `None` si pas de sélection par
    /// région active.
    fn selection_overlay_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let mask = self.selection_mask.as_ref()?;
        let hash = mask.content_hash();
        if let Some((h, tex)) = &self.selection_mask_texture {
            if *h == hash {
                return Some(tex.clone());
            }
        }
        let (w, h) = self.doc.size;
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        // Alpha max borné (120/255) : la teinte reste lisible sans masquer
        // complètement le contenu hors sélection.
        let pixels: Vec<Color32> = dense.iter().map(|&a| Color32::from_black_alpha((120 * (255 - a as u32) / 255) as u8)).collect();
        let image = egui::ColorImage { size: [w as usize, h as usize], pixels };
        let tex = ctx.load_texture("selection_mask_overlay", image, egui::TextureOptions::LINEAR);
        self.selection_mask_texture = Some((hash, tex.clone()));
        Some(tex)
    }

    /// Gère le glissé de guide manuel (Sprint R, point 95) : tirer depuis la
    /// règle du haut crée un guide horizontal, depuis celle de gauche un
    /// vertical ; avec l'outil Sélection, saisir un guide existant le
    /// déplace, et le relâcher hors du document (ou sur une règle) le
    /// supprime. Renvoie `true` si le geste est consommé (l'outil actif ne
    /// doit alors rien recevoir). Persisté avec le document (`history.touch`),
    /// pas de commande d'undo dédiée — même choix que les sélections nommées.
    pub(super) fn handle_guide_gesture(&mut self, response: &egui::Response, view: &ViewTransform) -> bool {
        const TH: f32 = 18.0; // épaisseur des règles (voir `paint_rulers`)
        let cr = self.last_canvas_rect;
        if response.drag_started() && self.guide_drag.is_none() {
            if let Some(p) = response.interact_pointer_pos() {
                let in_top = p.y >= cr.min.y && p.y <= cr.min.y + TH;
                let in_left = p.x >= cr.min.x && p.x <= cr.min.x + TH;
                if in_top && !in_left {
                    self.guide_drag = Some(GuideDrag { vertical: false, pos: view.screen_to_doc(p).1, existing: None });
                    return true;
                }
                if in_left && !in_top {
                    self.guide_drag = Some(GuideDrag { vertical: true, pos: view.screen_to_doc(p).0, existing: None });
                    return true;
                }
                // Saisie d'un guide existant — outil Sélection seulement,
                // pour ne pas voler le geste des outils de peinture.
                if self.active_tool == ActiveTool::Select {
                    let d = view.screen_to_doc(p);
                    let tol = 4.0 / self.zoom.max(0.01);
                    if let Some(i) = self.doc.guides.iter().position(|g| {
                        if g.vertical { (g.pos - d.0).abs() <= tol } else { (g.pos - d.1).abs() <= tol }
                    }) {
                        let g = self.doc.guides[i];
                        self.guide_drag = Some(GuideDrag { vertical: g.vertical, pos: g.pos, existing: Some(i) });
                        return true;
                    }
                }
            }
        }
        let Some(gd) = &mut self.guide_drag else { return false };
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                gd.pos = if gd.vertical { d.0 } else { d.1 };
            }
        }
        if response.drag_stopped() {
            let gd = self.guide_drag.take().expect("guide_drag vient d'être vérifié");
            let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
            let inside = if gd.vertical { (0.0..=w).contains(&gd.pos) } else { (0.0..=h).contains(&gd.pos) };
            match (gd.existing, inside) {
                (Some(i), true) if i < self.doc.guides.len() => {
                    self.doc.guides[i].pos = gd.pos;
                    self.history.touch();
                }
                (Some(i), false) if i < self.doc.guides.len() => {
                    self.doc.guides.remove(i);
                    self.history.touch();
                    self.info(t("Guide supprimé.", "Guide removed."));
                }
                (None, true) => {
                    self.doc.guides.push(crate::model::ManualGuide { vertical: gd.vertical, pos: gd.pos });
                    self.history.touch();
                    self.info(t("Guide ajouté (glisser sur une règle pour le retirer).", "Guide added (drag onto a ruler to remove it)."));
                }
                _ => {}
            }
        }
        true
    }

    /// Change la rotation de la vue (Sprint T, point 93) en gardant le
    /// **centre du document** fixe à l'écran : la transformation pivote
    /// autour de l'origine du document, on compense donc le pan pour que le
    /// centre ne bouge pas — sans quoi le document balaierait l'écran.
    pub fn set_view_angle(&mut self, angle: f32) {
        let center = (self.doc.size.0 as f32 / 2.0, self.doc.size.1 as f32 / 2.0);
        let before = self.current_view().doc_to_screen(center);
        self.view_angle = angle.rem_euclid(std::f32::consts::TAU);
        let after = self.current_view().doc_to_screen(center);
        self.pan += before - after;
    }

    /// Dessine une texture couvrant tout le document comme un quadrilatère
    /// texturé qui suit la rotation de la vue (Sprint T, point 93) — remplace
    /// les `painter.image(doc_rect)` axis-aligned pour le composite, la
    /// teinte de sélection et la pelure d'oignon.
    fn paint_doc_quad(&self, painter: &egui::Painter, view: &ViewTransform, tex_id: egui::TextureId, tint: Color32) {
        let (w, h) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
        let mut mesh = egui::Mesh::with_texture(tex_id);
        for (corner, uv) in [
            ((0.0, 0.0), egui::pos2(0.0, 0.0)),
            ((w, 0.0), egui::pos2(1.0, 0.0)),
            ((w, h), egui::pos2(1.0, 1.0)),
            ((0.0, h), egui::pos2(0.0, 1.0)),
        ] {
            mesh.vertices.push(egui::epaint::Vertex { pos: view.doc_to_screen(corner), uv, color: tint });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(mesh);
    }

    /// Position du guide en cours de glissé, pour l'aperçu de l'overlay
    /// (Sprint R, point 95) : `(vertical, position document)`.
    pub(super) fn guide_drag_preview(&self) -> Option<(bool, f32)> {
        self.guide_drag.as_ref().map(|g| (g.vertical, g.pos))
    }

    /// Texture du rendu composité d'une frame **non active** pour la pelure
    /// d'oignon (Sprint U). Recalculée seulement si l'historique a changé
    /// depuis le dernier rendu de cette frame (les frames inactives ne
    /// changent que par des opérations annulables). Compositeur jetable à
    /// chaque recalcul : les frames partagent les ids de calques, passer par
    /// `self.compositor` corromprait ses caches par calque.
    fn onion_texture(&mut self, ctx: &egui::Context, idx: usize) -> Option<egui::TextureHandle> {
        let rev = self.history.revision();
        if let Some((r, tex)) = self.onion_textures.get(&idx) {
            if *r == rev {
                return Some(tex.clone());
            }
        }
        let frame = self.doc.frames.get(idx)?;
        let mut temp = self.doc.clone();
        temp.layers = frame.layers.clone();
        temp.frames.clear();
        let mut comp = crate::render::compositor::Compositor::new();
        // Fond transparent : seul le contenu de la frame apparaît en
        // fantôme, pas un voile sur tout le document.
        let (w, h, rgba) = comp.render_to_rgba(ctx, &temp, Color32::TRANSPARENT)?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let tex = ctx.load_texture(format!("onion_{idx}"), image, egui::TextureOptions::LINEAR);
        self.onion_textures.insert(idx, (rev, tex.clone()));
        Some(tex)
    }

    /// Applique le thème d'interface (Sprint R, point 96) via la préférence
    /// native d'egui 0.29 (`ThemePreference`) : `System` suit le thème macOS
    /// remonté par winit, `Light`/`Dark` le forcent. Idempotent (ne pousse
    /// rien si la préférence est déjà la bonne).
    fn apply_theme(&self, ctx: &egui::Context) {
        let pref = match self.ui_theme {
            UiTheme::System => egui::ThemePreference::System,
            UiTheme::Light => egui::ThemePreference::Light,
            UiTheme::Dark => egui::ThemePreference::Dark,
        };
        ctx.options_mut(|o| {
            if o.theme_preference != pref {
                o.theme_preference = pref;
            }
        });
    }

    /// Recalcule (si nécessaire) les contours du masque de sélection pour les
    /// « fourmis en marche » (Sprint O, point 60) — même invalidation par
    /// hash de contenu que `selection_overlay_texture`.
    fn ensure_selection_ants(&mut self) {
        let Some(mask) = &self.selection_mask else {
            self.selection_ants = None;
            return;
        };
        let hash = mask.content_hash();
        if self.selection_ants.as_ref().is_some_and(|(h, _)| *h == hash) {
            return;
        }
        let (w, h) = self.doc.size;
        let dense = crate::tools::selection_mask::mask_to_dense(mask, w, h);
        let loops = crate::tools::selection_mask::contours(&dense, w as usize, h as usize);
        self.selection_ants = Some((hash, loops));
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
        ViewTransform { origin: self.last_canvas_rect.min + self.pan, scale: self.zoom, angle: self.view_angle }
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
            h = h.wrapping_mul(31).wrapping_add(l.adjustment.as_ref().map(|a| a.hash_key() + 1).unwrap_or(0));
        }
        h = h
            .wrapping_mul(31)
            .wrapping_add(self.doc.size.0 as u64)
            .wrapping_mul(31)
            .wrapping_add(self.doc.size.1 as u64);
        // Rebâtir quand on commence / finit d'éditer un texte (exclusion).
        h.wrapping_mul(31).wrapping_add(self.editing_text.unwrap_or(0))
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

    /// Pousse une copie miroir/symétrie (Sprint 11, modes miroir Sprint O) :
    /// en mode radial, `symmetry_axes` copies du trait réparties par rotation
    /// régulière autour du centre du document ; en mode miroir, le trait
    /// original plus sa (ses) réflexion(s) autour du (des) axe(s) central
    /// (aux) — en une seule commande d'undo (comme `duplicate_selection`).
    fn commit_symmetry_stroke(&mut self, stroke: Stroke) {
        if stroke.points.is_empty() {
            return;
        }
        let center = (self.doc.size.0 as f32 / 2.0, self.doc.size.1 as f32 / 2.0);
        // Chaque copie = une transformation point à point du trait capturé.
        let transforms: Vec<Box<dyn Fn((f32, f32)) -> (f32, f32)>> = match self.symmetry_mode {
            SymmetryMode::Radial => {
                let axes = self.symmetry_axes.max(1);
                (0..axes)
                    .map(|k| {
                        let angle = k as f32 * std::f32::consts::TAU / axes as f32;
                        let (ca, sa) = (angle.cos(), angle.sin());
                        Box::new(move |(x, y): (f32, f32)| {
                            let (dx, dy) = (x - center.0, y - center.1);
                            (center.0 + dx * ca - dy * sa, center.1 + dx * sa + dy * ca)
                        }) as Box<dyn Fn((f32, f32)) -> (f32, f32)>
                    })
                    .collect()
            }
            mode => {
                // Réflexions : identité + miroir X et/ou Y autour du centre.
                let flips: &[(bool, bool)] = match mode {
                    SymmetryMode::MirrorH => &[(false, false), (true, false)],
                    SymmetryMode::MirrorV => &[(false, false), (false, true)],
                    _ => &[(false, false), (true, false), (false, true), (true, true)],
                };
                flips
                    .iter()
                    .map(|&(fx, fy)| {
                        Box::new(move |(x, y): (f32, f32)| {
                            (
                                if fx { 2.0 * center.0 - x } else { x },
                                if fy { 2.0 * center.1 - y } else { y },
                            )
                        }) as Box<dyn Fn((f32, f32)) -> (f32, f32)>
                    })
                    .collect()
            }
        };
        let mut strokes = Vec::with_capacity(transforms.len());
        for tf in &transforms {
            let mut c = stroke.clone();
            c.id = self.next_id;
            self.next_id += 1;
            c.z = self.bump_z();
            for p in &mut c.points {
                p.pos = tf(p.pos);
            }
            strokes.push(c);
        }
        self.push_recent_color(stroke.color);
        let layer = self.doc.active_id();
        self.history.push(&mut self.doc, Command::AddMany { layer, strokes });
    }

    /// Motif de pointillés courant (previous_audit.md #55), ou `None`
    /// pour un trait plein. Relatif à l'épaisseur du trait plutôt que deux
    /// champs numériques séparés — un rapport plein/trou fixe (3:2) qui
    /// reste lisible à toute échelle sans exposer plus de réglages.
    fn dash_pattern(&self) -> Option<(f32, f32)> {
        self.dashed_stroke.then_some((self.brush.width * 3.0, self.brush.width * 2.0))
    }

    fn handle_draw(&mut self, ctx: &egui::Context, response: &egui::Response, view: &ViewTransform) {
        let now = ctx.input(|i| i.time);
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                let d = view.screen_to_doc(p);
                if let Some(sh) = self.active_tool.as_shape() {
                    let d = self.snap(d); // magnétisme grille pour les formes
                    self.shape_start = Some(d);
                    let mut preview = shape::build(sh, d, d, self.brush.color, self.brush.width, self.fill_shapes, self.poly_sides);
                    preview.dash = self.dash_pattern();
                    self.shape_preview = Some(preview);
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
                        let mut preview = shape::build(sh, start, d, self.brush.color, self.brush.width, self.fill_shapes, self.poly_sides);
                        preview.dash = self.dash_pattern();
                        self.shape_preview = Some(preview);
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
        self.apply_theme(ctx);
        self.autosave_tick();
        // Sans repaint périodique, une session restée inactive (aucune
        // interaction) ne redéclenche jamais `update` et l'autosave ne
        // tourne plus — un crash après une longue pause perdrait tout.
        ctx.request_repaint_after(Self::AUTOSAVE_INTERVAL);
        self.show_recovery_dialog(ctx);
        self.handle_screenshot(ctx);
        self.handle_native_menu();
        self.handle_shortcuts(ctx);
        self.handle_dropped_files(ctx);
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

        // Sans distraction (previous_audit.md #17) : Échap en sort
        // toujours, même si la barre d'outils (donc le bouton pour en
        // sortir) est justement ce qui est masqué — sans ça, un
        // utilisateur au clavier/à la souris seule serait coincé.
        if self.distraction_free && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.toggle_distraction_free(ctx);
        }

        let panel_frame = egui::Frame::default()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(Margin::symmetric(10.0, 6.0));

        if !self.distraction_free {
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
        }

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
            let view = ViewTransform { origin: rect.min + self.pan, scale: self.zoom, angle: self.view_angle };
            // Coins du document projetés (suivent la rotation de la vue) ;
            // `doc_rect` reste leur boîte englobante écran (clip, fit…).
            let (dw, dh) = (self.doc.size.0 as f32, self.doc.size.1 as f32);
            let doc_corners: [Pos2; 4] = [
                view.doc_to_screen((0.0, 0.0)),
                view.doc_to_screen((dw, 0.0)),
                view.doc_to_screen((dw, dh)),
                view.doc_to_screen((0.0, dh)),
            ];
            let doc_rect = doc_corners.iter().skip(1).fold(Rect::from_min_max(doc_corners[0], doc_corners[0]), |r, &c| r.union(Rect::from_min_max(c, c)));
            self.last_doc_rect = doc_rect;
            // Ombre portée + fond du document (polygones : suivent la rotation).
            let shadow: Vec<Pos2> = doc_corners.iter().map(|c| *c + Vec2::splat(3.0)).collect();
            painter.add(egui::Shape::convex_polygon(shadow, Color32::from_black_alpha(60), egui::Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(doc_corners.to_vec(), self.bg, egui::Stroke::NONE));
            // Pelure d'oignon (Sprint U) : frames voisines en fantôme teinté
            // (précédente en orange, suivante en bleu) sous la frame active.
            // Rouge/vert (couleurs d'origine) sont la paire la moins
            // distinguable en cas de daltonisme rouge-vert (proto/deutéranopie,
            // ~8 % des hommes) — orange/bleu reste net dans ce cas
            // (previous_audit.md critique n°2), en plus d'une opacité
            // différente entre les deux comme second signal.
            if self.onion_skin && self.doc.is_animated() {
                let active = self.doc.active_frame;
                let next = if active + 1 < self.doc.frames.len() { Some(active + 1) } else { None };
                for (idx, tint) in [
                    (active.checked_sub(1), Color32::from_rgba_unmultiplied(230, 159, 0, 90)),
                    (next, Color32::from_rgba_unmultiplied(0, 145, 230, 70)),
                ] {
                    if let Some(i) = idx {
                        if let Some(tex) = self.onion_texture(ctx, i) {
                            self.paint_doc_quad(&painter, &view, tex.id(), tint);
                        }
                    }
                }
            }
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
                let tex_id = self.compositor.texture(ctx, &self.doc, sig, self.editing_text).map(|t| t.id());
                if let Some(tex_id) = tex_id {
                    self.paint_doc_quad(&content, &view, tex_id, Color32::WHITE);
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
                                    // Mipmaps : évite le scintillement/pointillé quand
                                    // l'image est affichée plus petite que sa résolution.
                                    let opts = egui::TextureOptions::LINEAR
                                        .with_mipmap_mode(Some(egui::TextureFilter::Linear));
                                    ctx.load_texture(format!("img{}", im.id), ci, opts)
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
                // Décalage exprimé en doc : passer par la projection garantit
                // qu'il suit aussi la rotation de la vue (Sprint T, point 93).
                let off = ViewTransform {
                    origin: view.origin
                        + (view.doc_to_screen((self.move_delta.0, self.move_delta.1)) - view.doc_to_screen((0.0, 0.0))),
                    scale: self.zoom,
                    angle: self.view_angle,
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
                let guide_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 0, 200));
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
            painter.rect_stroke(doc_rect, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(120)));
            // Masque de sélection en pixels (Sprint H) : teinte semi-
            // transparente hors sélection, sous les poignées/pointillés de
            // la sélection d'objets classique.
            if let Some(tex) = self.selection_overlay_texture(ctx) {
                self.paint_doc_quad(&content, &view, tex.id(), Color32::WHITE);
            }
            // Fourmis en marche (Sprint O, point 60) : contour du masque en
            // pointillés animés (trait blanc continu + tirets noirs dont le
            // décalage avance avec le temps), par-dessus la teinte.
            self.ensure_selection_ants();
            if let Some((_, loops)) = &self.selection_ants {
                let offset = (ctx.input(|i| i.time) as f32 * 20.0) % 12.0;
                for lp in loops {
                    let mut pts: Vec<Pos2> = lp.iter().map(|&d| view.doc_to_screen(d)).collect();
                    if let Some(&first) = pts.first() {
                        pts.push(first); // referme la boucle
                    }
                    content.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.0_f32, Color32::WHITE)));
                    for s in egui::Shape::dashed_line_with_offset(
                        &pts,
                        egui::Stroke::new(1.0_f32, Color32::BLACK),
                        &[6.0],
                        &[6.0],
                        offset,
                    ) {
                        content.add(s);
                    }
                }
                if !loops.is_empty() {
                    ctx.request_repaint_after(std::time::Duration::from_millis(80));
                }
            }
            self.paint_selection(&painter, &view, moving);
            self.paint_pen(&content, &view, &response);
            self.paint_pen_edit(&content, &view);
            self.paint_crop(&painter, &view);
            self.paint_retouch(&painter, &view);
            self.paint_perspective_handles(&painter, &view);
            self.paint_marquee(&painter, &view);
            self.paint_manual_guides(&painter, &view);
            self.paint_measure(&painter, &view);
            self.paint_cursor(&painter, &response);
            if self.show_rulers && self.view_angle == 0.0 {
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
    // Vue tournée (Sprint T, point 93) : toujours passer par le maillage,
    // un Rect écran ne peut pas tourner.
    if im.rot.abs() < 1e-4 && view.angle == 0.0 {
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
    let underline = crate::render::text::underline_stroke(t, view.scale);
    let passes = crate::render::text::passes(t);
    let last = passes.len() - 1;
    for (i, (off, col)) in passes.into_iter().enumerate() {
        // Décalage de passe (unités document) → écran, tourné comme le texte.
        let (ox, oy) = (off.0 * view.scale, off.1 * view.scale);
        let pos = egui::pos2(anchor.x + ox * c - oy * s, anchor.y + ox * s + oy * c);
        let color = Color32::from_rgba_unmultiplied(col[0], col[1], col[2], col[3])
            .gamma_multiply(opacity);
        let mut shape = egui::epaint::TextShape::new(pos, galley.clone(), color);
        shape.override_text_color = Some(color);
        shape.angle = t.rot;
        // Soulignement uniquement sur le remplissage central (dernière
        // passe) : voir `render::text::underline_stroke`.
        if i == last {
            if let Some(mut u) = underline {
                u.color = u.color.gamma_multiply(opacity);
                shape.underline = u;
            }
        }
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

    /// Symétrie miroir (Sprint O, point 54) : en mode MirrorH, un trait est
    /// validé en deux copies — l'original et sa réflexion autour de l'axe
    /// vertical central — dans une seule commande d'undo.
    #[test]
    fn symmetry_mirror_h_commits_the_stroke_and_its_reflection() {
        let mut app = PaintApp::default();
        app.symmetry_mode = crate::tools::SymmetryMode::MirrorH;
        let cx = app.doc.size.0 as f32 / 2.0;
        let mut s = Stroke::new([255, 0, 0, 255], 2.0, Tool::Brush);
        s.points = vec![crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 }];

        app.commit_symmetry_stroke(s);

        let strokes = &app.doc.layers[0].strokes;
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].points[0].pos, (10.0, 20.0));
        assert_eq!(strokes[1].points[0].pos, (2.0 * cx - 10.0, 20.0), "réflexion autour de l'axe vertical central");
        app.history.undo(&mut app.doc);
        assert!(app.doc.layers[0].strokes.is_empty(), "une seule commande d'undo pour les deux copies");
    }

    /// Retourner l'image (Sprint O, point 66) : commande annulable qui
    /// renvoie le contenu à l'emplacement miroir.
    #[test]
    fn flip_document_is_undoable() {
        let mut app = PaintApp::default();
        let w = app.doc.size.0 as f32;
        add_stroke_at(&mut app, 0, 1, (10.0, 20.0));

        app.flip_document(true);
        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos, (w - 10.0, 20.0));
        app.history.undo(&mut app.doc);
        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos, (10.0, 20.0));
    }

    #[test]
    fn layer_lock_allows_only_non_destructive_tools() {
        assert!(!layer_lock_blocks_tool(ActiveTool::Pan));
        assert!(!layer_lock_blocks_tool(ActiveTool::Eyedropper));
        assert!(!layer_lock_blocks_tool(ActiveTool::Measure));
        assert!(layer_lock_blocks_tool(ActiveTool::Brush));
        assert!(layer_lock_blocks_tool(ActiveTool::Pencil));
        assert!(layer_lock_blocks_tool(ActiveTool::Eraser));
        assert!(layer_lock_blocks_tool(ActiveTool::Select));
        assert!(layer_lock_blocks_tool(ActiveTool::Text));
        assert!(layer_lock_blocks_tool(ActiveTool::Bucket));
    }

    #[test]
    fn push_move_is_blocked_when_position_is_locked() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        app.selection = [1].into_iter().collect();
        app.doc.layers[0].lock_position = true;

        app.push_move(5.0, 5.0);

        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos, (10.0, 10.0), "position verrouillée : pas de déplacement");
    }

    #[test]
    fn push_move_works_normally_without_lock_position() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        app.selection = [1].into_iter().collect();

        app.push_move(5.0, 5.0);

        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos, (15.0, 15.0));
    }

    #[test]
    fn lock_alpha_prevents_painting_into_transparent_pixels() {
        let mut app = app_with_layers(1);
        app.doc.layers[0].lock_alpha = true;
        let layer_id = app.doc.active_id();
        app.touch_raster_tiles(10.0, 10.0, 3.0, false);
        if let Some(layer) = app.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.raster.stamp(10.0, 10.0, 3.0, 1.0, [255, 0, 0, 255], false, None);
        }
        app.commit_raster_stroke(RasterOp::Brush, false);

        let px = app.doc.layers[0].raster.get_pixel(10, 10);
        assert_eq!(px[3], 0, "transparence verrouillée : le pixel reste transparent malgré la peinture");
    }

    #[test]
    fn lock_alpha_preserves_existing_alpha_when_repainting_color() {
        let mut app = app_with_layers(1);
        let layer_id = app.doc.active_id();
        // D'abord peindre normalement (pas verrouillé) un pixel semi-opaque.
        app.touch_raster_tiles(10.0, 10.0, 3.0, false);
        if let Some(layer) = app.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.raster.stamp(10.0, 10.0, 3.0, 1.0, [0, 0, 0, 128], false, None);
        }
        app.commit_raster_stroke(RasterOp::Brush, false);
        let before_alpha = app.doc.layers[0].raster.get_pixel(10, 10)[3];
        assert!(before_alpha > 0);

        // Verrouiller la transparence puis repeindre une autre couleur.
        app.doc.layers[0].lock_alpha = true;
        app.touch_raster_tiles(10.0, 10.0, 3.0, false);
        if let Some(layer) = app.doc.layers.iter_mut().find(|l| l.id == layer_id) {
            layer.raster.stamp(10.0, 10.0, 3.0, 1.0, [255, 0, 0, 255], false, None);
        }
        app.commit_raster_stroke(RasterOp::Brush, false);

        let after = app.doc.layers[0].raster.get_pixel(10, 10);
        assert_eq!(after[3], before_alpha, "alpha inchangé malgré la nouvelle couleur peinte par-dessus");
        assert_eq!(after[0], 255, "la couleur, elle, a bien changé");
    }

    /// UX-3.1 : glisser-déposer un calque vers l'avant (index croissant) ou
    /// vers l'arrière (index décroissant) doit produire le même ordre que le
    /// modèle mental « ce calque prend la place de la cible » — et le calque
    /// actif doit suivre son propre contenu, pas rester au même index brut.
    fn layer_ids(app: &PaintApp) -> Vec<u64> {
        app.doc.layers.iter().map(|l| l.id).collect()
    }

    #[test]
    fn crop_rgba_to_bounds_extracts_the_selection_rectangle() {
        // 4x4, chaque pixel = son index (0..15), pour vérifier précisément
        // quels pixels finissent dans le recadrage.
        let w = 4u32;
        let h = 4u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for i in 0..(w * h) {
            let v = i as u8;
            rgba[(i * 4) as usize..(i * 4 + 4) as usize].copy_from_slice(&[v, v, v, 255]);
        }
        // Sélection (1,1)-(3,3) : sous-carré 2x2 au centre (pixels 5,6,9,10).
        let (rw, rh, out) = crop_rgba_to_bounds(w, h, &rgba, (1.0, 1.0), (3.0, 3.0));
        assert_eq!((rw, rh), (2, 2));
        assert_eq!(out[0], 5); // (1,1)
        assert_eq!(out[4], 6); // (2,1)
        assert_eq!(out[8], 9); // (1,2)
        assert_eq!(out[12], 10); // (2,2)
    }

    #[test]
    fn crop_rgba_to_bounds_falls_back_to_full_buffer_for_a_degenerate_rect() {
        let rgba = vec![7u8; 4 * 4 * 4];
        let (rw, rh, out) = crop_rgba_to_bounds(4, 4, &rgba, (2.0, 2.0), (2.0, 2.0));
        assert_eq!((rw, rh), (4, 4));
        assert_eq!(out, rgba);
    }

    #[test]
    fn import_dropped_file_routes_a_png_to_place_image() {
        let mut app = app_with_layers(1);
        let mut path = std::env::temp_dir();
        path.push("quickpaint-test-dropped.png");
        image::RgbaImage::from_fn(3, 2, |_, _| image::Rgba([10, 20, 30, 255])).save(&path).expect("write png");
        app.import_dropped_file(&path);
        assert_eq!(app.doc.layers[0].images.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_dropped_file_routes_a_json_to_open_project() {
        let mut app = app_with_layers(1);
        let mut doc = crate::model::Document::new((50, 40));
        doc.layers[0].strokes.push({
            let mut s = crate::model::Stroke::new([1, 2, 3, 255], 2.0, crate::model::stroke::Tool::Brush);
            s.points.push(crate::model::StrokePoint { pos: (0.0, 0.0), width: 2.0 });
            s
        });
        let mut path = std::env::temp_dir();
        path.push("quickpaint-test-dropped.json");
        std::fs::write(&path, serde_json::to_string(&doc).unwrap()).expect("write json");
        app.import_dropped_file(&path);
        assert_eq!(app.doc.size, (50, 40));
        assert_eq!(app.doc.layers[0].strokes.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_dropped_file_routes_an_svg_to_editable_strokes() {
        let mut app = app_with_layers(1);
        let mut path = std::env::temp_dir();
        path.push("quickpaint-test-dropped.svg");
        std::fs::write(
            &path,
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30">
                <rect x="5" y="5" width="10" height="10" fill="#ff0000"/>
            </svg>"##,
        )
        .expect("write svg");
        app.import_dropped_file(&path);
        assert_eq!(app.doc.size, (40, 30));
        assert_eq!(app.doc.layers[0].strokes.len(), 1);
        assert!(app.doc.layers[0].strokes[0].fill);
        let _ = std::fs::remove_file(&path);
    }

    /// Chaque gabarit riche doit produire au moins un fond et des blocs de
    /// texte substituables, avec des ids uniques et des z croissants (le
    /// texte doit rester au-dessus du fond).
    #[test]
    fn seed_template_content_adds_a_background_and_editable_texts() {
        for content in [TemplateContent::InstagramPromo, TemplateContent::FacebookBanner] {
            let mut app = app_with_layers(1);
            app.doc.size = (1080, 1080);
            app.seed_template_content(content);

            let layer = &app.doc.layers[0];
            assert!(layer.strokes.iter().any(|s| s.fill), "fond attendu ({content:?})");
            assert!(layer.texts.len() >= 2, "textes substituables attendus ({content:?})");
            let bg_z = layer.strokes[0].z;
            assert!(layer.texts.iter().all(|t| t.z > bg_z), "le texte doit passer au-dessus du fond ({content:?})");
            let mut ids: Vec<u64> = layer.strokes.iter().map(|s| s.id).chain(layer.texts.iter().map(|t| t.id)).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), layer.strokes.len() + layer.texts.len(), "ids uniques ({content:?})");
        }
    }

    /// Chaque gabarit riche doit passer par `seed_template_content` sans
    /// paniquer et laisser un historique annulable jusqu'au document vide.
    #[test]
    fn seed_template_content_is_fully_undoable_for_every_template() {
        for content in [TemplateContent::InstagramPromo, TemplateContent::FacebookBanner] {
            let mut app = app_with_layers(1);
            app.doc.size = (1080, 1080);
            app.seed_template_content(content);
            let layer = &app.doc.layers[0];
            assert!(layer.strokes.len() + layer.texts.len() + layer.images.len() > 0);
            for _ in 0..64 {
                app.undo();
            }
            let layer = &app.doc.layers[0];
            assert_eq!(
                layer.strokes.len() + layer.texts.len() + layer.images.len(),
                0,
                "l'annulation complète doit revenir au document vide ({content:?})"
            );
        }
    }

    /// Rendu d'export hors écran des gabarits riches : le compositeur doit
    /// produire une image à la résolution native, non uniforme (fond + texte).
    #[test]
    fn seeded_templates_render_to_rgba_at_native_resolution() {
        for (content, size) in [(TemplateContent::InstagramPromo, (1080, 1080)), (TemplateContent::FacebookBanner, (1200, 630))] {
            let mut app = app_with_layers(1);
            app.doc.size = size;
            app.seed_template_content(content);

            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |_| {});
            let mut c = crate::render::compositor::Compositor::new();
            let (w, h, rgba) = c.render_to_rgba(&ctx, &app.doc, Color32::WHITE).expect("render");
            assert_eq!((w, h), size, "résolution native attendue ({content:?})");
            let first = &rgba[0..4];
            assert!(rgba.chunks_exact(4).any(|px| px != first), "rendu non uniforme attendu ({content:?})");
        }
    }

    /// Soulignement de texte (previous_audit.md #61) : preuve de bout en
    /// bout que le trait est bien blitté par le compositeur CPU (pas
    /// seulement que `underline_stroke()` retourne un `Stroke`, déjà couvert
    /// par des tests unitaires dans `render/text.rs`) — compare le rendu
    /// avec/sans soulignement et vérifie qu'un pixel juste sous la ligne de
    /// base change alors que rien au-dessus (le glyphe lui-même) ne bouge.
    #[test]
    fn text_underline_draws_a_bar_below_the_baseline() {
        let mut app = app_with_layers(1);
        app.doc.size = (200, 100);
        let mut item = crate::model::TextItem::new(1, (10.0, 10.0), 40.0, [0, 0, 0, 255]);
        item.text = "A".to_string();
        item.underline = true;
        app.doc.layers[0].texts.push(item.clone());

        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = crate::render::compositor::Compositor::new();
        let (w, h, with_underline) = c.render_to_rgba(&ctx, &app.doc, Color32::WHITE).expect("render");

        app.doc.layers[0].texts[0].underline = false;
        let mut c2 = crate::render::compositor::Compositor::new();
        let (_, _, without_underline) = c2.render_to_rgba(&ctx, &app.doc, Color32::WHITE).expect("render");

        assert_eq!(with_underline.len(), without_underline.len());
        assert_ne!(with_underline, without_underline, "le soulignement doit changer le rendu ({w}x{h})");
    }

    /// Perf smoke test (previous_audit.md P1.8 : aucun profilage n'existait avant
    /// ce test) — pas un vrai benchmark `criterion` (le crate est un seul
    /// binaire, sans cible `[lib]` séparée pour qu'un fichier `benches/`
    /// externe puisse le lier), mais une mesure grossière avec seuil large,
    /// pour détecter une régression catastrophique du compositeur sur un
    /// document réaliste plutôt que de partir sans aucune donnée chiffrée.
    /// `#[ignore]` : n'allonge pas `cargo test` par défaut, se lance
    /// explicitement via `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn compose_stays_reasonably_fast_on_a_large_document() {
        let mut app = app_with_layers(20);
        app.doc.size = (4000, 3000);
        for (li, layer) in app.doc.layers.iter_mut().enumerate() {
            for si in 0..50 {
                let mut stroke = Stroke::new([(si * 5) as u8, (li * 10) as u8, 120, 255], 6.0, Tool::Brush);
                stroke.id = (li * 1000 + si) as u64;
                for p in 0..20 {
                    stroke.points.push(crate::model::StrokePoint {
                        pos: ((si * 60 + p * 4) as f32 % 4000.0, (li * 130 + p * 7) as f32 % 3000.0),
                        width: 6.0,
                    });
                }
                layer.strokes.push(stroke);
            }
        }

        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = crate::render::compositor::Compositor::new();
        let start = std::time::Instant::now();
        let (w, h, _) = c.render_to_rgba(&ctx, &app.doc, Color32::WHITE).expect("render");
        let elapsed = start.elapsed();
        eprintln!("compose {w}x{h}, 20 calques x 50 traits : {elapsed:?}");
        // Seuil volontairement large (régression franche, pas un budget de
        // perf strict) : ce test n'a pas vocation à remplacer un vrai
        // benchmark, seulement à attraper un aller-retour O(n²) évident.
        assert!(elapsed.as_secs() < 5, "composition anormalement lente : {elapsed:?}");
    }

    /// Voir `compose_stays_reasonably_fast_on_a_large_document` — même
    /// principe côté undo/redo (previous_audit.md P1.8), sur une session longue
    /// de traits vectoriels (le chemin `PaintRaster`, par tuile, n'est pas
    /// couvert ici : il a sa propre garantie de coût par la conception même
    /// de `Command::PaintRaster`, voir `history.rs`).
    #[test]
    #[ignore]
    fn undo_redo_stays_reasonably_fast_over_a_long_session() {
        let mut app = app_with_layers(1);
        let layer = app.doc.layers[0].id;
        for i in 0..500 {
            let mut stroke = Stroke::new([10, 20, 30, 255], 4.0, Tool::Brush);
            stroke.id = i;
            stroke.points.push(crate::model::StrokePoint { pos: (i as f32 % 800.0, i as f32 % 600.0), width: 4.0 });
            app.history.push(&mut app.doc, Command::AddStroke { layer, stroke });
        }

        let start = std::time::Instant::now();
        for _ in 0..500 {
            app.history.undo(&mut app.doc);
        }
        for _ in 0..500 {
            app.history.redo(&mut app.doc);
        }
        let elapsed = start.elapsed();
        eprintln!("500 undo + 500 redo sur 500 traits : {elapsed:?}");
        assert!(elapsed.as_secs() < 5, "undo/redo anormalement lent : {elapsed:?}");
    }

    /// Texte → tracés (previous_audit.md #64) : le texte sélectionné
    /// disparaît, remplacé par au moins un trait non rempli par contour de
    /// glyphe visible, et l'opération s'annule en un coup (`Command::SetDoc`).
    #[test]
    fn convert_text_to_outlines_replaces_text_with_unfilled_strokes() {
        let mut app = app_with_layers(1);
        // Une police au hasard peut ne pas couvrir 'l' (police à icônes,
        // symboles…) — même repli que `tools::text_outline::tests`.
        let family = ["Helvetica", ".AppleSystemUIFont"]
            .into_iter()
            .find(|f| app.font_manager.font_bytes(f, false, false).is_some())
            .map(str::to_string)
            .or_else(|| app.font_manager.family_names().into_iter().find(|f| app.font_manager.font_bytes(f, false, false).is_some()))
            .expect("au moins une police système exploitable");
        let mut item = crate::model::TextItem::new(1, (10.0, 20.0), 60.0, [255, 0, 0, 255]);
        item.text = "l".to_string(); // lettre sans trou : résultat toujours correct même rempli
        item.font_family = Some(family);
        app.doc.layers[0].texts.push(item);
        app.selection = [1].into_iter().collect();
        let strokes_before = app.doc.layers[0].strokes.len();

        app.convert_text_to_outlines();

        assert!(app.doc.layers[0].texts.is_empty(), "le texte doit avoir disparu");
        assert!(app.doc.layers[0].strokes.len() > strokes_before, "au moins un trait de contour ajouté");
        assert!(app.doc.layers[0].strokes.iter().all(|s| !s.fill), "les contours de glyphe ne sont pas remplis par défaut");
        assert!(app.history.can_undo());
        app.history.undo(&mut app.doc);
        assert_eq!(app.doc.layers[0].strokes.len(), strokes_before, "annulable : le texte doit revenir, pas les traits");
        assert_eq!(app.doc.layers[0].texts.len(), 1);
    }

    #[test]
    fn convert_text_to_outlines_refuses_the_builtin_sans_font() {
        let mut app = app_with_layers(1);
        let mut item = crate::model::TextItem::new(1, (0.0, 0.0), 40.0, [0, 0, 0, 255]);
        item.text = "a".to_string();
        // `font_family: None` = police intégrée Sans/Mono (pas dans fontdb).
        app.doc.layers[0].texts.push(item);
        app.selection = [1].into_iter().collect();

        app.convert_text_to_outlines();

        assert_eq!(app.doc.layers[0].texts.len(), 1, "police intégrée : rien à convertir, le texte reste");
        assert!(app.doc.layers[0].strokes.is_empty());
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

    /// Fusion de calques (Sprint P, point 30) : le contenu peint du calque
    /// actif survit au merge down — il était silencieusement perdu avant.
    #[test]
    fn merge_down_keeps_painted_raster_content() {
        let mut app = app_with_layers(2);
        app.doc.active_layer = 1;
        app.doc.layers[1].raster.set_pixel(10, 10, [255, 0, 0, 255]);

        app.merge_down();

        assert_eq!(app.doc.layers.len(), 1);
        assert_eq!(app.doc.layers[0].raster.get_pixel(10, 10), [255, 0, 0, 255]);
        app.history.undo(&mut app.doc);
        assert_eq!(app.doc.layers.len(), 2, "annulable : la pile d'origine revient");
        assert_eq!(app.doc.layers[1].raster.get_pixel(10, 10)[3], 255);
    }

    /// Aplatir garde aussi le raster peint de chaque calque, composé de bas
    /// en haut (le calque du dessus recouvre).
    #[test]
    fn flatten_composites_raster_from_all_layers() {
        let mut app = app_with_layers(3);
        app.doc.layers[0].raster.set_pixel(5, 5, [0, 0, 255, 255]);
        app.doc.layers[2].raster.set_pixel(5, 5, [255, 0, 0, 255]);
        app.doc.layers[1].raster.set_pixel(7, 7, [0, 255, 0, 255]);

        app.flatten();

        assert_eq!(app.doc.layers.len(), 1);
        assert_eq!(app.doc.layers[0].raster.get_pixel(5, 5), [255, 0, 0, 255], "le calque du dessus recouvre");
        assert_eq!(app.doc.layers[0].raster.get_pixel(7, 7), [0, 255, 0, 255]);
    }

    #[test]
    fn airbrush_dab_increases_center_alpha_monotonically_when_held_still() {
        let mut app = app_with_layers(1);
        app.brush.color = [255, 0, 0, 255];
        app.brush.width = 20.0;
        let pos = (50.0, 50.0);
        let mut prev_alpha = 0u8;
        for _ in 0..5 {
            app.paint_airbrush_dab(pos);
            let alpha = app.doc.layers[0].raster.get_pixel(50, 50)[3];
            assert!(alpha > prev_alpha, "l'alpha devrait augmenter à chaque dépôt, got {alpha} after prev {prev_alpha}");
            prev_alpha = alpha;
        }
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

    /// Poignées de perspective glissables (previous_audit.md #87),
    /// remplaçant les anciens sliders X/Y par coin. Vue identité (origine
    /// nulle, échelle 1, sans rotation) : coordonnées écran = coordonnées
    /// document, pour des assertions directes.
    fn identity_view() -> crate::render::canvas::ViewTransform {
        crate::render::canvas::ViewTransform { origin: egui::Pos2::ZERO, scale: 1.0, angle: 0.0 }
    }

    #[test]
    fn perspective_handles_is_none_when_panel_closed() {
        let mut app = app_with_layers(1);
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 10, 10, vec![200u8; 10 * 10 * 4]);
        app.doc.layers[0].images.push(im);
        app.selection = [1].into_iter().collect();
        assert!(!app.show_perspective_panel);
        assert!(app.perspective_handles(&identity_view()).is_none());
    }

    #[test]
    fn perspective_handles_match_image_corners_at_zero_offset() {
        let mut app = app_with_layers(1);
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 10, 10, vec![200u8; 10 * 10 * 4]);
        app.doc.layers[0].images.push(im);
        app.selection = [1].into_iter().collect();
        app.show_perspective_panel = true;

        let handles = app.perspective_handles(&identity_view()).expect("image sélectionnée, panneau ouvert");
        assert_eq!(handles[0], egui::Pos2::new(0.0, 0.0), "haut-gauche");
        assert_eq!(handles[2], egui::Pos2::new(10.0, 10.0), "bas-droit");
    }

    #[test]
    fn dragging_a_perspective_handle_updates_its_fractional_offset() {
        let mut app = app_with_layers(1);
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 10, 10, vec![200u8; 10 * 10 * 4]);
        app.doc.layers[0].images.push(im);
        app.selection = [1].into_iter().collect();
        app.show_perspective_panel = true;
        let view = identity_view();

        // Clic sur la poignée haut-gauche (écran = doc = (0,0) à vue identité).
        assert!(app.start_perspective_drag_if_handle(egui::Pos2::new(0.0, 0.0), &view));
        // Glissé de 2px en x, 3px en y (image 10×10 → fractions 0.2/0.3).
        app.update_perspective_drag(egui::Pos2::new(2.0, 3.0), &view);

        let (ox, oy) = app.perspective_offsets[0];
        assert!((ox - 0.2).abs() < 1e-4, "décalage X attendu ≈0.2, got {ox}");
        assert!((oy - 0.3).abs() < 1e-4, "décalage Y attendu ≈0.3, got {oy}");
        // Les 3 autres coins restent à leur décalage nul.
        assert_eq!(app.perspective_offsets[1], (0.0, 0.0));
    }

    #[test]
    fn clicking_away_from_any_handle_does_not_start_a_perspective_drag() {
        let mut app = app_with_layers(1);
        // 100×100 : le centre (50,50) est à ~70px de chaque coin, largement
        // hors du rayon d'accroche de 10px des poignées (contrairement à une
        // image 10×10, où le centre serait à ~7px de chaque coin — donc dans
        // le rayon des 4 poignées à la fois).
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), 100, 100, vec![200u8; 100 * 100 * 4]);
        app.doc.layers[0].images.push(im);
        app.selection = [1].into_iter().collect();
        app.show_perspective_panel = true;

        assert!(!app.start_perspective_drag_if_handle(egui::Pos2::new(50.0, 50.0), &identity_view()));
        assert!(app.perspective_drag.is_none());
    }

    #[test]
    fn merge_selection_to_image_replaces_selected_elements_with_one_image() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        add_stroke_at(&mut app, 0, 2, (20.0, 20.0));
        app.selection = [1, 2].into_iter().collect();
        let ctx = egui::Context::default();

        app.merge_selection_to_image(&ctx);

        let l = &app.doc.layers[0];
        assert!(l.strokes.is_empty(), "les traits fusionnés doivent disparaître");
        assert_eq!(l.images.len(), 1, "remplacés par une seule image");
        assert_eq!(app.selection.len(), 1, "la sélection ne porte plus que la nouvelle image");
        assert!(app.selection.contains(&l.images[0].id));
    }

    #[test]
    fn merge_selection_to_image_is_a_noop_with_a_single_element() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        app.selection = [1].into_iter().collect();
        let ctx = egui::Context::default();

        app.merge_selection_to_image(&ctx);

        assert_eq!(app.doc.layers[0].strokes.len(), 1, "un seul élément : pas de fusion");
        assert!(app.doc.layers[0].images.is_empty());
    }

    #[test]
    fn group_selection_into_layer_moves_elements_to_a_new_layer_above() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        add_stroke_at(&mut app, 0, 2, (20.0, 20.0));
        add_stroke_at(&mut app, 0, 3, (30.0, 30.0)); // reste sur le calque source
        app.selection = [1, 2].into_iter().collect();

        app.group_selection_into_layer();

        assert_eq!(app.doc.layers.len(), 2, "un nouveau calque a été inséré");
        assert_eq!(app.doc.layers[0].strokes.len(), 1, "seul l'élément non sélectionné reste");
        assert_eq!(app.doc.layers[0].strokes[0].id, 3);
        let group = &app.doc.layers[1];
        assert_eq!(group.strokes.len(), 2, "les 2 éléments sélectionnés sont dans le nouveau calque");
        assert_eq!(app.doc.active_layer, 1, "le nouveau calque devient actif");
    }

    #[test]
    fn group_selection_into_layer_is_a_noop_with_a_single_element() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (10.0, 10.0));
        app.selection = [1].into_iter().collect();

        app.group_selection_into_layer();

        assert_eq!(app.doc.layers.len(), 1, "un seul élément : pas de regroupement");
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
        app.select_in_ellipse((0.0, 0.0), (20.0, 10.0), SelectionCombine::Replace);
        assert_eq!(app.selection, [1].into_iter().collect());
    }

    #[test]
    fn select_in_rect_populates_a_pixel_selection_mask() {
        let mut app = app_with_layers(1);
        app.select_in_rect((10.0, 10.0), (30.0, 30.0), SelectionCombine::Replace);
        let mask = app.selection_mask.as_ref().expect("un masque devrait exister après select_in_rect");
        assert_eq!(mask.get_pixel(20, 20)[3], 255, "à l'intérieur du rectangle");
        assert_eq!(mask.get_pixel(5, 5)[3], 0, "à l'extérieur du rectangle");
    }

    #[test]
    fn pixel_brush_respects_the_selection_mask() {
        let mut app = app_with_layers(1);
        app.brush.color = [255, 0, 0, 255];
        app.brush.width = 10.0;
        // Sélectionne seulement le quart supérieur-gauche du document.
        app.select_in_rect((0.0, 0.0), (50.0, 50.0), SelectionCombine::Replace);
        // Un point à l'intérieur de la sélection doit être peint.
        app.paint_raster_point((20.0, 20.0), false);
        assert!(app.doc.layers[0].raster.get_pixel(20, 20)[3] > 0, "dans la sélection, le pinceau doit peindre");
        // Un point hors sélection ne doit rien peindre.
        app.paint_raster_point((80.0, 80.0), false);
        assert_eq!(app.doc.layers[0].raster.get_pixel(80, 80)[3], 0, "hors sélection, le pinceau ne doit rien peindre");
    }

    #[test]
    fn feather_selection_softens_the_mask_edge() {
        let mut app = app_with_layers(1);
        app.select_in_rect((10.0, 10.0), (30.0, 30.0), SelectionCombine::Replace);
        app.feather_selection(3.0);
        let mask = app.selection_mask.as_ref().unwrap();
        let edge = mask.get_pixel(10, 20)[3];
        assert!(edge > 0 && edge < 255, "le bord devrait être un dégradé après feather, got {edge}");
    }

    #[test]
    fn dilate_selection_grows_the_mask() {
        let mut app = app_with_layers(1);
        app.select_in_rect((20.0, 20.0), (30.0, 30.0), SelectionCombine::Replace);
        assert_eq!(app.selection_mask.as_ref().unwrap().get_pixel(35, 25)[3], 0);
        app.dilate_selection(6);
        assert_eq!(app.selection_mask.as_ref().unwrap().get_pixel(35, 25)[3], 255, "dilater devrait grossir la sélection au-delà de son bord d'origine");
    }

    #[test]
    fn contract_selection_shrinks_the_mask() {
        let mut app = app_with_layers(1);
        app.select_in_rect((20.0, 20.0), (40.0, 40.0), SelectionCombine::Replace);
        assert_eq!(app.selection_mask.as_ref().unwrap().get_pixel(21, 30)[3], 255);
        app.contract_selection(3);
        assert_eq!(app.selection_mask.as_ref().unwrap().get_pixel(21, 30)[3], 0, "contracter devrait ronger le bord de la sélection");
    }

    /// Amélioration des bords (previous_audit.md #38) : bout-en-bout à
    /// travers l'API `PaintApp` (le mécanisme lui-même — durcir plus dans
    /// les zones texturées — est déjà couvert au niveau pur par
    /// `refine_edges_sharpens_boundary_more_in_textured_zones` dans
    /// `tools/bucket.rs`). Nécessite un bord déjà adouci (feather) : sur un
    /// masque net 0/255 comme `select_in_rect` seul le produit, l'affinage
    /// n'a par construction aucun effet (voir la doc de `refine_edges`).
    #[test]
    fn refine_selection_edges_sharpens_more_over_textured_content() {
        let mut app = app_with_layers(1);
        app.doc.size = (20, 6);
        // Moitié gauche unie, moitié droite en damier haute fréquence.
        // Sélection d'une bande centrale (x=5..15) : son bord gauche (x=5)
        // tombe en zone plate, son bord droit (x=15) en zone texturée —
        // même geste de feather des deux côtés, pour isoler l'effet de la
        // texture plutôt qu'un bord différent.
        for y in 0..6 {
            for x in 0..20 {
                let v: u8 = if x < 10 { 128 } else if (x + y) % 2 == 0 { 0 } else { 255 };
                app.doc.layers[0].raster.set_pixel(x, y, [v, v, v, 255]);
            }
        }
        app.select_in_rect((5.0, 0.0), (15.0, 6.0), SelectionCombine::Replace);
        app.feather_selection(3.0);
        let before = app.selection_mask.clone().unwrap();

        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        app.refine_selection_edges(&ctx, 2);
        let after = app.selection_mask.as_ref().unwrap();

        let flat_shift = (after.get_pixel(5, 3)[3] as i32 - before.get_pixel(5, 3)[3] as i32).abs();
        let textured_shift = (after.get_pixel(15, 3)[3] as i32 - before.get_pixel(15, 3)[3] as i32).abs();
        assert!(
            textured_shift > flat_shift,
            "le bord côté texturé doit bouger davantage que le bord côté plat : {textured_shift} vs {flat_shift}"
        );
    }

    #[test]
    fn refine_selection_edges_is_a_noop_without_a_region_selection() {
        let mut app = app_with_layers(1);
        assert!(app.selection_mask.is_none());
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        app.refine_selection_edges(&ctx, 2);
        assert!(app.selection_mask.is_none(), "sans sélection par région, rien à affiner");
    }

    #[test]
    fn select_in_rect_add_unions_with_existing_selection() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (5.0, 5.0));
        add_stroke_at(&mut app, 0, 2, (50.0, 50.0));
        app.selection = [1].into_iter().collect();
        app.select_in_rect((40.0, 40.0), (60.0, 60.0), SelectionCombine::Add);
        assert_eq!(app.selection, [1, 2].into_iter().collect());
    }

    #[test]
    fn select_in_rect_subtract_removes_hit_elements_only() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (5.0, 5.0));
        add_stroke_at(&mut app, 0, 2, (50.0, 50.0));
        app.selection = [1, 2].into_iter().collect();
        app.select_in_rect((0.0, 0.0), (10.0, 10.0), SelectionCombine::Subtract);
        assert_eq!(app.selection, [2].into_iter().collect());
    }

    #[test]
    fn select_in_rect_intersect_keeps_only_common_elements() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (5.0, 5.0));
        add_stroke_at(&mut app, 0, 2, (50.0, 50.0));
        add_stroke_at(&mut app, 0, 3, (55.0, 55.0));
        app.selection = [1, 2].into_iter().collect();
        // Le rectangle recoupe 2 et 3, mais seul 2 était déjà sélectionné.
        app.select_in_rect((40.0, 40.0), (60.0, 60.0), SelectionCombine::Intersect);
        assert_eq!(app.selection, [2].into_iter().collect());
    }

    #[test]
    fn invert_selection_keeps_only_the_unselected_elements() {
        let mut app = app_with_layers(1);
        for id in 1..=5u64 {
            add_stroke_at(&mut app, 0, id, (id as f32, id as f32));
        }
        app.selection = [1, 2].into_iter().collect();
        app.invert_selection();
        assert_eq!(app.selection, [3, 4, 5].into_iter().collect());
    }

    #[test]
    fn align_layer_to_document_left_moves_the_whole_layer_content() {
        let mut app = app_with_layers(1);
        app.doc.size = (200, 100);
        add_stroke_at(&mut app, 0, 1, (50.0, 50.0)); // bbox (48,48)-(52,52), width 4
        add_stroke_at(&mut app, 0, 2, (80.0, 60.0)); // bbox (78,58)-(82,62)
        app.align_layer_to_document(AlignMode::Left);
        let geoms = app.active_elements_geom();
        let min_x = geoms.iter().map(|(_, (mn, _), _)| mn.0).fold(f32::INFINITY, f32::min);
        assert!(min_x.abs() < 1e-3, "le bord gauche du calque devrait toucher x=0, got {min_x}");
        // L'espacement relatif entre les deux traits doit être conservé.
        let stroke1_x = app.doc.layers[0].strokes.iter().find(|s| s.id == 1).unwrap().points[0].pos.0;
        let stroke2_x = app.doc.layers[0].strokes.iter().find(|s| s.id == 2).unwrap().points[0].pos.0;
        assert!((stroke2_x - stroke1_x - 30.0).abs() < 1e-3);
    }

    #[test]
    fn align_layer_to_document_distribute_is_a_noop_with_an_info_message() {
        let mut app = app_with_layers(1);
        add_stroke_at(&mut app, 0, 1, (50.0, 50.0));
        let before = app.doc.layers[0].strokes[0].points[0].pos;
        app.align_layer_to_document(AlignMode::DistributeH);
        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos, before);
    }

    #[test]
    fn distribute_layers_spaces_centers_evenly_keeping_endpoints_fixed() {
        let mut app = app_with_layers(4);
        // Centres en x avant répartition : 0, 10, 80, 100 — très inégal.
        add_stroke_at(&mut app, 0, 1, (0.0, 0.0));
        add_stroke_at(&mut app, 1, 2, (10.0, 0.0));
        add_stroke_at(&mut app, 2, 3, (80.0, 0.0));
        add_stroke_at(&mut app, 3, 4, (100.0, 0.0));
        app.layer_multi_select = [1, 2, 3, 4].into_iter().collect();

        app.distribute_layers(true);

        let x = |i: usize| app.doc.layers[i].strokes[0].points[0].pos.0;
        assert_eq!(x(0), 0.0, "extrémité basse inchangée");
        assert_eq!(x(3), 100.0, "extrémité haute inchangée");
        assert!((x(1) - 33.333_33).abs() < 0.01, "espacement uniforme (1/3)");
        assert!((x(2) - 66.666_66).abs() < 0.01, "espacement uniforme (2/3)");
    }

    #[test]
    fn distribute_layers_is_a_noop_with_fewer_than_three_layers() {
        let mut app = app_with_layers(2);
        add_stroke_at(&mut app, 0, 1, (0.0, 0.0));
        add_stroke_at(&mut app, 1, 2, (100.0, 0.0));
        app.layer_multi_select = [1, 2].into_iter().collect();

        app.distribute_layers(true);

        assert_eq!(app.doc.layers[0].strokes[0].points[0].pos.0, 0.0);
        assert_eq!(app.doc.layers[1].strokes[0].points[0].pos.0, 100.0);
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
        app.magic_wand((1.0, 1.0), SelectionCombine::Replace);
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
        app.magic_wand((1.0, 1.0), SelectionCombine::Replace);
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

    /// Redressement par ligne tracée (previous_audit.md #88) : vérifie
    /// le sens de rotation bout-en-bout plutôt que de le déduire de tête —
    /// une image coupée en diagonale (y=x) où l'utilisateur trace la ligne
    /// (0,0)→(20,20) doit ressortir avec cette diagonale devenue
    /// **horizontale** (deux points de la même ligne de sortie, loin l'un
    /// de l'autre en x, doivent avoir la même couleur). Si le signe de
    /// `crop_angle` était inversé, la diagonale ressortirait verticale à la
    /// place, et ce test échouerait.
    #[test]
    fn straighten_line_makes_a_diagonal_split_horizontal() {
        let mut app = app_with_layers(1);
        let (w, h) = (20u32, 20u32);
        let (above, below) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(if y < x { &above } else { &below });
            }
        }
        let im = crate::model::ImageItem::from_rgba(1, (0.0, 0.0), w, h, rgba);

        app.start_straighten_line();
        assert!(app.straighten_line_mode);
        app.update_straighten_line((0.0, 0.0));
        app.update_straighten_line((20.0, 20.0));
        app.commit_straighten_line();
        assert!(!app.straighten_line_mode, "geste unique : se désactive après le tracé");
        assert!((app.crop_angle - std::f32::consts::FRAC_PI_4).abs() < 1e-4, "45° attendus, got {}", app.crop_angle.to_degrees());

        let (nw, nh, out) = straighten_and_crop(&im, (0.0, 0.0, w as f32, h as f32), 1.0, 1.0, app.crop_angle);
        let px = |x: u32, y: u32| out[((y * nw + x) * 4) as usize..][..4].to_vec();
        // Même ligne de sortie, loin en x de part et d'autre du centre —
        // doivent tomber du même côté d'une coupure devenue horizontale.
        let y = nh / 2 + 5; // net d'un côté de l'ancienne diagonale, pas pile au centre
        assert_eq!(px(2, y), px(nw - 3, y), "la diagonale doit être devenue horizontale");
    }

    #[test]
    fn commit_straighten_line_ignores_a_click_without_real_drag() {
        let mut app = app_with_layers(1);
        app.start_straighten_line();
        app.update_straighten_line((10.0, 10.0));
        app.update_straighten_line((10.5, 10.2)); // en-deçà du seuil de 4px
        app.commit_straighten_line();
        assert_eq!(app.crop_angle, 0.0, "un clic sans glissé réel ne doit pas modifier l'angle");
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

    /// Kit de marque (previous_audit.md #92) : enregistrer capture la
    /// palette + la police système courantes, appliquer les restaure sur un
    /// état différent — même round-trip que les autres presets.
    #[test]
    fn save_and_apply_brand_kit_round_trips_palette_and_font() {
        with_temp_home(|| {
            let mut app = PaintApp::default();
            app.custom_palette = vec![[255, 0, 0], [0, 255, 0]];
            app.text_font_family = Some("Helvetica".to_string());
            app.save_brand_kit("Acme".into());
            assert_eq!(app.brand_kits.len(), 1);
            assert_eq!(app.brand_kits[0].colors, vec![[255, 0, 0], [0, 255, 0]]);
            assert_eq!(app.brand_kits[0].fonts, vec!["Helvetica".to_string()]);

            app.custom_palette.clear();
            app.text_font_family = None;
            let kit = app.brand_kits[0].clone();
            app.apply_brand_kit(&kit);
            assert_eq!(app.custom_palette, vec![[255, 0, 0], [0, 255, 0]]);
            assert_eq!(app.text_font_family, Some("Helvetica".to_string()));
        });
    }

    /// Ré-enregistrer sous le même nom écrase le kit (même règle que les
    /// autres presets) mais garde le logo déjà associé — le logo se règle
    /// séparément, il ne doit pas disparaître à chaque sauvegarde de palette.
    #[test]
    fn saving_a_brand_kit_again_keeps_its_previously_set_logo() {
        with_temp_home(|| {
            let mut app = PaintApp::default();
            app.custom_palette = vec![[10, 10, 10]];
            app.save_brand_kit("Acme".into());
            app.brand_kits[0].set_logo(2, 2, &[255u8; 2 * 2 * 4]);
            crate::i18n::save_brand_kits(&app.brand_kits);
            assert!(app.brand_kits[0].logo_png_b64.is_some());

            app.custom_palette = vec![[20, 20, 20]];
            app.save_brand_kit("Acme".into());
            assert_eq!(app.brand_kits.len(), 1, "toujours un seul kit « Acme », pas un doublon");
            assert_eq!(app.brand_kits[0].colors, vec![[20, 20, 20]], "la palette a bien été mise à jour");
            assert!(app.brand_kits[0].logo_png_b64.is_some(), "le logo ne doit pas disparaître");
        });
    }

    #[test]
    fn save_and_apply_export_profile_round_trips_the_settings() {
        with_temp_home(|| {
            let mut app = PaintApp::default();
            app.batch_export.format = crate::export::ExportFormat::Jpg;
            app.jpeg_quality = 42;
            app.batch_export.scale_half = true;
            app.batch_export.scale_2 = false;
            app.batch_export.custom_enabled = true;
            app.batch_export.custom_width = "800".into();
            app.save_export_profile("web".into());
            assert_eq!(app.export_profiles.len(), 1);

            app.batch_export.format = crate::export::ExportFormat::Png;
            app.jpeg_quality = 90;
            app.batch_export.scale_half = false;
            app.batch_export.scale_2 = true;
            app.batch_export.custom_enabled = false;
            app.batch_export.custom_width.clear();
            let profile = app.export_profiles[0].clone();
            app.apply_export_profile(&profile);
            assert_eq!(app.batch_export.format, crate::export::ExportFormat::Jpg);
            assert_eq!(app.jpeg_quality, 42);
            assert!(app.batch_export.scale_half);
            assert!(!app.batch_export.scale_2);
            assert!(app.batch_export.custom_enabled);
            assert_eq!(app.batch_export.custom_width, "800");
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

