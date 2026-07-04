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

/// Variante non contiguë (Sprint 9.1, renforcement) : sélectionne **tous**
/// les pixels proches (≤ `tol` par canal) de la couleur de départ, peu
/// importe qu'ils soient connectés à `(sx, sy)` — utile pour un fond visible
/// par petits bouts (feuillage, grillage…) là où `flood` s'arrêterait au
/// premier pixel hors tolérance.
pub fn flood_global(rgba: &[u8], w: usize, h: usize, sx: usize, sy: usize, tol: i32) -> Vec<bool> {
    if sx >= w || sy >= h || rgba.len() < w * h * 4 {
        return vec![false; w * h];
    }
    let i = (sy * w + sx) * 4;
    let target = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    rgba.chunks_exact(4)
        .map(|c| d(c[0], target[0]) <= tol && d(c[1], target[1]) <= tol && d(c[2], target[2]) <= tol && d(c[3], target[3]) <= tol)
        .collect()
}

/// Bord dégradé par proximité de couleur (Sprint 9.1, sans réseau de
/// neurones) : les pixels déjà retenus par `flooded` valent 255 (couverture
/// pleine) ; les autres reçoivent une couverture continue selon leur
/// distance de couleur à la couleur cible — pleine dans la tolérance,
/// dégradée jusqu'à 0 à 2× la tolérance. Plus fidèle qu'un flou uniforme
/// après-coup (`feather`) sur des bords progressifs réels (flou de mise au
/// point, cheveux/fourrure, compression JPEG) : le dégradé suit le contenu
/// au lieu d'étaler la même largeur de flou partout, y compris sur des
/// contours nets qui n'en ont pas besoin.
pub fn soft_edge(rgba: &[u8], w: usize, h: usize, sx: usize, sy: usize, tol: i32, flooded: &[bool]) -> Vec<u8> {
    if sx >= w || sy >= h || rgba.len() < w * h * 4 || flooded.len() != w * h {
        return vec![0u8; w * h];
    }
    let i0 = (sy * w + sx) * 4;
    let target = [rgba[i0], rgba[i0 + 1], rgba[i0 + 2], rgba[i0 + 3]];
    let tol_f = (tol.max(1)) as f32;
    let mut out = vec![0u8; w * h];
    for (idx, px) in rgba.chunks_exact(4).enumerate() {
        if flooded[idx] {
            out[idx] = 255;
            continue;
        }
        let d = (0..4)
            .map(|c| (px[c] as i32 - target[c] as i32).abs())
            .max()
            .unwrap_or(0) as f32;
        let extra = (d - tol_f).max(0.0);
        let coverage = (1.0 - extra / tol_f).clamp(0.0, 1.0);
        out[idx] = (coverage * 255.0).round() as u8;
    }
    out
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

    #[test]
    fn soft_edge_grades_the_boundary_by_color_distance() {
        // 5×1 : cible blanc pur en x=0, puis un dégradé vers le noir.
        let colors: [u8; 5] = [255, 200, 150, 80, 0];
        let mut rgba = Vec::new();
        for &v in &colors {
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
        let tol = 20;
        let flooded = flood(&rgba, 5, 1, 0, 0, tol);
        let soft = soft_edge(&rgba, 5, 1, 0, 0, tol, &flooded);
        assert_eq!(soft[0], 255); // couleur cible exacte
        // La couverture doit décroître strictement à mesure qu'on s'éloigne
        // de la couleur cible (moins de tolérance restante).
        assert!(soft[1] >= soft[2]);
        assert!(soft[2] >= soft[3]);
        assert!(soft[3] >= soft[4]);
        assert_eq!(soft[4], 0); // à 255 de distance, bien au-delà de 2×tol
    }

    #[test]
    fn soft_edge_is_full_coverage_inside_the_flooded_region() {
        let rgba = vec![10u8, 10, 10, 255, 10, 10, 10, 255];
        let flooded = vec![true, true];
        let soft = soft_edge(&rgba, 2, 1, 0, 0, 16, &flooded);
        assert_eq!(soft, vec![255, 255]);
    }

    #[test]
    fn flood_global_selects_disconnected_pixels_of_similar_color() {
        // 3×1 : blanc, noir, blanc — les deux blancs ne sont pas connectés,
        // `flood` s'arrêterait au premier ; `flood_global` prend les deux.
        let rgba = vec![255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        let contiguous = flood(&rgba, 3, 1, 0, 0, 16);
        assert!(contiguous[0] && !contiguous[2]);
        let global = flood_global(&rgba, 3, 1, 0, 0, 16);
        assert!(global[0] && global[2] && !global[1]);
    }
}
