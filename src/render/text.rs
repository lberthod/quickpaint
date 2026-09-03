//! Rendu de texte riche partagé (Sprint 3).
//!
//! Les deux chemins de rendu — painter live (`app::draw_text`) et compositeur
//! CPU (`compositor::raster_text`) — appellent ces helpers pour rester
//! cohérents : même mise en page (police + alignement) et mêmes « passes » de
//! dépôt (contour, faux-bold, remplissage).

use crate::model::text::{TextAlign, TextArc, TextFont, TextItem};
use std::sync::Arc;

/// Un caractère positionné sur un arc (Sprint 7.1) : décalage (unités
/// document, depuis le centre `TextItem::pos`) et angle de rotation
/// (radians, tangente au cercle en ce point) de ce caractère précis.
pub struct ArcChar {
    pub ch: char,
    pub offset: (f32, f32),
    pub angle: f32,
}

/// Calcule la position/rotation de chaque caractère de `t.text` le long de
/// `arc`. Pur (pas de dépendance à egui/police) — la mise en page réelle de
/// chaque caractère individuel reste à la charge de l'appelant.
pub fn arc_chars(t: &TextItem, arc: &TextArc) -> Vec<ArcChar> {
    let dir = if arc.flip { -1.0 } else { 1.0 };
    t.text
        .chars()
        .zip(arc.char_angles(&t.text, t.size, t.letter_spacing))
        .map(|(ch, theta)| ArcChar {
            ch,
            offset: (arc.radius * theta.cos(), arc.radius * theta.sin()),
            angle: theta + std::f32::consts::FRAC_PI_2 * dir,
        })
        .collect()
}

/// Une passe de dépôt du texte : décalage (unités document) + couleur. Les
/// passes sont dessinées dans l'ordre (contour d'abord, remplissage en dernier).
pub type Pass = ((f32, f32), [u8; 4]);

/// Famille egui correspondant à la police du texte : une police système
/// (roadmap P1 #7) prime sur les deux polices intégrées (Sans/Mono) si
/// définie — à charge de l'appelant de l'avoir enregistrée au préalable
/// auprès d'egui (`fonts::FontManager::ensure_loaded`), sinon egui retombe
/// silencieusement sur la police par défaut (aucun crash).
pub fn family(t: &TextItem) -> egui::FontFamily {
    if let Some(name) = &t.font_family {
        // Italique (Sprint Q, point 82) : famille egui dédiée, toujours
        // enregistrée par `ensure_loaded` (vraie fonte italique si la
        // famille en a une, sinon repli romain — jamais un nom inconnu).
        if t.italic {
            return egui::FontFamily::Name(crate::fonts::FontManager::italic_key(name).as_str().into());
        }
        return egui::FontFamily::Name(name.as_str().into());
    }
    match t.font {
        TextFont::Proportional => egui::FontFamily::Proportional,
        TextFont::Monospace => egui::FontFamily::Monospace,
    }
}

/// Met en page le texte en un `Galley` aligné. `px_per_doc` convertit la taille
/// document en pixels écran (1.0 pour le compositeur en espace document).
///
/// L'alignement est obtenu en deux temps : mesure de la largeur naturelle du
/// bloc, puis re-layout borné à cette largeur avec le `halign` voulu — ainsi le
/// `pos` reste le bord gauche du bloc (ancrage stable pour la sélection).
pub fn layout(ctx: &egui::Context, t: &TextItem, px_per_doc: f32) -> Arc<egui::Galley> {
    let font_id = egui::FontId::new((t.size * px_per_doc).max(1.0), family(t));
    let text = if t.text.is_empty() { " ".to_string() } else { t.text.clone() };

    // Interligne et crénage réglables (Sprint Q, point 83) : portés par le
    // `TextFormat` — même format pour tous les alignements, la mise en page
    // reste cohérente entre le painter live et le compositeur CPU.
    let mut format = egui::text::TextFormat::simple(font_id, egui::Color32::WHITE);
    format.extra_letter_spacing = t.letter_spacing * px_per_doc;
    format.line_height = Some((t.size * t.line_height.max(0.5) * px_per_doc).max(1.0));

    let halign = match t.align {
        TextAlign::Left => egui::Align::LEFT,
        TextAlign::Center => egui::Align::Center,
        TextAlign::Right => egui::Align::RIGHT,
    };
    let mut job = egui::text::LayoutJob::single_section(text, format);
    if t.align == TextAlign::Left {
        return ctx.fonts(|f| f.layout_job(job));
    }

    // Largeur naturelle (sans retour à la ligne) puis alignement dans ce bloc.
    let natural = ctx.fonts(|f| f.layout_job(job.clone()));
    let block_w = natural.rect.width() + 2.0;
    job.halign = halign;
    job.wrap.max_width = block_w;
    ctx.fonts(|f| f.layout_job(job))
}

/// Passes de dépôt : ombre (si activée), contour (8 directions), faux-bold
/// (4 directions), puis le remplissage central. Décalages en unités document.
pub fn passes(t: &TextItem) -> Vec<Pass> {
    let mut v = Vec::new();
    if let Some(shadow) = t.shadow {
        v.push((shadow.offset, shadow.color));
    }
    if t.outline_w > 0.4 {
        let r = t.outline_w;
        let d = r * std::f32::consts::FRAC_1_SQRT_2;
        for off in [
            (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r),
            (d, d), (d, -d), (-d, d), (-d, -d),
        ] {
            v.push((off, t.outline_color));
        }
    }
    if t.bold {
        let b = (t.size * 0.05).max(0.8);
        for off in [(b, 0.0), (-b, 0.0), (0.0, b), (0.0, -b)] {
            v.push((off, t.color));
        }
    }
    v.push(((0.0, 0.0), t.color));
    v
}

/// Trait de soulignement (previous_audit.md #61), en unités écran
/// (`scale` = unités document → pixels). `None` si `t.underline` est
/// désactivé. Épaisseur proportionnelle à la taille de police, comme le
/// faux-bold de `passes()`. N'est appliqué qu'à la **dernière** passe (le
/// remplissage central) par l'appelant — pas aux passes d'ombre/contour/
/// faux-bold, qui dessineraient sinon un trait dédoublé ou plus épais
/// qu'attendu.
pub fn underline_stroke(t: &TextItem, scale: f32) -> Option<egui::Stroke> {
    if !t.underline {
        return None;
    }
    let width = (t.size * 0.08 * scale).max(1.0);
    let color = egui::Color32::from_rgba_unmultiplied(t.color[0], t.color[1], t.color[2], t.color[3]);
    Some(egui::Stroke::new(width, color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TextItem;

    #[test]
    fn plain_text_has_single_fill_pass() {
        let t = TextItem::new(1, (0.0, 0.0), 20.0, [10, 20, 30, 255]);
        let p = passes(&t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0], ((0.0, 0.0), [10, 20, 30, 255]));
    }

    #[test]
    fn underline_stroke_is_none_when_disabled() {
        let t = TextItem::new(1, (0.0, 0.0), 20.0, [10, 20, 30, 255]);
        assert!(!t.underline);
        assert!(underline_stroke(&t, 1.0).is_none());
    }

    #[test]
    fn underline_stroke_scales_with_size_and_view_scale() {
        let mut t = TextItem::new(1, (0.0, 0.0), 20.0, [10, 20, 30, 255]);
        t.underline = true;
        let s1 = underline_stroke(&t, 1.0).expect("underline enabled");
        assert_eq!(s1.color, egui::Color32::from_rgba_unmultiplied(10, 20, 30, 255));
        assert!(s1.width > 0.0);
        let s2 = underline_stroke(&t, 2.0).expect("underline enabled");
        assert!(s2.width > s1.width, "un plus grand facteur d'échelle doit épaissir le trait");
    }

    #[test]
    fn arc_chars_places_characters_on_the_circle() {
        let mut t = TextItem::new(1, (0.0, 0.0), 20.0, [0, 0, 0, 255]);
        t.text = "AB".to_string();
        t.arc = Some(TextArc { radius: 50.0, start_angle_deg: 0.0, flip: false });
        let arc = t.arc.unwrap();
        let chars = arc_chars(&t, &arc);
        assert_eq!(chars.len(), 2);
        for c in &chars {
            let dist = (c.offset.0.powi(2) + c.offset.1.powi(2)).sqrt();
            assert!((dist - 50.0).abs() < 1e-3, "le caractère doit être à `radius` du centre");
        }
    }

    #[test]
    fn outline_and_bold_add_passes() {
        let mut t = TextItem::new(1, (0.0, 0.0), 20.0, [0, 0, 0, 255]);
        t.outline_w = 2.0;
        t.bold = true;
        let p = passes(&t);
        // 8 (contour) + 4 (gras) + 1 (remplissage).
        assert_eq!(p.len(), 13);
        // Le remplissage est la dernière passe (au-dessus).
        assert_eq!(p.last().unwrap().0, (0.0, 0.0));
    }
}
