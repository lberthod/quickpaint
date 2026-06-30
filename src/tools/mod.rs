pub mod brush;
pub mod bucket;
pub mod eraser;
pub mod filter;
pub mod eyedropper;
pub mod hit;
pub mod pen;
pub mod shape;

pub use brush::Brush;
pub use eraser::Eraser;
pub use shape::Shape;

/// Outil actuellement sélectionné dans l'UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveTool {
    Select,
    Brush,
    Eraser,
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
