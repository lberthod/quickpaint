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
        .zip(arc.char_angles(&t.text, t.size))
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

    let halign = match t.align {
        TextAlign::Left => egui::Align::LEFT,
        TextAlign::Center => egui::Align::Center,
        TextAlign::Right => egui::Align::RIGHT,
    };
    if t.align == TextAlign::Left {
        return ctx.fonts_mut(|f| f.layout_no_wrap(text, font_id, egui::Color32::WHITE));
    }

    // Largeur naturelle (sans retour à la ligne) puis alignement dans ce bloc.
    let natural = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), font_id.clone(), egui::Color32::WHITE));
    let block_w = natural.rect.width() + 2.0;
    let mut job = egui::text::LayoutJob::single_section(
        text,
        egui::text::TextFormat::simple(font_id, egui::Color32::WHITE),
    );
    job.halign = halign;
    job.wrap.max_width = block_w;
    ctx.fonts_mut(|f| f.layout_job(job))
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
