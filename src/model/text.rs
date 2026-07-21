//! Élément texte (roadmap #2). Posé sur un calque, indépendant des traits.
//! Texte riche (Sprint 3) : police, gras, alignement, contour. Modèle pur — le
//! mapping vers egui est dans `render::text`, partagé par les deux chemins de
//! rendu (painter live et compositeur CPU) pour rester cohérent.

use serde::{Deserialize, Serialize};

/// Famille de police. On s'appuie sur les polices intégrées d'egui (aucun
/// fichier à embarquer) : proportionnelle ou monospace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextFont {
    #[default]
    Proportional,
    Monospace,
}

impl TextFont {
    pub const ALL: [TextFont; 2] = [TextFont::Proportional, TextFont::Monospace];
    pub fn label(self) -> &'static str {
        match self {
            TextFont::Proportional => "Sans",
            TextFont::Monospace => "Mono",
        }
    }
}

/// Alignement horizontal des lignes d'un bloc de texte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "⬅",
            TextAlign::Center => "⬌",
            TextAlign::Right => "➡",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextItem {
    pub id: u64,
    /// Coin haut-gauche, en coordonnées document.
    pub pos: (f32, f32),
    pub text: String,
    pub size: f32,
    pub color: [u8; 4],
    /// Rotation (radians) autour de `pos`.
    #[serde(default)]
    pub rot: f32,
    /// Profondeur de superposition (plus grand = au-dessus).
    #[serde(default)]
    pub z: f64,
    /// Police intégrée egui (Sprint 3) : utilisée si `font_family` est `None`.
    #[serde(default)]
    pub font: TextFont,
    /// Police système (roadmap P1 #7), par nom de famille — prioritaire sur
    /// `font` si présente. `None` = comportement historique (Sans/Mono).
    #[serde(default)]
    pub font_family: Option<String>,
    /// Graisse simulée (faux-bold) — double dépôt décalé au rendu.
    #[serde(default)]
    pub bold: bool,
    /// Italique (Sprint Q, point 82) : utilise la **vraie fonte italique**
    /// de la famille système quand elle existe (chargée d'office par
    /// `FontManager::ensure_loaded` sous une famille egui dédiée) ; si la
    /// famille n'a pas d'italique — ou pour les polices intégrées Sans/Mono
    /// — le rendu reste en romain (pas de faux-italique par cisaillement :
    /// egui ne sait pas incliner un galley, limite documentée).
    #[serde(default)]
    pub italic: bool,
    /// Interligne (Sprint Q, point 83), en multiple de `size` — 1.25 était
    /// la valeur codée en dur avant d'être réglable.
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// Espacement additionnel entre caractères (crénage global, Sprint Q,
    /// point 83), en unités document. 0 = métriques naturelles de la police.
    #[serde(default)]
    pub letter_spacing: f32,
    /// Alignement des lignes (Sprint 3).
    #[serde(default)]
    pub align: TextAlign,
    /// Épaisseur du contour (0 = aucun), en unités document.
    #[serde(default)]
    pub outline_w: f32,
    /// Couleur du contour.
    #[serde(default = "default_outline_color")]
    pub outline_color: [u8; 4],
    /// Ombre portée (Sprint 7.1) : `None` = aucune. Simple décalage coloré
    /// (pas de flou — le texte est déjà composé de traits fins, un flou par
    /// glyphe coûterait cher pour un gain visuel marginal à la taille où le
    /// texte est habituellement utilisé).
    #[serde(default)]
    pub shadow: Option<TextShadow>,
    /// Texte sur courbe (Sprint 7.1) : `None` = texte droit habituel (le
    /// chemin actuel, inchangé). `Some` = `pos` devient le **centre** du
    /// cercle plutôt que le coin haut-gauche, et chaque caractère est posé
    /// le long de l'arc plutôt qu'en ligne droite.
    #[serde(default)]
    pub arc: Option<TextArc>,
}

fn default_outline_color() -> [u8; 4] {
    [255, 255, 255, 255]
}

fn default_line_height() -> f32 {
    1.25
}

/// Ombre portée d'un texte (Sprint 7.1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextShadow {
    pub offset: (f32, f32),
    pub color: [u8; 4],
}

impl Default for TextShadow {
    fn default() -> Self {
        Self { offset: (2.0, 2.0), color: [0, 0, 0, 160] }
    }
}

/// Texte sur courbe (Sprint 7.1) : arc de cercle centré sur `TextItem::pos`.
/// Espacement des caractères supposé uniforme (`size * 0.6` par caractère)
/// plutôt que mesuré glyphe par glyphe — délibéré : évite de dépendre des
/// métriques exactes de la police (différentes entre le rendu live egui et
/// le compositeur CPU), au prix d'un espacement légèrement approximatif,
/// habituellement même préféré pour du texte de logo/titre.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextArc {
    pub radius: f32,
    /// Angle du premier caractère, en degrés (0 = droite, sens horaire).
    pub start_angle_deg: f32,
    /// `true` : texte à l'envers, lisible depuis l'intérieur du cercle
    /// (typiquement le bas d'un cercle) — inverse le sens de parcours et
    /// l'orientation de chaque caractère.
    pub flip: bool,
}

impl Default for TextArc {
    fn default() -> Self {
        Self { radius: 120.0, start_angle_deg: -90.0, flip: false }
    }
}

impl TextArc {
    /// Angle (radians) de chaque caractère de `text`, centré autour de
    /// `start_angle_deg` (le milieu du texte tombe sur l'angle de départ).
    /// `extra_advance` : espacement additionnel entre caractères (crénage,
    /// Sprint Q point 83), en unités document.
    pub fn char_angles(&self, text: &str, char_size: f32, extra_advance: f32) -> Vec<f32> {
        let n = text.chars().count();
        if n == 0 {
            return Vec::new();
        }
        let advance = (char_size * 0.6 + extra_advance).max(0.1);
        let dir = if self.flip { -1.0 } else { 1.0 };
        let total = advance * (n.saturating_sub(1)) as f32;
        let radius = self.radius.max(1.0);
        (0..n)
            .map(|i| {
                let x = i as f32 * advance - total * 0.5;
                self.start_angle_deg.to_radians() + dir * x / radius
            })
            .collect()
    }
}

impl TextItem {
    pub fn new(id: u64, pos: (f32, f32), size: f32, color: [u8; 4]) -> Self {
        Self {
            id,
            pos,
            text: String::new(),
            size,
            color,
            rot: 0.0,
            z: 0.0,
            font: TextFont::default(),
            font_family: None,
            bold: false,
            italic: false,
            line_height: default_line_height(),
            letter_spacing: 0.0,
            align: TextAlign::default(),
            outline_w: 0.0,
            outline_color: default_outline_color(),
            shadow: None,
            arc: None,
        }
    }

    /// Boîte englobante approximative (sans mesure de police) — suffit au
    /// test de sélection et au cadre. Tient compte du contour.
    pub fn approx_bounds(&self) -> ((f32, f32), (f32, f32)) {
        if let Some(arc) = &self.arc {
            // `pos` est le centre du cercle en mode arc : boîte englobante du
            // cercle entier (sur-approximation simple mais toujours correcte,
            // le texte ne dépasse jamais son propre rayon).
            let r = arc.radius.max(1.0) + self.size * 0.5;
            return ((self.pos.0 - r, self.pos.1 - r), (self.pos.0 + r, self.pos.1 + r));
        }
        let lines = self.text.lines().count().max(1) as f32;
        let cols = self.text.lines().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
        // Le monospace est plus large par caractère.
        let cw = if self.font == TextFont::Monospace { 0.62 } else { 0.55 };
        let w = cols * self.size * cw + (cols - 1.0).max(0.0) * self.letter_spacing.max(0.0);
        let h = lines * self.size * self.line_height.max(0.5);
        let pad = self.outline_w.max(0.0);
        (
            (self.pos.0 - pad, self.pos.1 - pad),
            (self.pos.0 + w.max(self.size) + pad, self.pos.1 + h + pad),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sprint Q (point 83) : l'interligne et l'espacement réglables se
    /// reflètent dans la boîte englobante approximative (sélection/cadre).
    #[test]
    fn approx_bounds_follow_line_height_and_letter_spacing() {
        let mut t = TextItem::new(1, (0.0, 0.0), 20.0, [0, 0, 0, 255]);
        t.text = "AB\nCD".to_string();
        let (_, mx_default) = t.approx_bounds();
        t.line_height = 2.5;
        t.letter_spacing = 10.0;
        let (_, mx_spaced) = t.approx_bounds();
        assert!(mx_spaced.1 > mx_default.1, "interligne plus grand → boîte plus haute");
        assert!(mx_spaced.0 > mx_default.0, "espacement plus grand → boîte plus large");
    }

    #[test]
    fn char_angles_centers_the_text_on_the_start_angle() {
        let arc = TextArc { radius: 100.0, start_angle_deg: 0.0, flip: false };
        let angles = arc.char_angles("ABC", 20.0, 0.0);
        assert_eq!(angles.len(), 3);
        // Nombre impair de caractères : le caractère du milieu tombe pile sur
        // l'angle de départ.
        assert!((angles[1] - 0.0_f32.to_radians()).abs() < 1e-5);
        // Symétrique de part et d'autre.
        assert!((angles[0] + angles[2]).abs() < 1e-5);
        assert!(angles[0] < angles[1] && angles[1] < angles[2]);
    }

    #[test]
    fn char_angles_flip_reverses_direction() {
        let normal = TextArc { radius: 100.0, start_angle_deg: 0.0, flip: false };
        let flipped = TextArc { radius: 100.0, start_angle_deg: 0.0, flip: true };
        let a = normal.char_angles("AB", 20.0, 0.0);
        let b = flipped.char_angles("AB", 20.0, 0.0);
        assert!(a[0] < a[1]); // sens normal : croissant
        assert!(b[0] > b[1]); // inversé : décroissant
    }

    #[test]
    fn char_angles_empty_text_is_empty() {
        let arc = TextArc::default();
        assert!(arc.char_angles("", 20.0, 0.0).is_empty());
    }

    #[test]
    fn arc_bounds_are_a_square_around_the_circle() {
        let mut t = TextItem::new(1, (50.0, 50.0), 20.0, [0, 0, 0, 255]);
        t.arc = Some(TextArc { radius: 100.0, start_angle_deg: 0.0, flip: false });
        let (mn, mx) = t.approx_bounds();
        assert!(mn.0 < 50.0 - 100.0 || (mn.0 - (50.0 - 100.0)).abs() < 20.0);
        assert!(mx.0 > 50.0 + 100.0 || (mx.0 - (50.0 + 100.0)).abs() < 20.0);
        assert!(mx.1 - mn.1 > 190.0); // au moins ~2×radius de haut
    }
}
