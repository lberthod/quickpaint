//! Extraction de palette dominante depuis une image (Sprint M.1).
//!
//! Histogramme 3D grossier (chaque canal quantifié à 5 bits, 32 768 cases) +
//! pics les plus fréquents, plutôt qu'un vrai k-moyennes : déterministe (pas
//! de graine aléatoire à fixer pour des tests reproductibles), et suffisant
//! pour dégager 5-8 couleurs dominantes d'une image.

/// Distance euclidienne entre deux couleurs RVB.
fn color_dist(a: [u8; 3], b: [u8; 3]) -> f32 {
    (0..3)
        .map(|i| {
            let d = a[i] as f32 - b[i] as f32;
            d * d
        })
        .sum::<f32>()
        .sqrt()
}

/// Extrait jusqu'à `k` couleurs dominantes d'un buffer RGBA8. Les pixels
/// transparents (alpha = 0) sont ignorés. Les cases de l'histogramme sont
/// triées par nombre de pixels (la plus fréquente en tête) ; deux pics trop
/// proches en couleur (distance < 24) sont fusionnés en ne gardant que le
/// plus fréquent, pour éviter de renvoyer plusieurs nuances quasi identiques
/// d'une même couleur dominante.
pub fn extract_palette(rgba: &[u8], k: usize) -> Vec<[u8; 3]> {
    const SHIFT: u32 = 3; // 8 bits → 5 bits par canal (32 niveaux)
    type BucketSum = (u64, u64, u64, u64); // (somme r, somme g, somme b, nombre de pixels)
    let mut buckets: std::collections::HashMap<(u8, u8, u8), BucketSum> = std::collections::HashMap::new();
    for px in rgba.chunks_exact(4) {
        if px[3] == 0 {
            continue;
        }
        let key = (px[0] >> SHIFT, px[1] >> SHIFT, px[2] >> SHIFT);
        let e = buckets.entry(key).or_insert((0, 0, 0, 0));
        e.0 += px[0] as u64;
        e.1 += px[1] as u64;
        e.2 += px[2] as u64;
        e.3 += 1;
    }
    let mut entries: Vec<BucketSum> = buckets.into_values().collect();
    entries.sort_by_key(|b| std::cmp::Reverse(b.3));

    let mut palette: Vec<[u8; 3]> = Vec::new();
    for (r, g, b, count) in entries {
        if palette.len() >= k || count == 0 {
            break;
        }
        let avg = [(r / count) as u8, (g / count) as u8, (b / count) as u8];
        if palette.iter().any(|&p| color_dist(p, avg) < 24.0) {
            continue;
        }
        palette.push(avg);
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_palette_finds_the_two_dominant_colors_of_a_checkerboard() {
        // Damier rouge/bleu 4x4 : deux couleurs dominantes à parts égales.
        let mut rgba = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let c = if (x + y) % 2 == 0 { [255u8, 0, 0, 255] } else { [0u8, 0, 255, 255] };
                rgba.extend_from_slice(&c);
            }
        }
        let palette = extract_palette(&rgba, 5);
        assert_eq!(palette.len(), 2);
        assert!(palette.contains(&[255, 0, 0]));
        assert!(palette.contains(&[0, 0, 255]));
    }

    #[test]
    fn extract_palette_ignores_transparent_pixels() {
        let mut rgba = Vec::new();
        rgba.extend_from_slice(&[255, 255, 255, 0]); // transparent, ignoré
        for _ in 0..4 {
            rgba.extend_from_slice(&[10, 20, 30, 255]);
        }
        let palette = extract_palette(&rgba, 5);
        assert_eq!(palette, vec![[10, 20, 30]]);
    }

    #[test]
    fn extract_palette_caps_at_k() {
        let mut rgba = Vec::new();
        for c in [[255u8, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]] {
            for _ in 0..4 {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let palette = extract_palette(&rgba, 2);
        assert_eq!(palette.len(), 2);
    }
}
