//! Réglages de l'outil pinceau.

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Brush {
    pub color: [u8; 4],
    pub width: f32,
    /// Tampon personnalisé (Sprint J.2), importé depuis une image — remplace
    /// la formule de couverture circulaire du pinceau pixel quand présent.
    /// `Arc` pour un clonage de `Brush` bon marché (le buffer peut être
    /// volumineux).
    pub custom_stamp: Option<Arc<BrushStamp>>,
}

impl Default for Brush {
    fn default() -> Self {
        Self { color: [20, 20, 30, 255], width: 6.0, custom_stamp: None }
    }
}

/// Tampon de brosse personnalisé (Sprint J.2) : la luminance de l'image
/// source (pondérée par son alpha) pilote la couverture par pixel, comme un
/// pochoir en niveaux de gris — blanc = pleine couverture, noir = aucune.
#[derive(Debug)]
pub struct BrushStamp {
    pub w: u32,
    pub h: u32,
    /// Couverture par pixel (0..=255).
    coverage: Vec<u8>,
}

impl BrushStamp {
    /// Construit un tampon depuis un buffer RGBA8 (import d'image, Sprint J.2).
    pub fn from_rgba(w: u32, h: u32, rgba: &[u8]) -> Self {
        let coverage = rgba
            .chunks_exact(4)
            .map(|px| {
                let luma = px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114;
                (luma * (px[3] as f32 / 255.0)).round().clamp(0.0, 255.0) as u8
            })
            .collect();
        Self { w, h, coverage }
    }

    /// Couverture (0.0..=1.0) au point normalisé `(u, v)` (chaque axe dans
    /// 0..=1), plus proche voisin — suffisant pour un tampon de brosse.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if self.w == 0 || self.h == 0 {
            return 0.0;
        }
        let x = ((u.clamp(0.0, 0.999) * self.w as f32) as u32).min(self.w - 1);
        let y = ((v.clamp(0.0, 0.999) * self.h as f32) as u32).min(self.h - 1);
        self.coverage[(y * self.w + x) as usize] as f32 / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_stamp_white_pixel_is_full_coverage() {
        let stamp = BrushStamp::from_rgba(1, 1, &[255, 255, 255, 255]);
        assert!((stamp.sample(0.5, 0.5) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn brush_stamp_black_pixel_is_zero_coverage() {
        let stamp = BrushStamp::from_rgba(1, 1, &[0, 0, 0, 255]);
        assert_eq!(stamp.sample(0.5, 0.5), 0.0);
    }

    #[test]
    fn brush_stamp_transparent_pixel_is_zero_coverage_even_if_bright() {
        let stamp = BrushStamp::from_rgba(1, 1, &[255, 255, 255, 0]);
        assert_eq!(stamp.sample(0.5, 0.5), 0.0);
    }

    #[test]
    fn brush_stamp_samples_correct_quadrant_of_a_2x2_image() {
        // Damier : (0,0)=noir, (1,0)=blanc, (0,1)=blanc, (1,1)=noir.
        let rgba = [0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255];
        let stamp = BrushStamp::from_rgba(2, 2, &rgba);
        assert!((stamp.sample(0.75, 0.25) - 1.0).abs() < 1e-3); // (1,0) blanc
        assert_eq!(stamp.sample(0.25, 0.25), 0.0); // (0,0) noir
    }
}
