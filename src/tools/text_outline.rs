//! Extraction de contours de glyphes pour « Texte → tracés » (roadmap
//! previous_audit.md #64) : convertit une chaîne en polylignes fermées
//! (une ou plusieurs par caractère — une lettre comme « O » a un contour
//! extérieur ET un contour intérieur), positionnées le long de la ligne de
//! base, en unités document (pas en unités de police).
//!
//! **Limite assumée** : chaque contour devient un `Stroke` non rempli
//! (`fill = false`). Un contour rempli individuellement ne pose pas de
//! problème pour une lettre sans trou (« L », « V »…), mais une lettre AVEC
//! trou (« O », « A », « 8»…) rendrait son intérieur plein plutôt qu'ajouré
//! si l'utilisateur active « Rempli » après coup : le modèle de remplissage
//! de QuickPaint (éventail depuis le barycentre, `ribbon::build_fill`) ne
//! gère qu'un contour simple, pas une règle pair-impair multi-contours. Documenté
//! ici plutôt que caché : le résultat non rempli (contour seul) est toujours
//! correct, un remplissage manuel ultérieur ne l'est que pour les lettres
//! sans trou.
//!
//! Indépendant d'egui/de `PaintApp` (renvoie des points bruts) → testable seul,
//! même schéma que `render::ribbon`.

use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Nombre de segments pour approximer une courbe quadratique/cubique.
const CURVE_STEPS: usize = 8;

/// Un contour fermé (coordonnées document, ligne de base à y=0, x croissant
/// vers la droite — à transformer par l'appelant selon la position/taille
/// réelle du texte).
pub type Contour = Vec<(f32, f32)>;

struct Collector {
    contours: Vec<Contour>,
    current: Contour,
    cursor: (f32, f32),
    start: (f32, f32),
    /// Mise à l'échelle unités de police → document, et symétrie verticale
    /// (les polices ont un axe Y montant, QuickPaint un axe Y descendant).
    scale: f32,
    offset_x: f32,
    /// Ascendant de la police, en unités de police — décale l'origine du
    /// haut du glyphe (ce que `TextItem::pos` représente ailleurs dans le
    /// modèle) plutôt que de la ligne de base.
    ascent: f32,
}

impl OutlineBuilder for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.current.len() > 1 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
        let p = self.map(x, y);
        self.cursor = p;
        self.start = p;
        self.current.push(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.cursor = p;
        self.current.push(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.map(x1, y1);
        let end = self.map(x, y);
        let start = self.cursor;
        for i in 1..=CURVE_STEPS {
            let t = i as f32 / CURVE_STEPS as f32;
            let mt = 1.0 - t;
            let px = mt * mt * start.0 + 2.0 * mt * t * c.0 + t * t * end.0;
            let py = mt * mt * start.1 + 2.0 * mt * t * c.1 + t * t * end.1;
            self.current.push((px, py));
        }
        self.cursor = end;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let end = self.map(x, y);
        let start = self.cursor;
        for i in 1..=CURVE_STEPS {
            let t = i as f32 / CURVE_STEPS as f32;
            let mt = 1.0 - t;
            let px = mt * mt * mt * start.0 + 3.0 * mt * mt * t * c1.0 + 3.0 * mt * t * t * c2.0 + t * t * t * end.0;
            let py = mt * mt * mt * start.1 + 3.0 * mt * mt * t * c1.1 + 3.0 * mt * t * t * c2.1 + t * t * t * end.1;
            self.current.push((px, py));
        }
        self.cursor = end;
    }

    fn close(&mut self) {
        self.current.push(self.start);
    }
}

impl Collector {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.offset_x + x * self.scale, (self.ascent - y) * self.scale)
    }
}

/// Contours de tous les glyphes de `text`, mis bout à bout selon l'avance
/// horizontale de chaque glyphe (pas de crénage de paires — repli simple et
/// prévisible, cohérent avec le reste du moteur de texte). `size_px` est la
/// taille de police en unités document (`TextItem::size`).
///
/// `None` si `face_bytes` n'est pas une police TrueType/OpenType valide.
pub fn glyph_contours(face_bytes: &[u8], text: &str, size_px: f32) -> Option<Vec<Contour>> {
    let face = Face::parse(face_bytes, 0).ok()?;
    let upem = face.units_per_em() as f32;
    if upem <= 0.0 {
        return None;
    }
    let scale = size_px / upem;
    let ascent = face.ascender() as f32;
    let mut out = Vec::new();
    let mut pen_x = 0.0f32;
    for ch in text.chars() {
        let Some(gid) = face.glyph_index(ch) else {
            pen_x += size_px * 0.5; // caractère absent de la police : avance forfaitaire
            continue;
        };
        let mut collector =
            Collector { contours: Vec::new(), current: Vec::new(), cursor: (0.0, 0.0), start: (0.0, 0.0), scale, offset_x: pen_x, ascent };
        if face.outline_glyph(gid, &mut collector).is_some() {
            if collector.current.len() > 1 {
                collector.contours.push(collector.current);
            }
            out.extend(collector.contours);
        }
        let advance = face.glyph_hor_advance(gid).unwrap_or(0) as f32;
        pen_x += advance * scale;
    }
    Some(out)
}

/// Fonction utilitaire pour les tests d'intégration ailleurs dans le crate :
/// GlyphId n'est utile qu'à `Collector`, jamais exposé publiquement.
#[allow(dead_code)]
fn _unused(_: GlyphId) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Police système garantie présente sur tout Mac — la même hypothèse
    /// que le reste du projet (aucune police embarquée, voir `fonts.rs`).
    fn system_font_bytes() -> Vec<u8> {
        let mgr = crate::fonts::FontManager::new();
        mgr.font_bytes("Helvetica", false, false)
            .or_else(|| mgr.font_bytes(".AppleSystemUIFont", false, false))
            .or_else(|| mgr.family_names().first().and_then(|f| mgr.font_bytes(f, false, false)))
            .expect("au moins une police système doit être présente pour ce test")
    }

    #[test]
    fn glyph_contours_produces_at_least_one_closed_contour_per_visible_glyph() {
        let bytes = system_font_bytes();
        let contours = glyph_contours(&bytes, "l", 100.0).expect("police valide");
        assert!(!contours.is_empty(), "la lettre 'l' doit produire au moins un contour");
        for c in &contours {
            assert!(c.len() > 2, "un contour doit avoir plus de 2 points");
            assert_eq!(c.first(), c.last(), "un contour doit être fermé (dernier point = premier)");
        }
    }

    #[test]
    fn space_produces_no_contour_but_still_advances() {
        let bytes = system_font_bytes();
        let a = glyph_contours(&bytes, "a", 100.0).unwrap();
        let a_space_a = glyph_contours(&bytes, "a a", 100.0).unwrap();
        // L'espace n'ajoute aucun contour propre, mais décale le 2e "a" —
        // les deux jeux de contours du "a" ne doivent pas se chevaucher.
        assert_eq!(a_space_a.len(), a.len() * 2, "2 lettres 'a', l'espace ne produit pas de contour");
    }

    #[test]
    fn larger_size_produces_a_larger_bounding_box() {
        let bytes = system_font_bytes();
        let bbox_width = |contours: &[Contour]| -> f32 {
            let xs: Vec<f32> = contours.iter().flatten().map(|p| p.0).collect();
            xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min)
        };
        let small = glyph_contours(&bytes, "M", 20.0).unwrap();
        let big = glyph_contours(&bytes, "M", 200.0).unwrap();
        assert!(bbox_width(&big) > bbox_width(&small) * 5.0, "une police 10× plus grande doit produire une géométrie ~10× plus large");
    }

    #[test]
    fn invalid_font_bytes_return_none() {
        assert!(glyph_contours(b"not a font", "a", 100.0).is_none());
    }
}
