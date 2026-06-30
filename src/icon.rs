//! Icône d'application générée par code (carte blanche arrondie + 4 pastilles
//! de couleur translucides façon palette de peinture). Pas de fichier externe.

/// Icône RGBA pour la fenêtre / le dock (256 px).
pub fn app_icon() -> egui::IconData {
    let (rgba, s) = render(256);
    egui::IconData { rgba, width: s as u32, height: s as u32 }
}

/// Pixels RGBA bruts à une taille donnée (pour générer l'`.icns`).
pub fn rgba_at(size: usize) -> (Vec<u8>, u32, u32) {
    let (rgba, s) = render(size);
    (rgba, s as u32, s as u32)
}

/// Dessine l'icône (carte arrondie + 4 pastilles palette) à la taille `s`.
fn render(s: usize) -> (Vec<u8>, usize) {
    let mut rgba = vec![0u8; s * s * 4];
    let k = s as f32 / 256.0; // tout est calibré pour 256 px
    let center = (s as f32 * 0.5, s as f32 * 0.5);
    let half = (s as f32 * 0.5 - 14.0 * k, s as f32 * 0.5 - 14.0 * k);
    let card = [250u8, 250, 252];

    for y in 0..s {
        for x in 0..s {
            let cov = rrect_cov(x as f32 + 0.5, y as f32 + 0.5, center, half, 52.0 * k);
            if cov > 0.0 {
                blend(&mut rgba, s, x, y, [card[0], card[1], card[2], (255.0 * cov) as u8]);
            }
        }
    }

    let dots: &[((f32, f32), [u8; 3])] = &[
        ((101.0, 101.0), [222, 49, 49]),
        ((155.0, 101.0), [243, 199, 45]),
        ((101.0, 155.0), [58, 181, 76]),
        ((155.0, 155.0), [44, 110, 222]),
    ];
    for ((cx, cy), col) in dots {
        circle(&mut rgba, s, cx * k, cy * k, 46.0 * k, [col[0], col[1], col[2], 205]);
    }
    (rgba, s)
}

/// Couverture (0..1) d'un rectangle arrondi (SDF).
fn rrect_cov(px: f32, py: f32, center: (f32, f32), half: (f32, f32), r: f32) -> f32 {
    let qx = (px - center.0).abs() - (half.0 - r);
    let qy = (py - center.1).abs() - (half.1 - r);
    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    (0.5 - d).clamp(0.0, 1.0)
}

/// Disque anti-aliasé.
fn circle(rgba: &mut [u8], s: usize, cx: f32, cy: f32, rad: f32, color: [u8; 4]) {
    let x0 = (cx - rad - 1.0).max(0.0) as usize;
    let x1 = ((cx + rad + 1.0) as usize).min(s);
    let y0 = (cy - rad - 1.0).max(0.0) as usize;
    let y1 = ((cy + rad + 1.0) as usize).min(s);
    for y in y0..y1 {
        for x in x0..x1 {
            let dist = (((x as f32 + 0.5) - cx).powi(2) + ((y as f32 + 0.5) - cy).powi(2)).sqrt();
            let cov = (rad - dist + 0.5).clamp(0.0, 1.0);
            if cov > 0.0 {
                let a = (color[3] as f32 * cov) as u8;
                blend(rgba, s, x, y, [color[0], color[1], color[2], a]);
            }
        }
    }
}

/// Composition « source over » d'un pixel sur le tampon.
fn blend(rgba: &mut [u8], s: usize, x: usize, y: usize, src: [u8; 4]) {
    let i = (y * s + x) * 4;
    let sa = src[3] as f32 / 255.0;
    for c in 0..3 {
        let d = rgba[i + c] as f32;
        rgba[i + c] = (src[c] as f32 * sa + d * (1.0 - sa)) as u8;
    }
    let da = rgba[i + 3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    rgba[i + 3] = (out_a * 255.0) as u8;
}
