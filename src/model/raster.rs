//! Calque raster tuilé (roadmap F1 — fondation empruntée à GIMP/Photoshop).
//!
//! Les pixels peints (pinceau/gomme pixel, et bientôt pot de peinture réel /
//! tampon de clonage) vivent dans des **tuiles 256×256** allouées à la
//! demande : un calque raster vide ne consomme aucune mémoire, et un coup de
//! pinceau ne touche que les quelques tuiles qu'il traverse. C'est ce
//! découpage qui permet un undo par tuile (on ne clone que ce qui a changé)
//! et, plus tard, un rendu par dirty-rects au lieu de tout re-rastériser.
//!
//! Persistance : pas de sérialisation tuile par tuile (complexité inutile à
//! ce stade) — le contenu est aplati en un PNG borné à sa boîte englobante,
//! exactement comme `ImageItem`. Le tuilage reste un détail d'implémentation
//! interne à l'édition.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Côté d'une tuile, en pixels document.
pub const TILE: i32 = 256;

/// Coordonnées d'une tuile (indices de grille, pas des pixels).
pub type TileKey = (i32, i32);

/// Effet de retouche locale (Sprint 11) — cf. `RasterLayer::apply_effect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelEffect {
    Lighten,
    Darken,
    Saturate,
    Desaturate,
    Blur,
    Sharpen,
}

/// Une tuile de pixels RGBA8 non prémultipliés.
#[derive(Clone)]
pub struct Tile {
    pub px: Box<[u8]>,
}

impl Tile {
    fn blank() -> Self {
        Self { px: vec![0u8; (TILE * TILE * 4) as usize].into_boxed_slice() }
    }
}

impl std::fmt::Debug for Tile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tile").field("bytes", &self.px.len()).finish()
    }
}

/// Contenu peint d'un calque, organisé en tuiles éparses.
#[derive(Clone, Debug, Default)]
pub struct RasterLayer {
    pub tiles: HashMap<TileKey, Tile>,
}

impl RasterLayer {
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn tile_of(x: i32, y: i32) -> TileKey {
        (x.div_euclid(TILE), y.div_euclid(TILE))
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let key = Self::tile_of(x, y);
        let Some(t) = self.tiles.get(&key) else { return [0, 0, 0, 0] };
        let (lx, ly) = (x.rem_euclid(TILE) as usize, y.rem_euclid(TILE) as usize);
        let i = (ly * TILE as usize + lx) * 4;
        [t.px[i], t.px[i + 1], t.px[i + 2], t.px[i + 3]]
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4]) {
        let key = Self::tile_of(x, y);
        let t = self.tiles.entry(key).or_insert_with(Tile::blank);
        let (lx, ly) = (x.rem_euclid(TILE) as usize, y.rem_euclid(TILE) as usize);
        let i = (ly * TILE as usize + lx) * 4;
        t.px[i..i + 4].copy_from_slice(&rgba);
    }

    /// Composite source-over d'un pixel, pondéré par une couverture 0..=1
    /// (anti-aliasing du pinceau).
    fn blend_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4], coverage: f32) {
        if coverage <= 0.0 {
            return;
        }
        let cov = coverage.min(1.0);
        let dst = self.get_pixel(x, y);
        let sa = (rgba[3] as f32 / 255.0) * cov;
        let ia = 1.0 - sa;
        let out = [
            (rgba[0] as f32 * sa + dst[0] as f32 * ia).round() as u8,
            (rgba[1] as f32 * sa + dst[1] as f32 * ia).round() as u8,
            (rgba[2] as f32 * sa + dst[2] as f32 * ia).round() as u8,
            ((sa + (dst[3] as f32 / 255.0) * ia) * 255.0).round() as u8,
        ];
        self.set_pixel(x, y, out);
    }

    /// Liste (dédupliquée) des tuiles recoupées par un disque — sert à ne
    /// snapshotter, pour l'undo, que les tuiles réellement touchées.
    pub fn tiles_touched(cx: f32, cy: f32, r: f32) -> Vec<TileKey> {
        let r = r.max(0.5);
        let (x0, y0) = ((cx - r).floor() as i32, (cy - r).floor() as i32);
        let (x1, y1) = ((cx + r).ceil() as i32, (cy + r).ceil() as i32);
        let (k0x, k0y) = Self::tile_of(x0, y0);
        let (k1x, k1y) = Self::tile_of(x1, y1);
        let mut v = Vec::new();
        for ty in k0y..=k1y {
            for tx in k0x..=k1x {
                v.push((tx, ty));
            }
        }
        v
    }

    /// Dépose un disque doux (feathering) : plein jusqu'à `hardness * radius`,
    /// dégradé linéaire de couverture ensuite. `erase = true` retire de
    /// l'alpha existant au lieu d'en déposer (gomme pixel).
    pub fn stamp(&mut self, cx: f32, cy: f32, radius: f32, hardness: f32, rgba: [u8; 4], erase: bool) {
        if radius <= 0.0 {
            return;
        }
        let hardness = hardness.clamp(0.0, 1.0);
        let edge = hardness * radius;
        let (x0, x1) = ((cx - radius).floor() as i32, (cx + radius).ceil() as i32);
        let (y0, y1) = ((cy - radius).floor() as i32, (cy + radius).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d > radius {
                    continue;
                }
                let cov = if radius <= edge || d <= edge { 1.0 } else { 1.0 - (d - edge) / (radius - edge) };
                if erase {
                    let dst = self.get_pixel(x, y);
                    if dst[3] == 0 {
                        continue;
                    }
                    let strength = cov * (rgba[3] as f32 / 255.0);
                    let na = (dst[3] as f32 * (1.0 - strength)).round().clamp(0.0, 255.0) as u8;
                    self.set_pixel(x, y, [dst[0], dst[1], dst[2], na]);
                } else {
                    self.blend_pixel(x, y, rgba, cov);
                }
            }
        }
    }

    /// Trace un tampon échantillonné le long d'un segment (trait continu).
    pub fn stroke_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        rgba: [u8; 4],
        erase: bool,
    ) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = (radius * 0.3).max(1.0);
        let n = (dist / step).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.stamp(from.0 + dx * t, from.1 + dy * t, radius, hardness, rgba, erase);
        }
    }

    /// Tampon de clonage : dépose un disque doux en échantillonnant chaque
    /// pixel depuis `(x + offset.0, y + offset.1)` — la source est figée en
    /// un instantané avant d'écrire quoi que ce soit, pour ne pas décaler le
    /// motif recopié si source et destination se chevauchent pendant ce même
    /// tampon. `opacity` (0..=1) module l'alpha des pixels source recopiés.
    pub fn clone_stamp(&mut self, cx: f32, cy: f32, radius: f32, hardness: f32, offset: (f32, f32), opacity: f32) {
        if radius <= 0.0 {
            return;
        }
        let hardness = hardness.clamp(0.0, 1.0);
        let edge = hardness * radius;
        let (x0, x1) = ((cx - radius).floor() as i32, (cx + radius).ceil() as i32);
        let (y0, y1) = ((cy - radius).floor() as i32, (cy + radius).ceil() as i32);
        let mut samples = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d > radius {
                    continue;
                }
                let cov = if radius <= edge || d <= edge { 1.0 } else { 1.0 - (d - edge) / (radius - edge) };
                let sx = (x as f32 + offset.0).round() as i32;
                let sy = (y as f32 + offset.1).round() as i32;
                samples.push((x, y, cov, self.get_pixel(sx, sy)));
            }
        }
        for (x, y, cov, src) in samples {
            let a = (src[3] as f32 * opacity.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
            self.blend_pixel(x, y, [src[0], src[1], src[2], a], cov);
        }
    }

    /// Trace le tampon de clonage le long d'un segment (trait continu).
    pub fn clone_stamp_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        offset: (f32, f32),
        opacity: f32,
    ) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = (radius * 0.3).max(1.0);
        let n = (dist / step).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.clone_stamp(from.0 + dx * t, from.1 + dy * t, radius, hardness, offset, opacity);
        }
    }

    /// Correcteur (healing brush, Sprint 8.3) : comme le tampon de clonage,
    /// mais recale la moyenne de couleur de la texture recopiée sur la
    /// moyenne locale de la zone cible avant de peindre — un mélange de
    /// Poisson simplifié (décalage constant de couleur, pas de résolution
    /// d'équation de Poisson complète) qui conserve le détail/texture de la
    /// source sans coller un patch dont la teinte moyenne détonnerait,
    /// contrairement au clonage pur.
    pub fn heal_stamp(&mut self, cx: f32, cy: f32, radius: f32, hardness: f32, offset: (f32, f32), opacity: f32) {
        if radius <= 0.0 {
            return;
        }
        let hardness = hardness.clamp(0.0, 1.0);
        let edge = hardness * radius;
        let (x0, x1) = ((cx - radius).floor() as i32, (cx + radius).ceil() as i32);
        let (y0, y1) = ((cy - radius).floor() as i32, (cy + radius).ceil() as i32);
        let mut src_sum = [0f32; 3];
        let mut dst_sum = [0f32; 3];
        let mut weight = 0f32;
        let mut samples = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d > radius {
                    continue;
                }
                let cov = if radius <= edge || d <= edge { 1.0 } else { 1.0 - (d - edge) / (radius - edge) };
                let sx = (x as f32 + offset.0).round() as i32;
                let sy = (y as f32 + offset.1).round() as i32;
                let src = self.get_pixel(sx, sy);
                let dst = self.get_pixel(x, y);
                // Pondère par la couverture du disque ET l'opacité des deux
                // côtés : un pixel transparent (source ou destination) ne
                // doit pas fausser la moyenne de couleur.
                let w = cov * (src[3] as f32 / 255.0) * (dst[3] as f32 / 255.0);
                for c in 0..3 {
                    src_sum[c] += src[c] as f32 * w;
                    dst_sum[c] += dst[c] as f32 * w;
                }
                weight += w;
                samples.push((x, y, cov, src));
            }
        }
        let shift = if weight > 0.0 {
            [(dst_sum[0] - src_sum[0]) / weight, (dst_sum[1] - src_sum[1]) / weight, (dst_sum[2] - src_sum[2]) / weight]
        } else {
            [0.0; 3]
        };
        for (x, y, cov, src) in samples {
            let a = (src[3] as f32 * opacity.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
            let mut rgb = [0u8; 3];
            for (c, slot) in rgb.iter_mut().enumerate() {
                *slot = (src[c] as f32 + shift[c]).round().clamp(0.0, 255.0) as u8;
            }
            self.blend_pixel(x, y, [rgb[0], rgb[1], rgb[2], a], cov);
        }
    }

    /// Trace le correcteur le long d'un segment (trait continu).
    pub fn heal_stamp_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        offset: (f32, f32),
        opacity: f32,
    ) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = (radius * 0.3).max(1.0);
        let n = (dist / step).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.heal_stamp(from.0 + dx * t, from.1 + dy * t, radius, hardness, offset, opacity);
        }
    }

    /// Effet appliqué au doigté (Sprint 11) : les 4 pinceaux de retouche
    /// locale (densité +/-, éponge, flou/netteté) partagent le même parcours
    /// de disque doux que `stamp`/`clone_stamp` — seule la fonction pixel
    /// change. `strength` (0..=1) module l'intensité par coup de pinceau
    /// (répétable en repassant plusieurs fois, comme Photoshop/GIMP).
    fn apply_effect(&mut self, cx: f32, cy: f32, radius: f32, hardness: f32, strength: f32, effect: PixelEffect) {
        if radius <= 0.0 {
            return;
        }
        let hardness = hardness.clamp(0.0, 1.0);
        let edge = hardness * radius;
        let (x0, x1) = ((cx - radius).floor() as i32, (cx + radius).ceil() as i32);
        let (y0, y1) = ((cy - radius).floor() as i32, (cy + radius).ceil() as i32);
        // Instantané avant écriture : le flou/netteté échantillonnent le
        // voisinage, il ne faut pas lire des pixels déjà modifiés par ce même
        // coup de tampon (sinon la moyenne dérive en balayant le disque).
        // Marge de 1 px au-delà de la boîte englobante : le flou/netteté
        // regardent les 8 voisins de chaque pixel du disque, y compris ceux
        // tout juste sur son bord — sans cette marge, ces voisins tombaient
        // hors de l'instantané et étaient traités comme transparents,
        // assombrissant artificiellement le contour de chaque coup de
        // pinceau (faux halo sombre).
        let snapshot: HashMap<(i32, i32), [u8; 4]> = (y0 - 1..=y1 + 1)
            .flat_map(|y| (x0 - 1..=x1 + 1).map(move |x| (x, y)))
            .map(|(x, y)| ((x, y), self.get_pixel(x, y)))
            .collect();
        let sample = |x: i32, y: i32| -> [u8; 4] { snapshot.get(&(x, y)).copied().unwrap_or([0, 0, 0, 0]) };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let src = snapshot[&(x, y)];
                if src[3] == 0 {
                    continue;
                }
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d > radius {
                    continue;
                }
                let cov = if radius <= edge || d <= edge { 1.0 } else { 1.0 - (d - edge) / (radius - edge) };
                let amount = (cov * strength).clamp(0.0, 1.0);
                if amount <= 0.0 {
                    continue;
                }
                let out = match effect {
                    PixelEffect::Lighten => {
                        let mix = |c: u8| (c as f32 + (255.0 - c as f32) * amount).round() as u8;
                        [mix(src[0]), mix(src[1]), mix(src[2]), src[3]]
                    }
                    PixelEffect::Darken => {
                        let mix = |c: u8| (c as f32 * (1.0 - amount)).round() as u8;
                        [mix(src[0]), mix(src[1]), mix(src[2]), src[3]]
                    }
                    PixelEffect::Saturate | PixelEffect::Desaturate => {
                        let (h, s, l) = crate::tools::filter::rgb_to_hsl(src[0], src[1], src[2]);
                        // Un pixel (quasi) gris n'a pas de teinte
                        // significative — `rgb_to_hsl` renvoie 0.0 par
                        // convention, ce qui introduirait une dérive vers le
                        // rouge si on l'utilisait pour ré-saturer. Rien à
                        // saturer : seul un pixel déjà teinté (s > 0) peut
                        // voir sa teinte accentuée.
                        if effect == PixelEffect::Saturate && s < 0.01 {
                            src
                        } else {
                            let sign = if effect == PixelEffect::Saturate { 1.0 } else { -1.0 };
                            let ns = (s + sign * amount).clamp(0.0, 1.0);
                            let (r, g, b) = crate::tools::filter::hsl_to_rgb(h, ns, l);
                            [r, g, b, src[3]]
                        }
                    }
                    PixelEffect::Blur => {
                        let mut sum = [0f32; 3];
                        let mut n = 0f32;
                        for ny in -1..=1 {
                            for nx in -1..=1 {
                                let p = sample(x + nx, y + ny);
                                if p[3] == 0 {
                                    continue;
                                }
                                for c in 0..3 {
                                    sum[c] += p[c] as f32;
                                }
                                n += 1.0;
                            }
                        }
                        if n == 0.0 {
                            src
                        } else {
                            let avg = [sum[0] / n, sum[1] / n, sum[2] / n];
                            let mix = |c: u8, a: f32| (c as f32 + (a - c as f32) * amount).round() as u8;
                            [mix(src[0], avg[0]), mix(src[1], avg[1]), mix(src[2], avg[2]), src[3]]
                        }
                    }
                    PixelEffect::Sharpen => {
                        let mut sum = [0f32; 3];
                        let mut n = 0f32;
                        for ny in -1..=1 {
                            for nx in -1..=1 {
                                if nx == 0 && ny == 0 {
                                    continue;
                                }
                                let p = sample(x + nx, y + ny);
                                for c in 0..3 {
                                    sum[c] += p[c] as f32;
                                }
                                n += 1.0;
                            }
                        }
                        let avg = [sum[0] / n.max(1.0), sum[1] / n.max(1.0), sum[2] / n.max(1.0)];
                        // Pousse chaque canal à l'opposé de sa moyenne
                        // voisine — accentue le contraste local (masque flou
                        // simplifié).
                        let mix = |c: u8, a: f32| (c as f32 + (c as f32 - a) * amount).round().clamp(0.0, 255.0) as u8;
                        [mix(src[0], avg[0]), mix(src[1], avg[1]), mix(src[2], avg[2]), src[3]]
                    }
                };
                self.set_pixel(x, y, out);
            }
        }
    }

    /// Trace un effet de retouche locale le long d'un segment (trait continu).
    pub fn effect_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        strength: f32,
        effect: PixelEffect,
    ) {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = (radius * 0.3).max(1.0);
        let n = (dist / step).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.apply_effect(from.0 + dx * t, from.1 + dy * t, radius, hardness, strength, effect);
        }
    }

    /// Estompe (smudge) : pousse la couleur échantillonnée à `from` vers
    /// `to`, mélangée à ce qui s'y trouve déjà — comme tirer le doigt dans de
    /// la peinture fraîche. `strength` (0..=1) module la part reprise de la
    /// couleur poussée à chaque pas (1 = remplace complètement, valeurs
    /// basses = traînée progressive sur plusieurs pas).
    pub fn smudge_segment(&mut self, from: (f32, f32), to: (f32, f32), radius: f32, hardness: f32, strength: f32) {
        let hardness = hardness.clamp(0.0, 1.0);
        let edge = hardness * radius;
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let step = (radius * 0.3).max(1.0);
        let n = (dist / step).ceil().max(1.0) as i32;
        let mut carried = self.get_pixel(from.0.round() as i32, from.1.round() as i32);
        for i in 1..=n {
            let t0 = (i - 1) as f32 / n as f32;
            let t1 = i as f32 / n as f32;
            let (px, py) = (from.0 + dx * t0, from.1 + dy * t0);
            let (cx, cy) = (from.0 + dx * t1, from.1 + dy * t1);
            let picked_up = self.get_pixel(px.round() as i32, py.round() as i32);
            for c in 0..4 {
                carried[c] = ((picked_up[c] as u16 + carried[c] as u16) / 2) as u8;
            }
            let (x0, x1) = ((cx - radius).floor() as i32, (cx + radius).ceil() as i32);
            let (y0, y1) = ((cy - radius).floor() as i32, (cy + radius).ceil() as i32);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (ddx, ddy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                    let d = (ddx * ddx + ddy * ddy).sqrt();
                    if d > radius {
                        continue;
                    }
                    let cov = if radius <= edge || d <= edge { 1.0 } else { 1.0 - (d - edge) / (radius - edge) };
                    self.blend_pixel(x, y, carried, cov * strength.clamp(0.0, 1.0));
                }
            }
        }
    }

    /// Remplissage par diffusion (pot de peinture pixel) depuis `(sx, sy)`,
    /// **borné** à `bounds` (min inclus, max exclu — typiquement le canevas
    /// document). Sans cette borne, un point de départ transparent n'a
    /// aucune limite naturelle sur un calque tuilé infini : le remplissage
    /// partirait à l'infini (chaque tuile jamais peinte renvoie du
    /// transparent, donc "proche" de la cible) et ferait exploser la
    /// mémoire — c'est un vrai piège du modèle tuilé épars, pas un détail.
    /// Non utilisé par l'outil Pot de peinture actuel : celui-ci détecte la
    /// zone à remplir sur la **composition visuelle** (tous calques/traits
    /// confondus, via une capture d'écran) puis écrit directement les pixels
    /// gagnés dans la couche raster (cf. `app::do_bucket_fill`), plutôt que
    /// de propager depuis la seule couleur déjà présente dans *cette* couche
    /// raster. Réservé à un futur mode « remplir dans ce calque seulement ».
    #[allow(dead_code)]
    pub fn flood_fill(&mut self, sx: i32, sy: i32, rgba: [u8; 4], tol: i32, bounds: ((i32, i32), (i32, i32))) {
        let (min, max) = bounds;
        if sx < min.0 || sy < min.1 || sx >= max.0 || sy >= max.1 {
            return;
        }
        let target = self.get_pixel(sx, sy);
        if target == rgba {
            return;
        }
        let close = |p: [u8; 4]| {
            (p[0] as i32 - target[0] as i32).abs() <= tol
                && (p[1] as i32 - target[1] as i32).abs() <= tol
                && (p[2] as i32 - target[2] as i32).abs() <= tol
                && (p[3] as i32 - target[3] as i32).abs() <= tol
        };
        let in_bounds = |x: i32, y: i32| x >= min.0 && y >= min.1 && x < max.0 && y < max.1;
        let mut seen: HashSet<(i32, i32)> = HashSet::new();
        let mut stack = vec![(sx, sy)];
        seen.insert((sx, sy));
        while let Some((x, y)) = stack.pop() {
            self.set_pixel(x, y, rgba);
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if !in_bounds(nx, ny) || seen.contains(&(nx, ny)) {
                    continue;
                }
                if close(self.get_pixel(nx, ny)) {
                    seen.insert((nx, ny));
                    stack.push((nx, ny));
                }
            }
        }
    }

    /// Couverture d'un pixel utilisé comme **masque de calque** (roadmap P2
    /// #14) : un pixel jamais peint est visible par défaut (255, comme un
    /// masque neuf, blanc) ; un pixel peint utilise son canal rouge comme
    /// niveau de gris (noir peint = masqué, blanc peint = visible), selon la
    /// convention Photoshop/GIMP. L'alpha du trait de pinceau n'intervient
    /// que pour mélanger avec la valeur précédente (déjà géré par `stamp`).
    pub fn mask_coverage(&self, x: i32, y: i32) -> u8 {
        let p = self.get_pixel(x, y);
        if p[3] == 0 {
            255
        } else {
            p[0]
        }
    }

    /// Boîte englobante (coords pixel, demi-ouverte) des tuiles non vides.
    pub fn bounds(&self) -> Option<((i32, i32), (i32, i32))> {
        if self.tiles.is_empty() {
            return None;
        }
        let mut min = (i32::MAX, i32::MAX);
        let mut max = (i32::MIN, i32::MIN);
        for &(tx, ty) in self.tiles.keys() {
            min.0 = min.0.min(tx * TILE);
            min.1 = min.1.min(ty * TILE);
            max.0 = max.0.max((tx + 1) * TILE);
            max.1 = max.1.max((ty + 1) * TILE);
        }
        Some((min, max))
    }

    /// Aplatit en un buffer RGBA dense : `(origine_x, origine_y, largeur, hauteur, pixels)`.
    pub fn flatten(&self) -> Option<(i32, i32, u32, u32, Vec<u8>)> {
        let (min, max) = self.bounds()?;
        let (w, h) = ((max.0 - min.0) as u32, (max.1 - min.1) as u32);
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let p = self.get_pixel(min.0 + x, min.1 + y);
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                out[i..i + 4].copy_from_slice(&p);
            }
        }
        Some((min.0, min.1, w, h, out))
    }

    /// Copie translatée (roadmap #4 : ancrage lors du changement de taille du
    /// canevas). Ré-échantillonne via `flatten`/`from_flat` : coûteux mais
    /// c'est une action ponctuelle déclenchée par l'utilisateur, pas une
    /// opération par frame.
    pub fn translated(&self, dx: i32, dy: i32) -> Self {
        let Some((ox, oy, w, h, rgba)) = self.flatten() else { return Self::default() };
        Self::from_flat(ox + dx, oy + dy, w, h, &rgba)
    }

    /// Copie mise à l'échelle (roadmap #4 : redimensionner l'image).
    pub fn scaled(&self, sx: f32, sy: f32) -> Self {
        let Some((ox, oy, w, h, rgba)) = self.flatten() else { return Self::default() };
        if w == 0 || h == 0 {
            return Self::default();
        }
        let (nw, nh) = ((w as f32 * sx).round().max(1.0) as u32, (h as f32 * sy).round().max(1.0) as u32);
        let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else { return Self::default() };
        let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
        let (nox, noy) = ((ox as f32 * sx).round() as i32, (oy as f32 * sy).round() as i32);
        Self::from_flat(nox, noy, nw, nh, resized.as_raw())
    }

    /// Reconstruit depuis un buffer RGBA dense placé à `(ox, oy)`.
    pub fn from_flat(ox: i32, oy: i32, w: u32, h: u32, rgba: &[u8]) -> Self {
        let mut r = RasterLayer::default();
        if rgba.len() < (w * h * 4) as usize {
            return r;
        }
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                let px = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
                if px[3] != 0 {
                    r.set_pixel(ox + x, oy + y, px);
                }
            }
        }
        r
    }

    /// Hash de contenu bon marché (cache d'invalidation du compositeur) :
    /// nombre de tuiles + échantillon de leurs pixels, pas une somme exacte.
    pub fn content_hash(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        mix(self.tiles.len() as u64);
        let mut keys: Vec<&TileKey> = self.tiles.keys().collect();
        keys.sort_unstable();
        for k in keys {
            mix(k.0 as u64);
            mix(k.1 as u64);
            let t = &self.tiles[k];
            for i in (0..t.px.len()).step_by(97) {
                mix(t.px[i] as u64);
            }
        }
        h
    }
}

/// Fragment de PNG persisté (companion des champs `raster_png`/`raster_origin`
/// de `Layer`, cf. `model::document`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RasterEncoded {
    pub png_b64: String,
    pub origin: (i32, i32),
}

pub fn encode(layer: &RasterLayer) -> RasterEncoded {
    let Some((ox, oy, w, h, rgba)) = layer.flatten() else {
        return RasterEncoded::default();
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use image::ImageEncoder;
    let mut buf = Vec::new();
    if image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .is_err()
    {
        return RasterEncoded::default();
    }
    RasterEncoded { png_b64: STANDARD.encode(buf), origin: (ox, oy) }
}

pub fn decode(enc: &RasterEncoded) -> RasterLayer {
    if enc.png_b64.is_empty() {
        return RasterLayer::default();
    }
    use base64::{engine::general_purpose::STANDARD, Engine};
    let Ok(bytes) = STANDARD.decode(&enc.png_b64) else { return RasterLayer::default() };
    let Ok(img) = image::load_from_memory(&bytes) else { return RasterLayer::default() };
    let img = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    RasterLayer::from_flat(enc.origin.0, enc.origin.1, w, h, img.as_raw())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_coverage_defaults_to_fully_visible() {
        let r = RasterLayer::default();
        assert_eq!(r.mask_coverage(5, 5), 255);
    }

    #[test]
    fn mask_coverage_uses_red_channel_once_painted() {
        let mut r = RasterLayer::default();
        r.set_pixel(5, 5, [40, 0, 0, 255]);
        assert_eq!(r.mask_coverage(5, 5), 40);
        // Voisin jamais peint : reste visible malgré la tuile allouée.
        assert_eq!(r.mask_coverage(6, 5), 255);
    }

    #[test]
    fn empty_layer_has_no_tiles_and_no_bounds() {
        let r = RasterLayer::default();
        assert!(r.is_empty());
        assert!(r.bounds().is_none());
    }

    #[test]
    fn set_pixel_allocates_only_touched_tile() {
        let mut r = RasterLayer::default();
        r.set_pixel(10, 10, [255, 0, 0, 255]);
        assert_eq!(r.tiles.len(), 1);
        assert_eq!(r.get_pixel(10, 10), [255, 0, 0, 255]);
        // Pixel voisin non peint : transparent.
        assert_eq!(r.get_pixel(11, 11), [0, 0, 0, 0]);
    }

    #[test]
    fn set_pixel_spans_tile_boundary() {
        let mut r = RasterLayer::default();
        r.set_pixel(TILE - 1, 0, [1, 2, 3, 255]);
        r.set_pixel(TILE, 0, [4, 5, 6, 255]);
        assert_eq!(r.tiles.len(), 2);
        assert_eq!(r.get_pixel(TILE - 1, 0), [1, 2, 3, 255]);
        assert_eq!(r.get_pixel(TILE, 0), [4, 5, 6, 255]);
    }

    #[test]
    fn negative_coords_use_correct_tile() {
        let mut r = RasterLayer::default();
        r.set_pixel(-1, -1, [9, 9, 9, 255]);
        assert_eq!(r.get_pixel(-1, -1), [9, 9, 9, 255]);
        assert_eq!(r.tiles.keys().next(), Some(&(-1, -1)));
    }

    #[test]
    fn stamp_opaque_center_full_coverage() {
        let mut r = RasterLayer::default();
        r.stamp(50.0, 50.0, 10.0, 1.0, [10, 20, 30, 255], false);
        assert_eq!(r.get_pixel(50, 50), [10, 20, 30, 255]);
        // Hors du disque : intact.
        assert_eq!(r.get_pixel(80, 80), [0, 0, 0, 0]);
    }

    #[test]
    fn stamp_erase_reduces_alpha_only() {
        let mut r = RasterLayer::default();
        r.set_pixel(50, 50, [200, 100, 50, 255]);
        r.stamp(50.0, 50.0, 5.0, 1.0, [0, 0, 0, 255], true);
        let p = r.get_pixel(50, 50);
        assert_eq!(&p[0..3], &[200, 100, 50]); // couleur inchangée
        assert!(p[3] < 10); // alpha quasi nulle
    }

    #[test]
    fn clone_stamp_copies_from_offset_source() {
        let mut r = RasterLayer::default();
        // Source : disque plein vert à (10,10).
        r.stamp(10.0, 10.0, 6.0, 1.0, [0, 200, 0, 255], false);
        // Peint à (50,50) avec un décalage de +40 en x : source = (50-40,50)=(10,50)?
        // Ici offset = source - dest fixé côté appelant ; le tampon échantillonne à
        // (x+offset.0, y+offset.1). Avec offset=(-40,-40), (50,50) lit (10,10).
        r.clone_stamp(50.0, 50.0, 4.0, 1.0, (-40.0, -40.0), 1.0);
        assert_eq!(r.get_pixel(50, 50), [0, 200, 0, 255]);
    }

    #[test]
    fn clone_stamp_partial_opacity_reduces_alpha() {
        let mut r = RasterLayer::default();
        r.stamp(10.0, 10.0, 6.0, 1.0, [100, 100, 100, 255], false);
        r.clone_stamp(50.0, 50.0, 4.0, 1.0, (-40.0, -40.0), 0.5);
        let p = r.get_pixel(50, 50);
        assert!(p[3] < 255 && p[3] > 0);
    }

    #[test]
    fn heal_stamp_shifts_toward_destination_mean_color() {
        let mut r = RasterLayer::default();
        // Source uniforme rouge vif, destination environnante bleue : le
        // correcteur doit se rapprocher de la teinte de destination — à la
        // différence du clonage pur qui recopierait le rouge tel quel.
        r.stamp(10.0, 10.0, 6.0, 1.0, [255, 0, 0, 255], false);
        for y in 44..56 {
            for x in 44..56 {
                r.set_pixel(x, y, [0, 0, 255, 255]);
            }
        }
        r.heal_stamp(50.0, 50.0, 4.0, 1.0, (-40.0, -40.0), 1.0);
        let p = r.get_pixel(50, 50);
        // Toujours teinté par la texture source (canal rouge > 0) mais
        // nettement rapproché du bleu environnant (bien en dessous de 255).
        assert!(p[0] < 255, "red channel should shift down: {p:?}");
    }

    #[test]
    fn heal_stamp_is_noop_when_source_matches_destination() {
        let mut r = RasterLayer::default();
        r.stamp(10.0, 10.0, 6.0, 1.0, [120, 80, 40, 255], false);
        r.stamp(50.0, 50.0, 6.0, 1.0, [120, 80, 40, 255], false);
        r.heal_stamp(50.0, 50.0, 4.0, 1.0, (-40.0, -40.0), 1.0);
        assert_eq!(r.get_pixel(50, 50), [120, 80, 40, 255]);
    }

    #[test]
    fn flood_fill_stops_at_color_boundary() {
        let mut r = RasterLayer::default();
        // Barrière verticale opaque noire en x=5, sur toute la hauteur bornée.
        for y in 0..10 {
            r.set_pixel(5, y, [0, 0, 0, 255]);
        }
        r.flood_fill(0, 0, [255, 0, 0, 255], 10, ((0, 0), (10, 10)));
        assert_eq!(r.get_pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(r.get_pixel(4, 4), [255, 0, 0, 255]);
        assert_eq!(r.get_pixel(5, 4), [0, 0, 0, 255]); // barrière intacte
        assert_eq!(r.get_pixel(6, 4), [0, 0, 0, 0]); // autre côté non atteint
    }

    /// Régression : sur un calque tuilé infini, un remplissage depuis un
    /// pixel transparent SANS borne partirait à l'infini (chaque tuile
    /// jamais peinte est transparente, donc "proche" de la cible) — d'où
    /// l'obligation d'un rectangle `bounds`. Vérifie qu'il ne déborde pas.
    #[test]
    fn flood_fill_never_paints_outside_bounds() {
        let mut r = RasterLayer::default();
        r.flood_fill(2, 2, [1, 2, 3, 255], 5, ((0, 0), (5, 5)));
        assert_eq!(r.get_pixel(2, 2), [1, 2, 3, 255]);
        assert_eq!(r.get_pixel(0, 0), [1, 2, 3, 255]);
        assert_eq!(r.get_pixel(4, 4), [1, 2, 3, 255]);
        // Juste hors des bornes : jamais touché.
        assert_eq!(r.get_pixel(5, 2), [0, 0, 0, 0]);
        assert_eq!(r.get_pixel(-1, 2), [0, 0, 0, 0]);
        // Aucune tuile allouée hors de la zone couverte par les bornes.
        for &(tx, ty) in r.tiles.keys() {
            assert_eq!((tx, ty), (0, 0));
        }
    }

    #[test]
    fn flood_fill_outside_bounds_is_noop() {
        let mut r = RasterLayer::default();
        r.flood_fill(100, 100, [1, 2, 3, 255], 5, ((0, 0), (5, 5)));
        assert!(r.is_empty());
    }

    #[test]
    fn flatten_and_from_flat_roundtrip() {
        let mut r = RasterLayer::default();
        r.set_pixel(300, 5, [1, 2, 3, 255]);
        r.set_pixel(10, 400, [4, 5, 6, 255]);
        let (ox, oy, w, h, rgba) = r.flatten().unwrap();
        let r2 = RasterLayer::from_flat(ox, oy, w, h, &rgba);
        assert_eq!(r2.get_pixel(300, 5), [1, 2, 3, 255]);
        assert_eq!(r2.get_pixel(10, 400), [4, 5, 6, 255]);
    }

    #[test]
    fn content_hash_changes_after_paint() {
        let mut r = RasterLayer::default();
        let h0 = r.content_hash();
        r.set_pixel(1, 1, [255, 255, 255, 255]);
        assert_ne!(h0, r.content_hash());
    }

    #[test]
    fn encode_decode_roundtrip() {
        let mut r = RasterLayer::default();
        r.set_pixel(20, 20, [7, 8, 9, 200]);
        let enc = encode(&r);
        assert!(!enc.png_b64.is_empty());
        let r2 = decode(&enc);
        assert_eq!(r2.get_pixel(20, 20), [7, 8, 9, 200]);
    }

    // --- Retouche locale (Sprint 11) -----------------------------------

    #[test]
    fn lighten_moves_channels_toward_white() {
        let mut r = RasterLayer::default();
        r.stamp(10.0, 10.0, 6.0, 1.0, [100, 100, 100, 255], false);
        r.effect_segment((10.0, 10.0), (10.0, 10.0), 6.0, 1.0, 1.0, PixelEffect::Lighten);
        let p = r.get_pixel(10, 10);
        assert!(p[0] > 100, "expected lighter channel, got {p:?}");
    }

    #[test]
    fn darken_moves_channels_toward_black() {
        let mut r = RasterLayer::default();
        r.stamp(10.0, 10.0, 6.0, 1.0, [200, 200, 200, 255], false);
        r.effect_segment((10.0, 10.0), (10.0, 10.0), 6.0, 1.0, 1.0, PixelEffect::Darken);
        let p = r.get_pixel(10, 10);
        assert!(p[0] < 200, "expected darker channel, got {p:?}");
    }

    #[test]
    fn saturate_and_desaturate_move_saturation_opposite_ways() {
        let mut base = RasterLayer::default();
        base.stamp(10.0, 10.0, 6.0, 1.0, [180, 120, 120, 255], false);

        let mut sat = base.clone();
        sat.effect_segment((10.0, 10.0), (10.0, 10.0), 6.0, 1.0, 1.0, PixelEffect::Saturate);
        let (_, s_sat, _) =
            crate::tools::filter::rgb_to_hsl(sat.get_pixel(10, 10)[0], sat.get_pixel(10, 10)[1], sat.get_pixel(10, 10)[2]);

        let mut desat = base.clone();
        desat.effect_segment((10.0, 10.0), (10.0, 10.0), 6.0, 1.0, 1.0, PixelEffect::Desaturate);
        let (_, s_desat, _) = crate::tools::filter::rgb_to_hsl(
            desat.get_pixel(10, 10)[0],
            desat.get_pixel(10, 10)[1],
            desat.get_pixel(10, 10)[2],
        );

        assert!(s_sat > s_desat, "saturate ({s_sat}) should exceed desaturate ({s_desat})");
    }

    /// Régression : `rgb_to_hsl` renvoie une teinte arbitraire (0.0) pour un
    /// pixel gris — sans garde, `Saturate` le teintait de rouge au lieu de le
    /// laisser gris (aucune teinte n'existe à accentuer).
    #[test]
    fn saturate_does_not_tint_a_gray_pixel() {
        let mut r = RasterLayer::default();
        r.stamp(10.0, 10.0, 6.0, 1.0, [128, 128, 128, 255], false);
        r.effect_segment((10.0, 10.0), (10.0, 10.0), 6.0, 1.0, 1.0, PixelEffect::Saturate);
        let p = r.get_pixel(10, 10);
        assert_eq!(&p[0..3], &[128, 128, 128], "gray pixel should stay gray, got {p:?}");
    }

    #[test]
    fn blur_pulls_a_lone_bright_pixel_toward_its_dark_neighbors() {
        let mut r = RasterLayer::default();
        for y in 8..13 {
            for x in 8..13 {
                r.set_pixel(x, y, [0, 0, 0, 255]);
            }
        }
        r.set_pixel(10, 10, [255, 255, 255, 255]);
        r.effect_segment((10.0, 10.0), (10.0, 10.0), 3.0, 1.0, 1.0, PixelEffect::Blur);
        let p = r.get_pixel(10, 10);
        assert!(p[0] < 255, "expected blurred center to darken toward neighbors, got {p:?}");
    }

    #[test]
    fn sharpen_pushes_bright_pixel_further_from_dark_neighbors() {
        let mut r = RasterLayer::default();
        for y in 8..13 {
            for x in 8..13 {
                r.set_pixel(x, y, [100, 100, 100, 255]);
            }
        }
        r.set_pixel(10, 10, [150, 150, 150, 255]);
        r.effect_segment((10.0, 10.0), (10.0, 10.0), 3.0, 1.0, 1.0, PixelEffect::Sharpen);
        let p = r.get_pixel(10, 10);
        assert!(p[0] > 150, "expected sharpened center to brighten further, got {p:?}");
    }

    #[test]
    fn smudge_pulls_source_color_toward_destination() {
        let mut r = RasterLayer::default();
        r.stamp(5.0, 50.0, 6.0, 1.0, [255, 0, 0, 255], false);
        for y in 44..56 {
            for x in 44..56 {
                r.set_pixel(x, y, [0, 0, 255, 255]);
            }
        }
        r.smudge_segment((5.0, 50.0), (50.0, 50.0), 5.0, 1.0, 1.0);
        let p = r.get_pixel(50, 50);
        // Un peu de rouge doit avoir migré vers la destination bleue.
        assert!(p[0] > 0, "expected some red channel picked up along the drag, got {p:?}");
    }
}
