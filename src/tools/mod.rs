pub mod assets;
pub mod boolean;
pub mod brush;
pub mod bucket;
pub mod eraser;
pub mod filter;
pub mod eyedropper;
pub mod guides;
pub mod hit;
pub mod inpaint;
pub mod lut;
pub mod perspective;
pub mod palette;
pub mod pen;
pub mod selection_mask;
pub mod shape;
pub mod text_outline;

pub use brush::Brush;
pub use eraser::Eraser;
pub use shape::Shape;

use crate::i18n::t;

/// Outil actuellement sélectionné dans l'UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveTool {
    Select,
    Brush,
    /// Crayon (point 40 de l'audit) : outil dédié plutôt qu'un simple
    /// préréglage — dessine exactement comme `Brush` (même chemin de geste
    /// dans `handle_draw`, non spécialisé ci-dessous), la différence tient
    /// entièrement à la sélection du bouton dans la barre d'outils, qui
    /// applique automatiquement le préréglage « Crayon fin » (trait fin,
    /// bord net, peu de lissage) — cosmétique/ergonomique, pas un nouveau
    /// moteur de dessin.
    Pencil,
    Eraser,
    /// Pinceau pixel (roadmap F1) : peint dans la couche raster du calque
    /// actif, avec dureté/feathering — à la différence du pinceau vectoriel.
    PixelBrush,
    /// Aérographe (Sprint J.1) : dépose en continu tant que le clic est
    /// maintenu, même sans déplacement (contrairement au pinceau pixel qui ne
    /// dépose qu'au fil du glissé) — accumulation progressive par petites
    /// touches à opacité réduite.
    Airbrush,
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
    /// peint (100 % local, sans modèle ni réseau — voir aussi journal git (ex-SPRINTS.md) 9.2).
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

/// Mode de l'outil Miroir/Symétrie (point 54 de l'audit, complété Sprint O) :
/// la répartition radiale existante (kaléidoscope), ou une vraie réflexion
/// miroir autour d'un axe passant par le centre du document — le mode
/// « miroir » classique de Krita/Procreate, que la rotation seule ne
/// produit pas (une rotation conserve l'orientation, un miroir l'inverse).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SymmetryMode {
    /// N copies par rotation régulière autour du centre (existant).
    #[default]
    Radial,
    /// Miroir gauche/droite (réflexion autour de l'axe vertical central).
    MirrorH,
    /// Miroir haut/bas (réflexion autour de l'axe horizontal central).
    MirrorV,
    /// Miroir sur les deux axes à la fois (4 copies).
    MirrorBoth,
}

impl SymmetryMode {
    pub const ALL: [SymmetryMode; 4] =
        [SymmetryMode::Radial, SymmetryMode::MirrorH, SymmetryMode::MirrorV, SymmetryMode::MirrorBoth];

    pub fn label(self) -> &'static str {
        match self {
            SymmetryMode::Radial => t("Radial", "Radial"),
            SymmetryMode::MirrorH => t("Miroir ⇋", "Mirror ⇋"),
            SymmetryMode::MirrorV => t("Miroir ⇵", "Mirror ⇵"),
            SymmetryMode::MirrorBoth => t("Miroir ⇋⇵", "Mirror ⇋⇵"),
        }
    }
}

/// Mode de retouche destructive d'image par glissé de rectangle (Sprint 4.3/
/// 4.4) : recadrage mis à part (`crop_mode`, plus ancien et avec sa propre
/// contrainte de ratio), ces trois modes partagent le même geste — glisser un
/// rectangle sur l'image sélectionnée — mais chacun applique un traitement
/// différent au relâchement, voir `PaintApp::apply_retouch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetouchKind {
    /// Suppression d'objet content-aware : remplissage par diffusion.
    Remove,
    /// Suppression des yeux rouges : neutralise la teinte rouge dominante
    /// dans l'ellipse inscrite au rectangle.
    RedEye,
    /// Retouche peau : lissage guidé par la luminance (préserve les contours
    /// nets — yeux, sourcils, lèvres — tout en adoucissant la peau).
    SkinSmooth,
}

/// Sous-mode de l'outil Sélection (Sprint 1). Détermine le geste « glisser sur
/// le vide » (rectangle vs lasso) ou le clic (baguette par couleur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectMode {
    /// Rectangle de sélection (marquee) : sélectionne les éléments recoupés.
    #[default]
    Rect,
    /// Ellipse de sélection : sélectionne les éléments dont le centre tombe
    /// dans l'ellipse inscrite dans le rectangle glissé (Sprint 2.1).
    Ellipse,
    /// Lasso libre : sélectionne les éléments dont le centre est dans le tracé.
    Lasso,
    /// Lasso polygonal (point 52 de l'audit) : chaque clic pose un sommet,
    /// double-clic ou clic près du premier sommet referme le polygone —
    /// même test « centre dans le tracé » que le lasso libre.
    PolyLasso,
    /// Baguette magique : clic → sélectionne les traits de couleur proche.
    Wand,
}

/// Mode de combinaison d'un geste de sélection par région avec la sélection
/// existante (Sprint G.1). Correspond aux modificateurs clavier classiques
/// façon Photoshop/GIMP/Krita : Maj = Add, Alt = Subtract, Maj+Alt = Intersect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionCombine {
    /// Remplace la sélection existante par le résultat du geste.
    #[default]
    Replace,
    /// Ajoute le résultat du geste à la sélection existante (Maj).
    Add,
    /// Retire le résultat du geste de la sélection existante (Alt).
    Subtract,
    /// Ne garde que l'intersection entre sélection existante et geste (Maj+Alt).
    Intersect,
}

impl SelectionCombine {
    /// Déduit le mode à partir des modificateurs clavier courants.
    pub fn from_modifiers(shift: bool, alt: bool) -> Self {
        match (shift, alt) {
            (true, true) => SelectionCombine::Intersect,
            (false, true) => SelectionCombine::Subtract,
            (true, false) => SelectionCombine::Add,
            (false, false) => SelectionCombine::Replace,
        }
    }
}

impl SelectMode {
    pub const ALL: [SelectMode; 5] =
        [SelectMode::Rect, SelectMode::Ellipse, SelectMode::Lasso, SelectMode::PolyLasso, SelectMode::Wand];

    pub fn label(self) -> &'static str {
        match self {
            SelectMode::Rect => t("Rectangle", "Rectangle"),
            SelectMode::Ellipse => t("Ellipse", "Ellipse"),
            SelectMode::Lasso => t("Lasso", "Lasso"),
            SelectMode::PolyLasso => t("Lasso polygonal", "Polygon lasso"),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Le Crayon (point 40 de l'audit) doit dessiner exactement comme le
    /// Pinceau : `as_shape() == None` pour les deux fait tomber leur geste
    /// dans la même branche « trait à main levée » de
    /// `PaintApp::handle_draw`, plutôt que la branche « forme ».
    #[test]
    fn pencil_is_not_a_shape_tool_same_as_brush() {
        assert_eq!(ActiveTool::Pencil.as_shape(), None);
        assert_eq!(ActiveTool::Brush.as_shape(), None);
    }
}
