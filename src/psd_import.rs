//! Import de fichiers PSD (Sprint 8.3) : mappe chaque calque Photoshop vers
//! un calque du document interne portant une unique image (les calques PSD
//! sont raster, pas vectoriels — pas de perte à la conversion, juste une
//! adaptation de représentation). Repose sur la crate `psd` (parseur pur
//! Rust, MIT/Apache-2.0) plutôt qu'un parseur maison — le format PSD a assez
//! de variantes/versions pour qu'une réimplémentation ne soit pas justifiée.
//!
//! Non conservé : styles de calque PSD, texte éditable (aplati en pixels,
//! comme une image), guides, chemins de détourage. Les groupes sont aplatis
//! (chaque calque de groupe redevient un calque de premier niveau) — le
//! modèle de document ne connaît pas encore de dossiers de calques.

use crate::model::{BlendMode, Document, ImageItem, Layer};

/// Charge un fichier `.psd` et le convertit en `Document`.
pub fn import_psd(path: &std::path::Path) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|e| format!("fichier illisible : {e}"))?;
    let psd = psd::Psd::from_bytes(&data).map_err(|e| format!("PSD invalide : {e}"))?;

    let (w, h) = (psd.width(), psd.height());
    crate::model::image::check_dims(w, h)?;

    let mut doc = Document::new((w, h));
    doc.layers.clear();

    // `psd.layers()` est **haut → bas** (le premier élément est le calque du
    // dessus, confirmé sur la fixture de test « two-layers-red-green-1x1 » :
    // `layers()[0].name() == "Red"`, la doc de la fixture dit « top layer is
    // red »). Notre `Document::layers` est l'inverse : le compositeur dessine
    // dans l'ordre de la liste, donc l'index 0 doit être le calque du
    // **dessous** — d'où le `.rev()` ci-dessous plutôt qu'un calcul de `z`.
    let mut next_id = 1u64;
    for pl in psd.layers().iter().rev() {
        // `layer_right()`/`layer_bottom()` ne sont pas fiables comme
        // `layer_left() + width()` dans cette version de la crate (constaté
        // en sondant la fixture de test : left=top=right=bottom=0 sur un
        // calque de 1×1 pixel) — `width()`/`height()` sont les valeurs sûres.
        let (left, top) = (pl.layer_left(), pl.layer_top());
        let (lw, lh) = (pl.width() as u32, pl.height() as u32);
        let mut layer = Layer::new(next_id, pl.name().to_string());
        next_id += 1;
        layer.visible = pl.visible();
        layer.opacity = pl.opacity() as f32 / 255.0;
        // `psd::BlendMode` n'est pas ré-exporté par la crate (type interne
        // qui fuite dans une API publique, cf. commentaire de
        // `map_blend_mode`) : on ne peut pas nommer le type pour le
        // matcher directement, d'où le détour par sa représentation `Debug`.
        layer.blend = map_blend_mode(&format!("{:?}", pl.blend_mode()));

        if lw > 0 && lh > 0 {
            let rgba = pl.rgba();
            if rgba.len() == (lw as usize) * (lh as usize) * 4 {
                let mut im = ImageItem::from_rgba(next_id, (left as f32, top as f32), lw, lh, rgba);
                next_id += 1;
                im.size = (lw as f32, lh as f32);
                layer.images.push(im);
            }
        }
        doc.layers.push(layer);
    }

    if doc.layers.is_empty() {
        // PSD aplati sans section de calques (rare, mentionné dans la doc de
        // la crate) : retombe sur l'image composée entière.
        let mut layer = Layer::new(1, "Calque 1".to_string());
        let mut im = ImageItem::from_rgba(1, (0.0, 0.0), w, h, psd.rgba());
        im.size = (w as f32, h as f32);
        layer.images.push(im);
        doc.layers.push(layer);
    }

    // Ids de calque/z définitifs recalculés par `apply_loaded`/`normalize_ids`
    // à l'ouverture (même chemin que les autres imports) : ces valeurs ne
    // servent qu'à laisser le document dans un état interne cohérent avant ça.
    doc.next_layer_id = doc.layers.len() as u64 + 1;
    doc.next_z = next_id as f64;
    doc.active_layer = doc.layers.len() - 1; // le calque du dessus (dernier ajouté)
    Ok(doc)
}

/// Convertit le mode de fusion PSD (via sa représentation `Debug`, voir
/// l'appelant) vers notre `BlendMode` (6 modes seulement). Le reste
/// (Dissolve, ColorBurn, LinearBurn, ColorDodge, LinearDodge, SoftLight,
/// HardLight, VividLight, LinearLight, PinLight, HardMix, Difference,
/// Exclusion, Subtract, Divide, Hue, Saturation, Color, Luminosity,
/// PassThrough) n'a pas d'équivalent : retombe sur Normal plutôt que de
/// refuser l'import — un rendu légèrement différent bat un import bloqué.
fn map_blend_mode(debug_name: &str) -> BlendMode {
    match debug_name {
        "Multiply" => BlendMode::Multiply,
        "Screen" => BlendMode::Screen,
        "Overlay" => BlendMode::Overlay,
        "Darken" | "DarkerColor" => BlendMode::Darken,
        "Lighten" | "LighterColor" => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture réelle empruntée aux tests de la crate `psd` elle-même
    /// (MIT/Apache-2.0, `tests/fixtures/two-layers-red-green-1x1.psd`) :
    /// deux calques 1×1, le bas vert nommé, le haut rouge — la seule façon
    /// de vérifier l'import sur un vrai fichier binaire PSD plutôt que sur
    /// la seule gestion d'erreur.
    #[test]
    fn imports_a_real_two_layer_psd() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-layers-red-green-1x1.psd");
        let doc = import_psd(&path).expect("should import a well-formed PSD");
        assert_eq!(doc.size, (1, 1));
        assert_eq!(doc.layers.len(), 2, "le fichier a deux calques");
        // Le calque du dessus (dernier de la liste, z le plus haut) est rouge.
        let top = doc.layers.last().unwrap();
        assert_eq!(top.images.len(), 1);
        assert_eq!(&top.images[0].rgba[0..3], &[255, 0, 0]);
        // Le calque du dessous est vert.
        let bottom = &doc.layers[0];
        assert_eq!(&bottom.images[0].rgba[0..3], &[0, 255, 0]);
    }

    #[test]
    fn rejects_a_non_psd_file() {
        let dir = std::env::temp_dir().join("quickpaint-test-psd-garbage.psd");
        std::fs::write(&dir, b"not a psd file at all").unwrap();
        assert!(import_psd(&dir).is_err());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn maps_common_blend_modes() {
        assert_eq!(map_blend_mode("Multiply"), BlendMode::Multiply);
        assert_eq!(map_blend_mode("Screen"), BlendMode::Screen);
        assert_eq!(map_blend_mode("Normal"), BlendMode::Normal);
        // Mode sans équivalent : repli sur Normal plutôt qu'une erreur.
        assert_eq!(map_blend_mode("HardLight"), BlendMode::Normal);
    }
}
