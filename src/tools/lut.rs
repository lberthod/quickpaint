//! Import de LUT `.cube` (Sprint 5.3) : table de correspondance couleur 3D
//! au format Adobe/Resolve — parseur texte simple + application par
//! interpolation trilinéaire, avec curseur d'intensité (mélange avec
//! l'original). Appliqué destructivement à une image sélectionnée (comme les
//! autres filtres de `filter.rs`) plutôt qu'en calque de réglage : la table
//! elle-même (des milliers de triplets) serait trop lourde à embarquer dans
//! chaque sauvegarde de projet pour un calque non-destructif.

/// Table de correspondance 3D `size`×`size`×`size`, indexée
/// `r + g*size + b*size²` (même ordre que le fichier `.cube`).
#[derive(Clone, Debug)]
pub struct Lut3D {
    pub size: usize,
    data: Vec<[f32; 3]>,
}

/// Parse un fichier `.cube` (LUT 3D uniquement — les LUT 1D sont rejetées).
pub fn parse_cube(text: &str) -> Result<Lut3D, String> {
    let mut size: Option<usize> = None;
    let mut data = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("LUT_3D_SIZE") {
            size = rest.trim().parse::<usize>().ok();
            continue;
        }
        if line.starts_with("LUT_1D_SIZE") {
            return Err("LUT 1D non supportée (seules les LUT 3D le sont)".into());
        }
        if line.starts_with("TITLE")
            || line.starts_with("DOMAIN_MIN")
            || line.starts_with("DOMAIN_MAX")
            || line.starts_with("LUT_3D_INPUT_RANGE")
        {
            continue; // méta-données ignorées (domaine 0..1 supposé)
        }
        let vals: Vec<f32> = line.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if vals.len() == 3 {
            data.push([vals[0], vals[1], vals[2]]);
        }
    }
    let size = size.ok_or_else(|| "LUT_3D_SIZE manquant".to_string())?;
    let expected = size * size * size;
    if data.len() != expected {
        return Err(format!("nombre de triplets incorrect : attendu {expected}, trouvé {}", data.len()));
    }
    Ok(Lut3D { size, data })
}

impl Lut3D {
    /// Échantillonne la LUT par interpolation trilinéaire (8 voisins).
    /// `r`/`g`/`b` en 0.0..=1.0.
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let n = self.size;
        let scale = (n - 1).max(1) as f32;
        let (fx, fy, fz) = (r.clamp(0.0, 1.0) * scale, g.clamp(0.0, 1.0) * scale, b.clamp(0.0, 1.0) * scale);
        let (x0, y0, z0) = (fx.floor() as usize, fy.floor() as usize, fz.floor() as usize);
        let (x1, y1, z1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, ty, tz) = (fx - x0 as f32, fy - y0 as f32, fz - z0 as f32);
        let at = |x: usize, y: usize, z: usize| self.data[x + y * n + z * n * n];
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
        };
        let c00 = lerp(at(x0, y0, z0), at(x1, y0, z0), tx);
        let c10 = lerp(at(x0, y1, z0), at(x1, y1, z0), tx);
        let c01 = lerp(at(x0, y0, z1), at(x1, y0, z1), tx);
        let c11 = lerp(at(x0, y1, z1), at(x1, y1, z1), tx);
        let c0 = lerp(c00, c10, ty);
        let c1 = lerp(c01, c11, ty);
        lerp(c0, c1, tz)
    }
}

/// Applique la LUT à `rgba`, mélangée avec l'original selon `intensity`
/// (0 = inchangé, 1 = LUT pleine). Alpha non modifié.
pub fn apply_lut(rgba: &mut [u8], lut: &Lut3D, intensity: f32) {
    let intensity = intensity.clamp(0.0, 1.0);
    for px in rgba.chunks_exact_mut(4) {
        let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
        let out = lut.sample(r, g, b);
        for c in 0..3 {
            let orig = px[c] as f32 / 255.0;
            let mixed = orig * (1.0 - intensity) + out[c] * intensity;
            px[c] = (mixed * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_identity_cube() {
        // LUT identité 2×2×2 : chaque coin de sortie = coin d'entrée.
        let text = "\
TITLE \"identity\"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
";
        let lut = parse_cube(text).expect("should parse");
        assert_eq!(lut.size, 2);
        let out = lut.sample(0.3, 0.7, 0.9);
        assert!((out[0] - 0.3).abs() < 1e-5);
        assert!((out[1] - 0.7).abs() < 1e-5);
        assert!((out[2] - 0.9).abs() < 1e-5);
    }

    #[test]
    fn rejects_wrong_data_count() {
        let text = "LUT_3D_SIZE 2\n0 0 0\n1 1 1\n";
        assert!(parse_cube(text).is_err());
    }

    #[test]
    fn rejects_missing_size() {
        let text = "0 0 0\n1 1 1\n";
        assert!(parse_cube(text).is_err());
    }

    #[test]
    fn rejects_1d_lut() {
        let text = "LUT_1D_SIZE 4\n0 0 0\n";
        assert!(parse_cube(text).is_err());
    }

    #[test]
    fn apply_lut_zero_intensity_is_noop() {
        let text = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = parse_cube(text).unwrap();
        let mut rgba = vec![10u8, 20, 30, 255];
        let original = rgba.clone();
        apply_lut(&mut rgba, &lut, 0.0);
        assert_eq!(rgba, original);
    }

    #[test]
    fn apply_lut_full_intensity_matches_identity_sample() {
        let text = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = parse_cube(text).unwrap();
        let mut rgba = vec![10u8, 20, 30, 255];
        apply_lut(&mut rgba, &lut, 1.0);
        // LUT identité : la couleur ne doit (quasi) pas bouger.
        assert!((rgba[0] as i32 - 10).abs() <= 1);
        assert!((rgba[1] as i32 - 20).abs() <= 1);
        assert!((rgba[2] as i32 - 30).abs() <= 1);
        assert_eq!(rgba[3], 255);
    }
}
