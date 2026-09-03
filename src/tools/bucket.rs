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
    rgba.as_chunks::<4>()
        .0
        .iter()
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
    for (idx, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
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

/// Luminance (Rec. 601) par pixel, pour la détection de texture fine.
fn luma_at(rgba: &[u8], i: usize) -> f32 {
    rgba[i * 4] as f32 * 0.299 + rgba[i * 4 + 1] as f32 * 0.587 + rgba[i * 4 + 2] as f32 * 0.114
}

/// Variance locale de luminance dans une fenêtre `(2*radius+1)²` autour de
/// chaque pixel — sert de détecteur de texture fine (cheveux, fourrure,
/// grillage) : une zone plate a une variance quasi nulle, une zone texturée
/// une variance élevée.
fn local_luma_variance(rgba: &[u8], w: usize, h: usize, radius: i32) -> Vec<f32> {
    let at = |x: i32, y: i32| -> f32 {
        let x = x.clamp(0, w as i32 - 1) as usize;
        let y = y.clamp(0, h as i32 - 1) as usize;
        luma_at(rgba, y * w + x)
    };
    let mut out = vec![0.0f32; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut sum = 0.0f32;
            let mut sumsq = 0.0f32;
            let mut n = 0.0f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let v = at(x + dx, y + dy);
                    sum += v;
                    sumsq += v * v;
                    n += 1.0;
                }
            }
            let mean = sum / n;
            out[(y as usize) * w + x as usize] = (sumsq / n - mean * mean).max(0.0);
        }
    }
    out
}

/// Affine un masque de bord dégradé (`soft_edge`) en préservant les détails
/// fins (audit_sprint_xx.md C.1) : `soft_edge` applique un rayon
/// d'adoucissement uniforme, ce qui noie les mèches de cheveux/la fourrure
/// dans un flou générique. Ici, les zones à forte variance de luminance
/// locale (texture fine) voient leur couverture repoussée vers 0/255
/// proportionnellement à cette variance — un bord nettement texturé reste
/// contrasté (donc net brin par brin) tandis que le reste du contour garde
/// le dégradé uniforme d'origine. `radius` : rayon (px) de la fenêtre de
/// variance locale (typiquement 2-3).
pub fn refine_edges(rgba: &[u8], w: usize, h: usize, mask: &[u8], radius: i32) -> Vec<u8> {
    if rgba.len() < w * h * 4 || mask.len() != w * h || w == 0 || h == 0 {
        return mask.to_vec();
    }
    let variance = local_luma_variance(rgba, w, h, radius.max(1));
    let max_var = variance.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    mask.iter()
        .zip(variance.iter())
        .map(|(&m, &v)| {
            let texture = (v / max_var).clamp(0.0, 1.0);
            if texture < 0.15 {
                return m; // zone plate : le dégradé uniforme de soft_edge reste tel quel
            }
            let m = m as f32;
            let sharpened = if m >= 128.0 { m + (255.0 - m) * texture } else { m - m * texture };
            sharpened.round().clamp(0.0, 255.0) as u8
        })
        .collect()
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
    fn refine_edges_sharpens_boundary_more_in_textured_zones() {
        // 10×1 : moitié gauche plate (variance nulle), moitié droite en
        // damier haute fréquence (variance élevée) — même valeur de masque
        // intermédiaire (128) partout, pour isoler l'effet de la texture.
        let w = 10;
        let h = 1;
        let mut rgba = vec![0u8; w * h * 4];
        for x in 0..w {
            let v = if x < 5 { 128u8 } else if x % 2 == 0 { 0 } else { 255 };
            let i = x * 4;
            rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
        let mask = vec![128u8; w * h];
        let refined = refine_edges(&rgba, w, h, &mask, 2);
        let flat_shift = (refined[2] as i32 - 128).abs();
        let textured_shift = (refined[7] as i32 - 128).abs();
        assert!(
            textured_shift > flat_shift,
            "la zone texturée doit s'écarter davantage de 128 que la zone plate : {textured_shift} vs {flat_shift}"
        );
    }

    #[test]
    fn refine_edges_leaves_flat_regions_untouched() {
        let w = 6;
        let h = 6;
        let rgba = vec![100u8; w * h * 4];
        let mask: Vec<u8> = (0..w * h).map(|i| (i * 20 % 256) as u8).collect();
        let refined = refine_edges(&rgba, w, h, &mask, 2);
        assert_eq!(refined, mask, "aucune texture : le masque ne doit pas bouger");
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
