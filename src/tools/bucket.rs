//! Pot de peinture (roadmap #6) : remplissage par diffusion (flood-fill) sur un
//! tampon RGBA, avec tolérance de couleur. Pur et testable — l'intégration
//! (capture de la composition affichée → dépôt en image) est dans `app`.

/// Renvoie un masque `w*h` des pixels atteints depuis `(sx, sy)` dont la
/// couleur reste dans `tol` (par canal) de la couleur de départ.
pub fn flood(rgba: &[u8], w: usize, h: usize, sx: usize, sy: usize, tol: i32) -> Vec<bool> {
    let mut mask = vec![false; w * h];
    if sx >= w || sy >= h || rgba.len() < w * h * 4 {
        return mask;
    }
    let at = |x: usize, y: usize| (y * w + x) * 4;
    let target = {
        let i = at(sx, sy);
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    let close = |x: usize, y: usize| {
        let i = at(x, y);
        let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
        d(rgba[i], target[0]) <= tol
            && d(rgba[i + 1], target[1]) <= tol
            && d(rgba[i + 2], target[2]) <= tol
            && d(rgba[i + 3], target[3]) <= tol
    };

    let mut stack = vec![(sx, sy)];
    mask[sy * w + sx] = true;
    while let Some((x, y)) = stack.pop() {
        let push = |nx: usize, ny: usize, mask: &mut Vec<bool>, stack: &mut Vec<(usize, usize)>| {
            if !mask[ny * w + nx] && close(nx, ny) {
                mask[ny * w + nx] = true;
                stack.push((nx, ny));
            }
        };
        if x > 0 {
            push(x - 1, y, &mut mask, &mut stack);
        }
        if x + 1 < w {
            push(x + 1, y, &mut mask, &mut stack);
        }
        if y > 0 {
            push(x, y - 1, &mut mask, &mut stack);
        }
        if y + 1 < h {
            push(x, y + 1, &mut mask, &mut stack);
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_contiguous_region_bounded_by_color() {
        // 3×3, colonne du milieu = barrière noire, reste blanc.
        let w = 3;
        let h = 3;
        let mut rgba = vec![255u8; w * h * 4];
        for y in 0..h {
            let i = (y * w + 1) * 4;
            rgba[i] = 0;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
        }
        let mask = flood(&rgba, w, h, 0, 0, 16);
        // Colonne gauche remplie, barrière et droite non.
        assert!(mask[0] && mask[3] && mask[6]);
        assert!(!mask[1] && !mask[2]);
    }
}
