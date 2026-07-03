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

/// Flou « boîte » séparable sur un tampon **1 canal** (Sprint 9.1) : adoucit
/// la frontière binaire du détourage plutôt que de laisser un bord à
/// l'emporte-pièce. Même algorithme que le flou RVBA de `tools::filter`, mais
/// sans le pas de 4 octets par pixel — un buffer de masque n'a qu'un canal.
pub fn feather(mask: &[u8], w: usize, h: usize, radius: usize) -> Vec<u8> {
    if w == 0 || h == 0 || mask.len() < w * h || radius == 0 {
        return mask.to_vec();
    }
    let pass = |src: &[u8], horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let idx = |i: usize| {
                let (x, y) = if horizontal { (i, o) } else { (o, i) };
                y * w + x
            };
            let mut sum = 0u32;
            let mut count = 0u32;
            for i in 0..=radius.min(inner - 1) {
                sum += src[idx(i)] as u32;
                count += 1;
            }
            for i in 0..inner {
                out[idx(i)] = (sum / count.max(1)) as u8;
                if i >= radius {
                    sum -= src[idx(i - radius)] as u32;
                    count -= 1;
                }
                let add = i + radius + 1;
                if add < inner {
                    sum += src[idx(add)] as u32;
                    count += 1;
                }
            }
        }
        out
    };
    pass(&pass(mask, true), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feather_smooths_a_hard_edge() {
        // 5×1 : bloc net 0/255 en son milieu → après adoucissement, la
        // transition ne doit plus être une marche brute.
        let mask = vec![0u8, 0, 255, 255, 255];
        let out = feather(&mask, 5, 1, 1);
        assert!(out[1] > 0 && out[1] < 255, "edge pixel should be graded: {out:?}");
        // Les extrémités, loin du bord, restent proches de leur valeur d'origine.
        assert_eq!(out[0], 0);
        assert_eq!(out[4], 255);
    }

    #[test]
    fn feather_is_noop_with_zero_radius() {
        let mask = vec![0u8, 255, 0, 255];
        assert_eq!(feather(&mask, 2, 2, 0), mask);
    }

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
