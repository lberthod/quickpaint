//! Bibliothèque d'éléments réutilisables (Sprint 10.1) : pictogrammes et
//! formes composées, au-delà des formes paramétriques (polygone/étoile)
//! déjà existantes. Chaque élément est un contour normalisé (case
//! [-1,1]×[-1,1] centrée sur l'origine), inséré comme un `Stroke` plein —
//! éditable ensuite comme n'importe quelle autre forme (nœuds, couleur,
//! dégradé…), pas une image bitmap figée. 100% embarqué dans le binaire,
//! aucun fichier ni réseau.

use crate::i18n::t;
use crate::model::{Stroke, StrokePoint, Tool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asset {
    Heart,
    SpeechBubble,
    Badge,
    Cross,
    Checkmark,
    Banner,
}

impl Asset {
    pub const ALL: [Asset; 6] =
        [Asset::Heart, Asset::SpeechBubble, Asset::Badge, Asset::Cross, Asset::Checkmark, Asset::Banner];

    pub fn label(self) -> &'static str {
        match self {
            Asset::Heart => t("Cœur", "Heart"),
            Asset::SpeechBubble => t("Bulle", "Speech bubble"),
            Asset::Badge => t("Badge", "Badge"),
            Asset::Cross => t("Croix", "Cross"),
            Asset::Checkmark => t("Coche", "Checkmark"),
            Asset::Banner => t("Bannière", "Banner"),
        }
    }

    /// `false` pour les tracés ouverts (une coche ne se remplit pas).
    pub fn closed(self) -> bool {
        !matches!(self, Asset::Checkmark)
    }

    /// Contour normalisé, centré sur l'origine, dans la case [-1,1]².
    pub fn points(self) -> Vec<(f32, f32)> {
        match self {
            Asset::Heart => heart(),
            Asset::SpeechBubble => speech_bubble(),
            Asset::Badge => badge(),
            Asset::Cross => cross(),
            Asset::Checkmark => checkmark(),
            Asset::Banner => banner(),
        }
    }
}

/// Construit le `Stroke` d'un élément, centré sur `center`, occupant un carré
/// de côté `size`. `fill` est ignoré pour les tracés ouverts (comme les
/// formes de `tools::shape`).
pub fn build(asset: Asset, center: (f32, f32), size: f32, color: [u8; 4], width: f32, fill: bool) -> Stroke {
    let mut stroke = Stroke::new(color, width, Tool::Brush);
    stroke.fill = fill && asset.closed();
    // Contour déjà exact : pas de lissage Catmull-Rom (mêmes raisons que
    // les formes géométriques de `tools::shape`).
    stroke.smooth = false;
    let half = size * 0.5;
    for (x, y) in asset.points() {
        stroke.points.push(StrokePoint { pos: (center.0 + x * half, center.1 + y * half), width });
    }
    stroke
}

/// Cœur classique, paramétrique (Valentine curve), normalisé dans [-1,1]².
fn heart() -> Vec<(f32, f32)> {
    let n = 48;
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());
            (x / 17.0, y / 17.0)
        })
        .collect()
}

/// Bulle de dialogue : rectangle arrondi (approx. polygonale) + pointe.
fn speech_bubble() -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    let corner_r = 0.22;
    let body_bottom = 0.55;
    // Coins arrondis en 6 segments chacun, dans le sens horaire depuis le
    // coin haut-gauche.
    let corners: [((f32, f32), f32, f32); 4] = [
        ((-1.0 + corner_r, -1.0 + corner_r), std::f32::consts::PI, 1.5 * std::f32::consts::PI), // haut-gauche
        ((1.0 - corner_r, -1.0 + corner_r), 1.5 * std::f32::consts::PI, std::f32::consts::TAU),  // haut-droite
        ((1.0 - corner_r, body_bottom - corner_r), 0.0, std::f32::consts::FRAC_PI_2),            // bas-droite
        ((-1.0 + corner_r, body_bottom - corner_r), std::f32::consts::FRAC_PI_2, std::f32::consts::PI), // bas-gauche
    ];
    for (i, (c, a0, a1)) in corners.iter().enumerate() {
        let n = 6;
        for k in 0..=n {
            let a = a0 + (a1 - a0) * (k as f32 / n as f32);
            pts.push((c.0 + a.cos() * corner_r, c.1 + a.sin() * corner_r));
        }
        // Pointe insérée juste après le coin bas-gauche, avant de refermer.
        if i == 3 {
            pts.push((-0.35, body_bottom));
            pts.push((-0.55, 1.0));
            pts.push((-0.05, body_bottom));
        }
    }
    if let Some(&first) = pts.first() {
        pts.push(first); // referme le contour (bord gauche)
    }
    pts
}

/// Badge/sceau : cercle à bords festonnés (comme une étoile très arrondie).
fn badge() -> Vec<(f32, f32)> {
    let n = 16;
    (0..=2 * n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / (2 * n) as f32 * std::f32::consts::TAU;
            let r = if i % 2 == 0 { 1.0 } else { 0.88 };
            (a.cos() * r, a.sin() * r)
        })
        .collect()
}

/// Croix (plus) à 12 sommets, bras de largeur réglée par `arm`.
fn cross() -> Vec<(f32, f32)> {
    let arm = 0.36;
    vec![
        (-arm, -1.0),
        (arm, -1.0),
        (arm, -arm),
        (1.0, -arm),
        (1.0, arm),
        (arm, arm),
        (arm, 1.0),
        (-arm, 1.0),
        (-arm, arm),
        (-1.0, arm),
        (-1.0, -arm),
        (-arm, -arm),
        (-arm, -1.0),
    ]
}

/// Coche : simple polyligne ouverte (jamais remplie).
fn checkmark() -> Vec<(f32, f32)> {
    vec![(-0.9, -0.1), (-0.3, 0.6), (0.9, -0.8)]
}

/// Bannière/ruban : rectangle horizontal aux extrémités échancrées en V.
fn banner() -> Vec<(f32, f32)> {
    vec![
        (-1.0, -0.35),
        (-0.75, 0.0),
        (-1.0, 0.35),
        (1.0, 0.35),
        (0.75, 0.0),
        (1.0, -0.35),
        (-1.0, -0.35), // referme le contour
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_assets_produce_at_least_a_triangle() {
        for asset in Asset::ALL {
            assert!(asset.points().len() >= 3, "{:?} has too few points", asset);
        }
    }

    #[test]
    fn checkmark_is_never_filled() {
        let s = build(Asset::Checkmark, (0.0, 0.0), 10.0, [0; 4], 2.0, true);
        assert!(!s.fill);
    }

    #[test]
    fn closed_assets_can_be_filled() {
        let s = build(Asset::Heart, (0.0, 0.0), 10.0, [0; 4], 2.0, true);
        assert!(s.fill);
    }

    #[test]
    fn build_centers_and_scales_points() {
        let s = build(Asset::Cross, (100.0, 100.0), 20.0, [0; 4], 2.0, false);
        // Toutes les coordonnées normalisées sont dans [-1,1] : le résultat
        // doit rester dans le carré [90,110]×[90,110] (centre ± moitié taille).
        for p in &s.points {
            assert!((90.0..=110.0).contains(&p.pos.0), "{:?}", p.pos);
            assert!((90.0..=110.0).contains(&p.pos.1), "{:?}", p.pos);
        }
    }
}
