//! Export SVG vectoriel (backlog roadmap). Naturel pour notre modèle : traits
//! → `<path>`, formes pleines → path rempli, images → `<image>` base64, textes
//! → `<text>`. L'opacité de calque passe par `<g opacity>` → **compositing
//! correct** dans le fichier exporté (contrairement au rendu écran par-trait).

use crate::model::{Document, Stroke, Tool};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_to_desktop(doc: &Document, bg: [u8; 3]) -> std::io::Result<PathBuf> {
    let svg = render(doc, bg);
    let path = desktop_path();
    std::fs::write(&path, svg)?;
    Ok(path)
}

fn render(doc: &Document, bg: [u8; 3]) -> String {
    let (w, h) = doc.size;
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    let _ = writeln!(s, r#"<rect width="{w}" height="{h}" fill="{}"/>"#, hex(bg));

    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let _ = writeln!(s, r#"<g opacity="{:.3}">"#, layer.opacity.clamp(0.0, 1.0));
        for stroke in &layer.strokes {
            write_stroke(&mut s, stroke);
        }
        for im in &layer.images {
            if im.png_b64.is_empty() {
                continue;
            }
            let _ = writeln!(
                s,
                r#"<image x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" xlink:href="data:image/png;base64,{}"/>"#,
                im.pos.0, im.pos.1, im.size.0, im.size.1, im.png_b64
            );
        }
        for t in &layer.texts {
            if t.text.is_empty() {
                continue;
            }
            let [r, g, b, a] = t.color;
            let _ = writeln!(
                s,
                r#"<text x="{:.2}" y="{:.2}" font-family="sans-serif" font-size="{:.2}" fill="{}" fill-opacity="{:.3}">{}</text>"#,
                t.pos.0,
                t.pos.1 + t.size * 0.8,
                t.size,
                hex([r, g, b]),
                a as f32 / 255.0,
                escape(&t.text)
            );
        }
        let _ = writeln!(s, "</g>");
    }
    let _ = writeln!(s, "</svg>");
    s
}

fn write_stroke(s: &mut String, stroke: &Stroke) {
    if stroke.tool == Tool::Eraser || stroke.points.is_empty() {
        return;
    }
    let [r, g, b, a] = stroke.color;
    let col = hex([r, g, b]);
    let op = a as f32 / 255.0;

    let mut d = String::new();
    for (i, p) in stroke.points.iter().enumerate() {
        let _ = write!(d, "{}{:.2} {:.2} ", if i == 0 { "M" } else { "L" }, p.pos.0, p.pos.1);
    }
    if stroke.fill {
        let _ = writeln!(
            s,
            r#"<path d="{d}Z" fill="{col}" fill-opacity="{op:.3}" stroke="none"/>"#
        );
    } else {
        let _ = writeln!(
            s,
            r#"<path d="{d}" fill="none" stroke="{col}" stroke-opacity="{op:.3}" stroke-width="{:.2}" stroke-linecap="round" stroke-linejoin="round"/>"#,
            stroke.base_width
        );
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn escape(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn desktop_path() -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut dir = PathBuf::from(home);
    dir.push("Desktop");
    if !dir.is_dir() {
        dir = PathBuf::from(".");
    }
    dir.push(format!("QuickPaint-{stamp}.svg"));
    dir
}
