pub mod assets;
pub mod boolean;
pub mod brush;
pub mod bucket;
pub mod eraser;
pub mod filter;
pub mod eyedropper;
pub mod guides;
pub mod hit;
pub mod pen;
pub mod shape;

pub use brush::Brush;
pub use eraser::Eraser;
pub use shape::Shape;

use crate::i18n::t;

/// Outil actuellement sélectionné dans l'UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveTool {
    Select,
    Brush,
    Eraser,
    /// Pinceau pixel (roadmap F1) : peint dans la couche raster du calque
    /// actif, avec dureté/feathering — à la différence du pinceau vectoriel.
    PixelBrush,
    /// Gomme pixel (roadmap F1) : retire de l'alpha dans la couche raster.
    PixelEraser,
    /// Tampon de clonage (roadmap P0 #5) : Alt+clic = source, glisser = peint
    /// en échantillonnant la couche raster avec un décalage constant.
    CloneStamp,
    /// Correcteur (Sprint 8.3) : même geste que le tampon de clonage, mais
    /// recale la couleur moyenne recopiée sur celle de la zone cible — pour
    /// effacer un défaut sans coller un patch visible.
    Healing,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Polygon,
    Star,
    Text,
    Pen,
    Bucket,
    Eyedropper,
    Pan,
    /// Détourage en un clic (Sprint 9.1) : clic sur le fond à retirer →
    /// flood-fill + adoucissement des bords, écrit dans le masque de calque
    /// peint (100 % local, sans modèle ni réseau — voir aussi SPRINTS.md 9.2).
    Cutout,
    /// Densité - (Sprint 11) : pinceau qui éclaircit progressivement les
    /// pixels de la couche raster sous le curseur.
    Dodge,
    /// Densité + (Sprint 11) : symétrique de Dodge, assombrit.
    Burn,
    /// Éponge — augmente la saturation locale (Sprint 11).
    Saturate,
    /// Éponge — diminue la saturation locale (Sprint 11).
    Desaturate,
    /// Flou localisé (Sprint 11) : moyenne 3×3 mélangée au pixel d'origine,
    /// répétable pour un flou plus prononcé.
    Blur,
    /// Netteté localisée (Sprint 11) : accentue l'écart à la moyenne 3×3
    /// voisine (masque flou simplifié).
    Sharpen,
    /// Estompe / doigt (Sprint 11) : pousse la couleur échantillonnée le long
    /// du glissé, mélangée à ce qui s'y trouve déjà.
    Smudge,
    /// Règle / mesure (Sprint 11) : glisser affiche distance (px) et angle,
    /// pur survol — ne modifie jamais le document.
    Measure,
    /// Miroir / symétrie (Sprint 11) : pinceau vectoriel dupliqué en miroir
    /// autour du centre du document (2/4/6/8 axes réglables).
    Symmetry,
    /// Dégradé interactif (Sprint 11) : glisser sur une forme pleine
    /// sélectionnée pose les deux points du dégradé directement sur le
    /// canevas, au lieu de valeurs par défaut via le menu Édition.
    Gradient,
}

/// Sous-mode de l'outil Sélection (Sprint 1). Détermine le geste « glisser sur
/// le vide » (rectangle vs lasso) ou le clic (baguette par couleur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectMode {
    /// Rectangle de sélection (marquee) : sélectionne les éléments recoupés.
    #[default]
    Rect,
    /// Lasso libre : sélectionne les éléments dont le centre est dans le tracé.
    Lasso,
    /// Baguette magique : clic → sélectionne les traits de couleur proche.
    Wand,
}

impl SelectMode {
    pub const ALL: [SelectMode; 3] = [SelectMode::Rect, SelectMode::Lasso, SelectMode::Wand];

    pub fn label(self) -> &'static str {
        match self {
            SelectMode::Rect => t("Rectangle", "Rectangle"),
            SelectMode::Lasso => t("Lasso", "Lasso"),
            SelectMode::Wand => t("Baguette", "Wand"),
        }
    }
}

impl ActiveTool {
    /// Forme associée si l'outil est un outil « forme ».
    pub fn as_shape(self) -> Option<Shape> {
        match self {
            ActiveTool::Line => Some(Shape::Line),
            ActiveTool::Arrow => Some(Shape::Arrow),
            ActiveTool::Rectangle => Some(Shape::Rectangle),
            ActiveTool::Ellipse => Some(Shape::Ellipse),
            ActiveTool::Polygon => Some(Shape::Polygon),
            ActiveTool::Star => Some(Shape::Star),
            _ => None,
        }
    }
}
