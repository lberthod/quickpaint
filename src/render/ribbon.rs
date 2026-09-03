//! Génération du « ruban » à épaisseur variable (section 4.4).
//!
//! Pipeline : points lissés → **rééchantillonnage Catmull-Rom** (trait fluide
//! même quand les évènements OS sont espacés, étape 3 du doc) → géométrie.
//!
//! Géométrie robuste : un **quad par segment** (décalé de ±width/2
//! perpendiculairement) + un **disque à chaque point**. Les disques font
//! office de jointures et de bouts ronds, ce qui évite le pincement dans les
//! virages serrés (problème classique du miter sur normale moyennée).
//!
//! Indépendant d'egui (renvoie des sommets bruts) → testable seul.

use crate::model::{Stroke, StrokePoint};

/// Espacement cible (px doc) entre deux échantillons Catmull-Rom.
const RESAMPLE_SPACING: f32 = 3.0;
/// Nombre de côtés d'un disque (jointure / bout rond).
const DISC_SIDES: usize = 14;

/// Un sommet du ruban en coordonnées document.
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: (f32, f32),
}

/// Maillage triangulé prêt à être converti pour le moteur de rendu.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    fn push_tri(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
        let base = self.vertices.len() as u32;
        for p in [a, b, c] {
            self.vertices.push(Vertex { pos: p });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn push_quad(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) {
        // a,b,c,d dans l'ordre (deux triangles a-b-c, a-c-d).
        let base = self.vertices.len() as u32;
        for p in [a, b, c, d] {
            self.vertices.push(Vertex { pos: p });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_disc(&mut self, center: (f32, f32), r: f32, sides: usize) {
        if r < 0.75 {
            return; // disque négligeable : on l'omet (gain de triangles)
        }
        let base = self.vertices.len() as u32;
        self.vertices.push(Vertex { pos: center });
        for i in 0..sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            self.vertices
                .push(Vertex { pos: (center.0 + a.cos() * r, center.1 + a.sin() * r) });
        }
        for i in 0..sides {
            let cur = base + 1 + i as u32;
            let next = base + 1 + ((i + 1) % sides) as u32;
            self.indices.extend_from_slice(&[base, cur, next]);
        }
    }
}

/// Un point rééchantillonné : position + demi-largeur.
#[derive(Clone, Copy)]
struct Sample {
    pos: (f32, f32),
    half: f32,
}

/// Construit la géométrie d'un trait selon son style :
/// - `fill`           → intérieur plein (formes pleines, idée 5) ;
/// - couleur opaque   → ruban robuste (quads + disques) ;
/// - couleur semi-tr. → ruban en bande continue, sans chevauchement (idée 4 :
///   évite les coutures plus foncées au double-blending).
pub fn build(stroke: &Stroke) -> Mesh {
    if stroke.fill {
        return build_fill(stroke);
    }
    let translucent = stroke.color[3] < 255;
    let samples = if stroke.smooth { resample(&stroke.points) } else { raw_samples(&stroke.points) };
    match stroke.dash {
        Some((on, off)) if on > 0.0 && off > 0.0 => build_dashed(&samples, on, off, translucent),
        _ if translucent => build_strip(&samples),
        _ => build_solid(&samples),
    }
}

/// Pointillés (previous_audit.md #55) : découpe `samples` en sous-runs
/// selon la longueur d'arc cumulée (période `on + off`), puis construit
/// chaque run séparément avec le ruban habituel (solide ou bande selon
/// l'opacité) — les jointures/bouts ronds existants restent inchangés,
/// seuls les segments dans un « trou » sont omis. Résolution des coupures
/// bornée par `RESAMPLE_SPACING` (segments déjà rééchantillonnés), pas du
/// pixel près — suffisant pour un motif visuel, pas un besoin d'impression
/// de précision.
fn build_dashed(samples: &[Sample], on: f32, off: f32, translucent: bool) -> Mesh {
    let build_run = |run: &[Sample]| if translucent { build_strip(run) } else { build_solid(run) };
    let mut mesh = Mesh::default();
    if samples.len() < 2 {
        return build_run(samples);
    }
    let period = on + off;
    let mut run: Vec<Sample> = vec![samples[0]];
    let mut dist = 0.0f32;
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg_len = ((b.pos.0 - a.pos.0).powi(2) + (b.pos.1 - a.pos.1).powi(2)).sqrt();
        if dist.rem_euclid(period) < on {
            run.push(b);
        } else {
            if run.len() >= 2 {
                append(&mut mesh, &build_run(&run));
            }
            run = vec![b];
        }
        dist += seg_len;
    }
    if run.len() >= 2 {
        append(&mut mesh, &build_run(&run));
    }
    mesh
}

/// Géométrie opaque d'un sous-ensemble de points (sans fill ni alpha). Utilisée
/// par le rendu incrémental du trait en cours (`canvas::ActiveStroke`).
pub fn build_slice(points: &[StrokePoint]) -> Mesh {
    build_solid(&resample(points))
}

/// Concatène `src` dans `dst` en décalant les index.
pub fn append(dst: &mut Mesh, src: &Mesh) {
    let base = dst.vertices.len() as u32;
    dst.vertices.extend_from_slice(&src.vertices);
    dst.indices.extend(src.indices.iter().map(|i| i + base));
}

/// Ruban opaque robuste : un quad par segment + un disque par point (jointures
/// et bouts ronds, aucun pincement dans les virages).
fn build_solid(samples: &[Sample]) -> Mesh {
    let mut mesh = Mesh::default();
    if samples.is_empty() {
        return mesh;
    }
    if samples.len() == 1 {
        mesh.push_disc(samples[0].pos, samples[0].half, DISC_SIDES);
        return mesh;
    }
    for w in samples.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dir = normalize((b.pos.0 - a.pos.0, b.pos.1 - a.pos.1));
        let n = (-dir.1, dir.0);
        mesh.push_quad(
            (a.pos.0 + n.0 * a.half, a.pos.1 + n.1 * a.half),
            (b.pos.0 + n.0 * b.half, b.pos.1 + n.1 * b.half),
            (b.pos.0 - n.0 * b.half, b.pos.1 - n.1 * b.half),
            (a.pos.0 - n.0 * a.half, a.pos.1 - n.1 * a.half),
        );
    }
    for s in samples {
        mesh.push_disc(s.pos, s.half, DISC_SIDES);
    }
    mesh
}

/// Ruban en bande continue à jointures « miter » (normale moyennée). Les
/// triangles ne se recouvrent quasiment pas → pas de coutures en semi-
/// transparence. Bouts plats (acceptable pour de l'encre translucide).
fn build_strip(samples: &[Sample]) -> Mesh {
    let mut mesh = Mesh::default();
    let n = samples.len();
    if n == 0 {
        return mesh;
    }
    if n == 1 {
        mesh.push_disc(samples[0].pos, samples[0].half, DISC_SIDES);
        return mesh;
    }
    // Bords gauche/droite par point, normale = moyenne des segments adjacents.
    let mut verts: Vec<(f32, f32)> = Vec::with_capacity(n * 2);
    for i in 0..n {
        let dir = if i == 0 {
            seg_dir(samples[0].pos, samples[1].pos)
        } else if i == n - 1 {
            seg_dir(samples[n - 2].pos, samples[n - 1].pos)
        } else {
            let a = seg_dir(samples[i - 1].pos, samples[i].pos);
            let b = seg_dir(samples[i].pos, samples[i + 1].pos);
            normalize((a.0 + b.0, a.1 + b.1))
        };
        let nrm = (-dir.1, dir.0);
        let p = samples[i].pos;
        let h = samples[i].half;
        verts.push((p.0 + nrm.0 * h, p.1 + nrm.1 * h));
        verts.push((p.0 - nrm.0 * h, p.1 - nrm.1 * h));
    }
    let base = mesh.vertices.len() as u32;
    for v in &verts {
        mesh.vertices.push(Vertex { pos: *v });
    }
    for i in 0..n - 1 {
        let l0 = base + (i * 2) as u32;
        let r0 = base + (i * 2 + 1) as u32;
        let l1 = base + (i * 2 + 2) as u32;
        let r1 = base + (i * 2 + 3) as u32;
        mesh.indices.extend_from_slice(&[l0, r0, l1, r0, r1, l1]);
    }
    mesh
}

/// Remplissage de l'intérieur d'un polygone (forme pleine). Éventail depuis le
/// barycentre — correct pour les formes convexes (rectangle, ellipse).
fn build_fill(stroke: &Stroke) -> Mesh {
    let mut mesh = Mesh::default();
    let pts = &stroke.points;
    if pts.len() < 3 {
        return mesh;
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for p in pts {
        cx += p.pos.0;
        cy += p.pos.1;
    }
    let c = (cx / pts.len() as f32, cy / pts.len() as f32);
    for w in pts.windows(2) {
        mesh.push_tri(c, w[0].pos, w[1].pos);
    }
    mesh
}

fn seg_dir(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    normalize((b.0 - a.0, b.1 - a.1))
}

/// Convertit les points bruts en échantillons sans lissage (`Stroke.smooth =
/// false`) : géométrie déjà exacte (formes, chemins de plume échantillonnés),
/// les angles doivent rester nets.
fn raw_samples(pts: &[StrokePoint]) -> Vec<Sample> {
    pts.iter().map(|p| Sample { pos: p.pos, half: (p.width * 0.5).max(0.5) }).collect()
}

/// Rééchantillonne le trait par splines Catmull-Rom. Interpole position et
/// largeur, avec une densité proportionnelle à la longueur de chaque segment.
fn resample(pts: &[StrokePoint]) -> Vec<Sample> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    if n <= 2 {
        return pts
            .iter()
            .map(|p| Sample { pos: p.pos, half: (p.width * 0.5).max(0.5) })
            .collect();
    }

    let mut out = Vec::new();
    for i in 0..n - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(n - 1)];

        let seg_len = dist(p1.pos, p2.pos);
        let steps = ((seg_len / RESAMPLE_SPACING).ceil() as usize).clamp(1, 64);
        // On exclut t=1 (= premier point du segment suivant) sauf au tout dernier.
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let pos = catmull_rom(p0.pos, p1.pos, p2.pos, p3.pos, t);
            let width = lerp(p1.width, p2.width, t);
            out.push(Sample { pos, half: (width * 0.5).max(0.5) });
        }
    }
    let last = pts[n - 1];
    out.push(Sample { pos: last.pos, half: (last.width * 0.5).max(0.5) });
    out
}

fn catmull_rom(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let f = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * ((2.0 * b)
            + (-a + c) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    (f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1))
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn normalize(v: (f32, f32)) -> (f32, f32) {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    if len < 1e-6 {
        (1.0, 0.0)
    } else {
        (v.0 / len, v.1 / len)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Stroke, StrokePoint, Tool};

    fn stroke_of(points: &[((f32, f32), f32)]) -> Stroke {
        let mut s = Stroke::new([0, 0, 0, 255], 4.0, Tool::Brush);
        for (pos, w) in points.iter() {
            s.points.push(StrokePoint { pos: *pos, width: *w });
        }
        s
    }

    #[test]
    fn two_points_make_geometry() {
        let s = stroke_of(&[((0.0, 0.0), 4.0), ((10.0, 0.0), 4.0)]);
        let m = build(&s);
        assert!(!m.indices.is_empty());
        assert_eq!(m.indices.len() % 3, 0);
    }

    #[test]
    fn single_point_makes_a_disc() {
        let s = stroke_of(&[((5.0, 5.0), 8.0)]);
        let m = build(&s);
        assert_eq!(m.indices.len(), DISC_SIDES * 3);
    }

    #[test]
    fn dashed_stroke_has_less_geometry_than_solid_over_the_same_path() {
        // ≥3 points collinéaires : `resample` ne rééchantillonne un segment
        // par sa longueur d'arc qu'à partir de 3 points (`n <= 2` renvoie les
        // points bruts tels quels, voir `resample`) — indispensable ici pour
        // que `build_dashed` ait plusieurs segments où carver des trous.
        let mut s = stroke_of(&[((0.0, 0.0), 4.0), ((50.0, 0.0), 4.0), ((100.0, 0.0), 4.0)]);
        let solid_tris = build(&s).indices.len();
        s.dash = Some((6.0, 6.0));
        let dashed = build(&s);
        assert!(!dashed.indices.is_empty(), "toujours de la géométrie, pas juste des trous");
        assert!(
            dashed.indices.len() < solid_tris,
            "les segments dans les trous ne doivent pas être rendus : {} vs plein {solid_tris}",
            dashed.indices.len()
        );
    }

    #[test]
    fn dash_with_zero_gap_is_equivalent_to_solid() {
        // `off = 0` (ou `on`/`off` non renseignés) ne doit pas produire un
        // trait fantomatique troué par erreur d'arrondi.
        let mut s = stroke_of(&[((0.0, 0.0), 4.0), ((50.0, 0.0), 4.0)]);
        let solid = build(&s).indices.len();
        s.dash = Some((6.0, 0.0));
        assert_eq!(build(&s).indices.len(), solid, "off=0 doit retomber sur un trait plein");
    }

    #[test]
    fn curve_is_densified() {
        // 4 points espacés → bien plus d'échantillons que de points d'entrée.
        let s = stroke_of(&[
            ((0.0, 0.0), 6.0),
            ((30.0, 20.0), 6.0),
            ((60.0, 0.0), 6.0),
            ((90.0, 20.0), 6.0),
        ]);
        let samples = resample(&s.points);
        assert!(samples.len() > 10);
    }
}
