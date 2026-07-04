//! Transformation en perspective (Sprint 7.2) : homographie à 4 points de
//! contrôle. Mappe le carré unité (0,0)-(1,0)-(1,1)-(0,1) vers un
//! quadrilatère arbitraire (méthode classique, cf. Paul Heckbert, « Fundamentals
//! of Texture Mapping and Image Warping », 1989) — utilisée ici à l'envers
//! (échantillonnage inverse) pour reprojeter une image dans ce quadrilatère.

/// Matrice 3×3 en `f64` (précision nécessaire : l'inversion peut amplifier
/// les petites erreurs pour des quadrilatères très aplatis).
pub type Mat3 = [[f64; 3]; 3];

/// Matrice mappant le carré unité vers `dst` (coins dans l'ordre haut-gauche,
/// haut-droit, bas-droit, bas-gauche).
pub fn mat3_from_quad(dst: [(f32, f32); 4]) -> Mat3 {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = dst.map(|(x, y)| (x as f64, y as f64));
    let dx1 = x1 - x2;
    let dx2 = x3 - x2;
    let dx3 = x0 - x1 + x2 - x3;
    let dy1 = y1 - y2;
    let dy2 = y3 - y2;
    let dy3 = y0 - y1 + y2 - y3;
    let (g, h) = if dx3.abs() < 1e-9 && dy3.abs() < 1e-9 {
        (0.0, 0.0) // cas affine (parallélogramme) : pas de terme projectif
    } else {
        let denom = dx1 * dy2 - dx2 * dy1;
        ((dx3 * dy2 - dx2 * dy3) / denom, (dx1 * dy3 - dx3 * dy1) / denom)
    };
    let a = x1 - x0 + g * x1;
    let b = x3 - x0 + h * x3;
    let c = x0;
    let d = y1 - y0 + g * y1;
    let e = y3 - y0 + h * y3;
    let f = y0;
    [[a, b, c], [d, e, f], [g, h, 1.0]]
}

/// Inverse d'une matrice 3×3 (cofacteurs), `None` si singulière.
pub fn mat3_inverse(m: Mat3) -> Option<Mat3> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ])
}

/// Applique `m` à un point homogène `(x, y, 1)`, renvoie `(x', y')` normalisé.
pub fn mat3_apply(m: &Mat3, x: f64, y: f64) -> (f64, f64) {
    let xp = m[0][0] * x + m[0][1] * y + m[0][2];
    let yp = m[1][0] * x + m[1][1] * y + m[1][2];
    let wp = m[2][0] * x + m[2][1] * y + m[2][2];
    (xp / wp, yp / wp)
}

/// Reprojette `src` (w×h) dans le quadrilatère `dst_corners` (coordonnées
/// pixels de sortie, **relatives à l'origine de l'image de sortie** —
/// l'appelant a déjà soustrait le minimum du quadrilatère). `out_w`/`out_h`
/// bornent le canevas de sortie (typiquement la boîte englobante du
/// quadrilatère). Échantillonnage inverse, nearest-neighbor.
pub fn apply_perspective(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    dst_corners: [(f32, f32); 4],
    out_w: usize,
    out_h: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; out_w * out_h * 4];
    let Some(inv) = mat3_inverse(mat3_from_quad(dst_corners)) else {
        return out;
    };
    for oy in 0..out_h {
        for ox in 0..out_w {
            let (u, v) = mat3_apply(&inv, ox as f64 + 0.5, oy as f64 + 0.5);
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let sx = ((u * src_w as f64) as usize).min(src_w.saturating_sub(1));
            let sy = ((v * src_h as f64) as usize).min(src_h.saturating_sub(1));
            let sidx = (sy * src_w + sx) * 4;
            let didx = (oy * out_w + ox) * 4;
            out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_quad_round_trips_every_corner() {
        let quad = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let m = mat3_from_quad(quad);
        let checks = [((0.0, 0.0), (0.0, 0.0)), ((1.0, 0.0), (10.0, 0.0)), ((1.0, 1.0), (10.0, 10.0)), ((0.0, 1.0), (0.0, 10.0))];
        for ((u, v), (ex, ey)) in checks {
            let (x, y) = mat3_apply(&m, u, v);
            assert!((x - ex).abs() < 1e-6 && (y - ey).abs() < 1e-6, "got ({x},{y}) expected ({ex},{ey})");
        }
    }

    #[test]
    fn inverse_undoes_the_forward_mapping() {
        // Trapèze (haut plus étroit que le bas) : cas non-affine, teste le
        // terme projectif (g,h).
        let quad = [(20.0, 0.0), (80.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        let m = mat3_from_quad(quad);
        let inv = mat3_inverse(m).expect("invertible");
        for &(u, v) in &[(0.0, 0.0), (0.3, 0.7), (1.0, 1.0), (0.5, 0.5)] {
            let (x, y) = mat3_apply(&m, u, v);
            let (u2, v2) = mat3_apply(&inv, x, y);
            assert!((u - u2).abs() < 1e-6, "u: {u} vs {u2}");
            assert!((v - v2).abs() < 1e-6, "v: {v} vs {v2}");
        }
    }

    #[test]
    fn apply_perspective_identity_copies_the_source() {
        let (w, h) = (4usize, 4usize);
        let mut src = vec![0u8; w * h * 4];
        for (i, px) in src.chunks_exact_mut(4).enumerate() {
            let v = (i * 17 % 256) as u8;
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let quad = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)];
        let out = apply_perspective(&src, w, h, quad, w, h);
        assert_eq!(out, src);
    }

    #[test]
    fn apply_perspective_narrows_content_toward_the_vanishing_side() {
        // Trapèze : le haut de la sortie est plus étroit → un pixel du bord
        // droit source doit se retrouver plus près du centre en haut qu'en bas.
        let (w, h) = (40usize, 40usize);
        let mut src = vec![0u8; w * h * 4];
        for px in src.chunks_exact_mut(4) {
            px.copy_from_slice(&[255, 255, 255, 255]);
        }
        let quad = [(10.0, 0.0), (30.0, 0.0), (40.0, 40.0), (0.0, 40.0)];
        let out = apply_perspective(&src, w, h, quad, w, h);
        // Coin haut-gauche de la sortie (0,0) : hors du quadrilatère (qui
        // commence à x=10 en haut) → transparent.
        assert_eq!(out[3], 0);
        // Le même x=5 mais en bas (le quadrilatère va de 0 à 40 en bas) :
        // dans la zone remplie.
        let bottom_i = ((h - 1) * w + 5) * 4;
        assert_eq!(out[bottom_i + 3], 255);
    }
}
