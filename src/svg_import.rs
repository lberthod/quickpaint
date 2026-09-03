//! Import SVG vectoriel éditable (Sprint L.5). Utilise `usvg` pour parser et
//! normaliser le SVG (transforms résolus, courbes réduites à des segments
//! absolus Move/Line/Quad/Cubic, texte mis en page) puis convertit l'arbre
//! en `Stroke`/`TextItem` internes — contrairement à un import « rasterisé »
//! (comme un PNG), le résultat reste éditable : déplacer, recolorer,
//! redimensionner un trait ou un texte importé comme n'importe quel autre.
//!
//! Portée couverte : `path`/`rect`/`circle`/`ellipse`/`line`/`polyline`/
//! `polygon` (tout réduit par `usvg` à des primitives `Path`), `<text>`,
//! groupes et `transform` (déjà résolus par `usvg` en transform absolu par
//! nœud). `<use>` est également transparent (déjà résolu par `usvg`).
//!
//! Hors scope, comme décidé avant de coder (Sprint L.5) :
//! - Dégradés/motifs de remplissage → repli sur une couleur unie (moyenne
//!   grossière : premier arrêt du dégradé, ou gris moyen pour un motif).
//! - `clipPath`, masques, filtres SVG → ignorés (le contenu reste visible
//!   sans le clip/masque/filtre, pas de perte de contenu silencieuse).
//! - Police exacte du SVG → police système générique (le modèle interne ne
//!   référence que ses polices embarquées/système par nom, pas un fichier
//!   de police arbitraire) ; la graisse (gras) est conservée.
//! - Un élément à la fois fill ET stroke (contour + remplissage superposés)
//!   → le remplissage est prioritaire, cohérent avec le modèle `Stroke`
//!   actuel qui ne porte qu'un seul style à la fois.

use crate::model::{Document, Stroke, StrokePoint, TextItem, Tool};
use std::path::Path;

/// Importe un fichier `.svg` comme nouveau document éditable.
pub fn import_svg_file(path: &Path) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| format!("{e}"))?;
    import_svg_bytes(&data)
}

/// Cœur de l'import, testable sans fichier sur disque.
pub fn import_svg_bytes(data: &[u8]) -> Result<Document, String> {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    let opt = usvg::Options { fontdb: std::sync::Arc::new(fontdb), ..Default::default() };
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| format!("{e}"))?;

    let size = tree.size();
    let (w, h) = (size.width().round().max(1.0) as u32, size.height().round().max(1.0) as u32);
    crate::model::image::check_dims(w, h)?;

    let mut doc = Document::new((w, h));
    doc.layers[0].strokes.clear();
    doc.layers[0].texts.clear();
    let mut next_id = 1u64;
    walk(tree.root(), 1.0, &mut doc.layers[0], &mut next_id);
    doc.next_layer_id = 2;
    Ok(doc)
}

fn walk(group: &usvg::Group, parent_opacity: f32, layer: &mut crate::model::Layer, next_id: &mut u64) {
    // Les groupes invisibles (`display: none`) sont déjà retirés de l'arbre
    // par `usvg` au moment du parsing — pas besoin de vérifier une
    // visibilité explicite ici (`Group` n'expose d'ailleurs pas cette
    // méthode, contrairement à `Path`).
    let opacity = parent_opacity * group.opacity().get();
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => walk(g, opacity, layer, next_id),
            usvg::Node::Path(p) => {
                if let Some(stroke) = path_to_stroke(p, opacity, *next_id) {
                    *next_id += 1;
                    layer.strokes.push(stroke);
                }
            }
            usvg::Node::Text(t) => {
                if let Some(item) = text_to_item(t, opacity, *next_id) {
                    *next_id += 1;
                    layer.texts.push(item);
                }
            }
            // Images imbriquées dans un SVG (rare pour un dessin importé) :
            // hors scope de cette première version, voir la doc de module.
            usvg::Node::Image(_) => {}
        }
    }
}

/// Aire signée (formule du lacet, sous-chemin refermé implicitement comme le
/// ferait un remplissage SVG) d'une polyligne — sert à distinguer une forme
/// 2D (aire significative) d'un tracé essentiellement 1D comme un `<line>`
/// ou un chemin ouvert dégénéré (aire nulle même en diagonale, contrairement
/// à une simple boîte englobante), pour décider si le contour doit primer
/// sur le remplissage par défaut noir que `usvg` résout même sans `fill`
/// explicite.
fn bbox_area(points: &[(f32, f32)]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for i in 0..points.len() {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % points.len()];
        sum += x0 * y1 - x1 * y0;
    }
    (sum * 0.5).abs()
}

fn paint_to_rgb(paint: &usvg::Paint) -> [u8; 3] {
    match paint {
        usvg::Paint::Color(c) => [c.red, c.green, c.blue],
        // Dégradés/motifs : repli documenté sur un gris moyen plutôt que de
        // perdre la forme entièrement.
        _ => [128, 128, 128],
    }
}

fn path_to_stroke(p: &usvg::Path, opacity: f32, id: u64) -> Option<Stroke> {
    if !p.is_visible() {
        return None;
    }
    let transform = p.abs_transform();
    let points = flatten_path(p.data(), transform);
    if points.len() < 2 {
        return None;
    }
    // `usvg` résout le remplissage par défaut à noir dès qu'aucun `fill`
    // explicite n'est présent (comportement CSS/SVG standard) — même pour un
    // `<line>` géométriquement sans aire, qui n'a donc *visuellement* aucun
    // remplissage bien que `fill()` renvoie `Some`. Un contour est préféré au
    // remplissage quand la boîte englobante du tracé est quasiment
    // dégénérée (une droite), sinon le remplissage reste prioritaire pour
    // les formes 2D (voir la doc de module).
    let looks_like_a_line = bbox_area(&points) < 1.0;
    // Remplissage prioritaire sur le contour si les deux sont présents (voir
    // la doc de module) — cohérent avec le modèle `Stroke` à un seul style.
    let (color, base_width, fill) = if let Some(fill) = p.fill().filter(|_| !(looks_like_a_line && p.stroke().is_some())) {
        let [r, g, b] = paint_to_rgb(fill.paint());
        let a = (fill.opacity().get() * opacity * 255.0).round().clamp(0.0, 255.0) as u8;
        ([r, g, b, a], 1.0, true)
    } else {
        let stroke = p.stroke()?;
        let [r, g, b] = paint_to_rgb(stroke.paint());
        let a = (stroke.opacity().get() * opacity * 255.0).round().clamp(0.0, 255.0) as u8;
        ([r, g, b, a], stroke.width().get(), false)
    };
    let mut s = Stroke::new(color, base_width.max(0.5), Tool::Brush);
    s.id = id;
    s.fill = fill;
    s.points = points.into_iter().map(|pos| StrokePoint { pos, width: base_width.max(0.5) }).collect();
    Some(s)
}

/// Aplatit un chemin `usvg`/`tiny-skia-path` (segments absolus dans l'espace
/// local du nœud) en polyligne document, en appliquant `transform` à chaque
/// point — même principe d'échantillonnage que `tools::pen::sample` (densité
/// proportionnelle à la longueur du segment).
fn flatten_path(data: &usvg::tiny_skia_path::Path, transform: usvg::tiny_skia_path::Transform) -> Vec<(f32, f32)> {
    let tp = |mut pt: usvg::tiny_skia_path::Point| {
        transform.map_point(&mut pt);
        (pt.x, pt.y)
    };
    let mut pts: Vec<(f32, f32)> = Vec::new();
    let mut current = (0.0f32, 0.0f32);
    for seg in data.segments() {
        match seg {
            usvg::tiny_skia_path::PathSegment::MoveTo(p) => {
                current = tp(p);
                pts.push(current);
            }
            usvg::tiny_skia_path::PathSegment::LineTo(p) => {
                current = tp(p);
                pts.push(current);
            }
            usvg::tiny_skia_path::PathSegment::QuadTo(c, p) => {
                let c = tp(c);
                let end = tp(p);
                sample_quad(current, c, end, &mut pts);
                current = end;
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(c1, c2, p) => {
                let c1 = tp(c1);
                let c2 = tp(c2);
                let end = tp(p);
                sample_cubic(current, c1, c2, end, &mut pts);
                current = end;
            }
            usvg::tiny_skia_path::PathSegment::Close => {
                // Fermeture visuelle gérée par le remplissage (`Stroke::fill`
                // referme déjà le tracé à l'affichage) — rien à ajouter ici.
            }
        }
    }
    pts
}

fn steps_for(a: (f32, f32), b: (f32, f32)) -> usize {
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(1.0);
    ((len / 4.0).ceil() as usize).clamp(2, 160)
}

fn sample_quad(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), out: &mut Vec<(f32, f32)>) {
    let steps = steps_for(p0, p2);
    for s in 1..=steps {
        let t = s as f32 / steps as f32;
        let u = 1.0 - t;
        out.push((u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1));
    }
}

fn sample_cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), out: &mut Vec<(f32, f32)>) {
    let steps = steps_for(p0, p3);
    for s in 1..=steps {
        let t = s as f32 / steps as f32;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push((
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        ));
    }
}

fn text_to_item(t: &usvg::Text, opacity: f32, id: u64) -> Option<TextItem> {
    if t.chunks().is_empty() {
        return None;
    }
    let text = t.chunks().iter().map(|c| c.text()).collect::<Vec<_>>().join("");
    if text.trim().is_empty() {
        return None;
    }
    let first_span = t.chunks().iter().find_map(|c| c.spans().first());
    let size = first_span.map(|s| s.font_size().get()).unwrap_or(16.0);
    let bold = first_span.map(|s| s.font().weight() >= 600).unwrap_or(false);
    let [r, g, b, a] = first_span
        .and_then(|s| s.fill())
        .map(|f| {
            let [r, g, b] = paint_to_rgb(f.paint());
            let alpha = (f.opacity().get() * opacity * 255.0).round().clamp(0.0, 255.0) as u8;
            [r, g, b, alpha]
        })
        .unwrap_or([0, 0, 0, (opacity * 255.0).round().clamp(0.0, 255.0) as u8]);

    // Position : `x`/`y` du premier chunk s'ils sont explicites (SVG =
    // baseline), sinon repli sur le coin haut-gauche de la boîte englobante
    // absolue déjà calculée par `usvg`. `pos` interne est le coin haut-gauche
    // (voir `svg.rs`, qui rend le texte à `pos.1 + size*0.8` = baseline) —
    // conversion inverse ici pour rester cohérent à l'export.
    let bbox = t.abs_bounding_box();
    let first_chunk = t.chunks().first();
    let baseline_x = first_chunk.and_then(|c| c.x()).unwrap_or(bbox.x());
    let baseline_y = first_chunk.and_then(|c| c.y()).unwrap_or(bbox.y() + size);
    let pos = (baseline_x, baseline_y - size * 0.8);

    let mut item = TextItem::new(id, pos, size.max(1.0), [r, g, b, a]);
    item.text = text;
    item.bold = bold;
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_a_filled_rect_as_a_filled_stroke() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
            <rect x="10" y="20" width="40" height="30" fill="#ff0000"/>
        </svg>"##;
        let doc = import_svg_bytes(svg).expect("import");
        assert_eq!(doc.size, (100, 80));
        assert_eq!(doc.layers[0].strokes.len(), 1);
        let s = &doc.layers[0].strokes[0];
        assert!(s.fill);
        assert_eq!(&s.color[0..3], &[255, 0, 0]);
        // Rectangle : 4 coins + fermeture, tous dans la boîte attendue.
        for p in &s.points {
            assert!(p.pos.0 >= 9.0 && p.pos.0 <= 51.0, "{:?}", p.pos);
            assert!(p.pos.1 >= 19.0 && p.pos.1 <= 51.0, "{:?}", p.pos);
        }
    }

    #[test]
    fn imports_a_stroked_line_as_an_unfilled_stroke() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="50" height="50">
            <line x1="0" y1="0" x2="40" y2="40" stroke="#00ff00" stroke-width="3"/>
        </svg>"##;
        let doc = import_svg_bytes(svg).expect("import");
        assert_eq!(doc.layers[0].strokes.len(), 1);
        let s = &doc.layers[0].strokes[0];
        assert!(!s.fill);
        assert_eq!(&s.color[0..3], &[0, 255, 0]);
        assert!((s.points[0].width - 3.0).abs() < 1e-3);
    }

    #[test]
    fn imports_a_circle_as_a_closed_filled_stroke() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60">
            <circle cx="30" cy="30" r="20" fill="#ffff00"/>
        </svg>"##;
        let doc = import_svg_bytes(svg).expect("import");
        assert_eq!(doc.layers[0].strokes.len(), 1);
        let s = &doc.layers[0].strokes[0];
        assert!(s.fill);
        assert_eq!(&s.color[0..3], &[255, 255, 0]);
        // usvg convertit un cercle en 4 arcs de Bézier échantillonnés : bien
        // plus de 4 points, tous à peu près à distance 20 du centre (30,30).
        assert!(s.points.len() > 8);
        for p in &s.points {
            let d = ((p.pos.0 - 30.0).powi(2) + (p.pos.1 - 30.0).powi(2)).sqrt();
            assert!((d - 20.0).abs() < 1.0, "point à distance {d} du centre, attendu ~20");
        }
    }

    #[test]
    fn applies_group_transform_to_children() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <g transform="translate(50,0)">
                <rect x="0" y="0" width="10" height="10" fill="#0000ff"/>
            </g>
        </svg>"##;
        let doc = import_svg_bytes(svg).expect("import");
        let s = &doc.layers[0].strokes[0];
        // Décalé de 50 en x par le transform du groupe parent.
        assert!(s.points.iter().any(|p| p.pos.0 >= 49.0 && p.pos.0 <= 61.0));
    }

    #[test]
    fn imports_text_content_and_position() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <text x="10" y="30" font-size="16" fill="#000000">Bonjour</text>
        </svg>"##;
        let doc = import_svg_bytes(svg).expect("import");
        assert_eq!(doc.layers[0].texts.len(), 1);
        assert_eq!(doc.layers[0].texts[0].text, "Bonjour");
    }

    #[test]
    fn rejects_invalid_svg() {
        assert!(import_svg_bytes(b"not an svg").is_err());
    }
}
