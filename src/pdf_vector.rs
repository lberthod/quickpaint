//! Export PDF vectoriel (Sprint L.7). Même principe que l'export SVG
//! (`svg.rs`) — traits → chemins PDF, formes pleines → chemin rempli, textes
//! → objets texte, images → XObject Image (JPEG) — mais en écrivant de vrais
//! opérateurs de dessin PDF plutôt que des balises SVG. Contrairement à
//! `export::write_pdf` (roadmap ANALYSE.md §12.2), qui rastérise tout le
//! document en une seule image JPEG, ce module garde le texte et les formes
//! nets à n'importe quelle résolution d'impression/zoom.
//!
//! Limites assumées (parité avec `svg.rs`, pas une régression) :
//! - Police standard PDF (Helvetica/Helvetica-Bold, `WinAnsiEncoding`) plutôt
//!   que la police réelle du document — pas d'embarquement de police système,
//!   trop lourd pour ce projet. Couvre le français (accents Latin-1) mais pas
//!   un alphabet arbitraire (CJK, emoji…).
//! - Dégradés, texte sur courbe, styles de calque et masques ne sont pas
//!   rendus (comme `svg.rs`) — seule la géométrie/couleur de base.
//! - Images intégrées en JPEG opaque (alpha aplati), même choix que le PDF
//!   rasterisé existant.

use crate::model::{Document, Stroke, TextItem, Tool};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_to_desktop(doc: &Document, bg: [u8; 3], jpeg_quality: u8) -> std::io::Result<PathBuf> {
    let bytes = render(doc, bg, jpeg_quality)?;
    let path = desktop_path();
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Table d'objets PDF indexés à partir de 1 — assemble l'en-tête, la table
/// xref et le trailer une fois tous les objets connus (mêmes conventions que
/// `export::build_pdf_bytes`, généralisées pour un nombre variable d'objets :
/// ce module a besoin d'un objet par image/police en plus des objets fixes).
#[derive(Default)]
struct PdfBuilder {
    bodies: Vec<Vec<u8>>,
}

impl PdfBuilder {
    /// Réserve un numéro d'objet à remplir plus tard (utile quand le corps
    /// dépend d'un numéro d'objet pas encore connu, ex. la page référence son
    /// flux de contenu avant qu'il soit généré).
    fn reserve(&mut self) -> usize {
        self.bodies.push(Vec::new());
        self.bodies.len()
    }

    fn set(&mut self, obj_num: usize, body: Vec<u8>) {
        self.bodies[obj_num - 1] = body;
    }

    fn add(&mut self, body: Vec<u8>) -> usize {
        self.bodies.push(body);
        self.bodies.len()
    }

    fn build(&self, root_obj: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");
        let mut offsets = vec![0usize; self.bodies.len() + 1];
        for (i, body) in self.bodies.iter().enumerate() {
            let n = i + 1;
            offsets[n] = out.len();
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", self.bodies.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!("trailer\n<< /Size {} /Root {root_obj} 0 R >>\nstartxref\n{xref_pos}\n%%EOF", self.bodies.len() + 1)
                .as_bytes(),
        );
        out
    }
}

fn render(doc: &Document, bg: [u8; 3], jpeg_quality: u8) -> std::io::Result<Vec<u8>> {
    let (w, h) = doc.size;
    let (pw, ph) = (w as f32, h as f32);

    let mut pdf = PdfBuilder::default();
    let catalog = pdf.reserve();
    let pages = pdf.reserve();
    let page = pdf.reserve();
    let content = pdf.reserve();
    // Polices standard (pas d'embarquement — voir la doc de module).
    let font_regular = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    let font_bold = pdf.add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());

    let mut cs = String::new(); // flux de contenu de la page
    let mut images: Vec<(String, usize)> = Vec::new(); // (nom XObject, numéro d'objet)
    let mut gs = GsTable::default();

    // Fond opaque (comme le rendu écran/l'export raster).
    let [br, bg_, bb] = bg;
    let _ = writeln!(cs, "{} {} {} rg", br as f32 / 255.0, bg_ as f32 / 255.0, bb as f32 / 255.0);
    let _ = writeln!(cs, "0 0 {pw} {ph} re f");

    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let layer_alpha = layer.opacity.clamp(0.0, 1.0);
        for stroke in &layer.strokes {
            write_stroke(&mut cs, stroke, ph, layer_alpha, &mut gs);
        }
        for im in &layer.images {
            if im.rgba.is_empty() || im.w == 0 || im.h == 0 {
                continue;
            }
            let rgb = image::RgbaImage::from_raw(im.w, im.h, im.rgba.clone())
                .map(image::DynamicImage::ImageRgba8)
                .map(|d| d.to_rgb8());
            let Some(rgb) = rgb else { continue };
            let mut jpeg = Vec::new();
            if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, jpeg_quality.clamp(1, 100))
                .encode_image(&rgb)
                .is_err()
            {
                continue;
            }
            let name = format!("Im{}", images.len());
            let obj = pdf.add(
                format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
                     /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode \
                     /Length {} >>\nstream\n",
                    im.w,
                    im.h,
                    jpeg.len()
                )
                .into_bytes()
                .into_iter()
                .chain(jpeg)
                .chain(b"\nendstream".to_vec())
                .collect(),
            );
            images.push((name.clone(), obj));
            let gs_name = gs.name(layer_alpha);
            // Place l'image dans son rectangle (coin haut-gauche → PDF
            // bas-gauche, axe y inversé) via la matrice `cm`.
            let (x, y_top) = (im.pos.0, im.pos.1);
            let (iw, ih) = (im.size.0, im.size.1);
            let y_pdf = ph - y_top - ih;
            let _ = writeln!(cs, "q /{gs_name} gs {iw:.2} 0 0 {ih:.2} {x:.2} {y_pdf:.2} cm /{name} Do Q");
        }
        for t in &layer.texts {
            write_text(&mut cs, t, ph, layer_alpha, &mut gs);
        }
    }

    pdf.set(content, format!("<< /Length {} >>\nstream\n{cs}\nendstream", cs.len()).into_bytes());

    let mut resources = String::from("<< /Font << ");
    let _ = write!(resources, "/F1 {font_regular} 0 R /F2 {font_bold} 0 R");
    resources.push_str(" >>");
    if !images.is_empty() {
        resources.push_str(" /XObject << ");
        for (name, obj) in &images {
            let _ = write!(resources, "/{name} {obj} 0 R ");
        }
        resources.push_str(">>");
    }
    if !gs.declared.is_empty() {
        resources.push_str(" /ExtGState << ");
        for (permille, name) in &gs.declared {
            let a = *permille as f32 / 1000.0;
            let _ = write!(resources, "/{name} << /ca {a:.3} /CA {a:.3} >> ");
        }
        resources.push_str(">>");
    }
    resources.push_str(" >>");

    pdf.set(
        page,
        format!(
            "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {pw} {ph}] /Resources {resources} /Contents {content} 0 R >>"
        )
        .into_bytes(),
    );
    pdf.set(pages, format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>").into_bytes());
    pdf.set(catalog, format!("<< /Type /Catalog /Pages {pages} 0 R >>").into_bytes());

    Ok(pdf.build(catalog))
}

/// Table des ExtGState PDF (opacité, ressource `/GSn`) — un seul par valeur
/// d'opacité distincte (précision au millième), réutilisé pour tous les
/// éléments qui la partagent plutôt qu'un ExtGState par élément.
#[derive(Default)]
struct GsTable {
    cache: Vec<(u16, String)>,
    declared: Vec<(u16, String)>,
}

impl GsTable {
    fn name(&mut self, alpha: f32) -> String {
        let key = (alpha.clamp(0.0, 1.0) * 1000.0).round() as u16;
        if let Some((_, name)) = self.cache.iter().find(|(k, _)| *k == key) {
            return name.clone();
        }
        let name = format!("GS{}", self.cache.len());
        self.cache.push((key, name.clone()));
        self.declared.push((key, name.clone()));
        name
    }
}

fn write_stroke(cs: &mut String, stroke: &Stroke, ph: f32, layer_alpha: f32, gs: &mut GsTable) {
    if stroke.tool == Tool::Eraser || stroke.points.is_empty() {
        return;
    }
    let [r, g, b, a] = stroke.color;
    let alpha = layer_alpha * (a as f32 / 255.0);
    let gs = gs.name(alpha);
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

    let _ = writeln!(cs, "q /{gs} gs");
    if stroke.fill {
        let _ = writeln!(cs, "{rf:.3} {gf:.3} {bf:.3} rg");
    } else {
        let _ = writeln!(cs, "{rf:.3} {gf:.3} {bf:.3} RG {:.2} w 1 J 1 j", stroke.base_width);
    }
    for (i, p) in stroke.points.iter().enumerate() {
        let y = ph - p.pos.1;
        let _ = writeln!(cs, "{:.2} {y:.2} {}", p.pos.0, if i == 0 { "m" } else { "l" });
    }
    if stroke.fill {
        let _ = writeln!(cs, "h f");
    } else {
        let _ = writeln!(cs, "S");
    }
    let _ = writeln!(cs, "Q");
}

fn write_text(cs: &mut String, t: &TextItem, ph: f32, layer_alpha: f32, gs: &mut GsTable) {
    if t.text.is_empty() {
        return;
    }
    let [r, g, b, a] = t.color;
    let alpha = layer_alpha * (a as f32 / 255.0);
    let gs = gs.name(alpha);
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let font = if t.bold { "F2" } else { "F1" };
    // Baseline approximative (même heuristique que `svg.rs` : 0.8 × la taille
    // sous le coin haut-gauche), axe y inversé (PDF = bas-gauche).
    let y_pdf = ph - (t.pos.1 + t.size * 0.8);
    let _ = writeln!(cs, "q /{gs} gs BT /{font} {:.2} Tf {rf:.3} {gf:.3} {bf:.3} rg", t.size);
    let _ = writeln!(cs, "1 0 0 1 {:.2} {y_pdf:.2} Tm", t.pos.0);
    let _ = writeln!(cs, "({}) Tj", escape_pdf_text(&t.text));
    let _ = writeln!(cs, "ET Q");
}

/// Échappe une chaîne pour un littéral PDF `(...)` — parenthèses et
/// antislash. Les caractères hors Latin-1 (WinAnsiEncoding) ne sont pas
/// représentables avec une police standard non embarquée ; ils passent tels
/// quels (le lecteur PDF les affichera au mieux, voir la doc de module).
fn escape_pdf_text(t: &str) -> String {
    t.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

fn desktop_path() -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut dir = PathBuf::from(home);
    dir.push("Desktop");
    if !dir.is_dir() {
        dir = PathBuf::from(".");
    }
    dir.push(format!("QuickPaint-{stamp}-vector.pdf"));
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Stroke, StrokePoint};

    fn doc_with_stroke() -> Document {
        let mut doc = Document::new((100, 80));
        let mut s = Stroke::new([255, 0, 0, 255], 3.0, Tool::Brush);
        s.points.push(StrokePoint { pos: (10.0, 10.0), width: 3.0 });
        s.points.push(StrokePoint { pos: (50.0, 60.0), width: 3.0 });
        doc.layers[0].strokes.push(s);
        doc
    }

    #[test]
    fn produces_a_well_formed_single_page_pdf() {
        let doc = doc_with_stroke();
        let bytes = render(&doc, [255, 255, 255], 90).expect("render");
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF"));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/Type /Catalog"));
        assert!(s.contains("/Type /Page"));
        assert!(s.contains("/MediaBox [0 0 100 80]"));
        assert!(s.contains("startxref"));
    }

    #[test]
    fn stroke_path_uses_pdf_stroke_operator_not_fill() {
        let doc = doc_with_stroke();
        let bytes = render(&doc, [255, 255, 255], 90).expect("render");
        let s = String::from_utf8_lossy(&bytes);
        // Trait non rempli : couleur de contour (RG), opérateur de tracé (S).
        assert!(s.contains(" RG"));
        assert!(s.contains("\nS\n") || s.contains(" S\n"));
    }

    #[test]
    fn filled_shape_uses_fill_operator() {
        let mut doc = Document::new((20, 20));
        let mut s = Stroke::new([0, 255, 0, 255], 1.0, Tool::Brush);
        s.fill = true;
        s.points.push(StrokePoint { pos: (0.0, 0.0), width: 1.0 });
        s.points.push(StrokePoint { pos: (10.0, 0.0), width: 1.0 });
        s.points.push(StrokePoint { pos: (10.0, 10.0), width: 1.0 });
        doc.layers[0].strokes.push(s);
        let bytes = render(&doc, [255, 255, 255], 90).expect("render");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains(" rg")); // couleur de remplissage
        assert!(s.contains("h f"));
    }

    #[test]
    fn hidden_layer_is_skipped() {
        let mut doc = doc_with_stroke();
        doc.layers[0].visible = false;
        doc.layers.push(Layer::new(2, "L2"));
        let bytes = render(&doc, [255, 255, 255], 90).expect("render");
        let s = String::from_utf8_lossy(&bytes);
        // Aucun opérateur de tracé (S) hors ceux du rectangle de fond (f).
        assert!(!s.contains(" RG"));
    }

    #[test]
    fn text_uses_bold_font_resource_when_flagged() {
        let mut doc = Document::new((100, 50));
        let mut t = TextItem::new(1, (5.0, 5.0), 14.0, [0, 0, 0, 255]);
        t.text = "Bonjour".into();
        t.bold = true;
        doc.layers[0].texts.push(t);
        let bytes = render(&doc, [255, 255, 255], 90).expect("render");
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("/F2"));
        assert!(s.contains("(Bonjour) Tj"));
    }
}
