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
