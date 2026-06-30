//! Filtres image (backlog) : appliqués aux pixels RGBA d'une image sélectionnée.
//! Purs et testables ; l'intégration (undo, ré-encodage PNG) est dans `app`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Brighter,
    Darker,
    Grayscale,
    Blur,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::Brighter => "Plus clair",
            Filter::Darker => "Plus sombre",
            Filter::Grayscale => "Noir & blanc",
            Filter::Blur => "Flou",
        }
    }
}

/// Applique le filtre en place (ou renvoie un nouveau buffer pour le flou).
pub fn apply(filter: Filter, rgba: &mut Vec<u8>, w: u32, h: u32) {
    match filter {
        Filter::Brighter => brightness(rgba, 1.2),
        Filter::Darker => brightness(rgba, 0.83),
        Filter::Grayscale => grayscale(rgba),
        Filter::Blur => *rgba = box_blur(rgba, w as usize, h as usize, 2),
    }
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
        let g = (px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114) as u8;
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
}
