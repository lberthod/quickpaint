//! Suppression d'objets « content-aware » (Sprint 4.3).
//!
//! Pas un vrai PatchMatch (pas de synthèse de texture) : la zone marquée est
//! d'abord ensemencée avec la couleur du pixel valide le plus proche
//! (remplissage par plus proche voisin, BFS multi-sources), puis lissée par
//! diffusion itérative (moyenne des 4 voisins, façon résolution de Laplace) —
//! un remplissage cohérent avec les couleurs environnantes, suffisant pour
//! effacer un petit objet sans laisser un aplat uniforme qui détonnerait.

/// Efface `mask` (true = pixel à remplacer) dans `rgba` (w×h, RGBA) par
/// diffusion depuis les pixels environnants valides. No-op si `mask` ne
/// couvre aucun pixel ou si les dimensions ne correspondent pas.
pub fn inpaint(rgba: &mut [u8], w: usize, h: usize, mask: &[bool]) {
    if w == 0 || h == 0 || mask.len() != w * h || rgba.len() < w * h * 4 {
        return;
    }
    if !mask.iter().any(|&m| m) {
        return;
    }
    seed_nearest(rgba, w, h, mask);
    // Le nombre d'itérations doit croître avec la taille du trou pour que la
    // diffusion ait le temps d'atteindre son centre (une itération ne
    // propage l'information que d'un pixel à la fois).
    let iterations = (w.max(h) / 2).clamp(20, 200);
    diffuse(rgba, w, h, mask, iterations);
}

fn neighbors4(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut v = Vec::with_capacity(4);
    if x > 0 {
        v.push((x - 1, y));
    }
    if x + 1 < w {
        v.push((x + 1, y));
    }
    if y > 0 {
        v.push((x, y - 1));
    }
    if y + 1 < h {
        v.push((x, y + 1));
    }
    v.into_iter()
}

/// Remplissage initial par plus proche voisin valide (BFS multi-sources
/// depuis tous les pixels hors masque) — donne déjà un résultat plausible
/// avant même la diffusion, et sert de point de départ à celle-ci.
fn seed_nearest(rgba: &mut [u8], w: usize, h: usize, mask: &[bool]) {
    use std::collections::VecDeque;
    let mut visited = vec![false; w * h];
    let mut queue = VecDeque::new();
    for i in 0..w * h {
        if !mask[i] {
            visited[i] = true;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let color: [u8; 4] = rgba[i * 4..i * 4 + 4].try_into().unwrap();
        for (nx, ny) in neighbors4(x, y, w, h) {
            let ni = ny * w + nx;
            if !visited[ni] {
                visited[ni] = true;
                rgba[ni * 4..ni * 4 + 4].copy_from_slice(&color);
                queue.push_back(ni);
            }
        }
    }
}

/// Lissage par diffusion (Jacobi) : chaque pixel masqué devient la moyenne de
/// ses 4 voisins à chaque passe. Les pixels hors masque restent fixes (bord
/// de la diffusion) — en `f32` pour ne pas accumuler d'arrondi sur les
/// nombreuses itérations.
fn diffuse(rgba: &mut [u8], w: usize, h: usize, mask: &[bool], iterations: usize) {
    let n = w * h;
    let mut cur: Vec<[f32; 4]> = (0..n)
        .map(|i| {
            let p = i * 4;
            [rgba[p] as f32, rgba[p + 1] as f32, rgba[p + 2] as f32, rgba[p + 3] as f32]
        })
        .collect();
    let mut next = cur.clone();
    for _ in 0..iterations {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !mask[i] {
                    continue;
                }
                let mut sum = [0.0f32; 4];
                let mut count = 0.0f32;
                for (nx, ny) in neighbors4(x, y, w, h) {
                    let nv = cur[ny * w + nx];
                    for c in 0..4 {
                        sum[c] += nv[c];
                    }
                    count += 1.0;
                }
                if count > 0.0 {
                    for c in 0..4 {
                        next[i][c] = sum[c] / count;
                    }
                }
            }
        }
        std::mem::swap(&mut cur, &mut next);
    }
    for i in 0..n {
        let p = i * 4;
        for c in 0..4 {
            rgba[p + c] = cur[i][c].round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_without_any_masked_pixel() {
        let mut rgba = vec![10u8, 20, 30, 255, 40, 50, 60, 255];
        let original = rgba.clone();
        inpaint(&mut rgba, 2, 1, &[false, false]);
        assert_eq!(rgba, original);
    }

    #[test]
    fn fills_a_hole_with_the_surrounding_uniform_color() {
        let w = 10;
        let h = 10;
        let mut rgba = vec![0u8; w * h * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[200, 100, 50, 255]);
        }
        // Trou carré au centre.
        let mut mask = vec![false; w * h];
        for y in 4..7 {
            for x in 4..7 {
                mask[y * w + x] = true;
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        inpaint(&mut rgba, w, h, &mask);
        // Entouré d'une couleur uniforme : le trou doit reconverger vers elle.
        for y in 4..7 {
            for x in 4..7 {
                let i = (y * w + x) * 4;
                assert!((rgba[i] as i32 - 200).abs() <= 2, "R attendu ~200, eu {}", rgba[i]);
                assert!((rgba[i + 1] as i32 - 100).abs() <= 2, "G attendu ~100, eu {}", rgba[i + 1]);
                assert!((rgba[i + 2] as i32 - 50).abs() <= 2, "B attendu ~50, eu {}", rgba[i + 2]);
            }
        }
    }

    #[test]
    fn blends_between_two_differently_colored_sides() {
        // Moitié gauche noire, moitié droite blanche, trou à cheval sur la
        // frontière : le centre du trou doit finir entre les deux (ni 0 ni 255).
        let w = 20;
        let h = 6;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let mut mask = vec![false; w * h];
        for y in 2..4 {
            for x in (w / 2 - 3)..(w / 2 + 3) {
                mask[y * w + x] = true;
            }
        }
        inpaint(&mut rgba, w, h, &mask);
        let center_i = (3 * w + w / 2) * 4;
        let v = rgba[center_i];
        assert!(v > 20 && v < 235, "attendu un mélange, eu {v}");
    }
}
