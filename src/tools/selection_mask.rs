//! Masque de sélection en pixels (Sprint H) : contour progressif (feather),
//! dilater/contracter — des opérations qui n'ont de sens que si un pixel
//! peut être « à moitié sélectionné », ce que ne permet pas le
//! `HashSet<u64>` d'ID d'éléments utilisé pour déplacer/transformer/
//! supprimer (`PaintApp::selection`). Le masque vit à part
//! (`PaintApp::selection_mask`, pas dans `Document` — même choix que
//! `selection` elle-même : une sélection est un état d'édition éphémère, pas
//! un contenu persisté du document) et se peuple directement depuis la
//! géométrie du **geste** de sélection (rectangle/ellipse/lasso), pas depuis
//! les éléments déjà sélectionnés — bien plus précis pour un feather/
//! dilater qui doit suivre la forme réellement tracée plutôt que la boîte
//! englobante des éléments touchés.
//!
//! Réutilise `RasterLayer` (tuiles éparses en niveaux de gris, canal alpha)
//! comme le suggérait l'audit — structurellement identique à un masque de
//! calque, seule la sémantique change (pilote quels pixels reçoivent l'effet
//! d'un outil, au lieu de piloter l'opacité du calque).

use crate::model::raster::TILE;
use crate::model::RasterLayer;
use crate::tools::SelectionCombine;

/// Peint la couverture d'un geste de sélection (`coverage_at`, 0.0..=1.0 par
/// pixel document) dans le masque existant, selon `combine` — même
/// sémantique que `SelectionCombine` pour les ID d'éléments : `Add` = union
/// (max), `Subtract` = différence, `Intersect` = minimum, `Replace` =
/// écrase. Ne parcourt que `bounds` (bornée au document) pour les modes
/// autres qu'`Intersect` ; `Intersect` doit en plus mettre à zéro tout ce qui
/// existait hors de `bounds` (une intersection avec une région est vide
/// ailleurs), donc parcourt aussi les tuiles déjà peuplées.
pub fn paint_mask_region(
    mask: &mut RasterLayer,
    combine: SelectionCombine,
    doc_w: u32,
    doc_h: u32,
    bounds: ((f32, f32), (f32, f32)),
    coverage_at: impl Fn(f32, f32) -> f32,
) {
    if combine == SelectionCombine::Replace {
        *mask = RasterLayer::default();
    }
    let (mn, mx) = bounds;
    let x0 = mn.0.floor().max(0.0) as i32;
    let y0 = mn.1.floor().max(0.0) as i32;
    let x1 = mx.0.ceil().min(doc_w as f32) as i32;
    let y1 = mx.1.ceil().min(doc_h as f32) as i32;

    if combine == SelectionCombine::Intersect {
        // Tout ce qui est hors de la nouvelle zone ne peut pas survivre à
        // une intersection : on le met à zéro avant d'appliquer le reste,
        // en ne parcourant que les tuiles déjà peintes (masque épars).
        let existing_keys: Vec<(i32, i32)> = mask.tiles.keys().copied().collect();
        for (tx, ty) in existing_keys {
            let (bx0, by0) = (tx * TILE, ty * TILE);
            for ly in 0..TILE {
                for lx in 0..TILE {
                    let (x, y) = (bx0 + lx, by0 + ly);
                    if (x < x0 || x >= x1 || y < y0 || y >= y1) && mask.get_pixel(x, y)[3] != 0 {
                        mask.set_pixel(x, y, [255, 255, 255, 0]);
                    }
                }
            }
        }
    }

    for y in y0..y1 {
        for x in x0..x1 {
            let c = (coverage_at(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            let existing = mask.get_pixel(x, y)[3];
            let new_alpha = match combine {
                SelectionCombine::Replace | SelectionCombine::Add => existing.max(c),
                SelectionCombine::Subtract => existing.saturating_sub(c),
                SelectionCombine::Intersect => existing.min(c),
            };
            if new_alpha != existing {
                mask.set_pixel(x, y, [255, 255, 255, new_alpha]);
            }
        }
    }
}

/// Aplatit le masque en buffer dense (un octet de couverture par pixel,
/// canal alpha de `RasterLayer`) sur tout le document — nécessaire pour les
/// filtres de voisinage (flou, dilatation/érosion) qui doivent lire les
/// pixels environnants indépendamment du découpage en tuiles.
pub fn mask_to_dense(mask: &RasterLayer, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            out[(y as u32 * w + x as u32) as usize] = mask.get_pixel(x, y)[3];
        }
    }
    out
}

/// Reconstruit un `RasterLayer` épars depuis un buffer dense — pixels à
/// zéro laissés absents (pas de tuile allouée), comme un masque tout juste
/// peint.
pub fn dense_to_mask(dense: &[u8], w: u32, h: u32) -> RasterLayer {
    let mut mask = RasterLayer::default();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let a = dense[(y as u32 * w + x as u32) as usize];
            if a > 0 {
                mask.set_pixel(x, y, [255, 255, 255, a]);
            }
        }
    }
    mask
}

/// Contour progressif (feather, Sprint H) : flou boîte (2 passes,
/// horizontale puis verticale) du canal de couverture — un bord net devient
/// un dégradé sur environ `radius` pixels de large de chaque côté.
/// `radius <= 0` = identité.
pub fn feather(dense: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    let r = radius.round() as i32;
    if r <= 0 || w == 0 || h == 0 {
        return dense.to_vec();
    }
    let pass = |src: &[u8], horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let mut sum = 0u32;
                let mut count = 0u32;
                for d in -r..=r {
                    let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                        continue;
                    }
                    sum += src[sy as usize * w + sx as usize] as u32;
                    count += 1;
                }
                out[y as usize * w + x as usize] = sum.checked_div(count).unwrap_or(0) as u8;
            }
        }
        out
    };
    let tmp = pass(dense, true);
    pass(&tmp, false)
}

/// Dilate la sélection (Sprint H) : chaque pixel prend la couverture
/// **maximale** dans un disque de rayon `amount` — grossit la zone
/// sélectionnée. `amount <= 0` = identité.
pub fn dilate(dense: &[u8], w: usize, h: usize, amount: i32) -> Vec<u8> {
    morphology(dense, w, h, amount, |a, b| a.max(b))
}

/// Contracte la sélection (Sprint H) : chaque pixel prend la couverture
/// **minimale** dans un disque de rayon `amount` — rétrécit la zone
/// sélectionnée. `amount <= 0` = identité.
pub fn erode(dense: &[u8], w: usize, h: usize, amount: i32) -> Vec<u8> {
    morphology(dense, w, h, amount, |a, b| a.min(b))
}

/// Filtre morphologique générique (dilatation si `combine = max`, érosion si
/// `combine = min`) sur un disque de rayon `amount`, en pixels.
fn morphology(dense: &[u8], w: usize, h: usize, amount: i32, combine: impl Fn(u8, u8) -> u8) -> Vec<u8> {
    if amount <= 0 || w == 0 || h == 0 {
        return dense.to_vec();
    }
    let r = amount;
    let mut out = vec![0u8; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut acc = dense[y as usize * w + x as usize];
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue; // disque, pas un carré
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                        continue;
                    }
                    acc = combine(acc, dense[sy as usize * w + sx as usize]);
                }
            }
            out[y as usize * w + x as usize] = acc;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_mask_region_replace_sets_full_coverage_inside_bounds() {
        let mut mask = RasterLayer::default();
        paint_mask_region(&mut mask, SelectionCombine::Replace, 10, 10, ((2.0, 2.0), (6.0, 6.0)), |_, _| 1.0);
        assert_eq!(mask.get_pixel(3, 3)[3], 255);
        assert_eq!(mask.get_pixel(8, 8)[3], 0);
    }

    #[test]
    fn paint_mask_region_add_unions_with_existing_coverage() {
        let mut mask = RasterLayer::default();
        paint_mask_region(&mut mask, SelectionCombine::Replace, 10, 10, ((0.0, 0.0), (3.0, 3.0)), |_, _| 1.0);
        paint_mask_region(&mut mask, SelectionCombine::Add, 10, 10, ((5.0, 5.0), (8.0, 8.0)), |_, _| 1.0);
        assert_eq!(mask.get_pixel(1, 1)[3], 255);
        assert_eq!(mask.get_pixel(6, 6)[3], 255);
    }

    #[test]
    fn paint_mask_region_subtract_removes_only_the_new_region() {
        let mut mask = RasterLayer::default();
        paint_mask_region(&mut mask, SelectionCombine::Replace, 10, 10, ((0.0, 0.0), (8.0, 8.0)), |_, _| 1.0);
        paint_mask_region(&mut mask, SelectionCombine::Subtract, 10, 10, ((0.0, 0.0), (4.0, 4.0)), |_, _| 1.0);
        assert_eq!(mask.get_pixel(1, 1)[3], 0);
        assert_eq!(mask.get_pixel(6, 6)[3], 255);
    }

    #[test]
    fn paint_mask_region_intersect_clears_pixels_outside_the_new_bounds() {
        let mut mask = RasterLayer::default();
        paint_mask_region(&mut mask, SelectionCombine::Replace, 10, 10, ((0.0, 0.0), (8.0, 8.0)), |_, _| 1.0);
        paint_mask_region(&mut mask, SelectionCombine::Intersect, 10, 10, ((4.0, 4.0), (10.0, 10.0)), |_, _| 1.0);
        assert_eq!(mask.get_pixel(1, 1)[3], 0, "hors de la nouvelle zone : doit disparaître");
        assert_eq!(mask.get_pixel(6, 6)[3], 255, "dans les deux zones : doit rester");
    }

    #[test]
    fn feather_creates_a_gradient_around_a_hard_edge() {
        let w = 20usize;
        let h = 1usize;
        let mut dense = vec![0u8; w];
        for v in dense.iter_mut().take(10) {
            *v = 255;
        }
        let out = feather(&dense, w, h, 3.0);
        // Loin du bord : encore plein/vide. Près du bord (index 9/10) : dégradé.
        assert_eq!(out[0], 255);
        assert_eq!(out[19], 0);
        assert!(out[9] < 255 && out[9] > 0, "près du bord, la couverture devrait être intermédiaire, got {}", out[9]);
        assert!(out[10] < 255 && out[10] > 0, "près du bord, la couverture devrait être intermédiaire, got {}", out[10]);
    }

    #[test]
    fn dilate_grows_the_selected_region() {
        let w = 10usize;
        let h = 10usize;
        let mut dense = vec![0u8; w * h];
        dense[5 * w + 5] = 255; // un seul pixel sélectionné au centre
        let out = dilate(&dense, w, h, 2);
        // Un voisin à distance 2 doit maintenant être couvert.
        assert_eq!(out[5 * w + 7], 255);
        // Un pixel loin doit rester à 0.
        assert_eq!(out[0], 0);
    }

    #[test]
    fn erode_shrinks_the_selected_region() {
        let w = 10usize;
        let h = 10usize;
        let mut dense = vec![255u8; w * h]; // tout sélectionné
        // Un trou non sélectionné près du bord.
        dense[0] = 0;
        let out = erode(&dense, w, h, 1);
        // Le pixel voisin du trou doit maintenant être rongé (min = 0).
        assert_eq!(out[1], 0);
        // Un pixel loin du trou (coin opposé) reste plein.
        assert_eq!(out[w * h - 1], 255);
    }

    #[test]
    fn mask_dense_round_trip_preserves_coverage() {
        let mut mask = RasterLayer::default();
        paint_mask_region(&mut mask, SelectionCombine::Replace, 8, 8, ((1.0, 1.0), (4.0, 4.0)), |_, _| 1.0);
        let dense = mask_to_dense(&mask, 8, 8);
        let back = dense_to_mask(&dense, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(mask.get_pixel(x, y), back.get_pixel(x, y), "mismatch at ({x},{y})");
            }
        }
    }
}
