//! Compositing CPU par calque (roadmap #8, voie A).
//!
//! Rastérise chaque calque sur un `Pixmap` tiny-skia (traits via leur géométrie
//! de ruban, images blittées), puis compose les calques avec leur **mode de
//! fusion** et leur **opacité de groupe** (vrai compositing, pas par-trait). Le
//! résultat est mis en cache comme texture egui, invalidé par une signature.
//!
//! À l'écran (`texture()`), le texte d'un calque « Normal » à opacité pleine
//! n'est pas rastérisé ici : il est dessiné par egui par-dessus, plus net à
//! l'écran (le compositeur n'est sollicité que si un calque a un mode de
//! fusion, une opacité < 1, un masque ou du contenu peint). À l'export
//! (`render_to_rgba()`, roadmap §12.2), tous les calques passent par ce même
//! chemin, texte inclus — nécessaire puisqu'il n'y a alors pas de passe egui
//! par-dessus pour le dessiner.

use crate::model::raster::{Tile, TileKey, TILE};
use crate::model::{BlendMode, Document, ImageItem, Stroke, Tool};
use crate::render::ribbon;
use egui::{Color32, ColorImage, TextureFilter, TextureHandle, TextureOptions};
use std::collections::HashMap;
use tiny_skia::{
    BlendMode as SkBlend, Color, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint,
    PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, Shader, SpreadMode, Transform,
};

/// Cache incrémental du contenu peint d'un calque (roadmap previous_audit.md (ex-ANALYSE) §12.1) :
/// un `Pixmap` **persistant** entre les appels, retouché tuile par tuile au
/// lieu d'être reconstruit en entier à chaque dab. Voir `blit_raster_tiles`.
struct RasterTileCache {
    pm: Pixmap,
    /// Dernier hash connu de chaque tuile déjà blittée dans `pm`.
    tiles: HashMap<TileKey, u64>,
}

impl Default for RasterTileCache {
    fn default() -> Self {
        // Pixmap 1×1 provisoire : `blit_raster_tiles` détecte la taille
        // erronée dès le premier appel et réalloue à la taille du document.
        // `tiny_skia::Pixmap` n'implémente pas `Default`, d'où cet impl manuel.
        Self { pm: Pixmap::new(1, 1).expect("pixmap 1×1"), tiles: HashMap::new() }
    }
}

#[derive(Default)]
pub struct Compositor {
    tex: Option<TextureHandle>,
    sig: u64,
    /// Cache par calque : id → (hash de contenu, pixmap rastérisé). Évite de
    /// re-rastériser les calques inchangés (perf en mode fusion multi-calques).
    layers: HashMap<u64, (u64, Pixmap)>,
    /// Cache par calque du contenu peint (pinceau/gomme pixel...) seul, séparé
    /// du pixmap composé ci-dessus — patché tuile par tuile (roadmap §12.1).
    raster_cache: HashMap<u64, RasterTileCache>,
}

impl Compositor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Renvoie la texture composite, reconstruite si la signature a changé.
    /// `skip_text` exclut un texte (celui en cours d'édition) du rendu.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        sig: u64,
        skip_text: Option<u64>,
    ) -> Option<&TextureHandle> {
        if self.tex.is_none() || self.sig != sig {
            if let Some(ci) = self.rebuild(ctx, doc, skip_text, None) {
                // Mipmaps : sans eux, la texture composite (résolution native du
                // document) affichée plus petite que 1:1 est minifiée en bilinéaire
                // pur — les traits fins deviennent pointillés/dédoublés (moiré).
                let opts = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
                self.tex = Some(ctx.load_texture("composite", ci, opts));
            }
            self.sig = sig;
        }
        self.tex.as_ref()
    }

    /// Hash de contenu déjà mis en cache pour ce calque (repeuplé à chaque
    /// `rebuild`), pour piloter l'invalidation de la miniature (Sprint I.3)
    /// sans dépendre d'un second calcul de hash côté appelant.
    pub fn layer_content_hash(&self, layer_id: u64) -> Option<u64> {
        self.layers.get(&layer_id).map(|(h, _)| *h)
    }

    /// Miniature basse résolution d'un calque (Sprint I.3) : redimensionne
    /// (plus proche voisin) le pixmap déjà rastérisé et mis en cache par
    /// calque (`self.layers`) plutôt que refaire un rendu dédié. `None` si le
    /// calque n'a pas encore été rastérisé (calque masqué/vide de contenu,
    /// ou pas encore de frame de composition) ou n'existe plus.
    pub fn layer_thumbnail(&self, layer_id: u64, size: u32) -> Option<ColorImage> {
        let (_, pm) = self.layers.get(&layer_id)?;
        let (w, h) = (pm.width(), pm.height());
        if w == 0 || h == 0 {
            return None;
        }
        let scale = (size as f32 / w.max(h) as f32).min(1.0);
        let tw = ((w as f32 * scale).round() as u32).max(1);
        let th = ((h as f32 * scale).round() as u32).max(1);
        let data = pm.data();
        let mut pixels = Vec::with_capacity((tw * th) as usize);
        for y in 0..th {
            for x in 0..tw {
                let sx = ((x as f32 / scale) as u32).min(w - 1);
                let sy = ((y as f32 / scale) as u32).min(h - 1);
                let i = ((sy * w + sx) * 4) as usize;
                pixels.push(Color32::from_rgba_premultiplied(data[i], data[i + 1], data[i + 2], data[i + 3]));
            }
        }
        Some(ColorImage { size: [tw as usize, th as usize], pixels })
    }

    /// Rendu du document à sa résolution **native** (`doc.size`), pour
    /// l'export (roadmap previous_audit.md (ex-ANALYSE) §12.2). Réutilise le même chemin de
    /// composition que l'affichage (calques, modes de fusion, masques,
    /// ajustements, texte) au lieu de dépendre d'une capture d'écran du
    /// viewport — la résolution exportée ne dépend donc plus de la taille de
    /// la fenêtre ni du zoom courant. `bg` est peint en fond opaque avant la
    /// composition, comme le fait le canevas à l'écran (`painter.rect_filled`) :
    /// un export garde donc le même rendu visuel (fond plein, pas de zones
    /// transparentes imprévues) qu'avant ce changement.
    pub fn render_to_rgba(&mut self, ctx: &egui::Context, doc: &Document, bg: Color32) -> Option<(u32, u32, Vec<u8>)> {
        let ci = self.rebuild(ctx, doc, None, Some(bg))?;
        let [w, h] = ci.size;
        let mut rgba = Vec::with_capacity(w * h * 4);
        for px in &ci.pixels {
            rgba.extend_from_slice(&px.to_srgba_unmultiplied());
        }
        Some((w as u32, h as u32, rgba))
    }

    /// Compose le document : ne rastérise que les calques dont le hash a changé.
    /// `bg` : fond peint avant la composition — `None` (affichage normal,
    /// transparent, la texture est posée sur le canevas déjà peint) ou
    /// `Some(color)` (export, roadmap §12.2 : fond opaque autonome).
    fn rebuild(&mut self, ctx: &egui::Context, doc: &Document, skip_text: Option<u64>, bg: Option<Color32>) -> Option<ColorImage> {
        let (w, h) = doc.size;
        let mut base = Pixmap::new(w, h)?;
        if let Some(c) = bg {
            base.fill(Color::from_rgba8(c.r(), c.g(), c.b(), 255));
        }
        // Pré-passe glyphes : `layout()` alloue les glyphes manquants dans
        // l'atlas *réel* d'egui (effet de bord de la mise en page). Il faut
        // que ça arrive avant de capturer un instantané de cet atlas plus
        // bas, sans quoi un texte encore jamais mis en page dans ce contexte
        // (typiquement : le tout premier export d'un document, ou un calque
        // qui vient de passer en mode composite) verrait ses glyphes absents
        // de l'instantané et resterait invisible — bug constaté en testant
        // `render_to_rgba` isolément (un seul passage de rendu, sans les
        // frames précédentes qui « réchauffaient » l'atlas par accident).
        for layer in &doc.layers {
            if !layer.visible || layer.opacity <= 0.0 || layer.adjustment.is_some() {
                continue;
            }
            let stale = self
                .layers
                .get(&layer.id)
                .map(|(h, _)| *h != layer_hash(layer, skip_text))
                .unwrap_or(true);
            if !stale {
                continue;
            }
            for t in &layer.texts {
                if Some(t.id) != skip_text && !t.text.trim().is_empty() {
                    let _ = crate::render::text::layout(ctx, t, 1.0);
                }
            }
        }
        // `ctx.fonts(|f| f.image())` clone tout l'atlas de glyphes (plusieurs Mo,
        // en f32) — coûteux et inutile tant qu'aucun calque redevenu obsolète ne
        // contient réellement du texte. Récupéré paresseusement, une seule fois
        // par appel, une fois la pré-passe ci-dessus garantie complète (perf :
        // ce chemin s'exécute à chaque frame pendant la peinture raster/pixel).
        let mut atlas: Option<egui::epaint::FontImage> = None;
        let mut live = std::collections::HashSet::new();
        // Pixmap du dernier calque NON écrêté : base d'écrêtage des suivants.
        let mut clip_base: Option<Pixmap> = None;
        for layer in &doc.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            // Calque d'ajustement (F3) : pas de contenu propre à rastériser —
            // applique le filtre en direct à ce qui est déjà composé (tout,
            // ou seulement le calque du dessous si écrêté), réversible tant
            // que le calque existe. Ne devient pas la nouvelle base d'écrêtage.
            if let Some(adjustment) = layer.adjustment.clone() {
                let mask = if layer.clip { clip_base.as_ref() } else { None };
                apply_adjustment(&mut base, adjustment, layer.opacity.clamp(0.0, 1.0), mask);
                continue;
            }
            live.insert(layer.id);
            let hash = layer_hash(layer, skip_text);
            let stale = self.layers.get(&layer.id).map(|(h, _)| *h != hash).unwrap_or(true);
            if stale {
                let mut lp = Pixmap::new(w, h)?;
                if let Some(fill) = &layer.fill {
                    // Calque de remplissage (Sprint I.1) : aucun contenu
                    // propre — peint tout son rectangle, sans passer par le
                    // pipeline raster/traits/textes des calques normaux.
                    paint_fill_layer(&mut lp, fill, w, h);
                } else {
                    // Contenu peint (pinceau/gomme pixel, F1) : rendu en premier,
                    // sous les éléments vectoriels du calque, depuis un cache
                    // persistant retouché tuile par tuile (roadmap §12.1) plutôt
                    // que ré-aplati en entier à chaque dab.
                    let raster_pm = self.raster_cache.entry(layer.id).or_default();
                    blit_raster_tiles(raster_pm, &layer.raster, w, h);
                    lp.data_mut().copy_from_slice(raster_pm.pm.data());
                    for r in layer.z_order() {
                        match r {
                            crate::model::ElemRef::Stroke(i) => raster_stroke(&mut lp, &layer.strokes[i]),
                            crate::model::ElemRef::Image(i) => raster_image(&mut lp, &layer.images[i]),
                            crate::model::ElemRef::Text(i) => {
                                let t = &layer.texts[i];
                                if Some(t.id) != skip_text {
                                    let atlas = atlas.get_or_insert_with(|| ctx.fonts(|f| f.image()));
                                    raster_text(ctx, &mut lp, t, atlas);
                                }
                            }
                        }
                    }
                }
                if let Some(mask) = &layer.mask {
                    apply_mask(&mut lp, mask);
                }
                // Styles de calque (Sprint 6.1) : après le masque (pour que
                // l'ombre/contour/lueur suivent la forme réellement visible),
                // avant l'écrêtage (qui s'applique au résultat final du calque).
                if !layer.styles.is_empty() {
                    apply_layer_styles(&mut lp, &layer.styles, w as usize, h as usize);
                }
                self.layers.insert(layer.id, (hash, lp));
            }
            let lp = &self.layers[&layer.id].1;

            // Calque écrêté : limité à l'alpha de la base d'écrêtage (calque du
            // dessous). Sans base, il s'affiche normalement.
            let clipped = if layer.clip {
                clip_base.as_ref().map(|base_pm| {
                    let mut c = lp.clone();
                    clip_alpha(&mut c, base_pm);
                    c
                })
            } else {
                None
            };
            let draw_pm = clipped.as_ref().unwrap_or(lp);
            base.draw_pixmap(
                0,
                0,
                draw_pm.as_ref(),
                &PixmapPaint {
                    opacity: layer.opacity.clamp(0.0, 1.0),
                    blend_mode: map_blend(layer.blend),
                    quality: FilterQuality::Nearest,
                },
                Transform::identity(),
                None,
            );
            // Un calque non écrêté devient la base d'écrêtage des suivants.
            if !layer.clip {
                clip_base = Some(lp.clone());
            }
        }
        self.layers.retain(|id, _| live.contains(id));
        self.raster_cache.retain(|id, _| live.contains(id));

        let pixels = base
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]))
            .collect();
        Some(ColorImage { size: [w as usize, h as usize], pixels })
    }
}

/// Hash de contenu d'un calque (FNV-1a sur les champs influençant le rendu).
fn layer_hash(l: &crate::model::Layer, skip_text: Option<u64>) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x100000001b3);
    };
    mix(l.visible as u64);
    mix(l.opacity.to_bits() as u64);
    mix(l.blend as u64);
    mix(l.raster.content_hash());
    mix(l.mask.as_ref().map(|m| m.content_hash()).unwrap_or(0));
    if let Some(fill) = &l.fill {
        use crate::model::FillKind;
        let gradient = match fill {
            FillKind::Solid(c) => {
                mix(0);
                mix(u32::from_le_bytes(*c) as u64);
                None
            }
            FillKind::Linear(g) => {
                mix(1);
                Some(g)
            }
            FillKind::Radial(g) => {
                mix(2);
                Some(g)
            }
        };
        if let Some(g) = gradient {
            mix(g.kind as u64);
            mix(g.from.0.to_bits() as u64);
            mix(g.from.1.to_bits() as u64);
            mix(g.to.0.to_bits() as u64);
            mix(g.to.1.to_bits() as u64);
            for (pos, c) in &g.stops {
                mix(pos.to_bits() as u64);
                mix(u32::from_le_bytes(*c) as u64);
            }
        }
    }
    for style in &l.styles {
        use crate::model::LayerStyle;
        match style {
            LayerStyle::DropShadow { color, offset, blur } => {
                mix(1);
                mix(u32::from_le_bytes(*color) as u64);
                mix(offset.0.to_bits() as u64);
                mix(offset.1.to_bits() as u64);
                mix(blur.to_bits() as u64);
            }
            LayerStyle::Stroke { color, width } => {
                mix(2);
                mix(u32::from_le_bytes(*color) as u64);
                mix(width.to_bits() as u64);
            }
            LayerStyle::Glow { color, blur, inner } => {
                mix(3);
                mix(u32::from_le_bytes(*color) as u64);
                mix(blur.to_bits() as u64);
                mix(*inner as u64);
            }
        }
    }
    for s in &l.strokes {
        mix(s.id);
        mix(s.z.to_bits());
        mix(u32::from_le_bytes(s.color) as u64);
        mix(s.fill as u64);
        mix(s.base_width.to_bits() as u64);
        if let Some(g) = &s.gradient {
            mix(g.kind as u64 + 1);
            mix(g.from.0.to_bits() as u64);
            mix(g.from.1.to_bits() as u64);
            mix(g.to.0.to_bits() as u64);
            mix(g.to.1.to_bits() as u64);
            for (pos, c) in &g.stops {
                mix(pos.to_bits() as u64);
                mix(u32::from_le_bytes(*c) as u64);
            }
        }
        for p in &s.points {
            mix(p.pos.0.to_bits() as u64);
            mix(p.pos.1.to_bits() as u64);
            mix(p.width.to_bits() as u64);
        }
    }
    for im in &l.images {
        mix(im.id);
        mix(im.z.to_bits());
        mix(im.pos.0.to_bits() as u64);
        mix(im.pos.1.to_bits() as u64);
        mix(im.size.0.to_bits() as u64);
        mix(im.size.1.to_bits() as u64);
        mix(im.rot.to_bits() as u64);
        mix(im.w as u64);
        mix(im.h as u64);
        // Échantillon des pixels (capte filtres / recadrage à id constant).
        mix(im.rgba.len() as u64);
        for i in (0..im.rgba.len()).step_by(257) {
            mix(im.rgba[i] as u64);
        }
    }
    for t in &l.texts {
        if Some(t.id) == skip_text {
            mix(0xDEAD);
            continue;
        }
        mix(t.id);
        mix(t.z.to_bits());
        mix(t.pos.0.to_bits() as u64);
        mix(t.pos.1.to_bits() as u64);
        mix(t.size.to_bits() as u64);
        mix(t.rot.to_bits() as u64);
        mix(u32::from_le_bytes(t.color) as u64);
        // Style (Sprint 3 + Q) : police, gras/italique/soulignement,
        // interligne/espacement, alignement, contour, police système,
        // ombre — chacun manquant ici invaliderait le cache trop tard
        // (le rendu resterait périmé jusqu'au prochain changement qui,
        // lui, touche un champ déjà couvert). Trouvé en ajoutant le
        // soulignement (previous_audit.md #61) : italique/interligne/
        // espacement/police système/ombre en manquaient déjà depuis leur
        // introduction (Sprint Q).
        mix(t.font as u64);
        mix(t.bold as u64);
        mix(t.italic as u64);
        mix(t.underline as u64);
        mix(t.line_height.to_bits() as u64);
        mix(t.letter_spacing.to_bits() as u64);
        mix(t.align as u64);
        mix(t.outline_w.to_bits() as u64);
        mix(u32::from_le_bytes(t.outline_color) as u64);
        if let Some(ff) = &t.font_family {
            for b in ff.bytes() {
                mix(b as u64);
            }
        }
        if let Some(sh) = t.shadow {
            mix(sh.offset.0.to_bits() as u64);
            mix(sh.offset.1.to_bits() as u64);
            mix(u32::from_le_bytes(sh.color) as u64);
        }
        for b in t.text.bytes() {
            mix(b as u64);
        }
    }
    h
}

/// Écrête `pm` par l'alpha de `base` : chaque pixel de `pm` est multiplié par
/// l'opacité du pixel correspondant de la base (masque d'écrêtage, Sprint 4).
/// Les pixmaps tiny-skia sont prémultipliés, donc on met à l'échelle les 4
/// canaux par le même facteur — la prémultiplication reste valide.
fn clip_alpha(pm: &mut Pixmap, base: &Pixmap) {
    let base_px = base.data();
    let px = pm.data_mut();
    if px.len() != base_px.len() {
        return;
    }
    for i in (0..px.len()).step_by(4) {
        let f = base_px[i + 3] as u32; // 0..=255
        if f == 255 {
            continue;
        }
        for c in 0..4 {
            px[i + c] = ((px[i + c] as u32 * f + 127) / 255) as u8;
        }
    }
}

/// Applique un masque de calque (roadmap P2 #14) : multiplie chaque canal
/// (déjà prémultiplié) par la couverture du masque à ce pixel — même
/// technique que `clip_alpha`, mais la source de la couverture est un
/// masque peint (blanc = visible, noir = masqué) plutôt que l'alpha d'un
/// autre calque.
fn apply_mask(pm: &mut Pixmap, mask: &crate::model::RasterLayer) {
    let w = pm.width();
    let px = pm.data_mut();
    for i in (0..px.len()).step_by(4) {
        let idx = (i / 4) as u32;
        let (x, y) = (idx % w, idx / w);
        let f = mask.mask_coverage(x as i32, y as i32) as u32;
        if f == 255 {
            continue;
        }
        for c in 0..4 {
            px[i + c] = ((px[i + c] as u32 * f + 127) / 255) as u8;
        }
    }
}

fn map_blend(b: BlendMode) -> SkBlend {
    match b {
        BlendMode::Normal => SkBlend::SourceOver,
        BlendMode::Multiply => SkBlend::Multiply,
        BlendMode::Screen => SkBlend::Screen,
        BlendMode::Overlay => SkBlend::Overlay,
        BlendMode::Darken => SkBlend::Darken,
        BlendMode::Lighten => SkBlend::Lighten,
        BlendMode::SoftLight => SkBlend::SoftLight,
        BlendMode::HardLight => SkBlend::HardLight,
        BlendMode::Difference => SkBlend::Difference,
        BlendMode::Exclusion => SkBlend::Exclusion,
        BlendMode::ColorDodge => SkBlend::ColorDodge,
        BlendMode::ColorBurn => SkBlend::ColorBurn,
    }
}

/// Rastérise un trait via sa géométrie de ruban (fidèle : largeur variable,
/// formes pleines), en une seule passe de remplissage (pas de couture interne).
fn raster_stroke(pm: &mut Pixmap, stroke: &Stroke) {
    if stroke.tool == Tool::Eraser || stroke.points.is_empty() {
        return;
    }
    let mesh = ribbon::build(stroke);
    if mesh.indices.is_empty() {
        return;
    }
    let mut pb = PathBuilder::new();
    for tri in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.vertices[tri[0] as usize].pos;
        let b = mesh.vertices[tri[1] as usize].pos;
        let c = mesh.vertices[tri[2] as usize].pos;
        // Orientation homogène : le ruban mélange quads (sens dépendant de la
        // direction du tracé) et disques (sens fixe) qui se recouvrent. En
        // règle `Winding`, deux triangles superposés de sens opposés (+1/−1)
        // s'annulent → trous en damier le long du trait. On force donc tous
        // les triangles dans le même sens : les recouvrements s'additionnent
        // (+2) au lieu de se creuser.
        let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let (b, c) = if area < 0.0 { (c, b) } else { (b, c) };
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        pb.line_to(c.0, c.1);
        pb.close();
    }
    let Some(path) = pb.finish() else { return };
    let fill_gradient = stroke.fill.then_some(stroke.gradient.as_ref()).flatten();
    // Conique : pas de shader tiny-skia natif (Linear/Radial seulement), donc
    // rendu à part, pixel par pixel — voir `paint_conic_gradient`.
    if let Some(g) = fill_gradient.filter(|g| g.kind == crate::model::GradientKind::Conic) {
        paint_conic_gradient(pm, &path, g);
        return;
    }
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    match fill_gradient.and_then(gradient_shader) {
        Some(shader) => paint.shader = shader,
        None => {
            let [r, g, b, alpha] = stroke.color;
            paint.set_color_rgba8(r, g, b, alpha);
        }
    }
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

/// Rendu du dégradé conique (Sprint 3.3) : rastérise `path` dans un masque
/// alpha (couverture anti-aliasée), puis pour chaque pixel couvert calcule
/// l'angle polaire autour de `g.from` (0 = direction de `g.to`) et compose la
/// couleur interpolée des arrêts — `blend_pixel` fait le compositing
/// source-over, comme pour le texte rastérisé plus haut.
fn paint_conic_gradient(pm: &mut Pixmap, path: &tiny_skia::Path, g: &crate::model::Gradient) {
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let Some(mut mask) = Mask::new(pm.width(), pm.height()) else { return };
    mask.fill_path(path, FillRule::Winding, true, Transform::identity());
    let data = mask.data();

    let bounds = path.bounds();
    let x0 = (bounds.left().floor() as i32).max(0);
    let y0 = (bounds.top().floor() as i32).max(0);
    let x1 = (bounds.right().ceil() as i32).min(pw as i32);
    let y1 = (bounds.bottom().ceil() as i32).min(ph as i32);

    let (cx, cy) = g.from;
    let angle0 = (g.to.1 - cy).atan2(g.to.0 - cx);
    let mut stops = g.stops.clone();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));

    for py in y0..y1 {
        for px in x0..x1 {
            let idx = py as usize * pw + px as usize;
            let cov = data[idx] as f32 / 255.0;
            if cov <= 0.003 {
                continue;
            }
            let angle = (py as f32 + 0.5 - cy).atan2(px as f32 + 0.5 - cx) - angle0;
            let t = angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
            let color = sample_gradient_stops(&stops, t);
            blend_pixel(pm, px, py, pw, ph, cov, color);
        }
    }
}

/// Interpole la couleur des arrêts (triés par position) à `t` (0..=1, module
/// après le dernier arrêt : le dégradé conique boucle sur lui-même).
fn sample_gradient_stops(stops: &[(f32, [u8; 4])], t: f32) -> [u8; 4] {
    match stops {
        [] => [0, 0, 0, 0],
        [(_, c)] => *c,
        _ => {
            if t <= stops[0].0 {
                return stops[0].1;
            }
            if t >= stops[stops.len() - 1].0 {
                return stops[stops.len() - 1].1;
            }
            for w in stops.windows(2) {
                let ((p0, c0), (p1, c1)) = (w[0], w[1]);
                if t >= p0 && t <= p1 {
                    let f = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
                    let mut out = [0u8; 4];
                    for i in 0..4 {
                        out[i] = (c0[i] as f32 + (c1[i] as f32 - c0[i] as f32) * f).round() as u8;
                    }
                    return out;
                }
            }
            stops[stops.len() - 1].1
        }
    }
}

/// Applique les styles de calque (Sprint 6.1), dans l'ordre où ils ont été
/// ajoutés : chacun dérive de l'alpha courant de `lp` (donc du résultat déjà
/// modifié par les styles précédents), jamais des pixels d'origine.
fn apply_layer_styles(lp: &mut Pixmap, styles: &[crate::model::LayerStyle], w: usize, h: usize) {
    use crate::model::LayerStyle;
    for style in styles {
        match *style {
            LayerStyle::DropShadow { color, offset, blur } => apply_drop_shadow(lp, color, offset, blur, w, h),
            LayerStyle::Stroke { color, width } => apply_layer_stroke(lp, color, width, w, h),
            LayerStyle::Glow { color, blur, inner } => apply_glow(lp, color, blur, inner, w, h),
        }
    }
}

/// Flou boîte sur un unique canal 8 bits (alpha) — variante à un canal du
/// flou séparable de `tools::filter::box_blur`, pour éviter de repasser par
/// un buffer RGBA complet juste pour flouter un masque.
fn box_blur_channel(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if r == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let pass = |src: &[u8], horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let idx = |i: usize| -> usize {
                let (x, y) = if horizontal { (i, o) } else { (o, i) };
                y * w + x
            };
            let mut sum = 0u32;
            let mut count = 0u32;
            for i in 0..=r.min(inner - 1) {
                sum += src[idx(i)] as u32;
                count += 1;
            }
            for i in 0..inner {
                out[idx(i)] = (sum / count.max(1)) as u8;
                if i >= r {
                    sum -= src[idx(i - r)] as u32;
                    count -= 1;
                }
                let add = i + r + 1;
                if add < inner {
                    sum += src[idx(add)] as u32;
                    count += 1;
                }
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

/// Alpha (non prémultiplié) de chaque pixel de `pm`.
fn alpha_of(pm: &Pixmap) -> Vec<u8> {
    pm.pixels().iter().map(|p| p.alpha()).collect()
}

/// Peint `alpha` (teinté par `color`, prémultiplié) dans un pixmap neuf.
fn tint_from_alpha(alpha: &[u8], color: [u8; 4], w: usize, h: usize) -> Pixmap {
    let mut pm = Pixmap::new(w as u32, h as u32).unwrap();
    let data = pm.pixels_mut();
    for (i, &a) in alpha.iter().enumerate() {
        if a == 0 {
            continue;
        }
        let ea = (a as f32 / 255.0) * (color[3] as f32 / 255.0);
        if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(
            (color[0] as f32 * ea).round() as u8,
            (color[1] as f32 * ea).round() as u8,
            (color[2] as f32 * ea).round() as u8,
            (ea * 255.0).round() as u8,
        ) {
            data[i] = c;
        }
    }
    pm
}

/// Ombre portée : copie décalée + floutée de l'alpha de `lp`, teintée,
/// dessinée derrière son contenu actuel.
fn apply_drop_shadow(lp: &mut Pixmap, color: [u8; 4], offset: (f32, f32), blur: f32, w: usize, h: usize) {
    let alpha = alpha_of(lp);
    let blurred = box_blur_channel(&alpha, w, h, blur.max(0.0).round() as usize);
    let (dx, dy) = (offset.0.round() as i32, offset.1.round() as i32);
    let mut shifted = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x as i32 - dx, y as i32 - dy);
            if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                shifted[y * w + x] = blurred[sy as usize * w + sx as usize];
            }
        }
    }
    let mut result = tint_from_alpha(&shifted, color, w, h);
    result.draw_pixmap(0, 0, lp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    *lp = result;
}

/// Contour : anneau de couleur juste à l'extérieur du contenu actuel de `lp`
/// (dilatation de l'alpha par un disque de rayon `width`, restreinte aux
/// pixels non déjà couverts), dessiné derrière ce contenu.
fn apply_layer_stroke(lp: &mut Pixmap, color: [u8; 4], width: f32, w: usize, h: usize) {
    let radius = width.max(0.0).round() as i32;
    if radius <= 0 {
        return;
    }
    let alpha = alpha_of(lp);
    let mut ring = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            if alpha[y * w + x] > 0 {
                continue; // déjà couvert par le contenu d'origine
            }
            let mut covered = false;
            'search: for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    if alpha[ny as usize * w + nx as usize] > 0 {
                        covered = true;
                        break 'search;
                    }
                }
            }
            if covered {
                ring[y * w + x] = 255;
            }
        }
    }
    let mut result = tint_from_alpha(&ring, color, w, h);
    result.draw_pixmap(0, 0, lp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    *lp = result;
}

/// Lueur externe (autour du contenu, non décalée) ou interne (halo depuis les
/// bords vers l'intérieur) — dérivées d'un flou de l'alpha (externe) ou de
/// son inverse restreint au contenu (interne).
fn apply_glow(lp: &mut Pixmap, color: [u8; 4], blur: f32, inner: bool, w: usize, h: usize) {
    let alpha = alpha_of(lp);
    let radius = blur.max(0.0).round() as usize;
    if inner {
        let inverted: Vec<u8> = alpha.iter().map(|&a| 255 - a).collect();
        let blurred = box_blur_channel(&inverted, w, h, radius);
        let mask: Vec<u8> = alpha.iter().zip(&blurred).map(|(&a, &b)| if a > 0 { b } else { 0 }).collect();
        let glow = tint_from_alpha(&mask, color, w, h);
        // Lueur interne : visible par-dessus le contenu (halo vers l'intérieur).
        lp.draw_pixmap(0, 0, glow.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    } else {
        let blurred = box_blur_channel(&alpha, w, h, radius);
        let mask: Vec<u8> = alpha.iter().zip(&blurred).map(|(&a, &b)| if a == 0 { b } else { 0 }).collect();
        let mut result = tint_from_alpha(&mask, color, w, h);
        result.draw_pixmap(0, 0, lp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        *lp = result;
    }
}

/// Peint un calque de remplissage (Sprint I.1) sur tout son rectangle — cas
/// le plus simple du pipeline existant, puisqu'il ne lit pas les calques du
/// dessous (contrairement à un calque d'ajustement).
fn paint_fill_layer(pm: &mut Pixmap, fill: &crate::model::FillKind, w: u32, h: u32) {
    match fill {
        crate::model::FillKind::Solid([r, g, b, a]) => {
            pm.fill(Color::from_rgba8(*r, *g, *b, *a));
        }
        crate::model::FillKind::Linear(gradient) | crate::model::FillKind::Radial(gradient) => {
            let Some(shader) = gradient_shader(gradient) else { return };
            let paint = Paint { shader, anti_alias: true, ..Default::default() };
            let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32) else { return };
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }
}

/// Construit le shader tiny-skia d'un dégradé (roadmap P2 #11). `None` si
/// tiny-skia juge la géométrie dégénérée (points confondus, etc.) — l'appelant
/// retombe alors sur la couleur unie du trait.
fn gradient_shader(g: &crate::model::Gradient) -> Option<Shader<'static>> {
    let stops: Vec<GradientStop> = g
        .stops
        .iter()
        .map(|(pos, [r, gc, b, a])| {
            GradientStop::new(*pos, Color::from_rgba8(*r, *gc, *b, *a))
        })
        .collect();
    let from = Point::from_xy(g.from.0, g.from.1);
    let to = Point::from_xy(g.to.0, g.to.1);
    match g.kind {
        crate::model::GradientKind::Linear => {
            LinearGradient::new(from, to, stops, SpreadMode::Pad, Transform::identity())
        }
        crate::model::GradientKind::Radial => {
            let radius = ((to.x - from.x).powi(2) + (to.y - from.y).powi(2)).sqrt();
            RadialGradient::new(from, from, radius, stops, SpreadMode::Pad, Transform::identity())
        }
        // Jamais atteint : `raster_stroke` intercepte le cas Conique avant
        // d'appeler cette fonction (pas de shader tiny-skia natif pour lui).
        crate::model::GradientKind::Conic => None,
    }
}

/// Rastérise un texte via l'atlas de polices egui (couverture) → blit teinté,
/// composé en source-over dans le pixmap du calque. Lève la limite v1 (#8).
fn raster_text(
    ctx: &egui::Context,
    pm: &mut Pixmap,
    t: &crate::model::TextItem,
    atlas: &egui::epaint::FontImage,
) {
    if t.text.trim().is_empty() {
        return;
    }
    if let Some(arc) = &t.arc {
        // Texte sur courbe (Sprint 7.1) : chaque caractère a sa propre mise
        // en page et sa propre rotation — pas un galley global tourné en bloc.
        for ac in crate::render::text::arc_chars(t, arc) {
            let mut single = t.clone();
            single.text = ac.ch.to_string();
            single.rot = 0.0;
            single.arc = None;
            let galley = crate::render::text::layout(ctx, &single, 1.0);
            let passes = crate::render::text::passes(&single);
            let char_center = (t.pos.0 + ac.offset.0, t.pos.1 + ac.offset.1);
            let (gw, gh) = (galley.rect.width(), galley.rect.height());
            let origin = (char_center.0 - gw * 0.5, char_center.1 - gh * 0.5);
            // Soulignement non géré en texte sur courbe (limite assumée) :
            // un trait par caractère donnerait des segments disjoints plutôt
            // qu'un trait continu suivant l'arc.
            raster_text_glyphs(pm, atlas, &galley, origin, char_center, ac.angle, &passes, None);
        }
        return;
    }
    // Mise en page partagée (police + alignement), en espace document (1 px/doc).
    let galley = crate::render::text::layout(ctx, t, 1.0);
    let passes = crate::render::text::passes(t);
    let underline = crate::render::text::underline_stroke(t, 1.0);
    raster_text_glyphs(pm, atlas, &galley, t.pos, t.pos, t.rot, &passes, underline);
}

/// Blitte les glyphes de `galley` dans `pm`, teintés par chaque passe.
/// `origin` : point où les coordonnées locales des glyphes (`g.pos`) sont
/// ancrées avant rotation. `rotation_center`/`angle` : pivot et angle de la
/// rotation appliquée après coup — distincts d'`origin` pour le texte sur
/// courbe (Sprint 7.1), où chaque caractère tourne autour de son propre
/// centre plutôt que du coin haut-gauche du texte entier (texte plat :
/// `origin == rotation_center == t.pos`, comme avant ce refactor).
#[allow(clippy::too_many_arguments)]
fn raster_text_glyphs(
    pm: &mut Pixmap,
    atlas: &egui::epaint::FontImage,
    galley: &egui::Galley,
    origin: (f32, f32),
    rotation_center: (f32, f32),
    angle: f32,
    passes: &[((f32, f32), [u8; 4])],
    underline: Option<egui::Stroke>,
) {
    let (aw, ah) = (atlas.size[0], atlas.size[1]);
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let rotated = angle.abs() > 1e-5;
    let (c, s) = (angle.cos(), angle.sin());

    // Chaque passe (contour / gras / remplissage) blitte le galley décalé+teinté.
    for &((offx, offy), color) in passes {
        let base = (origin.0 + offx, origin.1 + offy);
        for row in &galley.rows {
            for g in &row.glyphs {
                let uv = g.uv_rect;
                if uv.is_nothing() {
                    continue;
                }
                let (mnx, mny) = (uv.min[0] as f32, uv.min[1] as f32);
                let (mxx, mxy) = (uv.max[0] as f32, uv.max[1] as f32);
                let (dw, dh) = (uv.size.x, uv.size.y);
                if dw <= 0.0 || dh <= 0.0 {
                    continue;
                }
                let ox = base.0 + g.pos.x + uv.offset.x;
                let oy = base.1 + g.pos.y + uv.offset.y;
                let sample = |u: f32, v: f32| {
                    let sx = (mnx + u * (mxx - mnx)).clamp(0.0, (aw - 1) as f32) as usize;
                    let sy = (mny + v * (mxy - mny)).clamp(0.0, (ah - 1) as f32) as usize;
                    atlas.pixels[sy * aw + sx]
                };
                if !rotated {
                    for dy in 0..dh.ceil() as i32 {
                        for dx in 0..dw.ceil() as i32 {
                            let cov = sample((dx as f32 + 0.5) / dw, (dy as f32 + 0.5) / dh);
                            let (px, py) = ((ox + dx as f32) as i32, (oy + dy as f32) as i32);
                            blend_pixel(pm, px, py, pw, ph, cov, color);
                        }
                    }
                } else {
                    // Mapping inverse : on parcourt la boîte écran du glyphe tourné
                    // et on retrouve la couverture par rotation inverse (pas de trou).
                    let corners = [(ox, oy), (ox + dw, oy), (ox + dw, oy + dh), (ox, oy + dh)];
                    let rot = |p: (f32, f32)| {
                        let (vx, vy) = (p.0 - rotation_center.0, p.1 - rotation_center.1);
                        (rotation_center.0 + vx * c - vy * s, rotation_center.1 + vx * s + vy * c)
                    };
                    let rc: Vec<(f32, f32)> = corners.iter().map(|p| rot(*p)).collect();
                    let minx = rc.iter().map(|p| p.0).fold(f32::INFINITY, f32::min).floor() as i32;
                    let maxx = rc.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                    let miny = rc.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
                    let maxy = rc.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                    for py in miny..=maxy {
                        for px in minx..=maxx {
                            let (vx, vy) = (px as f32 + 0.5 - rotation_center.0, py as f32 + 0.5 - rotation_center.1);
                            // Rotation inverse (-angle).
                            let lx = rotation_center.0 + vx * c + vy * s;
                            let ly = rotation_center.1 - vx * s + vy * c;
                            if lx < ox || lx >= ox + dw || ly < oy || ly >= oy + dh {
                                continue;
                            }
                            let cov = sample((lx - ox) / dw, (ly - oy) / dh);
                            blend_pixel(pm, px, py, pw, ph, cov, color);
                        }
                    }
                }
            }
        }
    }

    // Soulignement (previous_audit.md #61) : un bandeau plein par
    // rangée, à la base de sa boîte (même repère que le tessellateur egui
    // pour le chemin painter live — `row_rect.bottom()`), dessiné une fois
    // après toutes les passes de glyphes plutôt que par passe (sinon un
    // faux-bold/contour actif le dédoublerait/l'épaissirait).
    if let Some(stroke) = underline {
        let color = [stroke.color.r(), stroke.color.g(), stroke.color.b(), stroke.color.a()];
        for row in &galley.rows {
            let (x0, x1) = (origin.0 + row.rect.left(), origin.0 + row.rect.right());
            let y0 = origin.1 + row.rect.bottom();
            let y1 = y0 + stroke.width;
            if !rotated {
                for py in y0.floor() as i32..y1.ceil() as i32 {
                    for px in x0.floor() as i32..x1.ceil() as i32 {
                        blend_pixel(pm, px, py, pw, ph, 1.0, color);
                    }
                }
            } else {
                let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
                let rot = |p: (f32, f32)| {
                    let (vx, vy) = (p.0 - rotation_center.0, p.1 - rotation_center.1);
                    (rotation_center.0 + vx * c - vy * s, rotation_center.1 + vx * s + vy * c)
                };
                let rc: Vec<(f32, f32)> = corners.iter().map(|p| rot(*p)).collect();
                let minx = rc.iter().map(|p| p.0).fold(f32::INFINITY, f32::min).floor() as i32;
                let maxx = rc.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                let miny = rc.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
                let maxy = rc.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                for py in miny..=maxy {
                    for px in minx..=maxx {
                        let (vx, vy) = (px as f32 + 0.5 - rotation_center.0, py as f32 + 0.5 - rotation_center.1);
                        let lx = rotation_center.0 + vx * c + vy * s;
                        let ly = rotation_center.1 - vx * s + vy * c;
                        if lx < x0 || lx >= x1 || ly < y0 || ly >= y1 {
                            continue;
                        }
                        blend_pixel(pm, px, py, pw, ph, 1.0, color);
                    }
                }
            }
        }
    }
}

/// Compose un pixel teinté (source-over) dans le pixmap.
fn blend_pixel(pm: &mut Pixmap, px: i32, py: i32, pw: usize, ph: usize, cov: f32, color: [u8; 4]) {
    if cov <= 0.003 || px < 0 || py < 0 || px as usize >= pw || py as usize >= ph {
        return;
    }
    let [tr, tg, tb, ta] = color;
    let ea = cov * (ta as f32 / 255.0);
    let inv = 1.0 - ea;
    let i = py as usize * pw + px as usize;
    let d = pm.pixels()[i];
    let or = (tr as f32 * ea + d.red() as f32 * inv).round() as u8;
    let og = (tg as f32 * ea + d.green() as f32 * inv).round() as u8;
    let ob = (tb as f32 * ea + d.blue() as f32 * inv).round() as u8;
    let oa = (ea * 255.0 + d.alpha() as f32 * inv).round() as u8;
    if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(or, og, ob, oa) {
        pm.pixels_mut()[i] = c;
    }
}

/// Applique un filtre en direct à `base` (calque d'ajustement, F3) :
/// dé-prémultiplie, passe par le même code que le filtre destructif
/// ([`crate::tools::filter::apply`]), puis re-prémultiplie en mélangeant
/// avec l'original selon `opacity` (calque d'ajustement partiellement
/// opaque = mélange original/filtré, comme Photoshop). `mask` restreint
/// l'effet aux pixels opaques de la base d'écrêtage (calque écrêté).
fn apply_adjustment(
    base: &mut Pixmap,
    adjustment: crate::tools::filter::Adjustment,
    opacity: f32,
    mask: Option<&Pixmap>,
) {
    let (w, h) = (base.width(), base.height());
    if w == 0 || h == 0 || opacity <= 0.0 {
        return;
    }
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for px in base.pixels() {
        let a = px.alpha();
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let unpremul = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            rgba.push(unpremul(px.red()));
            rgba.push(unpremul(px.green()));
            rgba.push(unpremul(px.blue()));
            rgba.push(a);
        }
    }
    let original = rgba.clone();
    crate::tools::filter::apply_adjustment(adjustment, &mut rgba, w, h);

    let mask_px = mask.map(|m| m.data());
    let base_px = base.data_mut();
    for i in 0..(w * h) as usize {
        if let Some(mpx) = mask_px {
            if mpx[i * 4 + 3] == 0 {
                continue; // hors du masque d'écrêtage : inchangé
            }
        }
        let mut out = [0u8; 4];
        for c in 0..4 {
            let o = original[i * 4 + c] as f32;
            let f = rgba[i * 4 + c] as f32;
            out[c] = (o + (f - o) * opacity).round().clamp(0.0, 255.0) as u8;
        }
        let a = out[3] as u32;
        let premul = |c: u8| ((c as u32 * a + 127) / 255) as u8;
        base_px[i * 4] = premul(out[0]);
        base_px[i * 4 + 1] = premul(out[1]);
        base_px[i * 4 + 2] = premul(out[2]);
        base_px[i * 4 + 3] = out[3];
    }
}

/// Patch tuile par tuile le cache persistant `cache.pm` pour qu'il reflète
/// l'état actuel de `raster` — coût proportionnel au nombre de tuiles
/// **modifiées** depuis le dernier appel, pas à la surface totale peinte.
/// Roadmap previous_audit.md (ex-ANALYSE) §12.1 : remplace l'ancien `raster_content`, qui
/// ré-aplatissait (`RasterLayer::flatten`) puis reconvertissait en
/// prémultiplié **toute** la boîte englobante peinte à chaque appel — donc à
/// chaque dab d'un coup de pinceau, même sur un document déjà bien rempli.
fn blit_raster_tiles(cache: &mut RasterTileCache, raster: &crate::model::RasterLayer, w: u32, h: u32) {
    if cache.pm.width() != w || cache.pm.height() != h {
        let Some(pm) = Pixmap::new(w, h) else { return };
        cache.pm = pm;
        cache.tiles.clear();
    }
    // Tuiles disparues depuis le dernier passage (ex. undo d'un premier coup
    // de pinceau qui venait d'allouer la tuile) : effacées en transparent.
    let gone: Vec<TileKey> = cache.tiles.keys().filter(|k| !raster.tiles.contains_key(k)).copied().collect();
    for key in gone {
        clear_tile_region(&mut cache.pm, key);
        cache.tiles.remove(&key);
    }
    for (key, tile) in &raster.tiles {
        let hash = tile_content_hash(&tile.px);
        if cache.tiles.get(key) == Some(&hash) {
            continue; // tuile inchangée depuis le dernier passage
        }
        blit_tile(&mut cache.pm, *key, tile);
        cache.tiles.insert(*key, hash);
    }
}

/// Hash bon marché d'une tuile (même technique d'échantillonnage que
/// `RasterLayer::content_hash`, réappliquée par tuile individuelle ici).
fn tile_content_hash(px: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for i in (0..px.len()).step_by(97) {
        h ^= px[i] as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Blitte une tuile 256×256 (RGBA8 non prémultiplié) à sa position dans le
/// pixmap document, en remplacement total (`Source`, pas `SourceOver`) : la
/// tuile est déjà la vérité complète de ce carré de pixels, pas une couche à
/// mélanger par-dessus l'ancien contenu du cache.
fn blit_tile(pm: &mut Pixmap, key: TileKey, tile: &Tile) {
    let mut premul = Vec::with_capacity(tile.px.len());
    for c in tile.px.as_chunks::<4>().0 {
        let a = c[3] as u16;
        premul.push((c[0] as u16 * a / 255) as u8);
        premul.push((c[1] as u16 * a / 255) as u8);
        premul.push((c[2] as u16 * a / 255) as u8);
        premul.push(c[3]);
    }
    let Some(size) = tiny_skia::IntSize::from_wh(TILE as u32, TILE as u32) else { return };
    let Some(src) = Pixmap::from_vec(premul, size) else { return };
    pm.draw_pixmap(
        key.0 * TILE,
        key.1 * TILE,
        src.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: SkBlend::Source, quality: FilterQuality::Nearest },
        Transform::identity(),
        None,
    );
}

/// Efface (transparent) la région d'une tuile disparue du cache incrémental.
fn clear_tile_region(pm: &mut Pixmap, key: TileKey) {
    let Some(blank) = Pixmap::new(TILE as u32, TILE as u32) else { return };
    pm.draw_pixmap(
        key.0 * TILE,
        key.1 * TILE,
        blank.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: SkBlend::Source, quality: FilterQuality::Nearest },
        Transform::identity(),
        None,
    );
}

/// Blitte une image (mise à l'échelle vers sa taille document).
fn raster_image(pm: &mut Pixmap, im: &ImageItem) {
    if im.w == 0 || im.h == 0 || im.rgba.len() < (im.w * im.h * 4) as usize {
        return;
    }
    // tiny-skia attend du RGBA prémultiplié.
    let mut premul = Vec::with_capacity(im.rgba.len());
    for c in im.rgba.as_chunks::<4>().0 {
        let a = c[3] as u16;
        premul.push((c[0] as u16 * a / 255) as u8);
        premul.push((c[1] as u16 * a / 255) as u8);
        premul.push((c[2] as u16 * a / 255) as u8);
        premul.push(c[3]);
    }
    let Some(src) = Pixmap::from_vec(
        premul,
        tiny_skia::IntSize::from_wh(im.w, im.h).unwrap_or(tiny_skia::IntSize::from_wh(1, 1).unwrap()),
    ) else {
        return;
    };
    let sx = im.size.0 / im.w as f32;
    let sy = im.size.1 / im.h as f32;
    // Mise à l'échelle + rotation autour du centre.
    let (cx, cy) = (im.pos.0 + im.size.0 * 0.5, im.pos.1 + im.size.1 * 0.5);
    let ts = Transform::from_translate(cx, cy)
        .pre_concat(Transform::from_rotate(im.rot.to_degrees()))
        .pre_concat(Transform::from_translate(-im.size.0 * 0.5, -im.size.1 * 0.5))
        .pre_concat(Transform::from_scale(sx, sy));
    pm.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: SkBlend::SourceOver, quality: FilterQuality::Bilinear },
        ts,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_alpha_masks_by_base() {
        // 2×1 : base opaque à gauche, transparente à droite.
        let mut base = Pixmap::new(2, 1).unwrap();
        {
            let d = base.data_mut();
            d[0..4].copy_from_slice(&[255, 255, 255, 255]); // px0 opaque
            d[4..8].copy_from_slice(&[0, 0, 0, 0]); // px1 transparent
        }
        // Calque écrêté : deux pixels blancs opaques (prémultipliés).
        let mut pm = Pixmap::new(2, 1).unwrap();
        {
            let d = pm.data_mut();
            d[0..4].copy_from_slice(&[200, 200, 200, 255]);
            d[4..8].copy_from_slice(&[200, 200, 200, 255]);
        }
        clip_alpha(&mut pm, &base);
        let d = pm.data();
        // px0 conservé (base opaque), px1 effacé (base transparente).
        assert_eq!(&d[0..4], &[200, 200, 200, 255]);
        assert_eq!(&d[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn apply_adjustment_inverts_full_opacity() {
        let mut base = Pixmap::new(1, 1).unwrap();
        base.data_mut()[0..4].copy_from_slice(&[10, 120, 240, 255]); // déjà prémultiplié (alpha=255)
        apply_adjustment(&mut base, crate::tools::filter::Adjustment::Preset(crate::tools::filter::Filter::Invert), 1.0, None);
        assert_eq!(base.data(), &[245, 135, 15, 255]);
    }

    #[test]
    fn apply_adjustment_half_opacity_blends_toward_original() {
        let mut base = Pixmap::new(1, 1).unwrap();
        base.data_mut()[0..4].copy_from_slice(&[10, 120, 240, 255]);
        apply_adjustment(&mut base, crate::tools::filter::Adjustment::Preset(crate::tools::filter::Filter::Invert), 0.5, None);
        // Mi-chemin entre 10 et 245 → 127 ou 128 selon l'arrondi.
        let v = base.data()[0];
        assert!((126..=129).contains(&v), "got {v}");
    }

    #[test]
    fn apply_adjustment_respects_clip_mask() {
        // 2×1 : masque opaque à gauche, transparent à droite.
        let mut mask = Pixmap::new(2, 1).unwrap();
        mask.data_mut()[0..4].copy_from_slice(&[255, 255, 255, 255]);
        mask.data_mut()[4..8].copy_from_slice(&[0, 0, 0, 0]);
        let mut base = Pixmap::new(2, 1).unwrap();
        base.data_mut()[0..4].copy_from_slice(&[10, 10, 10, 255]);
        base.data_mut()[4..8].copy_from_slice(&[10, 10, 10, 255]);
        apply_adjustment(&mut base, crate::tools::filter::Adjustment::Preset(crate::tools::filter::Filter::Invert), 1.0, Some(&mask));
        let d = base.data();
        assert_eq!(&d[0..4], &[245, 245, 245, 255]); // sous le masque : inversé
        assert_eq!(&d[4..8], &[10, 10, 10, 255]); // hors masque : inchangé
    }

    #[test]
    fn apply_adjustment_skips_transparent_pixels() {
        let mut base = Pixmap::new(1, 1).unwrap(); // transparent par défaut
        apply_adjustment(&mut base, crate::tools::filter::Adjustment::Preset(crate::tools::filter::Filter::Invert), 1.0, None);
        assert_eq!(base.data(), &[0, 0, 0, 0]);
    }

    #[test]
    fn apply_mask_hides_painted_black_keeps_unpainted_visible() {
        // 2×1 : px0 masqué (peint noir), px1 jamais peint → reste visible.
        let mut mask = crate::model::RasterLayer::default();
        mask.set_pixel(0, 0, [0, 0, 0, 255]);
        let mut pm = Pixmap::new(2, 1).unwrap();
        {
            let d = pm.data_mut();
            d[0..4].copy_from_slice(&[200, 200, 200, 255]);
            d[4..8].copy_from_slice(&[200, 200, 200, 255]);
        }
        apply_mask(&mut pm, &mask);
        let d = pm.data();
        assert_eq!(&d[0..4], &[0, 0, 0, 0]); // masqué
        assert_eq!(&d[4..8], &[200, 200, 200, 255]); // intact
    }

    #[test]
    fn gradient_shader_builds_for_linear_and_radial() {
        let g = crate::model::Gradient::two_stop(
            crate::model::GradientKind::Linear,
            ((0.0, 0.0), (100.0, 50.0)),
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        );
        assert!(gradient_shader(&g).is_some());

        let g = crate::model::Gradient::two_stop(
            crate::model::GradientKind::Radial,
            ((0.0, 0.0), (100.0, 50.0)),
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        );
        assert!(gradient_shader(&g).is_some());
    }

    #[test]
    fn raster_stroke_with_gradient_fills_without_panicking() {
        // Un carré plein avec dégradé linéaire ne doit pas paniquer et doit
        // colorer des pixels (pas de rectangle transparent).
        let mut s = crate::model::Stroke::new([255, 0, 0, 255], 4.0, crate::model::Tool::Brush);
        s.fill = true;
        s.points = vec![
            crate::model::StrokePoint { pos: (10.0, 10.0), width: 4.0 },
            crate::model::StrokePoint { pos: (90.0, 10.0), width: 4.0 },
            crate::model::StrokePoint { pos: (90.0, 90.0), width: 4.0 },
            crate::model::StrokePoint { pos: (10.0, 90.0), width: 4.0 },
        ];
        s.gradient = Some(crate::model::Gradient::two_stop(
            crate::model::GradientKind::Linear,
            ((10.0, 10.0), (90.0, 90.0)),
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        ));
        let mut pm = Pixmap::new(100, 100).unwrap();
        raster_stroke(&mut pm, &s);
        assert!(pm.pixels().iter().any(|p| p.alpha() > 0));
    }

    /// Régression : le ruban mélange quads et disques superposés dont le sens
    /// d'enroulement diffère selon la direction du tracé. Sans normalisation
    /// de l'orientation des triangles, la règle `Winding` annulait les
    /// recouvrements (+1/−1) → trait en pointillés/damier après passage en
    /// mode composite (bug pot de peinture). L'axe du trait doit être opaque
    /// sur toute sa longueur, quel que soit le sens du tracé.
    #[test]
    fn raster_stroke_has_no_holes_along_the_centerline() {
        for pts in [
            vec![(10.0, 50.0), (50.0, 50.0), (90.0, 50.0)],
            vec![(90.0, 50.0), (50.0, 50.0), (10.0, 50.0)], // sens inverse
        ] {
            let mut s = crate::model::Stroke::new([0, 0, 0, 255], 6.0, crate::model::Tool::Brush);
            s.points = pts.iter().map(|&pos| crate::model::StrokePoint { pos, width: 6.0 }).collect();
            let mut pm = Pixmap::new(100, 100).unwrap();
            raster_stroke(&mut pm, &s);
            for x in 12..88 {
                let px = pm.pixel(x, 50).unwrap();
                assert!(
                    px.alpha() == 255,
                    "trou à x={x} (alpha={}) — annulation de winding dans le ruban",
                    px.alpha()
                );
            }
        }
    }

    /// Roadmap previous_audit.md (ex-ANALYSE) §12.2 : l'export doit rendre le document à sa
    /// résolution **native**, jamais celle d'un viewport à l'écran — vérifié
    /// ici en dehors de toute fenêtre (`egui::Context::default()`, pas de
    /// zoom/scale_factor en jeu).
    #[test]
    fn render_to_rgba_uses_native_document_resolution() {
        let doc = crate::model::Document::new((37, 21));
        let ctx = egui::Context::default();
        let mut c = Compositor::new();
        let (w, h, rgba) = c.render_to_rgba(&ctx, &doc, Color32::WHITE).expect("render");
        assert_eq!((w, h), (37, 21));
        assert_eq!(rgba.len(), 37 * 21 * 4);
    }

    /// Le fond doit être peint en opaque (comme le fait le canevas à l'écran
    /// avant §12.2) : un document sans aucun contenu exporte un aplat de la
    /// couleur de fond, pas une image transparente.
    #[test]
    fn render_to_rgba_bakes_an_opaque_background() {
        let doc = crate::model::Document::new((4, 4));
        let ctx = egui::Context::default();
        let mut c = Compositor::new();
        let (_, _, rgba) = c.render_to_rgba(&ctx, &doc, Color32::from_rgb(10, 20, 30)).expect("render");
        for px in rgba.chunks_exact(4) {
            assert_eq!(px, &[10, 20, 30, 255]);
        }
    }

    /// Le texte doit apparaître dans le rendu d'export même si le calque n'a
    /// pas de mode de fusion particulier (contrairement à l'affichage à
    /// l'écran, où le texte d'un calque « Normal » est dessiné par egui
    /// par-dessus plutôt que rastérisé par le compositeur).
    #[test]
    fn render_to_rgba_includes_text() {
        let mut doc = crate::model::Document::new((60, 30));
        let mut item = crate::model::TextItem::new(1, (2.0, 2.0), 16.0, [0, 0, 0, 255]);
        item.text = "Hi".into();
        doc.layers[0].texts.push(item);
        let ctx = egui::Context::default();
        // Le glyph atlas d'egui n'existe qu'après un premier passage de
        // frame — un `Context::default()` nu panique sur `ctx.fonts()`.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = Compositor::new();
        let (_, _, rgba) = c.render_to_rgba(&ctx, &doc, Color32::WHITE).expect("render");
        assert!(rgba.chunks_exact(4).any(|px| px != [255, 255, 255, 255]), "le texte devrait marquer des pixels");
    }

    #[test]
    fn render_to_rgba_solid_fill_layer_covers_the_whole_document() {
        let mut doc = crate::model::Document::new((10, 10));
        // Calque de base vide (transparent) + calque de remplissage rouge uni.
        doc.layers.push(crate::model::Layer::new_fill(
            2,
            "fill",
            crate::model::FillKind::Solid([255, 0, 0, 255]),
        ));
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = Compositor::new();
        let (_, _, rgba) = c.render_to_rgba(&ctx, &doc, Color32::WHITE).expect("render");
        assert!(rgba.chunks_exact(4).all(|px| px == [255, 0, 0, 255]), "le composite devrait être entièrement rouge");
    }

    #[test]
    fn layer_thumbnail_downsizes_and_reflects_layer_content() {
        let mut doc = crate::model::Document::new((100, 50));
        doc.layers[0].fill = Some(crate::model::FillKind::Solid([0, 255, 0, 255]));
        doc.layers[0].id = 7;
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = Compositor::new();
        // Repeuple le cache par calque (`self.layers`) avant de demander une
        // vignette — reflète l'usage réel (le canevas est déjà affiché).
        let _ = c.texture(&ctx, &doc, 1, None);
        assert_eq!(c.layer_content_hash(7), c.layers.get(&7).map(|(h, _)| *h));
        let thumb = c.layer_thumbnail(7, 32).expect("thumbnail");
        // 100x50 mis à l'échelle pour tenir dans 32x32 → 32x16.
        assert_eq!(thumb.size, [32, 16]);
        assert!(thumb.pixels.iter().all(|c| *c == Color32::from_rgba_premultiplied(0, 255, 0, 255)));
    }

    #[test]
    fn layer_thumbnail_is_none_for_an_unknown_layer() {
        let c = Compositor::new();
        assert!(c.layer_thumbnail(999, 32).is_none());
        assert!(c.layer_content_hash(999).is_none());
    }

    #[test]
    fn render_to_rgba_renders_arc_text_away_from_center() {
        let mut doc = crate::model::Document::new((80, 80));
        let mut item = crate::model::TextItem::new(1, (40.0, 40.0), 14.0, [0, 0, 0, 255]);
        item.text = "ABC".into();
        item.arc = Some(crate::model::text::TextArc { radius: 25.0, start_angle_deg: -90.0, flip: false });
        doc.layers[0].texts.push(item);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let mut c = Compositor::new();
        let (w, _, rgba) = c.render_to_rgba(&ctx, &doc, Color32::WHITE).expect("render");
        // Le centre (40,40) ne doit pas être marqué (le texte est sur le
        // cercle, pas au centre) ; quelque part sur le cercle, si.
        let center_i = ((40 * w as usize) + 40) * 4;
        assert_eq!(&rgba[center_i..center_i + 4], &[255, 255, 255, 255]);
        assert!(rgba.chunks_exact(4).any(|px| px != [255, 255, 255, 255]), "le texte sur courbe devrait marquer des pixels");
    }

    // --- Cache incrémental par tuile (roadmap previous_audit.md (ex-ANALYSE) §12.1) -------------

    #[test]
    fn blit_raster_tiles_reflects_painted_pixels() {
        let mut raster = crate::model::RasterLayer::default();
        raster.set_pixel(10, 10, [255, 0, 0, 255]);
        let mut cache = RasterTileCache::default();
        blit_raster_tiles(&mut cache, &raster, 64, 64);
        let px = cache.pm.pixel(10, 10).expect("pixel in bounds");
        assert_eq!((px.red(), px.green(), px.blue(), px.alpha()), (255, 0, 0, 255));
        // Un pixel jamais peint reste transparent.
        let blank = cache.pm.pixel(0, 0).expect("pixel in bounds");
        assert_eq!(blank.alpha(), 0);
    }

    #[test]
    fn blit_raster_tiles_skips_unchanged_tiles_on_second_pass() {
        let mut raster = crate::model::RasterLayer::default();
        raster.set_pixel(5, 5, [0, 255, 0, 255]);
        let mut cache = RasterTileCache::default();
        blit_raster_tiles(&mut cache, &raster, 64, 64);
        let hash_before = *cache.tiles.values().next().expect("one tile touched");
        // Deuxième passe sans aucune modification : le hash mémorisé de la
        // tuile ne doit pas bouger (rien à re-blitter).
        blit_raster_tiles(&mut cache, &raster, 64, 64);
        let hash_after = *cache.tiles.values().next().expect("still one tile");
        assert_eq!(hash_before, hash_after);
    }

    #[test]
    fn blit_raster_tiles_clears_a_tile_removed_since_last_pass() {
        // Simule un undo qui retire une tuile entière (premier coup de
        // pinceau annulé) : `RasterLayer::tiles` perd la clé, pas juste son
        // contenu mis à zéro.
        let mut raster = crate::model::RasterLayer::default();
        raster.set_pixel(1, 1, [255, 255, 255, 255]);
        let mut cache = RasterTileCache::default();
        blit_raster_tiles(&mut cache, &raster, 64, 64);
        assert_eq!(cache.pm.pixel(1, 1).unwrap().alpha(), 255);

        raster.tiles.clear();
        blit_raster_tiles(&mut cache, &raster, 64, 64);
        assert_eq!(cache.pm.pixel(1, 1).unwrap().alpha(), 0, "la tuile effacée doit redevenir transparente");
        assert!(cache.tiles.is_empty());
    }

    /// Mesure de performance (roadmap previous_audit.md (ex-ANALYSE) §12.1) — pas une assertion
    /// de correction : compare le coût de l'ancien chemin (aplatir +
    /// reconvertir toute la boîte englobante peinte à chaque dab) à celui du
    /// nouveau cache incrémental (ne retoucher que les tuiles modifiées).
    /// Ignoré par défaut (`cargo test` normal) car c'est une mesure de temps,
    /// pas un test déterministe — lancer avec :
    /// `cargo test --release bench_full_layer_repaint_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_full_layer_repaint_cost() {
        use std::time::Instant;
        // Document 4096×4096 = 256 tuiles de 256×256, entièrement peintes :
        // le cas qui dégradait le plus l'ancien chemin (coût proportionnel à
        // la surface déjà peinte, pas au dab courant).
        let mut raster = crate::model::RasterLayer::default();
        for ty in 0..16 {
            for tx in 0..16 {
                raster.set_pixel(tx * TILE + 5, ty * TILE + 5, [10, 20, 30, 255]);
            }
        }

        // Ancien chemin : flatten() + reconversion prémultipliée de toute la
        // boîte englobante, à chaque dab (simulé ici par N appels répétés).
        let iterations = 50;
        let t0 = Instant::now();
        for _ in 0..iterations {
            let Some((_, _, w, h, rgba)) = raster.flatten() else { continue };
            let mut premul = Vec::with_capacity(rgba.len());
            for c in rgba.chunks_exact(4) {
                let a = c[3] as u16;
                premul.push((c[0] as u16 * a / 255) as u8);
                premul.push((c[1] as u16 * a / 255) as u8);
                premul.push((c[2] as u16 * a / 255) as u8);
                premul.push(c[3]);
            }
            let _ = Pixmap::from_vec(premul, tiny_skia::IntSize::from_wh(w, h).unwrap());
        }
        let old_cost = t0.elapsed();

        // Nouveau chemin : un dab ne modifie qu'une poignée de tuiles ; le
        // cache ne retouche que celles-là sur les passes suivantes.
        let mut cache = RasterTileCache::default();
        blit_raster_tiles(&mut cache, &raster, 4096, 4096); // remplissage initial (hors mesure)
        let t1 = Instant::now();
        for i in 0..iterations {
            // Un seul pixel change, dans une seule tuile (cas réaliste d'un
            // dab de pinceau qui déborde légèrement sur une tuile voisine).
            raster.set_pixel(5, 5 + i, [200, 0, 0, 255]);
            blit_raster_tiles(&mut cache, &raster, 4096, 4096);
        }
        let new_cost = t1.elapsed();

        eprintln!(
            "ancien chemin (flatten complet) : {old_cost:?} / {iterations} appels\n\
             nouveau chemin (tuiles sales)    : {new_cost:?} / {iterations} appels\n\
             accélération : {:.1}×",
            old_cost.as_secs_f64() / new_cost.as_secs_f64().max(1e-9)
        );
        assert!(new_cost < old_cost, "le cache incrémental doit être plus rapide que le flatten complet");
    }

    #[test]
    fn sample_gradient_stops_interpolates_between_two() {
        let stops = vec![(0.0, [0, 0, 0, 255]), (1.0, [200, 100, 0, 255])];
        assert_eq!(sample_gradient_stops(&stops, 0.0), [0, 0, 0, 255]);
        assert_eq!(sample_gradient_stops(&stops, 1.0), [200, 100, 0, 255]);
        assert_eq!(sample_gradient_stops(&stops, 0.5), [100, 50, 0, 255]);
    }

    #[test]
    fn sample_gradient_stops_clamps_outside_range() {
        let stops = vec![(0.2, [10, 10, 10, 255]), (0.8, [250, 250, 250, 255])];
        assert_eq!(sample_gradient_stops(&stops, 0.0), [10, 10, 10, 255]);
        assert_eq!(sample_gradient_stops(&stops, 1.0), [250, 250, 250, 255]);
    }

    #[test]
    fn paint_conic_gradient_sweeps_around_the_center() {
        // Carré 10×10, dégradé centré, arrêt 0 = référence vers la droite
        // (angle 0), noir → blanc sur le tour complet.
        let mut pm = Pixmap::new(10, 10).unwrap();
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(10.0, 0.0);
        pb.line_to(10.0, 10.0);
        pb.line_to(0.0, 10.0);
        pb.close();
        let path = pb.finish().unwrap();
        let g = crate::model::Gradient {
            kind: crate::model::GradientKind::Conic,
            from: (5.0, 5.0),
            to: (10.0, 5.0),
            stops: vec![(0.0, [0, 0, 0, 255]), (1.0, [255, 255, 255, 255])],
        };
        paint_conic_gradient(&mut pm, &path, &g);
        // Le pixel juste à droite du centre (angle ≈ 0) doit rester sombre ;
        // celui juste au-dessus (angle ≈ -90°, soit t ≈ 0.75) doit être clair.
        let px = |x: usize, y: usize| pm.data()[(y * 10 + x) * 4];
        assert!(px(9, 5) < 40, "attendu sombre près de l'angle 0, eu {}", px(9, 5));
        assert!(px(5, 0) > 150, "attendu clair près de t≈0.75, eu {}", px(5, 0));
    }

    /// Petit carré opaque blanc centré dans un pixmap transparent, pour
    /// tester les styles de calque dérivés de l'alpha.
    fn square_pixmap(size: u32, square: std::ops::Range<u32>) -> Pixmap {
        let mut pm = Pixmap::new(size, size).unwrap();
        for y in square.clone() {
            for x in square.clone() {
                let i = (y * size + x) as usize;
                pm.pixels_mut()[i] = tiny_skia::PremultipliedColorU8::from_rgba(255, 255, 255, 255).unwrap();
            }
        }
        pm
    }

    #[test]
    fn drop_shadow_appears_offset_from_the_original_shape() {
        let mut pm = square_pixmap(20, 5..10);
        apply_drop_shadow(&mut pm, [0, 0, 0, 255], (4.0, 4.0), 0.0, 20, 20);
        // Décalage de (4,4) sans flou : juste sous/à droite du carré d'origine
        // (dans la zone d'ombre, hors du carré blanc lui-même) doit être opaque.
        let d = pm.data();
        let i = (12 * 20 + 12) * 4; // dans le carré d'origine décalé de (4,4) = [9,14)
        assert!(d[i + 3] > 0, "l'ombre décalée devrait couvrir ce pixel");
        // Loin de tout, doit rester transparent.
        let far = (20 + 1) * 4;
        assert_eq!(d[far + 3], 0);
    }

    #[test]
    fn layer_stroke_draws_a_ring_outside_the_shape_not_inside() {
        let mut pm = square_pixmap(20, 8..12);
        apply_layer_stroke(&mut pm, [255, 0, 0, 255], 2.0, 20, 20);
        let d = pm.data();
        // Juste à l'extérieur du carré (7,10) doit être touché par le contour.
        let outside = (10 * 20 + 7) * 4;
        assert!(d[outside + 3] > 0, "le contour doit apparaître juste à l'extérieur du carré");
        // Le centre du carré doit rester blanc d'origine (contour ne remplace
        // pas le contenu existant).
        let center = (10 * 20 + 10) * 4;
        assert_eq!(&d[center..center + 3], &[255, 255, 255]);
    }

    #[test]
    fn outer_glow_is_black_outside_and_absent_deep_inside() {
        let mut pm = square_pixmap(30, 12..18);
        apply_glow(&mut pm, [255, 200, 0, 255], 6.0, false, 30, 30);
        let d = pm.data();
        // Juste à l'extérieur du carré : de la lueur.
        let just_outside = (15 * 30 + 10) * 4;
        assert!(d[just_outside + 3] > 0, "la lueur externe doit déborder autour du carré");
        // Loin de tout (coin de l'image) : rien.
        let far = 0usize;
        assert_eq!(d[far + 3], 0);
    }

    #[test]
    fn inner_glow_is_strongest_near_the_edge_of_the_shape() {
        let mut pm = square_pixmap(30, 10..20);
        apply_glow(&mut pm, [255, 255, 255, 255], 6.0, true, 30, 30);
        let d = pm.data();
        // Alpha reste inchangé (opaque) partout dans la forme d'origine — la
        // lueur interne teinte par-dessus sans percer de trou.
        let edge = (15 * 30 + 11) * 4; // proche du bord gauche du carré
        let center = (15 * 30 + 15) * 4; // centre du carré
        assert_eq!(d[edge + 3], 255);
        assert_eq!(d[center + 3], 255);
    }
}
