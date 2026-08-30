//! Filtres image : appliqués soit destructivement aux pixels d'une image
//! sélectionnée (backlog), soit en direct au compositing via un **calque
//! d'ajustement non destructif** (roadmap F3, cf. `render::compositor`).
//! Purs et testables ; l'intégration (undo, ré-encodage PNG) est dans `app`.

use crate::i18n::t;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    Brighter,
    Darker,
    Contrast,
    Saturate,
    Desaturate,
    Grayscale,
    Invert,
    Sharpen,
    Blur,
    /// Grain argentique (Sprint 5.2) : bruit procédural déterministe (même
    /// image → même grain, reproductible sans dépendance à un générateur
    /// aléatoire externe).
    FilmGrain,
    /// Vintage (Sprint 5.2) : virage chaud + désaturation légère + vignettage.
    Vintage,
    /// Croquis (Sprint 5.4) : détection de contours (Sobel) inversée en
    /// niveaux de gris — traits sombres sur fond clair.
    Sketch,
    /// Bande dessinée (Sprint 5.4) : quantification des couleurs (posterize)
    /// + contours noirs superposés (Sobel).
    Comic,
    /// Peinture à l'huile (Sprint 5.4) : filtre de Kuwahara — lisse les
    /// zones homogènes tout en préservant les contours, façon coup de pinceau.
    OilPainting,
    /// Aquarelle (Sprint 5.4) : lissage bilatéral (préserve les contours) +
    /// saturation légèrement augmentée + contours assombris (pigment qui
    /// s'accumule aux bords, comme du vrai papier aquarelle).
    Watercolor,
    /// Auto-correction en un clic (Sprint K.8) : étire l'histogramme de
    /// chaque canal pour que son 1er/99e centile touchent 0/255 —
    /// contrairement aux autres presets, les paramètres sont calculés à
    /// partir de l'image elle-même plutôt que fixés d'avance.
    AutoLevels,
    /// Détection de contours par l'algorithme de Canny (audit K.6) : par
    /// rapport à Sobel seul (utilisé par Croquis/BD, un simple seuil sur la
    /// magnitude), ajoute une suppression non maximale le long de la
    /// direction du gradient puis un double seuil + hystérésis — contours
    /// fins et continus plutôt que des bords épais/bruités.
    Canny,
}

impl Filter {
    /// Tous les filtres, dans l'ordre d'affichage du menu.
    pub const ALL: [Filter; 17] = [
        Filter::Brighter,
        Filter::Darker,
        Filter::Contrast,
        Filter::Saturate,
        Filter::Desaturate,
        Filter::Grayscale,
        Filter::Invert,
        Filter::Sharpen,
        Filter::Blur,
        Filter::FilmGrain,
        Filter::Vintage,
        Filter::Sketch,
        Filter::Comic,
        Filter::OilPainting,
        Filter::Watercolor,
        Filter::AutoLevels,
        Filter::Canny,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::Brighter => t("Plus clair", "Brighter"),
            Filter::Darker => t("Plus sombre", "Darker"),
            Filter::Contrast => t("Contraste +", "Contrast +"),
            Filter::Saturate => t("Saturation +", "Saturation +"),
            Filter::Desaturate => t("Saturation −", "Saturation −"),
            Filter::Grayscale => t("Noir & blanc", "Black & white"),
            Filter::Invert => t("Négatif", "Invert"),
            Filter::Sharpen => t("Netteté", "Sharpen"),
            Filter::Blur => t("Flou", "Blur"),
            Filter::FilmGrain => t("Grain argentique", "Film grain"),
            Filter::Vintage => t("Vintage", "Vintage"),
            Filter::Sketch => t("Croquis", "Sketch"),
            Filter::Comic => t("Bande dessinée", "Comic"),
            Filter::OilPainting => t("Peinture à l'huile", "Oil painting"),
            Filter::Watercolor => t("Aquarelle", "Watercolor"),
            Filter::AutoLevels => t("Correction automatique", "Auto correction"),
            Filter::Canny => t("Contours (Canny)", "Edges (Canny)"),
        }
    }
}

/// Réglage non destructif d'un calque d'ajustement (Sprint 8.1/8.2), en plus
/// des 9 presets discrets de [`Filter`] : niveaux et teinte/saturation à
/// paramètres continus, courbes libres par canal (Sprint S, point 73).
/// Plus `Copy` depuis le Sprint S (les courbes portent des `Vec` de points).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Adjustment {
    Preset(Filter),
    Levels { black: u8, white: u8, gamma: f32 },
    /// `hue` en degrés (-180..180), `sat`/`light` en écart relatif (-1.0..1.0).
    HueSaturation { hue: f32, sat: f32, light: f32 },
    /// Ancienne courbe à 3 points ancrés (ombres/tons moyens/hautes
    /// lumières) — conservée pour rouvrir les projets existants ; les
    /// nouveaux calques utilisent [`Adjustment::CurvesFree`].
    Curves { shadow: u8, mid: u8, highlight: u8 },
    /// Courbes libres par canal (Sprint S, point 73) : points de contrôle
    /// `(entrée, sortie)` — `master` s'applique aux trois canaux, puis les
    /// courbes `r`/`g`/`b` individuelles. Interpolation spline **monotone**
    /// (Fritsch–Carlson : pas de dépassement entre deux points), LUT 256
    /// précalculée à l'application. Deux points (0,0)–(255,255) = identité.
    CurvesFree {
        master: Vec<(u8, u8)>,
        r: Vec<(u8, u8)>,
        g: Vec<(u8, u8)>,
        b: Vec<(u8, u8)>,
    },
    /// Distorsion radiale (Sprint 4.2), -1.0..=1.0 : positif = barrel
    /// (bombé), négatif = pincushion (creusé). 0 = identité.
    Distortion { amount: f32 },
    /// Aberration chromatique (Sprint 4.2), 0.0..=1.0 : décale R et B en sens
    /// opposés depuis le centre, proportionnellement à la distance — visible
    /// surtout en périphérie de l'image, comme un vrai défaut optique.
    ChromaticAberration { amount: f32 },
    /// Flou de mouvement (Sprint 5.1) : moyenne le long d'une direction —
    /// `angle` en degrés, `distance` en pixels (longueur totale de la traînée).
    MotionBlur { angle: f32, distance: f32 },
    /// Flou bokeh (Sprint 5.1) : moyenne dans un disque de rayon `radius`
    /// (px), avec accentuation des hautes lumières (`boost`, 0..=1) pour le
    /// look « taches lumineuses floues » caractéristique.
    Bokeh { radius: f32, boost: f32 },
    /// Duotone (Sprint 5.2) : convertit en luminance puis interpole entre
    /// `shadow` (tons sombres) et `highlight` (tons clairs).
    Duotone { shadow: [u8; 3], highlight: [u8; 3] },
    /// Warp « Arc » (Sprint 7.2) : décale chaque colonne verticalement selon
    /// une courbe en sinus — bombe (amount > 0) ou creuse (amount < 0)
    /// l'image comme une bannière, façon Photoshop Edit ▸ Transform ▸ Warp
    /// ▸ Arc. `amount` en fraction de la hauteur (-1..=1).
    ArcWarp { amount: f32 },
    /// Exposition (audit_sprint_xx.md D.1) : gain multiplicatif en stops
    /// (`2^ev`), appliqué avant tout autre ajustement — contrairement à
    /// Luminosité (`Filter::Brighter`/`Darker`) qui est un facteur fixe, ici
    /// réglable en continu et centré sur 0 = identité.
    Exposure { ev: f32 },
    /// Vibrance (audit_sprint_xx.md D.2) : sature davantage les couleurs déjà
    /// peu saturées et épargne celles qui le sont déjà — contrairement à
    /// `HueSaturation.sat` qui sature tout uniformément. `amount` -1.0..=1.0.
    Vibrance { amount: f32 },
    /// Balance des blancs (audit_sprint_xx.md D.2) : `temp` décale vers le
    /// bleu (négatif) ou l'orange (positif), `tint` vers le vert (négatif)
    /// ou le magenta (positif). Les deux -1.0..=1.0, 0 = identité.
    WhiteBalance { temp: f32, tint: f32 },
    /// Réduction de bruit (audit_sprint_xx.md D.3) : réutilise le lissage
    /// bilatéral de `smooth_skin` (préserve les contours) sur toute l'image.
    /// `strength` 0.0..=1.0, 0 = identité.
    Denoise { strength: f32 },
    /// Flou gaussien réel (audit_sprint_xx.md E.1), noyau séparable —
    /// contrairement à `Filter::Blur` (moyenne de boîte répétée), le rayon
    /// est continu et le profil de poids est une vraie gaussienne. `radius`
    /// en pixels, <= 0 = identité.
    GaussianBlur { radius: f32 },
    /// Pixelisation / mosaïque (Sprint K.1) : moyenne chaque bloc `n×n` et
    /// remplace tous ses pixels par cette moyenne. `block` en pixels, <= 1 =
    /// identité (bloc d'un seul pixel).
    Pixelate { block: f32 },
    /// Halftone / trame (Sprint K.2) : convertit en luminance puis dessine,
    /// pour chaque cellule d'une grille tournée de `angle`, un disque dont le
    /// rayon est proportionnel à l'obscurité de la cellule (façon impression
    /// offset). `cell` en pixels, `angle` en degrés.
    Halftone { cell: f32, angle: f32 },
    /// Distorsion vague (Sprint K.3) : décale chaque colonne verticalement
    /// selon un sinus de longueur d'onde réglable — généralise `ArcWarp`, qui
    /// n'a qu'une seule demi-période fixe. `amplitude` en pixels,
    /// `wavelength` en pixels (> 0).
    Wave { amplitude: f32, wavelength: f32 },
    /// Distorsion sphère (Sprint K.3) : même principe que `Distortion` mais
    /// avec une formule de projection sphérique, plus prononcée en périphérie
    /// que le simple bombé actuel. `amount` -1.0..=1.0, 0 = identité.
    Sphere { amount: f32 },
    /// Distorsion tourbillon (Sprint K.3) : rotation de l'échantillonnage
    /// proportionnelle à la distance au centre — angle maximal au centre, nul
    /// en bordure. `angle` en degrés (rotation max au centre).
    Vortex { angle: f32 },
    /// Flou radial / zoom (Sprint K.4) : moyenne le long de rayons partant du
    /// centre de l'image (effet vitesse/explosion), complète `MotionBlur`
    /// (directionnel). `amount` 0.0..=1.0, 0 = identité.
    RadialBlur { amount: f32 },
    /// Vignette artistique indépendante (Sprint K.5) : assombrit les coins
    /// proportionnellement à la distance au centre — extrait de `vintage()`
    /// en un réglage autonome. `amount` 0.0..=1.0, 0 = identité.
    Vignette { amount: f32 },
    /// Mixeur de canaux pour le N&B (Sprint K.7) : poids réglables par
    /// canal (au lieu de la formule fixe 0.299/0.587/0.114 de `Filter::
    /// Grayscale`), façon « N&B personnalisé » Lightroom. Pas de
    /// normalisation forcée : l'utilisateur peut sur/sous-exposer certains
    /// canaux volontairement.
    ChannelMixerBw { r: f32, g: f32, b: f32 },
    /// Netteté réglable façon Unsharp Mask (audit_100_features.md #68) :
    /// `Filter::Sharpen` a un noyau 3×3 fixe (rayon 1, quantité fixe), sans
    /// seuil — amplifie le bruit dans les zones plates. Ici : flou gaussien
    /// de référence (`radius`), différence source-flou amplifiée par
    /// `amount` (0 = identité, comme un vrai Unsharp Mask), appliquée
    /// seulement là où l'écart dépasse `threshold` (0 = partout, comme
    /// désactivé) pour épargner les zones déjà lisses.
    UnsharpMask { radius: f32, amount: f32, threshold: f32 },
}

impl Adjustment {
    pub fn label(&self) -> String {
        match self {
            Adjustment::Preset(f) => f.label().to_string(),
            Adjustment::Levels { .. } => t("Niveaux", "Levels").to_string(),
            Adjustment::HueSaturation { .. } => t("Teinte/Saturation", "Hue/Saturation").to_string(),
            Adjustment::Curves { .. } => t("Courbes (3 points)", "Curves (3 points)").to_string(),
            Adjustment::CurvesFree { .. } => t("Courbes", "Curves").to_string(),
            Adjustment::Distortion { .. } => t("Distorsion", "Distortion").to_string(),
            Adjustment::ChromaticAberration { .. } => t("Aberration chromatique", "Chromatic aberration").to_string(),
            Adjustment::MotionBlur { .. } => t("Flou de mouvement", "Motion blur").to_string(),
            Adjustment::Bokeh { .. } => t("Bokeh", "Bokeh").to_string(),
            Adjustment::Duotone { .. } => t("Duotone", "Duotone").to_string(),
            Adjustment::ArcWarp { .. } => t("Warp : Arc", "Warp: Arc").to_string(),
            Adjustment::Exposure { .. } => t("Exposition", "Exposure").to_string(),
            Adjustment::Vibrance { .. } => t("Vibrance", "Vibrance").to_string(),
            Adjustment::WhiteBalance { .. } => t("Balance des blancs", "White balance").to_string(),
            Adjustment::Denoise { .. } => t("Réduction de bruit", "Noise reduction").to_string(),
            Adjustment::GaussianBlur { .. } => t("Flou gaussien", "Gaussian blur").to_string(),
            Adjustment::Pixelate { .. } => t("Pixelisation", "Pixelate").to_string(),
            Adjustment::Halftone { .. } => t("Halftone", "Halftone").to_string(),
            Adjustment::Wave { .. } => t("Warp : Vague", "Warp: Wave").to_string(),
            Adjustment::Sphere { .. } => t("Warp : Sphère", "Warp: Sphere").to_string(),
            Adjustment::Vortex { .. } => t("Warp : Tourbillon", "Warp: Vortex").to_string(),
            Adjustment::RadialBlur { .. } => t("Flou radial", "Radial blur").to_string(),
            Adjustment::Vignette { .. } => t("Vignette", "Vignette").to_string(),
            Adjustment::ChannelMixerBw { .. } => t("Mixeur de canaux N&B", "Channel mixer B&W").to_string(),
            Adjustment::UnsharpMask { .. } => t("Netteté (Unsharp Mask)", "Sharpen (Unsharp Mask)").to_string(),
        }
    }

    pub fn default_levels() -> Self {
        Adjustment::Levels { black: 0, white: 255, gamma: 1.0 }
    }

    pub fn default_hue_saturation() -> Self {
        Adjustment::HueSaturation { hue: 0.0, sat: 0.0, light: 0.0 }
    }

    /// Courbes libres identité (Sprint S) : deux points (0,0)–(255,255) sur
    /// chaque canal.
    pub fn default_curves() -> Self {
        let id = vec![(0u8, 0u8), (255u8, 255u8)];
        Adjustment::CurvesFree { master: id.clone(), r: id.clone(), g: id.clone(), b: id }
    }

    pub fn default_distortion() -> Self {
        Adjustment::Distortion { amount: 0.0 }
    }

    pub fn default_chromatic_aberration() -> Self {
        Adjustment::ChromaticAberration { amount: 0.0 }
    }

    pub fn default_motion_blur() -> Self {
        Adjustment::MotionBlur { angle: 0.0, distance: 0.0 }
    }

    pub fn default_bokeh() -> Self {
        Adjustment::Bokeh { radius: 0.0, boost: 0.5 }
    }

    /// Bleu nuit → orange pâle par défaut, un duotone classique et lisible
    /// dès le premier essai plutôt qu'un noir/blanc qui masquerait l'effet.
    pub fn default_duotone() -> Self {
        Adjustment::Duotone { shadow: [20, 30, 80], highlight: [255, 220, 180] }
    }

    pub fn default_arc_warp() -> Self {
        Adjustment::ArcWarp { amount: 0.2 }
    }

    pub fn default_exposure() -> Self {
        Adjustment::Exposure { ev: 0.0 }
    }

    pub fn default_vibrance() -> Self {
        Adjustment::Vibrance { amount: 0.0 }
    }

    pub fn default_white_balance() -> Self {
        Adjustment::WhiteBalance { temp: 0.0, tint: 0.0 }
    }

    pub fn default_denoise() -> Self {
        Adjustment::Denoise { strength: 0.0 }
    }

    pub fn default_gaussian_blur() -> Self {
        Adjustment::GaussianBlur { radius: 0.0 }
    }

    pub fn default_pixelate() -> Self {
        Adjustment::Pixelate { block: 1.0 }
    }

    pub fn default_halftone() -> Self {
        Adjustment::Halftone { cell: 8.0, angle: 15.0 }
    }

    pub fn default_wave() -> Self {
        Adjustment::Wave { amplitude: 0.0, wavelength: 40.0 }
    }

    pub fn default_sphere() -> Self {
        Adjustment::Sphere { amount: 0.0 }
    }

    pub fn default_vortex() -> Self {
        Adjustment::Vortex { angle: 0.0 }
    }

    pub fn default_radial_blur() -> Self {
        Adjustment::RadialBlur { amount: 0.0 }
    }

    pub fn default_vignette() -> Self {
        Adjustment::Vignette { amount: 0.0 }
    }

    pub fn default_channel_mixer_bw() -> Self {
        Adjustment::ChannelMixerBw { r: 0.299, g: 0.587, b: 0.114 }
    }

    pub fn default_unsharp_mask() -> Self {
        Adjustment::UnsharpMask { radius: 1.0, amount: 0.5, threshold: 0.0 }
    }

    /// Signature FNV-1a des paramètres, pour l'invalidation du cache de rendu
    /// (`Adjustment` porte des `f32`, donc pas de `Hash`/`Eq` dérivable).
    pub fn hash_key(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        match self {
            Adjustment::Preset(f) => {
                mix(1);
                mix(*f as u64);
            }
            Adjustment::Levels { black, white, gamma } => {
                mix(2);
                mix(*black as u64);
                mix(*white as u64);
                mix(gamma.to_bits() as u64);
            }
            Adjustment::HueSaturation { hue, sat, light } => {
                mix(3);
                mix(hue.to_bits() as u64);
                mix(sat.to_bits() as u64);
                mix(light.to_bits() as u64);
            }
            Adjustment::Curves { shadow, mid, highlight } => {
                mix(4);
                mix(*shadow as u64);
                mix(*mid as u64);
                mix(*highlight as u64);
            }
            Adjustment::CurvesFree { master, r, g, b } => {
                mix(27);
                for chan in [master, r, g, b] {
                    mix(chan.len() as u64);
                    for &(x, y) in chan {
                        mix(x as u64);
                        mix(y as u64);
                    }
                }
            }
            Adjustment::Distortion { amount } => {
                mix(5);
                mix(amount.to_bits() as u64);
            }
            Adjustment::ChromaticAberration { amount } => {
                mix(6);
                mix(amount.to_bits() as u64);
            }
            Adjustment::MotionBlur { angle, distance } => {
                mix(7);
                mix(angle.to_bits() as u64);
                mix(distance.to_bits() as u64);
            }
            Adjustment::Bokeh { radius, boost } => {
                mix(8);
                mix(radius.to_bits() as u64);
                mix(boost.to_bits() as u64);
            }
            Adjustment::Duotone { shadow, highlight } => {
                mix(9);
                mix(u64::from_be_bytes([0, 0, 0, 0, 0, shadow[0], shadow[1], shadow[2]]));
                mix(u64::from_be_bytes([0, 0, 0, 0, 0, highlight[0], highlight[1], highlight[2]]));
            }
            Adjustment::ArcWarp { amount } => {
                mix(10);
                mix(amount.to_bits() as u64);
            }
            Adjustment::Exposure { ev } => {
                mix(11);
                mix(ev.to_bits() as u64);
            }
            Adjustment::Vibrance { amount } => {
                mix(12);
                mix(amount.to_bits() as u64);
            }
            Adjustment::WhiteBalance { temp, tint } => {
                mix(13);
                mix(temp.to_bits() as u64);
                mix(tint.to_bits() as u64);
            }
            Adjustment::Denoise { strength } => {
                mix(14);
                mix(strength.to_bits() as u64);
            }
            Adjustment::GaussianBlur { radius } => {
                mix(15);
                mix(radius.to_bits() as u64);
            }
            Adjustment::Pixelate { block } => {
                mix(16);
                mix(block.to_bits() as u64);
            }
            Adjustment::Halftone { cell, angle } => {
                mix(17);
                mix(cell.to_bits() as u64);
                mix(angle.to_bits() as u64);
            }
            Adjustment::Wave { amplitude, wavelength } => {
                mix(18);
                mix(amplitude.to_bits() as u64);
                mix(wavelength.to_bits() as u64);
            }
            Adjustment::Sphere { amount } => {
                mix(19);
                mix(amount.to_bits() as u64);
            }
            Adjustment::Vortex { angle } => {
                mix(20);
                mix(angle.to_bits() as u64);
            }
            Adjustment::RadialBlur { amount } => {
                mix(21);
                mix(amount.to_bits() as u64);
            }
            Adjustment::Vignette { amount } => {
                mix(22);
                mix(amount.to_bits() as u64);
            }
            Adjustment::ChannelMixerBw { r, g, b } => {
                mix(23);
                mix(r.to_bits() as u64);
                mix(g.to_bits() as u64);
                mix(b.to_bits() as u64);
            }
            Adjustment::UnsharpMask { radius, amount, threshold } => {
                mix(28);
                mix(radius.to_bits() as u64);
                mix(amount.to_bits() as u64);
                mix(threshold.to_bits() as u64);
            }
        }
        h
    }
}

/// Applique un réglage d'ajustement en place (canal composite RVB, alpha
/// inchangé).
pub fn apply_adjustment(adj: Adjustment, rgba: &mut Vec<u8>, w: u32, h: u32) {
    match adj {
        Adjustment::Preset(f) => apply(f, rgba, w, h),
        Adjustment::Levels { black, white, gamma } => levels(rgba, black, white, gamma),
        Adjustment::HueSaturation { hue, sat, light } => hue_saturation(rgba, hue, sat, light),
        Adjustment::Curves { shadow, mid, highlight } => curves(rgba, shadow, mid, highlight),
        Adjustment::CurvesFree { master, r, g, b } => curves_free(rgba, &master, &r, &g, &b),
        Adjustment::Distortion { amount } => *rgba = distort_radial(rgba, w as usize, h as usize, amount),
        Adjustment::ChromaticAberration { amount } => *rgba = chromatic_aberration(rgba, w as usize, h as usize, amount),
        Adjustment::MotionBlur { angle, distance } => *rgba = motion_blur(rgba, w as usize, h as usize, angle, distance),
        Adjustment::Bokeh { radius, boost } => *rgba = bokeh_blur(rgba, w as usize, h as usize, radius, boost),
        Adjustment::Duotone { shadow, highlight } => duotone(rgba, shadow, highlight),
        Adjustment::ArcWarp { amount } => *rgba = arc_warp(rgba, w as usize, h as usize, amount),
        Adjustment::Exposure { ev } => exposure(rgba, ev),
        Adjustment::Vibrance { amount } => vibrance(rgba, amount),
        Adjustment::WhiteBalance { temp, tint } => white_balance(rgba, temp, tint),
        Adjustment::Denoise { strength } => {
            let full_mask = vec![true; (w as usize) * (h as usize)];
            smooth_skin(rgba, w as usize, h as usize, &full_mask, strength.clamp(0.0, 1.0));
        }
        Adjustment::GaussianBlur { radius } => *rgba = gaussian_blur(rgba, w as usize, h as usize, radius),
        Adjustment::Pixelate { block } => pixelate(rgba, w as usize, h as usize, block),
        Adjustment::Halftone { cell, angle } => *rgba = halftone(rgba, w as usize, h as usize, cell, angle),
        Adjustment::Wave { amplitude, wavelength } => *rgba = wave_warp(rgba, w as usize, h as usize, amplitude, wavelength),
        Adjustment::Sphere { amount } => *rgba = sphere_warp(rgba, w as usize, h as usize, amount),
        Adjustment::Vortex { angle } => *rgba = vortex_warp(rgba, w as usize, h as usize, angle),
        Adjustment::RadialBlur { amount } => *rgba = radial_blur(rgba, w as usize, h as usize, amount),
        Adjustment::Vignette { amount } => apply_vignette(rgba, w as usize, h as usize, amount),
        Adjustment::ChannelMixerBw { r, g, b } => channel_mixer_bw(rgba, r, g, b),
        Adjustment::UnsharpMask { radius, amount, threshold } => {
            *rgba = unsharp_mask(rgba, w as usize, h as usize, radius, amount, threshold)
        }
    }
}

/// Exposition (audit_sprint_xx.md D.1) : gain multiplicatif en stops,
/// `out = in * 2^ev`. `ev = 0` laisse l'image inchangée.
fn exposure(rgba: &mut [u8], ev: f32) {
    if ev.abs() < 1e-4 {
        return;
    }
    let gain = 2f32.powf(ev);
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = (*c as f32 * gain).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Vibrance (audit_sprint_xx.md D.2) : comme `saturate`, mais le gain de
/// saturation est pondéré par `1 - saturation_actuelle` — les couleurs déjà
/// vives changent peu, les couleurs ternes gagnent le plus. `amount = 0` est
/// un no-op exact (facteur toujours 1).
fn vibrance(rgba: &mut [u8], amount: f32) {
    if amount.abs() < 1e-4 {
        return;
    }
    for px in rgba.chunks_exact_mut(4) {
        let (_, s, _) = rgb_to_hsl(px[0], px[1], px[2]);
        let factor = 1.0 + amount * (1.0 - s);
        let g = luma(px);
        for c in px.iter_mut().take(3) {
            *c = (g + (*c as f32 - g) * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Balance des blancs (audit_sprint_xx.md D.2) : décalage linéaire simple —
/// `temp` pousse R à la hausse et B à la baisse (ou l'inverse si négatif),
/// `tint` pousse G à la hausse et R+B à la baisse (ou l'inverse). Formule
/// volontairement simple (pas d'estimation de température de couleur en
/// Kelvin) : suffisante pour corriger une dominante, pas une calibration
/// colorimétrique. `(0, 0)` est un no-op exact.
fn white_balance(rgba: &mut [u8], temp: f32, tint: f32) {
    if temp.abs() < 1e-4 && tint.abs() < 1e-4 {
        return;
    }
    let (dr, db) = (temp * 40.0, -temp * 40.0);
    let (dg, drb) = (tint * 40.0, -tint * 20.0);
    for px in rgba.chunks_exact_mut(4) {
        px[0] = (px[0] as f32 + dr + drb).clamp(0.0, 255.0) as u8;
        px[1] = (px[1] as f32 + dg).clamp(0.0, 255.0) as u8;
        px[2] = (px[2] as f32 + db + drb).clamp(0.0, 255.0) as u8;
    }
}

/// Poids gaussiens 1D normalisés (somme = 1) pour un rayon donné — 3 écarts
/// types couvrent le rayon demandé, comme une implémentation Gaussienne
/// classique tronquée.
fn gaussian_kernel(radius: f32) -> Vec<f32> {
    let r = radius.max(0.5);
    let sigma = r / 3.0;
    let n = r.ceil() as i32;
    let mut kernel: Vec<f32> = (-n..=n).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = kernel.iter().sum();
    for v in kernel.iter_mut() {
        *v /= sum;
    }
    kernel
}

/// Flou gaussien séparable (audit_sprint_xx.md E.1) : passe horizontale puis
/// verticale avec un vrai noyau gaussien (contrairement à `box_blur`, qui
/// moyenne uniformément). `radius <= 0` = identité.
fn gaussian_blur(src: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || radius <= 0.05 {
        return src.to_vec();
    }
    let kernel = gaussian_kernel(radius);
    let n = (kernel.len() / 2) as i32;
    let pass = |src: &[u8], horizontal: bool| -> Vec<u8> {
        let mut out = vec![0u8; w * h * 4];
        let at = |x: i32, y: i32, c: usize| -> f32 {
            let x = x.clamp(0, w as i32 - 1) as usize;
            let y = y.clamp(0, h as i32 - 1) as usize;
            src[(y * w + x) * 4 + c] as f32
        };
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let mut sum = [0.0f32; 4];
                for (k, &weight) in kernel.iter().enumerate() {
                    let d = k as i32 - n;
                    let (sx, sy) = if horizontal { (x + d, y) } else { (x, y + d) };
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += at(sx, sy, c) * weight;
                    }
                }
                let didx = (y as usize * w + x as usize) * 4;
                for c in 0..4 {
                    out[didx + c] = sum[c].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    };
    let tmp = pass(src, true);
    pass(&tmp, false)
}

/// Warp « Arc » (Sprint 7.2) : décale la colonne `x` verticalement de
/// `amount * h * 0.3 * sin(pi * x / w)` — bombe le milieu vers le haut
/// (`amount > 0`) ou vers le bas (`amount < 0`), les bords restent fixes
/// (sin nul en x=0 et x=w).
fn arc_warp(src: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amount.abs() < 1e-4 {
        return src.to_vec();
    }
    let max_shift = amount * h as f32 * 0.3;
    let mut out = vec![0u8; w * h * 4];
    for x in 0..w {
        let shift = max_shift * (std::f32::consts::PI * x as f32 / w.max(1) as f32).sin();
        for y in 0..h {
            let sy = y as f32 - shift;
            if sy < 0.0 || sy as usize >= h {
                continue;
            }
            let sidx = (sy as usize * w + x) * 4;
            let didx = (y * w + x) * 4;
            out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
        }
    }
    out
}

/// Distorsion vague (Sprint K.3) : généralise `arc_warp` avec une longueur
/// d'onde réglable au lieu d'une seule demi-période fixe sur toute la
/// largeur. `amplitude <= 0` (en valeur absolue) ou `wavelength <= 0` =
/// identité.
fn wave_warp(src: &[u8], w: usize, h: usize, amplitude: f32, wavelength: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amplitude.abs() < 1e-4 || wavelength <= 1e-4 {
        return src.to_vec();
    }
    let mut out = vec![0u8; w * h * 4];
    for x in 0..w {
        let shift = amplitude * (2.0 * std::f32::consts::PI * x as f32 / wavelength).sin();
        for y in 0..h {
            let sy = y as f32 - shift;
            if sy < 0.0 || sy as usize >= h {
                continue;
            }
            let sidx = (sy as usize * w + x) * 4;
            let didx = (y * w + x) * 4;
            out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
        }
    }
    out
}

/// Distorsion sphère (Sprint K.3) : même échantillonnage inverse que
/// `distort_radial`, mais avec un facteur non-linéaire (`1 - amount *
/// (1 - r²)`) qui pousse davantage la périphérie qu'un simple bombé barrel —
/// une vraie projection sphérique plutôt qu'une distorsion optique légère.
/// `amount` -1.0..=1.0, 0 = identité.
fn sphere_warp(src: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amount.abs() < 1e-4 {
        return src.to_vec();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let scale = (cx * cx + cy * cy).sqrt().max(1.0);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 + 0.5 - cx) / scale, (y as f32 + 0.5 - cy) / scale);
            let r2 = (nx * nx + ny * ny).min(1.0);
            let factor = 1.0 - amount * (1.0 - r2);
            let didx = (y * w + x) * 4;
            if factor.abs() < 1e-6 {
                continue;
            }
            let (sx, sy) = (cx + (nx / factor) * scale, cy + (ny / factor) * scale);
            if sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h {
                let sidx = ((sy as usize) * w + (sx as usize)) * 4;
                out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
            }
        }
    }
    out
}

/// Distorsion tourbillon (Sprint K.3) : rotation de l'échantillonnage
/// proportionnelle à `1 - r` (rotation maximale au centre, nulle en bordure).
/// `angle` en degrés = rotation maximale au centre ; `angle.abs() < 1e-4` =
/// identité.
fn vortex_warp(src: &[u8], w: usize, h: usize, angle_deg: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || angle_deg.abs() < 1e-4 {
        return src.to_vec();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let max_r = (cx * cx + cy * cy).sqrt().max(1.0);
    let max_angle = angle_deg.to_radians();
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let r = (dx * dx + dy * dy).sqrt();
            let t = (r / max_r).min(1.0);
            let rot = max_angle * (1.0 - t);
            let (sin, cos) = rot.sin_cos();
            let (sx, sy) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
            let didx = (y * w + x) * 4;
            if sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h {
                let sidx = ((sy as usize) * w + (sx as usize)) * 4;
                out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
            }
        }
    }
    out
}

/// Pixelisation / mosaïque (Sprint K.1) : moyenne chaque bloc `n×n` (n =
/// `block.round()`, minimum 1) et remplace tous ses pixels par cette
/// moyenne. `block <= 1` = identité.
fn pixelate(rgba: &mut [u8], w: usize, h: usize, block: f32) {
    let n = (block.round() as usize).max(1);
    if w == 0 || h == 0 || n <= 1 {
        return;
    }
    let mut by = 0usize;
    while by < h {
        let bh = n.min(h - by);
        let mut bx = 0usize;
        while bx < w {
            let bw = n.min(w - bx);
            let mut sum = [0u32; 4];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        sum[c] += rgba[i + c] as u32;
                    }
                }
            }
            let count = (bw * bh) as u32;
            let avg: [u8; 4] = std::array::from_fn(|c| (sum[c] / count) as u8);
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let i = (y * w + x) * 4;
                    rgba[i..i + 4].copy_from_slice(&avg);
                }
            }
            bx += n;
        }
        by += n;
    }
}

/// Halftone / trame (Sprint K.2) : convertit en luminance puis, pour chaque
/// cellule d'une grille de taille `cell` tournée de `angle`, dessine un
/// disque centré sur la cellule dont le rayon est proportionnel à
/// l'obscurité moyenne de la cellule (cellule noire → disque couvrant toute
/// la cellule ; blanche → rayon nul). Fond blanc entre les disques, comme un
/// tramage niveaux de gris classique.
fn halftone(src: &[u8], w: usize, h: usize, cell: f32, angle_deg: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || cell <= 1e-4 {
        return src.to_vec();
    }
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    // Repère de la grille tournée : (u, v) = rotation de (x, y) par -angle,
    // pour que la grille de cellules soit axée sur (u, v).
    let to_grid = |x: f32, y: f32| -> (f32, f32) { (x * cos + y * sin, -x * sin + y * cos) };
    let from_grid = |u: f32, v: f32| -> (f32, f32) { (u * cos - v * sin, u * sin + v * cos) };
    // Rayon max = demi-diagonale de la cellule : une cellule entièrement
    // noire produit un disque qui couvre exactement toute la cellule
    // (coins inclus), une cellule blanche un rayon nul.
    let max_radius = cell * std::f32::consts::SQRT_2 * 0.5;
    let mut out = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = to_grid(x as f32 + 0.5, y as f32 + 0.5);
            let (cu, cv) = ((u / cell).floor() + 0.5, (v / cell).floor() + 0.5);
            let center_u = cu * cell;
            let center_v = cv * cell;
            // Luminance échantillonnée au centre de la cellule (image
            // d'origine, avant rotation) — suffisant pour un rendu léger.
            let (center_x, center_y) = from_grid(center_u, center_v);
            let sx = (center_x as i64).clamp(0, w as i64 - 1) as usize;
            let sy = (center_y as i64).clamp(0, h as i64 - 1) as usize;
            let l = luma(&src[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]) / 255.0;
            let radius = max_radius * (1.0 - l);
            let dist = ((u - center_u).powi(2) + (v - center_v).powi(2)).sqrt();
            let didx = (y * w + x) * 4;
            let v_out = if dist <= radius { 0u8 } else { 255u8 };
            out[didx] = v_out;
            out[didx + 1] = v_out;
            out[didx + 2] = v_out;
            out[didx + 3] = src[didx + 3];
        }
    }
    out
}

/// Flou radial / zoom (Sprint K.4) : moyenne le long de rayons partant du
/// centre de l'image, complète `motion_blur` (directionnel constant) —
/// l'échantillonnage suit la direction radiale de chaque pixel plutôt qu'une
/// direction fixe. `amount <= 0` = identité.
fn radial_blur(src: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amount <= 1e-4 {
        return src.to_vec();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let steps = 8;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let mut sum = [0.0f32; 4];
            let mut count = 0.0f32;
            for s in 0..=steps {
                let t = 1.0 - amount.min(1.0) * (s as f32 / steps as f32);
                let sx = (cx + dx * t) as i64;
                let sy = (cy + dy * t) as i64;
                if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                    continue;
                }
                let i = (sy as usize * w + sx as usize) * 4;
                for c in 0..4 {
                    sum[c] += src[i + c] as f32;
                }
                count += 1.0;
            }
            let didx = (y * w + x) * 4;
            if count > 0.0 {
                for c in 0..4 {
                    out[didx + c] = (sum[c] / count).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Mixeur de canaux pour le N&B (Sprint K.7) : remplace la formule fixe de
/// `luma()` (0.299/0.587/0.114) par des poids réglables par canal, façon
/// « N&B personnalisé » Lightroom. Pas de normalisation forcée des poids.
fn channel_mixer_bw(rgba: &mut [u8], r: f32, g: f32, b: f32) {
    for px in rgba.chunks_exact_mut(4) {
        let v = (px[0] as f32 * r + px[1] as f32 * g + px[2] as f32 * b).clamp(0.0, 255.0) as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
}

/// Duotone (Sprint 5.2) : luminance → interpolation entre `shadow`/`highlight`.
fn duotone(rgba: &mut [u8], shadow: [u8; 3], highlight: [u8; 3]) {
    for px in rgba.chunks_exact_mut(4) {
        let l = luma(px) / 255.0;
        for c in 0..3 {
            px[c] = (shadow[c] as f32 + (highlight[c] as f32 - shadow[c] as f32) * l).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Niveaux façon Photoshop : point noir/blanc + gamma. `out = ((in-black)/(white-black))^(1/gamma)`.
fn levels(rgba: &mut [u8], black: u8, white: u8, gamma: f32) {
    let b = black as f32;
    let w = (white.max(black.saturating_add(1))) as f32;
    let inv_gamma = 1.0 / gamma.max(0.01);
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            let v = ((*c as f32 - b) / (w - b)).clamp(0.0, 1.0);
            *c = (v.powf(inv_gamma) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Courbe à 3 points ancrés (x = 0/128/255), interpolation linéaire entre eux.
fn curve_lut(shadow: u8, mid: u8, highlight: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let (s, m, h) = (shadow as f32, mid as f32, highlight as f32);
    for (i, slot) in lut.iter_mut().enumerate() {
        let x = i as f32;
        let y = if x <= 128.0 { s + (m - s) * (x / 128.0) } else { m + (h - m) * ((x - 128.0) / 127.0) };
        *slot = y.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Courbes libres par canal (Sprint S, point 73) : composition LUT master
/// puis LUT du canal — `sortie = canal[master[entrée]]`.
fn curves_free(rgba: &mut [u8], master: &[(u8, u8)], r: &[(u8, u8)], g: &[(u8, u8)], b: &[(u8, u8)]) {
    let ml = points_lut(master);
    let luts = [points_lut(r), points_lut(g), points_lut(b)];
    for px in rgba.chunks_exact_mut(4) {
        for c in 0..3 {
            px[c] = luts[c][ml[px[c] as usize] as usize];
        }
    }
}

/// LUT 256 d'une courbe à points de contrôle `(entrée, sortie)` : tri par
/// entrée (doublons fusionnés, dernier gagne), puis spline **monotone** de
/// Fritsch–Carlson entre les points — lisse comme une courbe Photoshop mais
/// sans jamais dépasser l'intervalle des sorties voisines (pas d'oscillation
/// parasite). Hors de la plage des points : prolongé à plat. Moins de 2
/// points = identité.
pub fn points_lut(points: &[(u8, u8)]) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let mut pts: Vec<(u8, u8)> = points.to_vec();
    pts.sort_by_key(|p| p.0);
    pts.dedup_by_key(|p| p.0);
    if pts.len() < 2 {
        for (i, v) in lut.iter_mut().enumerate() {
            *v = i as u8;
        }
        return lut;
    }
    let xs: Vec<f32> = pts.iter().map(|p| p.0 as f32).collect();
    let ys: Vec<f32> = pts.iter().map(|p| p.1 as f32).collect();
    let n = pts.len();
    // Pentes des segments, puis tangentes de Fritsch–Carlson.
    let d: Vec<f32> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i])).collect();
    let mut m = vec![0.0f32; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) * 0.5 };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[i] = t * a * d[i];
            m[i + 1] = t * b * d[i];
        }
    }
    for (x, v) in lut.iter_mut().enumerate() {
        let xf = x as f32;
        *v = if xf <= xs[0] {
            ys[0].round() as u8
        } else if xf >= xs[n - 1] {
            ys[n - 1].round() as u8
        } else {
            let i = (0..n - 1).find(|&i| xf <= xs[i + 1]).unwrap_or(n - 2);
            let h = xs[i + 1] - xs[i];
            let t = (xf - xs[i]) / h;
            let (t2, t3) = (t * t, t * t * t);
            let y = ys[i] * (2.0 * t3 - 3.0 * t2 + 1.0)
                + m[i] * h * (t3 - 2.0 * t2 + t)
                + ys[i + 1] * (-2.0 * t3 + 3.0 * t2)
                + m[i + 1] * h * (t3 - t2);
            y.round().clamp(0.0, 255.0) as u8
        };
    }
    lut
}

fn curves(rgba: &mut [u8], shadow: u8, mid: u8, highlight: u8) {
    let lut = curve_lut(shadow, mid, highlight);
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = lut[*c as usize];
        }
    }
}

/// Teinte/saturation/luminosité via un aller-retour RVB↔HSL par pixel.
fn hue_saturation(rgba: &mut [u8], hue_deg: f32, sat: f32, light: f32) {
    for px in rgba.chunks_exact_mut(4) {
        let (h0, s0, l0) = rgb_to_hsl(px[0], px[1], px[2]);
        let h = (h0 + hue_deg / 360.0).rem_euclid(1.0);
        let s = (s0 * (1.0 + sat)).clamp(0.0, 1.0);
        let l = (l0 + light).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    }
}

pub(crate) fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s <= 0.0 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue2rgb = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };
    let r = hue2rgb(p, q, h + 1.0 / 3.0);
    let g = hue2rgb(p, q, h);
    let b = hue2rgb(p, q, h - 1.0 / 3.0);
    let conv = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    (conv(r), conv(g), conv(b))
}

/// Distorsion radiale (Sprint 4.2), échantillonnage inverse (nearest-neighbor)
/// centré sur l'image : `amount` > 0 bombe (barrel), < 0 creuse (pincushion).
/// Coordonnées normalisées par la demi-diagonale pour rester cohérent quel
/// que soit le ratio largeur/hauteur.
fn distort_radial(src: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amount.abs() < 1e-4 {
        return src.to_vec();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let scale = (cx * cx + cy * cy).sqrt().max(1.0);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 + 0.5 - cx) / scale, (y as f32 + 0.5 - cy) / scale);
            let r2 = nx * nx + ny * ny;
            let factor = 1.0 + amount * r2;
            let (sx, sy) = (cx + nx * factor * scale, cy + ny * factor * scale);
            let didx = (y * w + x) * 4;
            if sx >= 0.0 && sy >= 0.0 && (sx as usize) < w && (sy as usize) < h {
                let sidx = ((sy as usize) * w + (sx as usize)) * 4;
                out[didx..didx + 4].copy_from_slice(&src[sidx..sidx + 4]);
            }
        }
    }
    out
}

/// Aberration chromatique (Sprint 4.2) : R échantillonné légèrement vers
/// l'extérieur du centre, B vers l'intérieur, G inchangé — décalage
/// proportionnel à la distance au centre, nul en son milieu (comme un vrai
/// défaut d'objectif, imperceptible au centre de l'image).
fn chromatic_aberration(src: &[u8], w: usize, h: usize, amount: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || amount.abs() < 1e-4 {
        return src.to_vec();
    }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let scale = (cx * cx + cy * cy).sqrt().max(1.0);
    let max_shift = amount * scale * 0.04; // décalage max modeste, en pixels
    let sample = |sx: f32, sy: f32, c: usize| -> u8 {
        if sx < 0.0 || sy < 0.0 || sx as usize >= w || sy as usize >= h {
            return 0;
        }
        src[((sy as usize) * w + (sx as usize)) * 4 + c]
    };
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 + 0.5 - cx) / scale, (y as f32 + 0.5 - cy) / scale);
            let r = (nx * nx + ny * ny).sqrt().max(1e-6);
            let (dx, dy) = (nx / r, ny / r);
            let shift = max_shift * r;
            let didx = (y * w + x) * 4;
            let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
            out[didx] = sample(xf + dx * shift, yf + dy * shift, 0); // R vers l'extérieur
            out[didx + 1] = src[didx + 1]; // G inchangé
            out[didx + 2] = sample(xf - dx * shift, yf - dy * shift, 2); // B vers l'intérieur
            out[didx + 3] = src[didx + 3]; // alpha inchangé
        }
    }
    out
}

/// Suppression des yeux rouges (Sprint 4.4) : neutralise la teinte rouge
/// dominante des pixels de `mask` (le reste de l'image n'est pas touché,
/// contrairement à un simple filtre appliqué à toute la zone), en tirant R
/// vers la moyenne de G/B — comme les autres canaux ne bougent pas, un pixel
/// gris/blanc (reflet du flash) reste inchangé, seule la vraie rougeur de la
/// pupille est corrigée.
pub fn reduce_red_eye(rgba: &mut [u8], w: usize, h: usize, mask: &[bool]) {
    if mask.len() != w * h || rgba.len() < w * h * 4 {
        return;
    }
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        if !mask[i] {
            continue;
        }
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        // « Rouge dominant » : R nettement au-dessus de G et B, pas juste une
        // peau/carnation normale (qui a aussi R > G,B, mais avec un écart
        // bien plus faible qu'une pupille rouge de flash).
        if r > g * 1.3 && r > b * 1.3 && r > 60.0 {
            let neutral = (g + b) * 0.5;
            px[0] = neutral.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Retouche peau (Sprint 4.4) : flou guidé par la luminance (bilatéral
/// simplifié) — moyenne les voisins d'un rayon 3, mais ne pondère que ceux
/// dont la luminance est proche du pixel central, pour lisser la peau sans
/// effacer les contours nets (yeux, sourcils, lèvres). `strength` (0..=1)
/// mélange le résultat lissé avec l'original.
pub fn smooth_skin(rgba: &mut [u8], w: usize, h: usize, mask: &[bool], strength: f32) {
    if w == 0 || h == 0 || mask.len() != w * h || rgba.len() < w * h * 4 {
        return;
    }
    let strength = strength.clamp(0.0, 1.0);
    let radius = 3isize;
    let luma_thresh = 30.0f32;
    let src = rgba.to_vec();
    let at = |x: isize, y: isize, c: usize| -> f32 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        src[(y * w + x) * 4 + c] as f32
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !mask[i] {
                continue;
            }
            let center_luma = luma(&src[i * 4..i * 4 + 4]);
            let mut sum = [0.0f32; 3];
            let mut weight = 0.0f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    let px = [at(nx, ny, 0), at(nx, ny, 1), at(nx, ny, 2)];
                    let nluma = px[0] * 0.299 + px[1] * 0.587 + px[2] * 0.114;
                    if (nluma - center_luma).abs() > luma_thresh {
                        continue; // bord net (contour) : ne contribue pas
                    }
                    for c in 0..3 {
                        sum[c] += px[c];
                    }
                    weight += 1.0;
                }
            }
            if weight <= 0.0 {
                continue;
            }
            let p = i * 4;
            for c in 0..3 {
                let smoothed = sum[c] / weight;
                let blended = src[p + c] as f32 * (1.0 - strength) + smoothed * strength;
                rgba[p + c] = blended.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Flou de mouvement (Sprint 5.1) : moyenne `2*steps+1` échantillons le long
/// de la direction `angle_deg`, sur une longueur totale `distance` (px).
/// `distance <= 0` = identité (pas de traînée).
fn motion_blur(src: &[u8], w: usize, h: usize, angle_deg: f32, distance: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || distance <= 0.5 {
        return src.to_vec();
    }
    let (dx, dy) = (angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
    let steps = (distance.round() as i32).max(1);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 4];
            let mut count = 0.0f32;
            for s in -steps..=steps {
                let t = s as f32 * 0.5; // pas d'un demi-pixel pour un échantillonnage plus dense
                let sx = (x as f32 + dx * t).round() as i32;
                let sy = (y as f32 + dy * t).round() as i32;
                if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                    continue;
                }
                let i = (sy as usize * w + sx as usize) * 4;
                for c in 0..4 {
                    sum[c] += src[i + c] as f32;
                }
                count += 1.0;
            }
            let didx = (y * w + x) * 4;
            if count > 0.0 {
                for c in 0..4 {
                    out[didx + c] = (sum[c] / count).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Flou bokeh (Sprint 5.1) : moyenne dans un disque de rayon `radius` (px),
/// en accentuant le poids des pixels lumineux (`boost`, 0..=1) pour le look
/// « taches lumineuses floues » d'un vrai bokeh optique. `radius <= 0` =
/// identité.
fn bokeh_blur(src: &[u8], w: usize, h: usize, radius: f32, boost: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || radius <= 0.5 {
        return src.to_vec();
    }
    let r = radius.round() as i32;
    let boost = boost.clamp(0.0, 1.0);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 4];
            let mut weight = 0.0f32;
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx * dx + dy * dy) as f32 > radius * radius {
                        continue; // hors du disque
                    }
                    let (sx, sy) = (x as i32 + dx, y as i32 + dy);
                    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
                        continue;
                    }
                    let i = (sy as usize * w + sx as usize) * 4;
                    let l = luma(&src[i..i + 4]);
                    // Accentuation des hautes lumières : un pixel clair pèse
                    // jusqu'à 4× plus qu'un pixel sombre à pleine intensité.
                    let w_px = 1.0 + boost * 3.0 * (l / 255.0).powi(2);
                    for c in 0..4 {
                        sum[c] += src[i + c] as f32 * w_px;
                    }
                    weight += w_px;
                }
            }
            let didx = (y * w + x) * 4;
            if weight > 0.0 {
                for c in 0..4 {
                    out[didx + c] = (sum[c] / weight).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Histogramme RGB (Sprint 4.1) : compte des valeurs 0..=255 par canal,
/// alpha ignoré. Pur et testable ; l'affichage (barres normalisées) vit dans
/// `ui::toolbar`.
pub fn histogram_rgb(rgba: &[u8]) -> [[u32; 256]; 3] {
    let mut hist = [[0u32; 256]; 3];
    for px in rgba.chunks_exact(4) {
        for c in 0..3 {
            hist[c][px[c] as usize] += 1;
        }
    }
    hist
}

/// Rectangle minimal englobant les pixels d'alpha > 0 (Sprint G.3, « Rogner
/// les bords vides »). Renvoie `None` si l'image est entièrement transparente
/// (pas de recadrage à 0×0 dans ce cas).
pub fn trim_bounds(rgba: &[u8], w: usize, h: usize) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0usize; // exclusif
    let mut max_y = 0usize; // exclusif
    for y in 0..h {
        for x in 0..w {
            let a = rgba[(y * w + x) * 4 + 3];
            if a > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    Some((min_x as u32, min_y as u32, (max_x - min_x) as u32, (max_y - min_y) as u32))
}

/// Applique le filtre en place (ou renvoie un nouveau buffer pour les filtres à
/// voisinage : flou, netteté).
pub fn apply(filter: Filter, rgba: &mut Vec<u8>, w: u32, h: u32) {
    match filter {
        Filter::Brighter => brightness(rgba, 1.2),
        Filter::Darker => brightness(rgba, 0.83),
        Filter::Contrast => contrast(rgba, 1.25),
        Filter::Saturate => saturate(rgba, 1.4),
        Filter::Desaturate => saturate(rgba, 0.6),
        Filter::Grayscale => grayscale(rgba),
        Filter::Invert => invert(rgba),
        Filter::Sharpen => *rgba = sharpen(rgba, w as usize, h as usize),
        Filter::Blur => *rgba = box_blur(rgba, w as usize, h as usize, 2),
        Filter::FilmGrain => film_grain(rgba, w as usize, h as usize),
        Filter::Vintage => vintage(rgba, w as usize, h as usize),
        Filter::Sketch => *rgba = sketch(rgba, w as usize, h as usize),
        Filter::Comic => *rgba = comic(rgba, w as usize, h as usize),
        Filter::OilPainting => *rgba = oil_painting(rgba, w as usize, h as usize, 3),
        Filter::Watercolor => *rgba = watercolor(rgba, w as usize, h as usize),
        Filter::AutoLevels => auto_levels(rgba),
        Filter::Canny => *rgba = canny_filter(rgba, w as usize, h as usize),
    }
}

/// Auto-correction en un clic (Sprint K.8) : étire l'histogramme de chaque
/// canal RVB pour que son 1er/99e centile touchent 0/255 — réutilise
/// `histogram_rgb` déjà existant pour calculer les bornes.
fn auto_levels(rgba: &mut [u8]) {
    let hist = histogram_rgb(rgba);
    let total: u32 = hist[0].iter().sum();
    if total == 0 {
        return;
    }
    let percentile = |channel: &[u32; 256], p: f32| -> u8 {
        let target = ((total as f32 * p).round() as u32).max(1);
        let mut acc = 0u32;
        for (v, &count) in channel.iter().enumerate() {
            acc += count;
            if acc >= target {
                return v as u8;
            }
        }
        255
    };
    // Bornes par canal : (lo, hi), lo < hi sinon pas d'étirement sur ce canal.
    let bounds: [(u8, u8); 3] = std::array::from_fn(|c| {
        let lo = percentile(&hist[c], 0.01);
        let hi = percentile(&hist[c], 0.99);
        if lo < hi { (lo, hi) } else { (0, 255) }
    });
    for px in rgba.chunks_exact_mut(4) {
        for c in 0..3 {
            let (lo, hi) = bounds[c];
            let stretched = (px[c] as f32 - lo as f32) * 255.0 / (hi as f32 - lo as f32);
            px[c] = stretched.clamp(0.0, 255.0) as u8;
        }
    }
}

/// Luminance par pixel (0..255), pour la détection de contours.
fn luma_buffer(rgba: &[u8], w: usize, h: usize) -> Vec<f32> {
    (0..w * h).map(|i| luma(&rgba[i * 4..i * 4 + 4])).collect()
}

/// Magnitude de contour par Sobel sur une image en luminance (0..~1020,
/// non normalisée — l'appelant choisit son propre seuil/échelle).
fn sobel_magnitude(gray: &[f32], w: usize, h: usize) -> Vec<f32> {
    let at = |x: isize, y: isize| -> f32 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        gray[y * w + x]
    };
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let gx = -at(xi - 1, yi - 1) - 2.0 * at(xi - 1, yi) - at(xi - 1, yi + 1) + at(xi + 1, yi - 1)
                + 2.0 * at(xi + 1, yi)
                + at(xi + 1, yi + 1);
            let gy = -at(xi - 1, yi - 1) - 2.0 * at(xi, yi - 1) - at(xi + 1, yi - 1) + at(xi - 1, yi + 1)
                + 2.0 * at(xi, yi + 1)
                + at(xi + 1, yi + 1);
            out[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    out
}

/// Lissage boîte 3×3 sur un buffer de luminance — réduction de bruit avant
/// dérivation (étape 1 de Canny), plus simple qu'une vraie gaussienne mais
/// suffisant en pré-passe pour une détection de contours.
fn blur_gray_3x3(gray: &[f32], w: usize, h: usize) -> Vec<f32> {
    let at = |x: isize, y: isize| -> f32 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        gray[y * w + x]
    };
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let mut sum = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    sum += at(xi + dx, yi + dy);
                }
            }
            out[y * w + x] = sum / 9.0;
        }
    }
    out
}

/// Détection de contours de Canny (audit K.6) : lissage, gradients Sobel
/// (magnitude *et* direction, contrairement à `sobel_magnitude`), suppression
/// non maximale le long de la direction du gradient (garde seulement les
/// maxima locaux perpendiculaires au contour), puis double seuil +
/// hystérésis (un pixel « faible » n'est retenu que s'il est connecté à un
/// pixel « fort », 8-connexité) — élimine les faux positifs de bord épais
/// que produirait un simple seuil sur la magnitude Sobel.
fn canny_edges(gray: &[f32], w: usize, h: usize, low: f32, high: f32) -> Vec<bool> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let blurred = blur_gray_3x3(gray, w, h);
    let at = |g: &[f32], x: isize, y: isize| -> f32 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        g[y * w + x]
    };
    let mut mag = vec![0.0f32; w * h];
    let mut dir = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let gx = -at(&blurred, xi - 1, yi - 1) - 2.0 * at(&blurred, xi - 1, yi) - at(&blurred, xi - 1, yi + 1)
                + at(&blurred, xi + 1, yi - 1)
                + 2.0 * at(&blurred, xi + 1, yi)
                + at(&blurred, xi + 1, yi + 1);
            let gy = -at(&blurred, xi - 1, yi - 1) - 2.0 * at(&blurred, xi, yi - 1) - at(&blurred, xi + 1, yi - 1)
                + at(&blurred, xi - 1, yi + 1)
                + 2.0 * at(&blurred, xi, yi + 1)
                + at(&blurred, xi + 1, yi + 1);
            mag[y * w + x] = (gx * gx + gy * gy).sqrt();
            dir[y * w + x] = gy.atan2(gx);
        }
    }
    let mut nms = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let m = mag[y * w + x];
            if m <= 0.0 {
                continue;
            }
            // Direction arrondie à l'un des 4 axes (0°/45°/90°/135°) : compare
            // la magnitude aux deux voisins perpendiculaires au contour.
            let angle = dir[y * w + x].to_degrees().rem_euclid(180.0);
            let (dx1, dy1, dx2, dy2): (isize, isize, isize, isize) = if !(22.5..157.5).contains(&angle) {
                (1, 0, -1, 0)
            } else if angle < 67.5 {
                (1, -1, -1, 1)
            } else if angle < 112.5 {
                (0, 1, 0, -1)
            } else {
                (1, 1, -1, -1)
            };
            let (xi, yi) = (x as isize, y as isize);
            let m1 = at(&mag, xi + dx1, yi + dy1);
            let m2 = at(&mag, xi + dx2, yi + dy2);
            if m >= m1 && m >= m2 {
                nms[y * w + x] = m;
            }
        }
    }
    let mut strong = vec![false; w * h];
    let mut weak = vec![false; w * h];
    for i in 0..w * h {
        if nms[i] >= high {
            strong[i] = true;
        } else if nms[i] >= low {
            weak[i] = true;
        }
    }
    let mut edge = strong.clone();
    let mut stack: Vec<usize> = (0..w * h).filter(|&i| strong[i]).collect();
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                if weak[ni] && !edge[ni] {
                    edge[ni] = true;
                    stack.push(ni);
                }
            }
        }
    }
    edge
}

/// Filtre « Contours (Canny) » : traits noirs sur fond blanc, même
/// convention visuelle que Croquis (`sketch`) — seuils fixes calibrés sur
/// l'échelle de `sobel_magnitude` (0..~1020, non normalisée).
fn canny_filter(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let gray = luma_buffer(src, w, h);
    let edges = canny_edges(&gray, w, h, 200.0, 500.0);
    let mut out = src.to_vec();
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        let v = if edges[i] { 0u8 } else { 255u8 };
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
    out
}

/// Croquis (Sprint 5.4) : contours Sobel inversés en niveaux de gris — un
/// contour marqué devient un trait sombre, le reste tend vers le blanc.
fn sketch(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let gray = luma_buffer(src, w, h);
    let edges = sobel_magnitude(&gray, w, h);
    let mut out = src.to_vec();
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        let v = (255.0 - edges[i]).clamp(0.0, 255.0) as u8;
        px[0] = v;
        px[1] = v;
        px[2] = v;
    }
    out
}

/// Bande dessinée (Sprint 5.4) : posterize (5 niveaux/canal) + contours noirs
/// superposés là où la magnitude Sobel dépasse un seuil.
fn comic(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let gray = luma_buffer(src, w, h);
    let edges = sobel_magnitude(&gray, w, h);
    let levels = 5.0;
    let mut out = src.to_vec();
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        if edges[i] > 220.0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            continue;
        }
        for channel in px.iter_mut().take(3) {
            let v = (*channel as f32 / 255.0 * levels).round() / levels * 255.0;
            *channel = v.clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Peinture à l'huile (Sprint 5.4) : filtre de Kuwahara — partage le
/// voisinage de rayon `radius` en 4 quadrants chevauchant le pixel central,
/// retient la moyenne du quadrant à la variance la plus faible (zone la plus
/// homogène) → lisse les aplats tout en gardant les contours nets, façon
/// coup de pinceau.
fn oil_painting(src: &[u8], w: usize, h: usize, radius: i32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let at = |x: i32, y: i32, c: usize| -> f32 {
        let x = x.clamp(0, w as i32 - 1) as usize;
        let y = y.clamp(0, h as i32 - 1) as usize;
        src[(y * w + x) * 4 + c] as f32
    };
    let quadrants: [(i32, i32); 4] = [(-radius, -radius), (0, -radius), (-radius, 0), (0, 0)];
    let mut out = src.to_vec();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut best_var = f32::INFINITY;
            let mut best_mean = [0.0f32; 3];
            for (qx, qy) in quadrants {
                let mut sum = [0.0f32; 3];
                let mut sumsq = [0.0f32; 3];
                let mut n = 0.0f32;
                for dy in 0..=radius {
                    for dx in 0..=radius {
                        for c in 0..3 {
                            let v = at(x + qx + dx, y + qy + dy, c);
                            sum[c] += v;
                            sumsq[c] += v * v;
                        }
                        n += 1.0;
                    }
                }
                let mean = [sum[0] / n, sum[1] / n, sum[2] / n];
                let variance: f32 = (0..3).map(|c| (sumsq[c] / n - mean[c] * mean[c]).max(0.0)).sum();
                if variance < best_var {
                    best_var = variance;
                    best_mean = mean;
                }
            }
            let didx = (y as usize * w + x as usize) * 4;
            for c in 0..3 {
                out[didx + c] = best_mean[c].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Aquarelle (Sprint 5.4) : lissage bilatéral léger (préserve les contours,
/// réutilise le même principe que `smooth_skin`) + légère augmentation de
/// saturation + assombrissement des contours marqués (le pigment qui
/// s'accumule aux bords sur du vrai papier aquarelle).
fn watercolor(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let full_mask = vec![true; w * h];
    let mut out = src.to_vec();
    smooth_skin(&mut out, w, h, &full_mask, 0.85);
    saturate(&mut out, 1.15);
    let gray = luma_buffer(&out, w, h);
    let edges = sobel_magnitude(&gray, w, h);
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        if edges[i] > 80.0 {
            let darken = 1.0 - (edges[i] / 1400.0).min(0.35);
            for c in px.iter_mut().take(3) {
                *c = (*c as f32 * darken).clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Bruit pseudo-aléatoire déterministe par pixel (hash entier, pas de RNG à
/// état) — mêmes coordonnées → même bruit à chaque appel, reproductible.
fn pixel_noise(i: usize) -> f32 {
    let mut h = i as u64 ^ 0x9E3779B97F4A7C15;
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    // [0,1) → [-1,1)
    (h as f32 / u64::MAX as f32) * 2.0 - 1.0
}

/// Grain argentique (Sprint 5.2) : ajoute un bruit déterministe (± 18 niveaux
/// max) à chaque canal, atténué dans les tons clairs et sombres extrêmes
/// (comme un vrai grain de pellicule, plus visible dans les tons moyens).
fn film_grain(rgba: &mut [u8], w: usize, h: usize) {
    if w == 0 || h == 0 {
        return;
    }
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        let l = luma(px) / 255.0;
        let envelope = 1.0 - (l - 0.5).abs() * 2.0; // 1 au milieu, 0 aux extrêmes
        let n = pixel_noise(i) * 18.0 * envelope;
        for c in px.iter_mut().take(3) {
            *c = (*c as f32 + n).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Vintage (Sprint 5.2) : virage chaud (R +, B −), légère désaturation, et
/// vignettage (assombrissement progressif vers les coins).
fn vintage(rgba: &mut [u8], w: usize, h: usize) {
    if w == 0 || h == 0 {
        return;
    }
    saturate(rgba, 0.85);
    for px in rgba.chunks_exact_mut(4) {
        px[0] = (px[0] as f32 * 1.08).clamp(0.0, 255.0) as u8; // virage chaud : + rouge
        px[2] = (px[2] as f32 * 0.9).clamp(0.0, 255.0) as u8; // − bleu
    }
    apply_vignette(rgba, w, h, 0.5);
}

/// Vignette artistique indépendante (Sprint K.5) : assombrit les pixels
/// proportionnellement au carré de leur distance au centre (normalisée par la
/// demi-diagonale). `amount` 0.0..=1.0 pilote l'assombrissement maximal aux
/// coins ; extrait de l'ancienne formule fixe de `vintage()` (qui utilisait
/// `amount = 0.5`) pour en faire un réglage autonome. `amount <= 0` = identité.
fn apply_vignette(rgba: &mut [u8], w: usize, h: usize, amount: f32) {
    if w == 0 || h == 0 || amount <= 1e-4 {
        return;
    }
    let amount = amount.min(1.0);
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let max_dist = (cx * cx + cy * cy).sqrt().max(1.0);
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i % w, i / w);
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
        let dist = (dx * dx + dy * dy).sqrt() / max_dist;
        let vignette = 1.0 - (dist * dist * amount).min(amount);
        for c in px.iter_mut().take(3) {
            *c = (*c as f32 * vignette).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Luminance perçue (Rec. 601) d'un pixel.
fn luma(px: &[u8]) -> f32 {
    px[0] as f32 * 0.299 + px[1] as f32 * 0.587 + px[2] as f32 * 0.114
}

/// Contraste autour du point pivot 128 : `out = (in - 128) * factor + 128`.
fn contrast(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = ((*c as f32 - 128.0) * factor + 128.0).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Saturation : interpole chaque canal entre le gris (luma) et sa valeur.
/// `factor` > 1 sature, < 1 désature, 0 = noir & blanc.
fn saturate(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        let g = luma(px);
        for c in px.iter_mut().take(3) {
            *c = (g + (*c as f32 - g) * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Négatif : inverse les canaux RVB (alpha conservé).
fn invert(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = 255 - *c;
        }
    }
}

/// Renforcement de netteté par noyau 3×3 (somme = 1) :
/// centre 5, voisins cardinaux −1. Bords répétés (clamp).
fn sharpen(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let mut out = src.to_vec();
    let at = |x: usize, y: usize, c: usize| src[(y * w + x) * 4 + c];
    for y in 0..h {
        for x in 0..w {
            // L'alpha n'est pas affecté (canal 3 copié tel quel).
            for c in 0..3 {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(w - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(h - 1);
                let v = 5.0 * at(x, y, c) as f32
                    - at(xm, y, c) as f32
                    - at(xp, y, c) as f32
                    - at(x, ym, c) as f32
                    - at(x, yp, c) as f32;
                out[(y * w + x) * 4 + c] = v.clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Netteté réglable (audit_100_features.md #68) : Unsharp Mask classique —
/// `sharpened = src + amount * (src - blurred)`, `blurred` un flou gaussien
/// de rayon `radius` (référence de basses fréquences à retirer), appliqué
/// seulement où l'écart dépasse `threshold` (0..=255, en niveaux par canal)
/// pour épargner les zones déjà lisses/le bruit résiduel. `amount` <= 0 ou
/// `radius` <= 0 = identité.
fn unsharp_mask(src: &[u8], w: usize, h: usize, radius: f32, amount: f32, threshold: f32) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 || radius <= 0.0 || amount <= 0.0 {
        return src.to_vec();
    }
    let blurred = gaussian_blur(src, w, h, radius);
    let mut out = src.to_vec();
    for i in (0..out.len()).step_by(4) {
        for c in 0..3 {
            let s = src[i + c] as f32;
            let b = blurred[i + c] as f32;
            let diff = s - b;
            if diff.abs() < threshold {
                continue;
            }
            out[i + c] = (s + amount * diff).clamp(0.0, 255.0) as u8;
        }
        // Alpha (canal 3) non affecté.
    }
    out
}

fn brightness(rgba: &mut [u8], factor: f32) {
    for px in rgba.chunks_exact_mut(4) {
        for c in px.iter_mut().take(3) {
            *c = (*c as f32 * factor).clamp(0.0, 255.0) as u8;
        }
    }
}

fn grayscale(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let g = luma(px) as u8;
        px[0] = g;
        px[1] = g;
        px[2] = g;
    }
}

/// Flou « boîte » séparable (2 passes), tous canaux.
fn box_blur(src: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if w == 0 || h == 0 || src.len() < w * h * 4 {
        return src.to_vec();
    }
    let tmp = blur_pass(src, w, h, r, true);
    blur_pass(&tmp, w, h, r, false)
}

fn blur_pass(src: &[u8], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    for o in 0..outer {
        for ch in 0..4 {
            let mut sum = 0u32;
            let mut count = 0u32;
            let idx = |i: usize| {
                let (x, y) = if horizontal { (i, o) } else { (o, i) };
                (y * w + x) * 4 + ch
            };
            // Fenêtre initiale.
            for i in 0..=r.min(inner - 1) {
                sum += src[idx(i)] as u32;
                count += 1;
            }
            for i in 0..inner {
                out[idx(i)] = (sum / count.max(1)) as u8;
                // Avance la fenêtre [i-r, i+r].
                if i >= r {
                    sum -= src[idx(i - r)] as u32;
                    count -= 1;
                }
                let add = i + r + 1;
                if add < inner {
                    sum += src[idx(add)] as u32;
                    count += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_bounds_finds_the_opaque_rect_inside_a_transparent_border() {
        // Image 5x5, entièrement transparente sauf un carré 2x2 aux coords
        // (1,2)..(3,4) (bornes exclusives).
        let w = 5usize;
        let h = 5usize;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 2..4 {
            for x in 1..3 {
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        assert_eq!(trim_bounds(&rgba, w, h), Some((1, 2, 2, 2)));
    }

    #[test]
    fn trim_bounds_is_none_for_a_fully_transparent_image() {
        let rgba = vec![0u8; 4 * 4 * 4];
        assert_eq!(trim_bounds(&rgba, 4, 4), None);
    }

    #[test]
    fn auto_levels_stretches_a_low_contrast_image_to_full_range() {
        // 100 pixels compris entre 100 et 150 seulement (faible contraste) :
        // après auto-correction, le min doit descendre près de 0 et le max
        // remonter près de 255 (percentiles 1/99, pas les bornes exactes).
        let mut rgba = Vec::new();
        for v in 100u8..150u8 {
            rgba.extend_from_slice(&[v, v, v, 255]);
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
        apply(Filter::AutoLevels, &mut rgba, 10, 10);
        let min = rgba.iter().step_by(4).min().copied().unwrap();
        let max = rgba.iter().step_by(4).max().copied().unwrap();
        assert!(min < 20, "min should be stretched near 0, got {min}");
        assert!(max > 235, "max should be stretched near 255, got {max}");
    }

    #[test]
    fn auto_levels_is_a_noop_on_an_already_full_range_flat_image() {
        let mut rgba = vec![128u8, 128, 128, 255, 128, 128, 128, 255];
        apply(Filter::AutoLevels, &mut rgba, 2, 1);
        assert_eq!(rgba, vec![128u8, 128, 128, 255, 128, 128, 128, 255]);
    }

    #[test]
    fn canny_marks_a_sharp_vertical_edge_and_nothing_in_flat_regions() {
        // Damier 20x20 : moitié gauche noire, moitié droite blanche — un seul
        // bord net vertical à x=10, tout le reste parfaitement plat.
        let (w, h) = (20usize, 20usize);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < 10 { 0u8 } else { 255u8 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let out = canny_filter(&rgba, w, h);
        // Loin du bord (à ±4 colonnes), aucun contour détecté (fond blanc partout).
        for y in 0..h {
            for x in [0usize, 1, 2, 18, 19] {
                let i = (y * w + x) * 4;
                assert_eq!(out[i], 255, "pas de contour loin du bord (x={x})");
            }
        }
        // Autour de x=10, au moins un pixel de contour (noir) par ligne.
        for y in 2..h - 2 {
            let has_edge = (7..13).any(|x| out[(y * w + x) * 4] == 0);
            assert!(has_edge, "contour attendu près du bord vertical (y={y})");
        }
    }

    #[test]
    fn canny_is_all_white_on_a_perfectly_flat_image() {
        let (w, h) = (8usize, 8usize);
        let rgba = [200u8, 200, 200, 255].repeat(w * h);
        let out = canny_filter(&rgba, w, h);
        assert!(out.chunks_exact(4).all(|px| px[0] == 255), "aucun contour sur une image plate");
    }

    #[test]
    fn pixelate_makes_every_pixel_in_a_block_identical() {
        // Damier fin 4x4 (alternance noir/blanc) : un bloc de taille 4
        // moyenne tout le damier en un seul gris uniforme.
        let w = 4usize;
        let h = 4usize;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        pixelate(&mut rgba, w, h, 4.0);
        let first = rgba[0];
        for px in rgba.chunks_exact(4) {
            assert_eq!(px[0], first);
        }
    }

    #[test]
    fn pixelate_is_a_noop_for_a_block_of_one_pixel() {
        let original = vec![10u8, 20, 30, 255, 200, 210, 220, 255];
        let mut rgba = original.clone();
        pixelate(&mut rgba, 2, 1, 1.0);
        assert_eq!(rgba, original);
    }

    #[test]
    fn halftone_fully_black_cell_covers_the_whole_cell() {
        let w = 8usize;
        let h = 8usize;
        let rgba = vec![0u8; w * h * 4]; // tout noir, alpha 255... alpha=0 ici mais luma indépendante de l'alpha
        let out = halftone(&rgba, w, h, 8.0, 0.0);
        assert!(out.chunks_exact(4).all(|px| px[0] == 0), "cellule noire → disque devrait couvrir toute la cellule");
    }

    #[test]
    fn halftone_fully_white_cell_stays_white() {
        let w = 8usize;
        let h = 8usize;
        let rgba = vec![255u8; w * h * 4];
        let out = halftone(&rgba, w, h, 8.0, 0.0);
        assert!(out.chunks_exact(4).all(|px| px[0] == 255), "cellule blanche → rayon nul, aucun disque");
    }

    #[test]
    fn wave_sphere_vortex_are_noop_at_neutral_parameters() {
        let original = vec![10u8, 20, 30, 255, 200, 210, 220, 255, 5, 5, 5, 255, 250, 100, 50, 255];
        assert_eq!(wave_warp(&original, 2, 2, 0.0, 40.0), original);
        assert_eq!(sphere_warp(&original, 2, 2, 0.0), original);
        assert_eq!(vortex_warp(&original, 2, 2, 0.0), original);
    }

    #[test]
    fn wave_shifts_a_column_vertically() {
        // Colonne x=2 : à un quart de la longueur d'onde (wavelength=8),
        // sin(2*pi*2/8) = sin(pi/2) = 1 → décalage vertical maximal, visible
        // sur un dégradé vertical.
        let w = 8usize;
        let h = 8usize;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                rgba[i] = (y * 30) as u8;
                rgba[i + 3] = 255;
            }
        }
        let out = wave_warp(&rgba, w, h, 2.0, 8.0);
        let col = |buf: &[u8], x: usize| -> Vec<u8> { (0..h).map(|y| buf[(y * w + x) * 4]).collect() };
        assert_ne!(col(&out, 2), col(&rgba, 2));
    }

    #[test]
    fn radial_blur_is_noop_at_zero_amount() {
        let original = vec![10u8, 20, 30, 255, 200, 210, 220, 255, 5, 5, 5, 255, 250, 100, 50, 255];
        assert_eq!(radial_blur(&original, 2, 2, 0.0), original);
    }

    #[test]
    fn radial_blur_blends_a_bright_pixel_into_its_neighbors() {
        // Pixel lumineux à l'écart du centre exact (l'échantillonnage radial
        // n'a aucun effet pile au centre, où tous les rayons convergent) :
        // le flou doit l'assombrir en moyennant avec les échantillons plus
        // proches du centre, restés noirs.
        let w = 5usize;
        let h = 5usize;
        let mut rgba = vec![0u8; w * h * 4];
        let idx = (w + 1) * 4;
        rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
        let out = radial_blur(&rgba, w, h, 1.0);
        assert!(out[idx] < 255, "le pixel devrait s'assombrir après flou radial");
    }

    #[test]
    fn apply_vignette_darkens_corners_more_than_center() {
        let w = 9usize;
        let h = 9usize;
        let mut rgba = vec![200u8; w * h * 4];
        apply_vignette(&mut rgba, w, h, 0.8);
        let center = (4 * w + 4) * 4;
        let corner = 0usize;
        assert!(rgba[corner] < rgba[center], "les coins doivent être plus sombres que le centre");
    }

    #[test]
    fn apply_vignette_is_noop_at_zero_amount() {
        let original = vec![200u8; 4 * 4 * 4];
        let mut rgba = original.clone();
        apply_vignette(&mut rgba, 4, 4, 0.0);
        assert_eq!(rgba, original);
    }

    #[test]
    fn channel_mixer_bw_matches_grayscale_weights_at_default() {
        let mut px = vec![200u8, 100, 50, 255];
        channel_mixer_bw(&mut px, 0.299, 0.587, 0.114);
        let expected = (200.0f32 * 0.299 + 100.0 * 0.587 + 50.0 * 0.114).round() as u8;
        assert_eq!(px[0], expected);
        assert_eq!(px[1], expected);
        assert_eq!(px[2], expected);
    }

    #[test]
    fn grayscale_equalizes_channels() {
        let mut px = vec![200, 100, 50, 255];
        apply(Filter::Grayscale, &mut px, 1, 1);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
    }

    #[test]
    fn brighter_increases_values() {
        let mut px = vec![100, 100, 100, 255];
        apply(Filter::Brighter, &mut px, 1, 1);
        assert!(px[0] > 100);
    }

    #[test]
    fn invert_is_involutive() {
        let original = vec![10, 120, 240, 255];
        let mut px = original.clone();
        apply(Filter::Invert, &mut px, 1, 1);
        assert_eq!(px, vec![245, 135, 15, 255]); // alpha conservé
        apply(Filter::Invert, &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn contrast_pushes_away_from_mid() {
        // Au-dessus du pivot 128 → plus clair ; en dessous → plus sombre.
        let mut hi = vec![200, 200, 200, 255];
        apply(Filter::Contrast, &mut hi, 1, 1);
        assert!(hi[0] > 200);
        let mut lo = vec![50, 50, 50, 255];
        apply(Filter::Contrast, &mut lo, 1, 1);
        assert!(lo[0] < 50);
    }

    #[test]
    fn desaturate_pulls_channels_together() {
        let mut px = vec![200, 100, 50, 255];
        let spread0 = (px[0] as i32 - px[2] as i32).abs();
        apply(Filter::Desaturate, &mut px, 1, 1);
        let spread1 = (px[0] as i32 - px[2] as i32).abs();
        assert!(spread1 < spread0);
    }

    #[test]
    fn sharpen_preserves_flat_region() {
        // Image unie : la netteté ne doit rien changer (somme du noyau = 1).
        let mut px = vec![120u8; 3 * 3 * 4];
        apply(Filter::Sharpen, &mut px, 3, 3);
        assert!(px.iter().all(|&v| v == 120));
    }

    #[test]
    fn unsharp_mask_is_noop_on_a_flat_region() {
        // Comme sharpen_preserves_flat_region : pas de hautes fréquences à
        // amplifier sur une image unie, quelle que soit la quantité.
        let mut px = vec![120u8; 5 * 5 * 4];
        apply_adjustment(Adjustment::UnsharpMask { radius: 2.0, amount: 2.0, threshold: 0.0 }, &mut px, 5, 5);
        assert!(px.iter().all(|&v| v == 120), "aucune texture à renforcer sur du plat");
    }

    #[test]
    fn unsharp_mask_zero_amount_is_identity() {
        let original: Vec<u8> = (0..(4 * 4 * 4)).map(|i| (i * 7 % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::UnsharpMask { radius: 2.0, amount: 0.0, threshold: 0.0 }, &mut px, 4, 4);
        assert_eq!(px, original);
    }

    #[test]
    fn unsharp_mask_threshold_spares_small_differences() {
        // Damier faible contraste (118/122, écart de 4) : un seuil de 10
        // doit épargner ce détail, pas l'amplifier.
        let mut px = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x + y) % 2 == 0 { 118 } else { 122 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let original = px.clone();
        apply_adjustment(Adjustment::UnsharpMask { radius: 1.0, amount: 2.0, threshold: 10.0 }, &mut px, 4, 4);
        assert_eq!(px, original, "un écart sous le seuil ne doit pas être renforcé");
    }

    #[test]
    fn unsharp_mask_sharpens_a_hard_edge() {
        // Bord net 2 blocs : un pixel juste après la transition doit
        // s'éloigner davantage de la valeur voisine (contraste local accru).
        let (w, h) = (8usize, 1usize);
        let mut px = vec![0u8; w * h * 4];
        for x in 0..w {
            let v: u8 = if x < w / 2 { 50 } else { 200 };
            px[x * 4] = v;
            px[x * 4 + 1] = v;
            px[x * 4 + 2] = v;
            px[x * 4 + 3] = 255;
        }
        let mut sharpened = px.clone();
        apply_adjustment(Adjustment::UnsharpMask { radius: 1.5, amount: 1.5, threshold: 0.0 }, &mut sharpened, w as u32, h as u32);
        let edge_x = w / 2;
        assert!(
            sharpened[edge_x * 4] > px[edge_x * 4],
            "le côté clair du bord doit devenir plus clair (dépassement caractéristique de l'Unsharp Mask)"
        );
        assert!(
            sharpened[(edge_x - 1) * 4] < px[(edge_x - 1) * 4],
            "le côté sombre du bord doit devenir plus sombre"
        );
    }

    #[test]
    fn levels_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_levels(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn levels_raises_black_point() {
        let mut px = vec![10u8, 10, 10, 255];
        apply_adjustment(Adjustment::Levels { black: 20, white: 255, gamma: 1.0 }, &mut px, 1, 1);
        // En dessous du point noir : écrêté à 0.
        assert_eq!(px[0], 0);
    }

    /// Sprint S (point 73) : deux points (0,0)-(255,255) = LUT identité.
    #[test]
    fn points_lut_two_endpoint_points_is_identity() {
        let lut = points_lut(&[(0, 0), (255, 255)]);
        for (i, &v) in lut.iter().enumerate() {
            assert_eq!(v as usize, i);
        }
    }

    /// La spline de Fritsch-Carlson est monotone : des sorties croissantes
    /// donnent une LUT jamais décroissante (pas d'oscillation entre points).
    #[test]
    fn points_lut_is_monotone_for_increasing_points() {
        let lut = points_lut(&[(0, 0), (64, 200), (128, 210), (255, 255)]);
        for w in lut.windows(2) {
            assert!(w[1] >= w[0], "LUT non monotone : {} puis {}", w[0], w[1]);
        }
        assert_eq!(lut[64], 200);
        assert_eq!(lut[255], 255);
    }

    /// Une courbe sur le canal rouge seul ne touche ni le vert ni le bleu.
    #[test]
    fn curves_free_red_channel_only_affects_red() {
        let id = vec![(0u8, 0u8), (255u8, 255u8)];
        let boost = vec![(0u8, 80u8), (255u8, 255u8)];
        let mut px = vec![10u8, 10, 10, 255];
        apply_adjustment(
            Adjustment::CurvesFree { master: id.clone(), r: boost, g: id.clone(), b: id },
            &mut px,
            1,
            1,
        );
        assert!(px[0] > 10, "rouge relevé");
        assert_eq!(px[1], 10);
        assert_eq!(px[2], 10);
        assert_eq!(px[3], 255);
    }

    #[test]
    fn curves_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_curves(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn curves_lifted_shadows_brighten_dark_pixels() {
        let mut px = vec![10u8, 10, 10, 255];
        apply_adjustment(Adjustment::Curves { shadow: 40, mid: 128, highlight: 255 }, &mut px, 1, 1);
        assert!(px[0] > 10);
    }

    #[test]
    fn hue_saturation_identity_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_hue_saturation(), &mut px, 1, 1);
        // Tolérance : l'aller-retour RVB→HSL→RVB peut arrondir de ±1.
        for c in 0..3 {
            assert!((px[c] as i32 - original[c] as i32).abs() <= 1, "channel {c}: {} vs {}", px[c], original[c]);
        }
    }

    #[test]
    fn histogram_counts_pixels_per_channel() {
        let rgba = vec![10, 20, 30, 255, 10, 200, 30, 255];
        let hist = histogram_rgb(&rgba);
        assert_eq!(hist[0][10], 2); // les deux pixels partagent R=10
        assert_eq!(hist[1][20], 1);
        assert_eq!(hist[1][200], 1);
        assert_eq!(hist[2][30], 2);
        assert_eq!(hist[0].iter().sum::<u32>(), 2); // total = nombre de pixels
    }

    #[test]
    fn distortion_identity_at_zero_amount_is_noop() {
        let original: Vec<u8> = (0..(6 * 6 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_distortion(), &mut px, 6, 6);
        assert_eq!(px, original);
    }

    #[test]
    fn distortion_moves_the_center_pixel_the_least() {
        // Le centre est le point fixe de la distorsion radiale : quel que
        // soit `amount`, le pixel central doit rester (quasi) inchangé,
        // contrairement aux coins qui doivent bouger.
        let w = 20usize;
        let mut original = vec![0u8; w * w * 4];
        for (i, px) in original.chunks_exact_mut(4).enumerate() {
            let v = (i % 256) as u8;
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let mut distorted = original.clone();
        apply_adjustment(Adjustment::Distortion { amount: 0.8 }, &mut distorted, w as u32, w as u32);
        let center = (w / 2) * w + (w / 2);
        assert_eq!(distorted[center * 4], original[center * 4]);
        let corner = 0;
        assert_ne!(distorted[corner * 4..corner * 4 + 4], original[corner * 4..corner * 4 + 4]);
    }

    #[test]
    fn chromatic_aberration_identity_at_zero_amount_is_noop() {
        let original: Vec<u8> = (0..(6 * 6 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_chromatic_aberration(), &mut px, 6, 6);
        assert_eq!(px, original);
    }

    #[test]
    fn chromatic_aberration_shifts_red_and_blue_but_not_green() {
        // Grande image + damier pixel par pixel (fréquence spatiale maximale) :
        // même un décalage sous-pixellique fait forcément différer R/B quelque
        // part, alors qu'un damier à blocs larges pourrait rater un décalage
        // trop petit pour franchir une frontière de bloc.
        let w = 300usize;
        let mut original = vec![0u8; w * w * 4];
        for y in 0..w {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                let i = (y * w + x) * 4;
                original[i..i + 4].copy_from_slice(&[v, 128, v, 255]);
            }
        }
        let mut aberrated = original.clone();
        apply_adjustment(Adjustment::ChromaticAberration { amount: 1.0 }, &mut aberrated, w as u32, w as u32);
        // G ne bouge jamais.
        for (o, a) in original.chunks_exact(4).zip(aberrated.chunks_exact(4)) {
            assert_eq!(o[1], a[1]);
        }
        // R et/ou B doivent différer quelque part (décalage visible en périphérie).
        let r_changed = original.chunks_exact(4).zip(aberrated.chunks_exact(4)).any(|(o, a)| o[0] != a[0]);
        let b_changed = original.chunks_exact(4).zip(aberrated.chunks_exact(4)).any(|(o, a)| o[2] != a[2]);
        assert!(r_changed || b_changed);
    }

    #[test]
    fn reduce_red_eye_neutralizes_dominant_red_within_mask() {
        let mut rgba = vec![200u8, 20, 20, 255, 200, 20, 20, 255];
        reduce_red_eye(&mut rgba, 2, 1, &[true, false]);
        // Masqué : R tiré vers la moyenne G/B (20).
        assert!((rgba[0] as i32 - 20).abs() <= 1);
        // Hors masque : inchangé, même rouge dominant.
        assert_eq!(rgba[4], 200);
    }

    #[test]
    fn reduce_red_eye_leaves_non_red_pixels_alone() {
        // Gris clair (reflet du flash) : pas de rougeur dominante, inchangé.
        let mut rgba = vec![220u8, 210, 200, 255];
        let original = rgba.clone();
        reduce_red_eye(&mut rgba, 1, 1, &[true]);
        assert_eq!(rgba, original);
    }

    #[test]
    fn smooth_skin_blends_a_noisy_patch_toward_uniform() {
        // Damier de bruit fin (contraste local élevé mais luminance proche
        // pixel à pixel) : le lissage doit réduire l'écart-type sans effacer
        // la moyenne globale.
        let w = 12;
        let h = 12;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 140 } else { 120 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let original = rgba.clone();
        let mask = vec![true; w * h];
        smooth_skin(&mut rgba, w, h, &mask, 1.0);
        let variance = |data: &[u8]| -> f32 {
            let vals: Vec<f32> = data.chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = vals.iter().sum::<f32>() / vals.len() as f32;
            vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32
        };
        assert!(variance(&rgba) < variance(&original), "le lissage doit réduire la variance locale");
    }

    #[test]
    fn smooth_skin_preserves_a_hard_edge() {
        // Bord net (moitié noire/moitié blanche) : le seuil de luminance doit
        // empêcher le mélange à travers la frontière.
        let w = 10;
        let h = 4;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let mask = vec![true; w * h];
        smooth_skin(&mut rgba, w, h, &mask, 1.0);
        // Un pixel loin de la frontière de chaque côté doit rester proche de
        // sa valeur d'origine (pas de fuite à travers le contour).
        let left_i = (2 * w + 1) * 4;
        let right_i = (2 * w + (w - 2)) * 4;
        assert!(rgba[left_i] < 40, "attendu sombre à gauche, eu {}", rgba[left_i]);
        assert!(rgba[right_i] > 215, "attendu clair à droite, eu {}", rgba[right_i]);
    }

    #[test]
    fn motion_blur_identity_at_zero_distance_is_noop() {
        let original: Vec<u8> = (0..(8 * 8 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_motion_blur(), &mut px, 8, 8);
        assert_eq!(px, original);
    }

    #[test]
    fn motion_blur_smooths_a_single_bright_pixel_along_its_axis() {
        // Un seul pixel blanc sur fond noir : le flou horizontal doit
        // étaler sa luminosité sur ses voisins horizontaux.
        let w = 21;
        let h = 5;
        let mut rgba = vec![0u8; w * h * 4];
        let cy = h / 2;
        let cx = w / 2;
        let ci = (cy * w + cx) * 4;
        rgba[ci..ci + 4].copy_from_slice(&[255, 255, 255, 255]);
        apply_adjustment(Adjustment::MotionBlur { angle: 0.0, distance: 10.0 }, &mut rgba, w as u32, h as u32);
        let neighbor_i = (cy * w + cx + 2) * 4;
        assert!(rgba[neighbor_i] > 0, "le voisin horizontal doit recevoir de la lumière étalée");
    }

    #[test]
    fn bokeh_identity_at_zero_radius_is_noop() {
        let original: Vec<u8> = (0..(8 * 8 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_bokeh(), &mut px, 8, 8);
        assert_eq!(px, original);
    }

    #[test]
    fn bokeh_boosts_bright_pixels_more_than_dark_ones() {
        // Un pixel blanc isolé au centre d'un fond gris moyen : avec le
        // boost des hautes lumières, le centre doit rester nettement plus
        // clair que le même flou sans boost.
        let w = 15;
        let h = 15;
        let mut base = vec![0u8; w * h * 4];
        for px in base.chunks_exact_mut(4) {
            px.copy_from_slice(&[100, 100, 100, 255]);
        }
        let center_i = ((h / 2) * w + w / 2) * 4;
        base[center_i..center_i + 4].copy_from_slice(&[255, 255, 255, 255]);

        let mut no_boost = base.clone();
        apply_adjustment(Adjustment::Bokeh { radius: 6.0, boost: 0.0 }, &mut no_boost, w as u32, h as u32);
        let mut with_boost = base.clone();
        apply_adjustment(Adjustment::Bokeh { radius: 6.0, boost: 1.0 }, &mut with_boost, w as u32, h as u32);

        // Un voisin proche doit recevoir plus de lumière avec le boost.
        let near_i = ((h / 2) * w + w / 2 + 2) * 4;
        assert!(
            with_boost[near_i] > no_boost[near_i],
            "boost {} devrait dépasser sans-boost {}",
            with_boost[near_i],
            no_boost[near_i]
        );
    }

    #[test]
    fn film_grain_perturbs_but_stays_close_to_original() {
        let w = 10;
        let h = 10;
        let mut rgba = vec![128u8; w * h * 4];
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let original = rgba.clone();
        apply(Filter::FilmGrain, &mut rgba, w as u32, h as u32);
        assert_ne!(rgba, original, "le grain doit changer l'image");
        for (o, n) in original.chunks_exact(4).zip(rgba.chunks_exact(4)) {
            for c in 0..3 {
                assert!((o[c] as i32 - n[c] as i32).abs() <= 20, "le grain ne doit pas s'écarter trop du pixel d'origine");
            }
            assert_eq!(o[3], n[3], "alpha inchangé");
        }
    }

    #[test]
    fn film_grain_is_deterministic() {
        let w = 6;
        let h = 6;
        let base = vec![100u8; w * h * 4];
        let mut a = base.clone();
        let mut b = base.clone();
        apply(Filter::FilmGrain, &mut a, w as u32, h as u32);
        apply(Filter::FilmGrain, &mut b, w as u32, h as u32);
        assert_eq!(a, b, "même image en entrée → même grain (reproductible)");
    }

    #[test]
    fn vintage_darkens_corners_more_than_center() {
        let w = 20;
        let h = 20;
        let mut rgba = vec![0u8; w * h * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[150, 150, 150, 255]);
        }
        apply(Filter::Vintage, &mut rgba, w as u32, h as u32);
        let center_i = ((h / 2) * w + w / 2) * 4;
        let corner_i = 0;
        assert!(rgba[corner_i + 1] < rgba[center_i + 1], "le vignettage doit assombrir les coins plus que le centre");
    }

    #[test]
    fn duotone_maps_black_and_white_to_shadow_and_highlight() {
        let shadow = [10u8, 20, 200];
        let highlight = [250u8, 240, 30];
        let mut rgba = vec![0u8, 0, 0, 255, 255, 255, 255, 255];
        apply_adjustment(Adjustment::Duotone { shadow, highlight }, &mut rgba, 2, 1);
        assert_eq!(&rgba[0..3], &shadow[..]);
        assert_eq!(&rgba[4..7], &highlight[..]);
    }

    #[test]
    fn sketch_darkens_a_sharp_edge_and_lightens_flat_regions() {
        let w = 10;
        let h = 10;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 20 } else { 220 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        apply(Filter::Sketch, &mut rgba, w as u32, h as u32);
        let flat_i = (5 * w + 1) * 4; // loin du bord, zone plate
        let edge_i = (5 * w + w / 2) * 4; // sur le contour
        assert!(rgba[flat_i] > 200, "zone plate attendue claire, eu {}", rgba[flat_i]);
        assert!(rgba[edge_i] < rgba[flat_i], "le contour doit être plus sombre que la zone plate");
    }

    #[test]
    fn comic_posterizes_flat_areas_to_a_small_palette() {
        let w = 8;
        let h = 8;
        let mut rgba = vec![0u8; w * h * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[130, 60, 200, 255]);
        }
        apply(Filter::Comic, &mut rgba, w as u32, h as u32);
        // Une image plate (pas de contour) doit voir toutes ses valeurs de
        // canal alignées sur l'un des 6 niveaux de la posterization
        // (k/5×255 pour k=0..=5), à l'arrondi près.
        let levels = [0.0, 51.0, 102.0, 153.0, 204.0, 255.0];
        for (c, &channel) in rgba.iter().enumerate().take(3) {
            let v = channel as f32;
            assert!(levels.iter().any(|l| (v - l).abs() < 2.0), "canal {c} = {v} pas proche d'un niveau");
        }
    }

    #[test]
    fn oil_painting_smooths_fine_noise() {
        let w = 12;
        let h = 12;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 140 } else { 120 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let original = rgba.clone();
        apply(Filter::OilPainting, &mut rgba, w as u32, h as u32);
        let variance = |data: &[u8]| -> f32 {
            let vals: Vec<f32> = data.chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = vals.iter().sum::<f32>() / vals.len() as f32;
            vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32
        };
        assert!(variance(&rgba) < variance(&original));
    }

    #[test]
    fn oil_painting_preserves_a_hard_edge() {
        let w = 14;
        let h = 6;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0 } else { 255 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        apply(Filter::OilPainting, &mut rgba, w as u32, h as u32);
        let left_i = (3 * w + 1) * 4;
        let right_i = (3 * w + (w - 2)) * 4;
        assert!(rgba[left_i] < 40, "attendu sombre à gauche, eu {}", rgba[left_i]);
        assert!(rgba[right_i] > 215, "attendu clair à droite, eu {}", rgba[right_i]);
    }

    #[test]
    fn watercolor_changes_the_image_without_crashing() {
        let w = 16;
        let h = 16;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if (x / 3 + y / 3) % 2 == 0 { 200 } else { 60 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v / 2, v, 255]);
            }
        }
        let original = rgba.clone();
        apply(Filter::Watercolor, &mut rgba, w as u32, h as u32);
        assert_ne!(rgba, original);
        assert_eq!(rgba.len(), original.len());
    }

    #[test]
    fn arc_warp_identity_at_zero_amount_is_noop() {
        let original: Vec<u8> = (0..(10 * 10 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::ArcWarp { amount: 0.0 }, &mut px, 10, 10);
        assert_eq!(px, original);
    }

    #[test]
    fn arc_warp_keeps_edges_fixed_and_shifts_the_middle() {
        let w = 20;
        let h = 20;
        let mut rgba = vec![0u8; w * h * 4];
        // Rangée blanche unique à y=10, reste transparent/noir.
        for x in 0..w {
            let i = (10 * w + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        apply_adjustment(Adjustment::ArcWarp { amount: 0.5 }, &mut rgba, w as u32, h as u32);
        // Au bord (x=0), sin(0)=0 : la ligne blanche doit rester à y=10.
        let edge_i = (10 * w) * 4;
        assert_eq!(rgba[edge_i], 255);
        // Au milieu (x=w/2), le décalage est maximal : la ligne blanche a bougé.
        let mid_before_i = (10 * w + w / 2) * 4;
        assert_ne!(rgba[mid_before_i], 255, "le milieu doit avoir bougé, plus à y=10");
    }

    #[test]
    fn hue_saturation_zero_saturation_grayscales() {
        let mut px = vec![200u8, 100, 50, 255];
        apply_adjustment(Adjustment::HueSaturation { hue: 0.0, sat: -1.0, light: 0.0 }, &mut px, 1, 1);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
    }

    #[test]
    fn exposure_identity_at_zero_ev_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_exposure(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn exposure_one_stop_doubles_values_before_clamp() {
        let mut px = vec![50u8, 50, 50, 255];
        apply_adjustment(Adjustment::Exposure { ev: 1.0 }, &mut px, 1, 1);
        assert_eq!(px[0], 100);
    }

    #[test]
    fn vibrance_identity_at_zero_is_noop() {
        let original = vec![200u8, 100, 50, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_vibrance(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn vibrance_boosts_dull_colors_more_than_saturated_ones() {
        // Couleur déjà très saturée : la vibrance doit peu la changer.
        let mut saturated = vec![255u8, 0, 0, 255];
        apply_adjustment(Adjustment::Vibrance { amount: 1.0 }, &mut saturated, 1, 1);
        assert_eq!(saturated, vec![255, 0, 0, 255]);
        // Couleur terne (proche du gris) : la vibrance doit visiblement l'écarter.
        let mut dull = vec![140u8, 120, 130, 255];
        let before = dull.clone();
        apply_adjustment(Adjustment::Vibrance { amount: 1.0 }, &mut dull, 1, 1);
        assert_ne!(dull, before);
    }

    #[test]
    fn white_balance_identity_at_zero_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_white_balance(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn white_balance_warms_toward_orange() {
        let mut px = vec![128u8, 128, 128, 255];
        apply_adjustment(Adjustment::WhiteBalance { temp: 1.0, tint: 0.0 }, &mut px, 1, 1);
        assert!(px[0] > 128, "le rouge doit augmenter");
        assert!(px[2] < 128, "le bleu doit diminuer");
    }

    #[test]
    fn denoise_identity_at_zero_strength_is_noop() {
        let original = vec![10u8, 120, 240, 255];
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_denoise(), &mut px, 1, 1);
        assert_eq!(px, original);
    }

    #[test]
    fn denoise_reduces_local_variance() {
        let w = 12;
        let h = 12;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 140 } else { 120 };
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let original = rgba.clone();
        apply_adjustment(Adjustment::Denoise { strength: 1.0 }, &mut rgba, w as u32, h as u32);
        let variance = |data: &[u8]| -> f32 {
            let vals: Vec<f32> = data.chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = vals.iter().sum::<f32>() / vals.len() as f32;
            vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32
        };
        assert!(variance(&rgba) < variance(&original));
    }

    #[test]
    fn gaussian_blur_identity_at_zero_radius_is_noop() {
        let original: Vec<u8> = (0..(8 * 8 * 4)).map(|i| (i % 256) as u8).collect();
        let mut px = original.clone();
        apply_adjustment(Adjustment::default_gaussian_blur(), &mut px, 8, 8);
        assert_eq!(px, original);
    }

    #[test]
    fn gaussian_blur_spreads_a_single_bright_pixel_isotropically() {
        let w = 15;
        let h = 15;
        let mut rgba = vec![0u8; w * h * 4];
        let ci = ((h / 2) * w + w / 2) * 4;
        rgba[ci..ci + 4].copy_from_slice(&[255, 255, 255, 255]);
        apply_adjustment(Adjustment::GaussianBlur { radius: 3.0 }, &mut rgba, w as u32, h as u32);
        let right_i = ((h / 2) * w + w / 2 + 2) * 4;
        let below_i = ((h / 2 + 2) * w + w / 2) * 4;
        assert!(rgba[right_i] > 0, "le flou doit s'étaler horizontalement");
        assert!(rgba[below_i] > 0, "le flou doit s'étaler verticalement");
        // Isotrope : étalement comparable dans les deux directions (± tolérance).
        assert!(
            (rgba[right_i] as i32 - rgba[below_i] as i32).abs() < 5,
            "le flou gaussien doit être isotrope, eu {} vs {}",
            rgba[right_i],
            rgba[below_i]
        );
    }
}
