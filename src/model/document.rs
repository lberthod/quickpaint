//! Document = couches + traits. Modèle pur, ne sait rien du rendu (section 1).

use super::image::ImageItem;
use super::raster::{self, RasterLayer};
use super::stroke::Stroke;
use super::text::TextItem;
use crate::i18n::t;
use serde::{Deserialize, Serialize};

/// Mode de fusion d'un calque (roadmap #8 ; étendu Sprint P, point 23 :
/// 6 modes supplémentaires, tous supportés nativement par tiny-skia).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    ColorDodge,
    ColorBurn,
}

impl BlendMode {
    pub const ALL: [BlendMode; 12] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => t("Normal", "Normal"),
            BlendMode::Multiply => t("Produit", "Multiply"),
            BlendMode::Screen => t("Écran", "Screen"),
            BlendMode::Overlay => t("Incrustation", "Overlay"),
            BlendMode::Darken => t("Obscurcir", "Darken"),
            BlendMode::Lighten => t("Éclaircir", "Lighten"),
            // Noms français de Photoshop, repères connus des utilisateurs.
            BlendMode::SoftLight => t("Lumière tamisée", "Soft light"),
            BlendMode::HardLight => t("Lumière crue", "Hard light"),
            BlendMode::Difference => t("Différence", "Difference"),
            BlendMode::Exclusion => t("Exclusion", "Exclusion"),
            BlendMode::ColorDodge => t("Densité couleur −", "Color dodge"),
            BlendMode::ColorBurn => t("Densité couleur +", "Color burn"),
        }
    }
}

/// Guide manuel (Sprint R, point 95) : ligne repère tirée depuis une règle,
/// persistée avec le document (contrairement aux guides intelligents,
/// calculés à la volée pendant un glissé). `vertical` = ligne verticale à
/// `pos` (coordonnée X document) ; sinon ligne horizontale à `pos` (Y).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManualGuide {
    pub vertical: bool,
    pub pos: f32,
}

/// Remplissage d'un calque de remplissage (Sprint I.1), sur le même modèle
/// que les calques d'ajustement : pas de contenu propre (traits/textes/
/// images), peint tout son rectangle (borné par le masque/écrêtage s'il y en
/// a un) — réutilise le type `Gradient` déjà existant pour les traits pleins.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FillKind {
    Solid([u8; 4]),
    Linear(crate::model::Gradient),
    Radial(crate::model::Gradient),
}

impl FillKind {
    pub fn label(&self) -> &'static str {
        match self {
            FillKind::Solid(_) => t("Uni", "Solid"),
            FillKind::Linear(_) => t("Dégradé linéaire", "Linear gradient"),
            FillKind::Radial(_) => t("Dégradé radial", "Radial gradient"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    /// Identifiant stable : l'historique référence les calques par id, pas par
    /// index, pour rester cohérent après suppression / réordonnancement.
    #[serde(default)]
    pub id: u64,
    /// Nom affiché dans le panneau de calques.
    pub name: String,
    pub visible: bool,
    /// Opacité du calque (0 = transparent, 1 = opaque). Non destructif.
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Mode de fusion avec les calques inférieurs (roadmap #8).
    #[serde(default)]
    pub blend: BlendMode,
    /// Masque d'écrêtage (Sprint 4) : si vrai, le calque n'est visible qu'à
    /// travers l'alpha du calque non écrêté situé immédiatement en dessous.
    #[serde(default)]
    pub clip: bool,
    /// Groupe (dossier) auquel appartient le calque, le cas échéant.
    #[serde(default)]
    pub group: Option<String>,
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub texts: Vec<TextItem>,
    #[serde(default)]
    pub images: Vec<ImageItem>,
    /// Contenu peint (pinceau/gomme pixel, roadmap F1). Rendu **sous** les
    /// éléments vectoriels du calque (le pinceau pixel peint le « fond » du
    /// calque, comme dans GIMP/Photoshop).
    #[serde(skip)]
    pub raster: RasterLayer,
    /// Persistance paresseuse du raster : PNG base64 borné à sa boîte
    /// englobante + origine, reconstruit au chargement (cf. `ImageItem`).
    #[serde(default)]
    raster_png: String,
    #[serde(default)]
    raster_origin: (i32, i32),
    /// Calque d'ajustement non destructif (roadmap F3) : si présent, ce
    /// calque ne porte aucun contenu propre — il applique le filtre en
    /// direct au rendu déjà composé des calques du dessous (tout, ou
    /// seulement le calque juste en dessous si `clip` est actif),
    /// réversible et re-réglable à tout moment (≠ filtre destructif).
    #[serde(default)]
    pub adjustment: Option<crate::tools::filter::Adjustment>,
    /// Calque de remplissage (Sprint I.1) : aucun contenu propre, peint tout
    /// son rectangle (uni/dégradé) — voir [`FillKind`].
    #[serde(default)]
    pub fill: Option<FillKind>,
    /// Masque de calque peint (roadmap P2 #14) : réutilise le moteur raster
    /// (F1) comme surface peignable en niveaux de gris. Un pixel jamais peint
    /// est **visible** par défaut (comme un masque tout juste créé, blanc) ;
    /// peindre en noir masque, en blanc redonne la visibilité — convention
    /// Photoshop/GIMP. Multiplie l'alpha du calque au compositing.
    #[serde(skip)]
    pub mask: Option<RasterLayer>,
    #[serde(default)]
    mask_png: String,
    #[serde(default)]
    mask_origin: (i32, i32),
    /// Styles de calque non destructifs (Sprint 6.1) : appliqués au rendu
    /// déjà aplati du calque (traits + textes + images + raster), avant
    /// écrêtage/fusion avec les calques du dessous — voir
    /// `render::compositor::apply_layer_styles`.
    #[serde(default)]
    pub styles: Vec<LayerStyle>,
    /// Verrouillage (audit_sprint_xx.md B.1) : bloque toute peinture/édition
    /// de contenu sur ce calque (traits, raster, texte, images,
    /// déplacement/transformation) tant qu'il est vrai. La visibilité,
    /// l'opacité et le réordonnancement restent autorisés — un calque
    /// verrouillé sert de référence qu'on ne veut pas modifier par erreur,
    /// pas un calque figé qu'on ne peut plus du tout gérer.
    #[serde(default)]
    pub locked: bool,
    /// Verrouillage granulaire — position (point 28 de l'audit) : en plus du
    /// verrou global ci-dessus (`locked`, tout ou rien), bloque
    /// spécifiquement le glisser-déplacer d'éléments sélectionnés
    /// (`PaintApp::push_move`) même quand `locked` est faux — pour figer la
    /// position d'un calque sans empêcher d'y peindre/éditer le contenu.
    /// N'affecte pas le réordonnancement des calques (z-order de la pile),
    /// périmètre volontairement limité au déplacement du contenu.
    #[serde(default)]
    pub lock_position: bool,
    /// Verrouillage granulaire — transparence (point 28 de l'audit) :
    /// protège l'alpha existant du contenu peint (pinceau/gomme pixel,
    /// aérographe) — peindre ne peut plus rendre opaque un pixel
    /// transparent, ni la gomme en rendre un transparent ; la couleur des
    /// pixels déjà opaques reste modifiable. Indépendant de `locked`/
    /// `lock_position` ; ne s'applique qu'au contenu (pas au masque de
    /// calque peint, qui n'a pas de notion de « transparence » comparable).
    #[serde(default)]
    pub lock_alpha: bool,
    /// Code couleur (Sprint I.5) : étiquette visuelle uniquement, aucun
    /// effet sur le rendu — sert à repérer un groupe de calques d'un coup
    /// d'œil dans une pile chargée, façon Photoshop/Figma.
    #[serde(default)]
    pub color_tag: Option<[u8; 3]>,
}

fn default_opacity() -> f32 {
    1.0
}

/// Style de calque non destructif (Sprint 6.1), à la manière de Photoshop —
/// dérivé du rendu du calque (son alpha), jamais des pixels d'origine.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayerStyle {
    /// Ombre portée : copie décalée et floutée de l'alpha du calque, colorée,
    /// dessinée derrière son contenu.
    DropShadow { color: [u8; 4], offset: (f32, f32), blur: f32 },
    /// Contour : anneau de couleur juste à l'extérieur du contenu du calque.
    Stroke { color: [u8; 4], width: f32 },
    /// Lueur : externe (`inner = false`, autour du contenu, comme l'ombre
    /// mais non décalée) ou interne (`inner = true`, un halo vers l'intérieur
    /// depuis les bords).
    Glow { color: [u8; 4], blur: f32, inner: bool },
}

impl LayerStyle {
    pub fn label(self) -> &'static str {
        match self {
            LayerStyle::DropShadow { .. } => t("Ombre portée", "Drop shadow"),
            LayerStyle::Stroke { .. } => t("Contour", "Stroke"),
            LayerStyle::Glow { inner: false, .. } => t("Lueur externe", "Outer glow"),
            LayerStyle::Glow { inner: true, .. } => t("Lueur interne", "Inner glow"),
        }
    }

    pub fn default_drop_shadow() -> Self {
        LayerStyle::DropShadow { color: [0, 0, 0, 160], offset: (4.0, 4.0), blur: 4.0 }
    }

    pub fn default_stroke() -> Self {
        LayerStyle::Stroke { color: [255, 255, 255, 255], width: 3.0 }
    }

    pub fn default_outer_glow() -> Self {
        LayerStyle::Glow { color: [255, 220, 100, 200], blur: 8.0, inner: false }
    }

    pub fn default_inner_glow() -> Self {
        LayerStyle::Glow { color: [255, 255, 255, 200], blur: 8.0, inner: true }
    }
}

/// Référence d'un élément d'un calque (index dans le vec de son type).
#[derive(Clone, Copy, Debug)]
pub enum ElemRef {
    Stroke(usize),
    Image(usize),
    Text(usize),
}

impl Layer {
    /// Éléments du calque triés par profondeur `z` (du dessous au-dessus).
    /// Tri stable : à `z` égal, l'ordre reste traits → images → textes.
    pub fn z_order(&self) -> Vec<ElemRef> {
        let mut v: Vec<(f64, ElemRef)> = Vec::with_capacity(
            self.strokes.len() + self.images.len() + self.texts.len(),
        );
        for (i, s) in self.strokes.iter().enumerate() {
            v.push((s.z, ElemRef::Stroke(i)));
        }
        for (i, im) in self.images.iter().enumerate() {
            v.push((im.z, ElemRef::Image(i)));
        }
        for (i, t) in self.texts.iter().enumerate() {
            v.push((t.z, ElemRef::Text(i)));
        }
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(_, r)| r).collect()
    }

    /// (id, z) de tous les éléments.
    pub fn each_z(&self) -> Vec<(u64, f64)> {
        let mut v = Vec::new();
        v.extend(self.strokes.iter().map(|s| (s.id, s.z)));
        v.extend(self.images.iter().map(|im| (im.id, im.z)));
        v.extend(self.texts.iter().map(|t| (t.id, t.z)));
        v
    }

    /// Affecte la profondeur d'un élément par id.
    pub fn set_elem_z(&mut self, id: u64, z: f64) {
        if let Some(s) = self.strokes.iter_mut().find(|s| s.id == id) {
            s.z = z;
        } else if let Some(im) = self.images.iter_mut().find(|im| im.id == id) {
            im.z = z;
        } else if let Some(t) = self.texts.iter_mut().find(|t| t.id == id) {
            t.z = z;
        }
    }

    /// Octets approximatifs retenus par le contenu décodé du calque (tuiles
    /// peintes, masque peint, pixels d'image, points de trait) — sert au
    /// plafond mémoire de l'historique (`history.rs`,
    /// plan_implementation.md étape 6).
    pub fn approx_bytes(&self) -> usize {
        self.raster.approx_bytes()
            + self.mask.as_ref().map(crate::model::raster::RasterLayer::approx_bytes).unwrap_or(0)
            + self.images.iter().map(|im| im.rgba.len()).sum::<usize>()
            + self.strokes.iter().map(|s| s.points.len() * std::mem::size_of::<crate::model::stroke::StrokePoint>()).sum::<usize>()
            + self.texts.iter().map(|t| t.text.len()).sum::<usize>()
    }
}

impl Layer {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            clip: false,
            group: None,
            strokes: Vec::new(),
            texts: Vec::new(),
            images: Vec::new(),
            raster: RasterLayer::default(),
            raster_png: String::new(),
            raster_origin: (0, 0),
            adjustment: None,
            fill: None,
            mask: None,
            mask_png: String::new(),
            mask_origin: (0, 0),
            styles: Vec::new(),
            locked: false,
            lock_position: false,
            lock_alpha: false,
            color_tag: None,
        }
    }

    /// Calque d'ajustement (roadmap F3) : aucun contenu propre, applique
    /// `adjustment` en direct au rendu des calques du dessous.
    pub fn new_adjustment(id: u64, name: impl Into<String>, adjustment: crate::tools::filter::Adjustment) -> Self {
        Self { adjustment: Some(adjustment), ..Self::new(id, name) }
    }

    /// Calque de remplissage (Sprint I.1) : aucun contenu propre, peint tout
    /// son rectangle avec `fill` au rendu (voir `render::compositor::rebuild`).
    pub fn new_fill(id: u64, name: impl Into<String>, fill: FillKind) -> Self {
        Self { fill: Some(fill), ..Self::new(id, name) }
    }

    /// Encode le raster en PNG base64 si nécessaire (avant sérialisation).
    pub fn ensure_raster_encoded(&mut self) {
        if self.raster.is_empty() {
            self.raster_png.clear();
            return;
        }
        let enc = raster::encode(&self.raster);
        self.raster_png = enc.png_b64;
        self.raster_origin = enc.origin;
    }

    /// Reconstruit le raster depuis son PNG base64 (après chargement projet).
    pub fn decode_raster(&mut self) {
        if self.raster_png.is_empty() {
            return;
        }
        self.raster = raster::decode(&raster::RasterEncoded {
            png_b64: std::mem::take(&mut self.raster_png),
            origin: self.raster_origin,
        });
    }

    /// Encode le masque en PNG base64 si nécessaire (avant sérialisation).
    pub fn ensure_mask_encoded(&mut self) {
        let Some(mask) = &self.mask else {
            self.mask_png.clear();
            return;
        };
        let enc = raster::encode(mask);
        self.mask_png = enc.png_b64;
        self.mask_origin = enc.origin;
    }

    /// Reconstruit le masque depuis son PNG base64 (après chargement projet).
    pub fn decode_mask(&mut self) {
        if self.mask_png.is_empty() {
            return;
        }
        self.mask = Some(raster::decode(&raster::RasterEncoded {
            png_b64: std::mem::take(&mut self.mask_png),
            origin: self.mask_origin,
        }));
    }

    /// Ajoute un masque vide (tout visible tant que rien n'est peint dessus).
    pub fn add_mask(&mut self) {
        self.mask = Some(RasterLayer::default());
    }

    pub fn remove_mask(&mut self) {
        self.mask = None;
        self.mask_png.clear();
    }
}

/// Version du format de fichier `.json` (previous_audit.md (ex-ANALYSE) §8.2/§9). Stampée à
/// `CURRENT_FORMAT_VERSION` à **chaque sauvegarde** (pas seulement à la
/// création) : un projet rouvert puis resauvegardé reflète toujours la
/// version du binaire qui l'a écrit. Un projet dont la version dépasse celle
/// que ce binaire connaît est refusé explicitement à l'ouverture plutôt que
/// mal interprété en silence (cf. `project::open_dialog`). Une valeur absente
/// (anciens projets, avant l'introduction de ce champ) vaut `1`.
pub const CURRENT_FORMAT_VERSION: u32 = 1;

fn default_format_version() -> u32 {
    1
}

/// Sélection nommée (Sprint 1.2) : ensemble d'ids d'éléments (traits, textes,
/// images) d'un calque donné, sauvegardée dans le document pour être
/// rechargée plus tard sans avoir à retracer la même zone. Un id disparu
/// depuis (élément supprimé) est simplement ignoré au chargement — pas
/// d'erreur, la sélection restaurée contient juste ce qu'il en reste.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedSelection {
    pub name: String,
    pub layer: u64,
    pub ids: Vec<u64>,
}

/// Une frame d'animation (Sprint L.6) : un instantané complet de la pile de
/// calques, pas un delta ni une référence — plus simple à raisonner, à
/// sérialiser et à annuler/rétablir qu'un système de keyframes par calque,
/// au prix d'un fichier projet plus lourd si beaucoup de frames se
/// ressemblent (compromis assumé, Sprint L.6).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationFrame {
    pub layers: Vec<Layer>,
    /// Délai d'affichage de cette frame, en millisecondes (converti en
    /// centièmes de seconde à l'export GIF, l'unité native du format).
    #[serde(default = "default_frame_delay_ms")]
    pub delay_ms: u32,
}

fn default_frame_delay_ms() -> u32 {
    100
}

impl AnimationFrame {
    pub fn new(layers: Vec<Layer>) -> Self {
        Self { layers, delay_ms: default_frame_delay_ms() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    pub size: (u32, u32),
    /// Prochain id de calque à attribuer.
    #[serde(default)]
    pub next_layer_id: u64,
    /// Prochaine profondeur à attribuer (compteur monotone, superposition).
    #[serde(default)]
    pub next_z: f64,
    /// Version du format `.json` — voir `CURRENT_FORMAT_VERSION`.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// Sélections nommées enregistrées par l'utilisateur (Sprint 1.2).
    #[serde(default)]
    pub named_selections: Vec<NamedSelection>,
    /// Guides manuels (Sprint R, point 95), persistés avec le document.
    #[serde(default)]
    pub guides: Vec<ManualGuide>,
    /// Frames d'animation (Sprint L.6). Vide = document statique, le
    /// comportement historique inchangé — `layers` est alors la seule
    /// image. Non vide : `layers` reflète le contenu de la frame active
    /// (`active_frame`), les *autres* frames vivent ici ; la frame active
    /// elle-même n'a pas de copie à jour dans `frames` tant qu'on n'a pas
    /// changé de frame ou exporté (voir `PaintApp::sync_active_frame`).
    #[serde(default)]
    pub frames: Vec<AnimationFrame>,
    /// Index de la frame actuellement éditée dans `frames`.
    #[serde(default)]
    pub active_frame: usize,
}

impl Document {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            layers: vec![Layer::new(1, t("Calque 1", "Layer 1"))],
            active_layer: 0,
            size,
            next_layer_id: 2,
            next_z: 1.0,
            format_version: CURRENT_FORMAT_VERSION,
            named_selections: Vec::new(),
            guides: Vec::new(),
            frames: Vec::new(),
            active_frame: 0,
        }
    }

    /// `true` si le document a plus d'une frame d'animation.
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Octets approximatifs retenus par tout le contenu décodé du document —
    /// `layers` (frame active) ET `frames` (les autres frames d'animation,
    /// sinon un document animé multi-Go passerait inaperçu). Sert au
    /// plafond mémoire de l'historique (`history.rs`,
    /// plan_implementation.md étape 6).
    pub fn approx_bytes(&self) -> usize {
        self.layers.iter().map(Layer::approx_bytes).sum::<usize>()
            + self.frames.iter().flat_map(|f| f.layers.iter()).map(Layer::approx_bytes).sum::<usize>()
    }

    /// Id du calque actif.
    pub fn active_id(&self) -> u64 {
        self.layers[self.active_layer].id
    }

    /// Met à l'échelle tout le contenu (traits, textes, images) depuis
    /// l'origine du document. Les épaisseurs et tailles de texte suivent le
    /// facteur uniforme √(sx·sy) — redimensionnement d'image façon PhotoFiltre.
    pub fn scale_content(&mut self, sx: f32, sy: f32) {
        let uni = (sx.abs() * sy.abs()).sqrt();
        for layer in &mut self.layers {
            for s in &mut layer.strokes {
                for p in &mut s.points {
                    p.pos.0 *= sx;
                    p.pos.1 *= sy;
                    p.width *= uni;
                }
                s.base_width *= uni;
            }
            for t in &mut layer.texts {
                t.pos.0 *= sx;
                t.pos.1 *= sy;
                t.size = (t.size * uni).max(1.0);
                t.outline_w *= uni;
            }
            for im in &mut layer.images {
                im.pos.0 *= sx;
                im.pos.1 *= sy;
                im.size.0 *= sx;
                im.size.1 *= sy;
            }
            if !layer.raster.is_empty() {
                layer.raster = layer.raster.scaled(sx, sy);
            }
        }
    }

    /// Translate tout le contenu (taille du canevas avec ancrage : le document
    /// change de taille, le contenu est décalé, jamais déformé).
    pub fn translate_content(&mut self, dx: f32, dy: f32) {
        for layer in &mut self.layers {
            for s in &mut layer.strokes {
                for p in &mut s.points {
                    p.pos.0 += dx;
                    p.pos.1 += dy;
                }
            }
            for t in &mut layer.texts {
                t.pos.0 += dx;
                t.pos.1 += dy;
            }
            for im in &mut layer.images {
                im.pos.0 += dx;
                im.pos.1 += dy;
            }
            if !layer.raster.is_empty() {
                layer.raster = layer.raster.translated(dx.round() as i32, dy.round() as i32);
            }
        }
    }

    /// Retourne tout le contenu en miroir (point 66 de l'audit : Retourner
    /// horizontalement/verticalement), dans les bornes actuelles du document.
    /// Traits, dégradés, ancres de plume et pixels (raster, masques, images)
    /// sont réellement inversés ; les textes ne sont **pas** mis en miroir
    /// glyphe par glyphe (ils deviendraient illisibles) : leur boîte est
    /// repositionnée à l'emplacement miroir et leur rotation inversée, le
    /// contenu reste lisible — même logique que le déplacement d'ancre de
    /// `Command::Scale` pour les textes.
    pub fn flip_content(&mut self, horizontal: bool) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let fp = |p: (f32, f32)| if horizontal { (w - p.0, p.1) } else { (p.0, h - p.1) };
        let flip_gradient = |g: &mut crate::model::Gradient| {
            g.from = fp(g.from);
            g.to = fp(g.to);
        };
        for layer in &mut self.layers {
            for s in &mut layer.strokes {
                for p in &mut s.points {
                    p.pos = fp(p.pos);
                }
                if let Some(g) = &mut s.gradient {
                    flip_gradient(g);
                }
                if let Some(path) = &mut s.anchors {
                    for a in &mut path.anchors {
                        a.pos = fp(a.pos);
                        a.h_in = fp(a.h_in);
                        a.h_out = fp(a.h_out);
                    }
                }
            }
            for t in &mut layer.texts {
                let (mn, mx) = t.approx_bounds();
                if horizontal {
                    t.pos.0 += w - (mx.0 + mn.0);
                } else {
                    t.pos.1 += h - (mx.1 + mn.1);
                }
                t.rot = -t.rot;
            }
            for im in &mut layer.images {
                if horizontal {
                    im.pos.0 = w - (im.pos.0 + im.size.0);
                } else {
                    im.pos.1 = h - (im.pos.1 + im.size.1);
                }
                im.rot = -im.rot;
                im.flip_pixels(horizontal);
            }
            match &mut layer.fill {
                Some(FillKind::Linear(g)) | Some(FillKind::Radial(g)) => flip_gradient(g),
                _ => {}
            }
            if !layer.raster.is_empty() {
                layer.raster = layer.raster.flipped(horizontal, self.size.0, self.size.1);
            }
            if let Some(mask) = &layer.mask {
                if !mask.is_empty() {
                    layer.mask = Some(mask.flipped(horizontal, self.size.0, self.size.1));
                }
            }
        }
    }

    /// Répare les id après chargement d'un ancien projet (id manquants /
    /// dupliqués) : réattribue des id uniques et recalcule le compteur.
    pub fn normalize_ids(&mut self) {
        for (i, layer) in self.layers.iter_mut().enumerate() {
            layer.id = i as u64 + 1;
        }
        self.next_layer_id = self.layers.len() as u64 + 1;
        if self.active_layer >= self.layers.len() {
            self.active_layer = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Retourner horizontalement (point 66) : un point à x est envoyé à
    /// w − x, le raster peint suit, et deux flips successifs redonnent
    /// exactement le contenu d'origine (involution).
    #[test]
    fn flip_content_mirrors_strokes_and_raster_and_is_involutive() {
        let mut doc = Document::new((100, 50));
        let mut s = crate::model::Stroke::new([255, 0, 0, 255], 2.0, crate::model::Tool::Brush);
        s.points = vec![crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 }];
        doc.layers[0].strokes.push(s);
        doc.layers[0].raster.set_pixel(10, 20, [1, 2, 3, 255]);

        doc.flip_content(true);
        assert_eq!(doc.layers[0].strokes[0].points[0].pos, (90.0, 20.0));
        assert_eq!(doc.layers[0].raster.get_pixel(89, 20), [1, 2, 3, 255], "le pixel raster suit le miroir (centre de pixel : x → w−1−x)");

        doc.flip_content(true);
        assert_eq!(doc.layers[0].strokes[0].points[0].pos, (10.0, 20.0));
        assert_eq!(doc.layers[0].raster.get_pixel(10, 20), [1, 2, 3, 255]);
    }

    /// Retourner verticalement : même garantie sur l'axe Y, et l'image
    /// voit ses pixels réellement inversés (pas seulement son ancre).
    #[test]
    fn flip_content_vertical_flips_image_pixels() {
        let mut doc = Document::new((40, 40));
        // Image 1×2 : pixel haut rouge, pixel bas bleu.
        let rgba = vec![255, 0, 0, 255, 0, 0, 255, 255];
        let im = crate::model::ImageItem::from_rgba(1, (5.0, 5.0), 1, 2, rgba);
        doc.layers[0].images.push(im);

        doc.flip_content(false);
        let im = &doc.layers[0].images[0];
        assert_eq!(im.pos, (5.0, 33.0), "ancre repositionnée en miroir : y = h − (pos.y + size.y)");
        assert_eq!(&im.rgba[0..4], &[0, 0, 255, 255], "le pixel bleu (bas) est passé en haut");
        assert!(im.png_b64.is_empty(), "le PNG persisté est invalidé, ré-encodé à la sauvegarde");
    }

    #[test]
    fn doc_has_one_layer_at_start() {
        let doc = Document::new((800, 600));
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.active_layer, 0);
    }

    #[test]
    fn scale_content_scales_positions_and_widths() {
        let mut doc = Document::new((100, 100));
        let mut s = crate::model::Stroke::new([0, 0, 0, 255], 4.0, crate::model::Tool::Brush);
        s.points.push(crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 });
        doc.layers[0].strokes.push(s);
        doc.scale_content(2.0, 2.0);
        let p = doc.layers[0].strokes[0].points[0];
        assert_eq!(p.pos, (20.0, 40.0));
        assert!((p.width - 4.0).abs() < 1e-5);
        assert!((doc.layers[0].strokes[0].base_width - 8.0).abs() < 1e-5);
    }

    #[test]
    fn translate_content_offsets_everything() {
        let mut doc = Document::new((100, 100));
        let mut s = crate::model::Stroke::new([0, 0, 0, 255], 4.0, crate::model::Tool::Brush);
        s.points.push(crate::model::StrokePoint { pos: (10.0, 20.0), width: 2.0 });
        doc.layers[0].strokes.push(s);
        doc.translate_content(5.0, -5.0);
        assert_eq!(doc.layers[0].strokes[0].points[0].pos, (15.0, 15.0));
    }
}
