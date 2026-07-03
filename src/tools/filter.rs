//! Filtres image : appliqués soit destructivement aux pixels d'une image
//! sélectionnée (backlog), soit en direct au compositing via un **calque
//! d'ajustement non destructif** (roadmap F3, cf. `render::compositor`).
//! Purs et testables ; l'intégration (undo, ré-encodage PNG) est dans `app`.

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
            Filter::Brighter => "Plus clair",
            Filter::Darker => "Plus sombre",
            Filter::Contrast => "Contraste +",
            Filter::Saturate => "Saturation +",
            Filter::Desaturate => "Saturation −",
            Filter::Grayscale => "Noir & blanc",
            Filter::Invert => "Négatif",
            Filter::Sharpen => "Netteté",
            Filter::Blur => "Flou",
        }
    }
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
}
