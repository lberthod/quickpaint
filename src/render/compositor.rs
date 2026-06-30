//! Compositing CPU par calque (roadmap #8, voie A).
//!
//! Rastérise chaque calque sur un `Pixmap` tiny-skia (traits via leur géométrie
//! de ruban, images blittées), puis compose les calques avec leur **mode de
//! fusion** et leur **opacité de groupe** (vrai compositing, pas par-trait). Le
//! résultat est mis en cache comme texture egui, invalidé par une signature.
//!
//! Limite v1 : le **texte** n'est pas rastérisé ici (pas de rasteriseur de
//! polices) — il est dessiné par egui par-dessus. Activé seulement quand un
//! calque a un mode ≠ Normal ou une opacité < 1 (sinon rendu vectoriel net).

use crate::model::{BlendMode, Document, ImageItem, Stroke, Tool};
use crate::render::ribbon;
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use tiny_skia::{
    BlendMode as SkBlend, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint,
    Transform,
};

#[derive(Default)]
pub struct Compositor {
    tex: Option<TextureHandle>,
    sig: u64,
    /// Cache par calque : id → (hash de contenu, pixmap rastérisé). Évite de
    /// re-rastériser les calques inchangés (perf en mode fusion multi-calques).
    layers: std::collections::HashMap<u64, (u64, Pixmap)>,
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
            if let Some(ci) = self.rebuild(ctx, doc, skip_text) {
                self.tex = Some(ctx.load_texture("composite", ci, TextureOptions::LINEAR));
            }
            self.sig = sig;
        }
        self.tex.as_ref()
    }

    /// Compose le document : ne rastérise que les calques dont le hash a changé.
    fn rebuild(&mut self, ctx: &egui::Context, doc: &Document, skip_text: Option<u64>) -> Option<ColorImage> {
        let (w, h) = doc.size;
        let mut base = Pixmap::new(w, h)?;
        let atlas = ctx.fonts(|f| f.image());
        let mut live = std::collections::HashSet::new();
        for layer in &doc.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            live.insert(layer.id);
            let hash = layer_hash(layer, skip_text);
            let stale = self.layers.get(&layer.id).map(|(h, _)| *h != hash).unwrap_or(true);
            if stale {
                let mut lp = Pixmap::new(w, h)?;
                for r in layer.z_order() {
                    match r {
                        crate::model::ElemRef::Stroke(i) => raster_stroke(&mut lp, &layer.strokes[i]),
                        crate::model::ElemRef::Image(i) => raster_image(&mut lp, &layer.images[i]),
                        crate::model::ElemRef::Text(i) => {
                            let t = &layer.texts[i];
                            if Some(t.id) != skip_text {
                                raster_text(ctx, &mut lp, t, &atlas);
                            }
                        }
                    }
                }
                self.layers.insert(layer.id, (hash, lp));
            }
            let lp = &self.layers[&layer.id].1;
            base.draw_pixmap(
                0,
                0,
                lp.as_ref(),
                &PixmapPaint {
                    opacity: layer.opacity.clamp(0.0, 1.0),
                    blend_mode: map_blend(layer.blend),
                    quality: FilterQuality::Nearest,
                },
                Transform::identity(),
                None,
            );
        }
        self.layers.retain(|id, _| live.contains(id));

        let pixels = base
            .data()
            .chunks_exact(4)
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
    for s in &l.strokes {
        mix(s.id);
        mix(s.z.to_bits());
        mix(u32::from_le_bytes(s.color) as u64);
        mix(s.fill as u64);
        mix(s.base_width.to_bits() as u64);
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
        // Style (Sprint 3) : police, gras, alignement, contour.
        mix(t.font as u64);
        mix(t.bold as u64);
        mix(t.align as u64);
        mix(t.outline_w.to_bits() as u64);
        mix(u32::from_le_bytes(t.outline_color) as u64);
        for b in t.text.bytes() {
            mix(b as u64);
        }
    }
    h
}

fn map_blend(b: BlendMode) -> SkBlend {
    match b {
        BlendMode::Normal => SkBlend::SourceOver,
        BlendMode::Multiply => SkBlend::Multiply,
        BlendMode::Screen => SkBlend::Screen,
        BlendMode::Overlay => SkBlend::Overlay,
        BlendMode::Darken => SkBlend::Darken,
        BlendMode::Lighten => SkBlend::Lighten,
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
    for tri in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices[tri[0] as usize].pos;
        let b = mesh.vertices[tri[1] as usize].pos;
        let c = mesh.vertices[tri[2] as usize].pos;
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        pb.line_to(c.0, c.1);
        pb.close();
    }
    let Some(path) = pb.finish() else { return };
    let [r, g, b, alpha] = stroke.color;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, alpha);
    paint.anti_alias = true;
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
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
    // Mise en page partagée (police + alignement), en espace document (1 px/doc).
    let galley = crate::render::text::layout(ctx, t, 1.0);
    let (aw, ah) = (atlas.size[0], atlas.size[1]);
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let rotated = t.rot.abs() > 1e-5;
    let (c, s) = (t.rot.cos(), t.rot.sin());

    // Chaque passe (contour / gras / remplissage) blitte le galley décalé+teinté.
    for ((offx, offy), color) in crate::render::text::passes(t) {
        let base = (t.pos.0 + offx, t.pos.1 + offy);
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
                        let (vx, vy) = (p.0 - t.pos.0, p.1 - t.pos.1);
                        (t.pos.0 + vx * c - vy * s, t.pos.1 + vx * s + vy * c)
                    };
                    let rc: Vec<(f32, f32)> = corners.iter().map(|p| rot(*p)).collect();
                    let minx = rc.iter().map(|p| p.0).fold(f32::INFINITY, f32::min).floor() as i32;
                    let maxx = rc.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                    let miny = rc.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
                    let maxy = rc.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
                    for py in miny..=maxy {
                        for px in minx..=maxx {
                            let (vx, vy) = (px as f32 + 0.5 - t.pos.0, py as f32 + 0.5 - t.pos.1);
                            // Rotation inverse (-rot).
                            let lx = t.pos.0 + vx * c + vy * s;
                            let ly = t.pos.1 - vx * s + vy * c;
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

/// Blitte une image (mise à l'échelle vers sa taille document).
fn raster_image(pm: &mut Pixmap, im: &ImageItem) {
    if im.w == 0 || im.h == 0 || im.rgba.len() < (im.w * im.h * 4) as usize {
        return;
    }
    // tiny-skia attend du RGBA prémultiplié.
    let mut premul = Vec::with_capacity(im.rgba.len());
    for c in im.rgba.chunks_exact(4) {
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
