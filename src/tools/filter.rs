//! Filtres image : appliqués soit destructivement aux pixels d'une image
//! sélectionnée (backlog), soit en direct au compositing via un **calque
//! d'ajustement non destructif** (roadmap F3, cf. `render::compositor`).
//! Purs et testables ; l'intégration (undo, ré-encodage PNG) est dans `app`.

use crate::i18n::t;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    Brighter,
    Darker,
    Contrast,
    Saturate,
    Desaturate,
    Grayscale,
    Invert,
    Sharpen,
    Blur,
}

impl Filter {
    /// Tous les filtres, dans l'ordre d'affichage du menu.
    pub const ALL: [Filter; 9] = [
        Filter::Brighter,
        Filter::Darker,
        Filter::Contrast,
        Filter::Saturate,
        Filter::Desaturate,
        Filter::Grayscale,
        Filter::Invert,
        Filter::Sharpen,
        Filter::Blur,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::Brighter => t("Plus clair", "Brighter"),
            Filter::Darker => t("Plus sombre", "Darker"),
            Filter::Contrast => t("Contraste +", "Contrast +"),
            Filter::Saturate => t("Saturation +", "Saturation +"),
            Filter::Desaturate => t("Saturation −", "Saturation −"),
            Filter::Grayscale => t("Noir & blanc", "Black & white"),
            Filter::Invert => t("Négatif", "Invert"),
            Filter::Sharpen => t("Netteté", "Sharpen"),
            Filter::Blur => t("Flou", "Blur"),
        }
    }
}

/// Réglage non destructif d'un calque d'ajustement (Sprint 8.1/8.2), en plus
/// des 9 presets discrets de [`Filter`] : niveaux et teinte/saturation à
/// paramètres continus, courbe à 3 points ancrés (ombres/tons
/// moyens/hautes lumières). Canal composite RVB uniquement (pas de réglage
/// par canal séparé) — suffisant pour retrouver le cœur PhotoFiltre/Photoshop
/// sans la complexité d'un éditeur de courbe à points libres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Adjustment {
    Preset(Filter),
    Levels { black: u8, white: u8, gamma: f32 },
    /// `hue` en degrés (-180..180), `sat`/`light` en écart relatif (-1.0..1.0).
    HueSaturation { hue: f32, sat: f32, light: f32 },
    Curves { shadow: u8, mid: u8, highlight: u8 },
}

impl Adjustment {
    pub fn label(self) -> String {
        match self {
            Adjustment::Preset(f) => f.label().to_string(),
            Adjustment::Levels { .. } => t("Niveaux", "Levels").to_string(),
            Adjustment::HueSaturation { .. } => t("Teinte/Saturation", "Hue/Saturation").to_string(),
            Adjustment::Curves { .. } => t("Courbes", "Curves").to_string(),
        }
    }

    pub fn default_levels() -> Self {
        Adjustment::Levels { black: 0, white: 255, gamma: 1.0 }
    }

    pub fn default_hue_saturation() -> Self {
        Adjustment::HueSaturation { hue: 0.0, sat: 0.0, light: 0.0 }
    }

    pub fn default_curves() -> Self {
        Adjustment::Curves { shadow: 0, mid: 128, highlight: 255 }
    }

    /// Signature FNV-1a des paramètres, pour l'invalidation du cache de rendu
    /// (`Adjustment` porte des `f32`, donc pas de `Hash`/`Eq` dérivable).
    pub fn hash_key(self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        match self {
            Adjustment::Preset(f) => {
                mix(1);
                mix(f as u64);
            }
            Adjustment::Levels { black, white, gamma } => {
                mix(2);
                mix(black as u64);
                mix(white as u64);
                mix(gamma.to_bits() as u64);
            }
            Adjustment::HueSaturation { hue, sat, light } => {
                mix(3);
                mix(hue.to_bits() as u64);
                mix(sat.to_bits() as u64);
                mix(light.to_bits() as u64);
            }
            Adjustment::Curves { shadow, mid, highlight } => {
                mix(4);
                mix(shadow as u64);
                mix(mid as u64);
                mix(highlight as u64);
            }
        }
        h
    }
}

/// Applique un réglage d'ajustement en place (canal composite RVB, alpha
/// inchangé).
pub fn apply_adjustment(adj: Adjustment, rgba: &mut Vec<u8>, w: u32, h: u32) {
    match adj {
        Adjustment::Preset(f) => apply(f, rgba, w, h),
        Adjustment::Levels { black, white, gamma } => levels(rgba, black, white, gamma),
        Adjustment::HueSaturation { hue, sat, light } => hue_saturation(rgba, hue, sat, light),
        Adjustment::Curves { shadow, mid, highlight } => curves(rgba, shadow, mid, highlight),
    }
}

/// Niveaux façon Photoshop : point noir/blanc + gamma. `out = ((in-black)/(white-black))^(1/gamma)`.
fn levels(rgba: &mut [u8], black: u8, white: u8, gamma: f32) {
    let b = black as f32;
    let w = (white.max(black.saturating_add(1))) as f32;
    let inv_gamma = 1.0 / gamma.max(0.01);
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            let v = ((*c as f32 - b) / (w - b)).clamp(0.0, 1.0);
            *c = (v.powf(inv_gamma) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Courbe à 3 points ancrés (x = 0/128/255), interpolation linéaire entre eux.
fn curve_lut(shadow: u8, mid: u8, highlight: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let (s, m, h) = (shadow as f32, mid as f32, highlight as f32);
    for (i, slot) in lut.iter_mut().enumerate() {
        let x = i as f32;
        let y = if x <= 128.0 { s + (m - s) * (x / 128.0) } else { m + (h - m) * ((x - 128.0) / 127.0) };
        *slot = y.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

fn curves(rgba: &mut [u8], shadow: u8, mid: u8, highlight: u8) {
    let lut = curve_lut(shadow, mid, highlight);
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = lut[*c as usize];
        }
    }
}

/// Teinte/saturation/luminosité via un aller-retour RVB↔HSL par pixel.
fn hue_saturation(rgba: &mut [u8], hue_deg: f32, sat: f32, light: f32) {
    for px in rgba.chunks_exact_mut(4) {
        let (h0, s0, l0) = rgb_to_hsl(px[0], px[1], px[2]);
        let h = (h0 + hue_deg / 360.0).rem_euclid(1.0);
        let s = (s0 * (1.0 + sat)).clamp(0.0, 1.0);
        let l = (l0 + light).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s <= 0.0 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };
    let r = hue2rgb(p, q, h + 1.0 / 3.0);
    let g = hue2rgb(p, q, h);
    let b = hue2rgb(p, q, h - 1.0 / 3.0);
    let conv = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    (conv(r), conv(g), conv(b))
}

/// Applique le filtre en place (ou renvoie un nouveau buffer pour les filtres à
/// voisinage : flou, netteté).
pub fn apply(filter: Filter, rgba: &mut Vec<u8>, w: u32, h: u32) {
    match filter {
        Filter::Brighter => brightness(rgba, 1.2),
        Filter::Darker => brightness(rgba, 0.83),
        Filter::Contrast => contrast(rgba, 1.25),
        Filter::Saturate => saturate(rgba, 1.4),
        Filter::Desaturate => saturate(rgba, 0.6),
        Filter::Grayscale => grayscale(rgba),
        Filter::Invert => invert(rgba),
        Filter::Sharpen => *rgba = sharpen(rgba, w as usize, h as usize),
        Filter::Blur => *rgba = box_blur(rgba, w as usize, h as usize, 2),
    }
}

/// Luminance perçue (Rec. 601) d'un pixel.
fn luma(px: &[u8]) -> f32 {
    px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114
}

/// Contraste autour du point pivot 128 : `out = (in - 128) * factor + 128`.
fn contrast(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = ((*c as f32 - 128.0) * factor + 128.0).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Saturation : interpole chaque canal entre le gris (luma) et sa valeur.
/// `factor` > 1 sature, < 1 désature, 0 = noir & blanc.
fn saturate(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        let g = luma(px);
        for c in px.iter_mut().take(3) {
            *c = (g + (*c as f32 - g) * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Négatif : inverse les canaux RVB (alpha conservé).
fn invert(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = 255 - *c;
        }
    }
}

/// Renforcement de netteté par noyau 3×3 (somme = 1) :
/// centre 5, voisins cardinaux −1. Bords répétés (clamp).
fn sharpen(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let mut out = src.to_vec();
    let at = |x: usize, y: usize, c: usize| src[(y * w + x) * 4 + c];
    for y in 0..h {
        for x in 0..w {
            // L'alpha n'est pas affecté (canal 3 copié tel quel).
            for c in 0..3 {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(w - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(h - 1);
                let v = 5.0 * at(x, y, c) as f32
                    - at(xm, y, c) as f32
                    - at(xp, y, c) as f32
                    - at(x, ym, c) as f32
                    - at(x, yp, c) as f32;
                out[(y * w + x) * 4 + c] = v.clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn brightness(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = (*c as f32 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

fn grayscale(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let g = luma(px) as u8;
        px[0] = g;
        px[1] = g;
        px[2] = g;
    }
}

/// Flou « boîte » séparable (2 passes), tous canaux.
fn box_blur(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let tmp = blur_pass(src, w, h, r, true);
    blur_pass(&tmp, w, h, r, false)
}

fn blur_pass(src: &[u8], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    for o in 0..outer {
        for ch in 0..4 {
            let mut sum = 0u32;
            let mut count = 0u32;
            let idx = |i: usize| {
                let (x, y) = if horizontal { (i, o) } else { (o, i) };
                (y * w + x) * 4 + ch
            };
            // Fenêtre initiale.
            for i in 0..=r.min(inner - 1) {
                sum += src[idx(i)] as u32;
                count += 1;
            }
            for i in 0..inner {
                out[idx(i)] = (sum / count.max(1)) as u8;
                // Avance la fenêtre [i-r, i+r].
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
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_equalizes_channels() {
        let mut px = vec![200, 100, 50, 255];
        apply(Filter::Grayscale, &mut px, 1, 1);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
    }

    #[test]
    fn brighter_increases_values() {
        let mut px = vec![100, 100, 100, 255];
        apply(Filter::Brighter, &mut px, 1, 1);
        assert!(px[0] > 100);
    }

    #[test]
    fn invert_is_involutive() {
        let original = vec![10, 120, 240, 255];
        let mut px = original.clone();
        apply(Filter::Invert, &mut px, 1, 1);
        assert_eq!(px, vec![245, 135, 15, 255]); // alpha conservé
        apply(Filter::Invert, &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn contrast_pushes_away_from_mid() {
        // Au-dessus du pivot 128 → plus clair ; en dessous → plus sombre.
        let mut hi = vec![200, 200, 200, 255];
        apply(Filter::Contrast, &mut hi, 1, 1);
        assert!(hi[0] > 200);
        let mut lo = vec![50, 50, 50, 255];
        apply(Filter::Contrast, &mut lo, 1, 1);
        assert!(lo[0] < 50);
    }

    #[test]
    fn desaturate_pulls_channels_together() {
        let mut px = vec![200, 100, 50, 255];
        let spread0 = (px[0] as i32 - px[2] as i32).abs();
        apply(Filter::Desaturate, &mut px, 1, 1);
        let spread1 = (px[0] as i32 - px[2] as i32).abs();
        assert!(spread1 < spread0);
    }

    #[test]
    fn sharpen_preserves_flat_region() {
        // Image unie : la netteté ne doit rien changer (somme du noyau = 1).
        let mut px = vec![120u8; 3 * 3 * 4];
        apply(Filter::Sharpen, &mut px, 3, 3);
        assert!(px.iter().all(|&v| v == 120));
    }

    #[test]
    fn levels_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_levels(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn levels_raises_black_point() {
        let mut px = vec![10u8, 10, 10, 255];
        apply_adjustment(Adjustment::Levels { black: 20, white: 255, gamma: 1.0 }, &mut px, 1, 1);
        // En dessous du point noir : écrêté à 0.
        assert_eq!(px[0], 0);
    }

    #[test]
    fn curves_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_curves(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn curves_lifted_shadows_brighten_dark_pixels() {
        let mut px = vec![10u8, 10, 10, 255];
        apply_adjustment(Adjustment::Curves { shadow: 40, mid: 128, highlight: 255 }, &mut px, 1, 1);
        assert!(px[0] > 10);
    }

    #[test]
    fn hue_saturation_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_hue_saturation(), &mut px, 1, 1);
        // Tolérance : l'aller-retour RVB→HSL→RVB peut arrondir de ±1.
        for c in 0..3 {
            assert!((px[c] as i32 - original[c] as i32).abs() <= 1, "channel {c}: {} vs {}", px[c], original[c]);
        }
    }

    #[test]
    fn hue_saturation_zero_saturation_grayscales() {
        let mut px = vec![200u8, 100, 50, 255];
        apply_adjustment(Adjustment::HueSaturation { hue: 0.0, sat: -1.0, light: 0.0 }, &mut px, 1, 1);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
    }
}
