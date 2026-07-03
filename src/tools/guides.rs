//! Guides intelligents (roadmap P1 #8) : accroche une boîte en déplacement
//! sur les bords/centres d'autres boîtes (objets du calque, ou canevas).
//! Pur et testable — l'intégration (lecture des éléments du calque, rendu
//! des lignes magenta) est dans `app`.

/// Boîte englobante en coordonnées document : (coin min, coin max).
pub type Box2 = ((f32, f32), (f32, f32));

/// Une ligne de guide affichée pendant le glissé (coordonnée document).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuideLine {
    Vertical(f32),
    Horizontal(f32),
}

/// Ajuste `delta` pour que la boîte `moving` (une fois translatée par
/// `delta`) accroche un de ses bords/centre à un bord/centre de `targets`,
/// axe X et axe Y indépendamment, dans la limite de `threshold` (unités
/// document — déjà converti depuis un seuil écran par l'appelant, pour
/// rester constant visuellement quel que soit le zoom). Renvoie le delta
/// corrigé et les guides à afficher (au plus un par axe : le plus proche).
pub fn snap(
    moving: Box2,
    targets: &[Box2],
    threshold: f32,
    delta: (f32, f32),
) -> ((f32, f32), Vec<GuideLine>) {
    let (mn0, mx0) = moving;
    let (mn, mx) = ((mn0.0 + delta.0, mn0.1 + delta.1), (mx0.0 + delta.0, mx0.1 + delta.1));
    let (cx, cy) = ((mn.0 + mx.0) * 0.5, (mn.1 + mx.1) * 0.5);

    let mut xs = Vec::with_capacity(targets.len() * 3);
    let mut ys = Vec::with_capacity(targets.len() * 3);
    for &(tmn, tmx) in targets {
        xs.push(tmn.0);
        xs.push(tmx.0);
        xs.push((tmn.0 + tmx.0) * 0.5);
        ys.push(tmn.1);
        ys.push(tmx.1);
        ys.push((tmn.1 + tmx.1) * 0.5);
    }

    // (distance, décalage à appliquer, position exacte de la ligne accrochée)
    let best_axis = |candidates: &[f32], probes: [f32; 3]| -> Option<(f32, f32, f32)> {
        let mut best: Option<(f32, f32, f32)> = None;
        for &cand in candidates {
            for probe in probes {
                let d = (cand - probe).abs();
                if d <= threshold && best.map(|(bd, _, _)| d < bd).unwrap_or(true) {
                    best = Some((d, cand - probe, cand));
                }
            }
        }
        best
    };

    let x_hit = best_axis(&xs, [mn.0, cx, mx.0]);
    let y_hit = best_axis(&ys, [mn.1, cy, mx.1]);

    let mut guides = Vec::new();
    let dx = x_hit.map(|(_, d, _)| d).unwrap_or(0.0);
    let dy = y_hit.map(|(_, d, _)| d).unwrap_or(0.0);
    if let Some((_, _, line)) = x_hit {
        guides.push(GuideLine::Vertical(line));
    }
    if let Some((_, _, line)) = y_hit {
        guides.push(GuideLine::Horizontal(line));
    }

    ((delta.0 + dx, delta.1 + dy), guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_left_edge_to_target_right_edge() {
        // Boîte mobile 0..10, cible à x=50..70 ; delta amène le bord gauche
        // près de 50 (à 2px près) → doit s'accrocher exactement dessus.
        let moving = ((0.0, 0.0), (10.0, 10.0));
        let targets = [((50.0, 0.0), (70.0, 10.0))];
        // Plage Y du bloc cible très différente de celle du mobile, pour ne
        // pas accrocher accidentellement l'axe Y aussi (isole le test à X).
        let (delta, guides) = snap(moving, &targets, 6.0, (48.0, 500.0));
        assert_eq!(delta, (50.0, 500.0));
        assert_eq!(guides, vec![GuideLine::Vertical(50.0)]);
    }

    #[test]
    fn no_snap_outside_threshold() {
        let moving = ((0.0, 0.0), (10.0, 10.0));
        let targets = [((50.0, 0.0), (70.0, 10.0))];
        let (delta, guides) = snap(moving, &targets, 6.0, (20.0, 500.0));
        assert_eq!(delta, (20.0, 500.0));
        assert!(guides.is_empty());
    }

    #[test]
    fn snaps_center_independently_on_each_axis() {
        // Cible centrée en (100, 200) ; boîte mobile 10×10 approchée de sorte
        // que son centre tombe près de x=100 (axe X) et y=200 (axe Y).
        let moving = ((0.0, 0.0), (10.0, 10.0)); // centre (5,5)
        let targets = [((95.0, 195.0), (105.0, 205.0))]; // centre (100,200)
        let (delta, guides) = snap(moving, &targets, 6.0, (93.0, 193.0));
        assert_eq!(delta, (95.0, 195.0));
        assert_eq!(guides.len(), 2);
    }

    #[test]
    fn picks_closest_candidate_when_several_in_range() {
        let moving = ((0.0, 0.0), (10.0, 10.0));
        // Cibles à largeur nulle (bord = centre = une seule valeur), pour ne
        // pas mélanger « bord le plus proche » et « centre le plus proche »
        // dans ce test : seule la distance entre candidats compte ici.
        let targets = [((49.0, 500.0), (49.0, 510.0)), ((53.0, 500.0), (53.0, 510.0))];
        // Bord gauche mobile visé vers x≈49.5 : 49 (dist 0.5) bat 53 (dist 1.5).
        let (delta, _) = snap(moving, &targets, 6.0, (49.5, 500.0));
        assert_eq!(delta, (49.0, 500.0));
    }
}
