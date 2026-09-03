//! Pipeline d'export : rendu composite → encodage → écriture disque, aperçu/
//! poids estimé avant export (Sprint L.1/L.2), export par lots (Sprint 7.3),
//! profils d'export nommés (Sprint L.8), export SVG/PDF vectoriels (Sprint
//! L.7) — extrait de `app` en sous-module (sprint.md T3.7, suite de
//! T3.1-T3.6) : un seul rendu natif par export (`render_for_export`), tout
//! le reste dérive de ce même buffer RGBA plutôt que de ré-échantillonner
//! l'écran.

use super::{crop_rgba_to_bounds, t, ExportPreviewDialog, PaintApp};
use egui::Color32;

impl PaintApp {
    /// Rendu du document à sa résolution native pour l'export (roadmap
    /// previous_audit.md (ex-ANALYSE) §12.2) — via le compositeur, jamais via une capture d'écran
    /// du viewport : la résolution exportée est toujours `doc.size`, quels
    /// que soient le zoom et la taille de la fenêtre à l'écran.
    fn render_for_export(&mut self, ctx: &egui::Context) -> Option<(u32, u32, Vec<u8>)> {
        self.compositor.render_to_rgba(ctx, &self.doc, self.bg)
    }

    /// Histogramme RGB du canevas entier (audit_sprint_xx.md D.4), en dehors
    /// de toute sélection d'image — réutilise le même chemin de rendu que
    /// l'export (`render_for_export`) plutôt qu'une capture d'écran du
    /// viewport, pour ne pas dépendre du zoom/de la taille de fenêtre.
    pub fn canvas_histogram(&mut self, ctx: &egui::Context) -> Option<[[u32; 256]; 3]> {
        let (_, _, rgba) = self.render_for_export(ctx)?;
        Some(crate::tools::filter::histogram_rgb(&rgba))
    }

    /// « Rogner les bords vides » (Sprint G.3) : recadre le canevas au
    /// rectangle minimal englobant les pixels non transparents du rendu
    /// composite. Ne fait rien (message d'info) sur un document entièrement
    /// transparent, plutôt qu'un recadrage à 0×0.
    pub fn trim_transparent_borders(&mut self, ctx: &egui::Context) {
        let Some((w, h, rgba)) = self.render_for_export(ctx) else {
            self.fail(t("Échec du rendu du canevas.", "Canvas render failed."));
            return;
        };
        let Some((x, y, tw, th)) = crate::tools::filter::trim_bounds(&rgba, w as usize, h as usize) else {
            self.info(t("Document entièrement transparent : rien à rogner.", "Document is fully transparent: nothing to trim."));
            return;
        };
        if (x, y, tw, th) == (0, 0, w, h) {
            self.info(t("Aucun bord vide à rogner.", "No empty border to trim."));
            return;
        }
        let mut after = self.doc.clone();
        after.translate_content(-(x as f32), -(y as f32));
        after.size = (tw, th);
        self.push_doc_snapshot(after, t("Rogner les bords vides", "Trim empty borders"));
        self.info(format!("{} : {tw}×{th}", t("Canevas rogné", "Canvas trimmed")));
    }

    /// Exporte le document au format `format`, à sa résolution native.
    pub fn request_export(&mut self, ctx: &egui::Context, format: crate::export::ExportFormat) {
        let Some((w, h, rgba)) = self.render_for_export(ctx) else {
            self.fail(t("Échec du rendu à l'export.", "Export render failed."));
            return;
        };
        match crate::export::save_dialog(w, h, &rgba, format, self.jpeg_quality) {
            Ok(p) => self.info(format!("{} {} : {}", format.label(), t("enregistré", "saved"), p.display())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export", "Export failed"))),
        }
    }

    /// Rendu de l'export, éventuellement borné au rectangle englobant de la
    /// sélection (Sprint L.1) plutôt qu'au document entier — recadre le
    /// rendu complet déjà produit par `render_for_export` (pas de second
    /// chemin de rendu à maintenir).
    fn render_export_region(&mut self, ctx: &egui::Context, selection_only: bool) -> Option<(u32, u32, Vec<u8>)> {
        let (w, h, rgba) = self.render_for_export(ctx)?;
        if !selection_only {
            return Some((w, h, rgba));
        }
        let Some((mn, mx)) = self.selection_bounds() else { return Some((w, h, rgba)) };
        Some(crop_rgba_to_bounds(w, h, &rgba, mn, mx))
    }

    /// Ouvre le dialogue d'aperçu/poids estimé avant export (Sprint L.1/L.2).
    /// Coche « sélection uniquement » par défaut si une sélection existe.
    pub fn open_export_dialog(&mut self, ctx: &egui::Context, format: crate::export::ExportFormat) {
        let selection_only = !self.selection.is_empty();
        self.export_dialog = self.build_export_dialog(ctx, format, selection_only);
        if self.export_dialog.is_none() {
            self.fail(t("Échec du rendu à l'export.", "Export render failed."));
        }
    }

    /// (Re)calcule l'aperçu/poids pour l'état courant du dialogue (format et
    /// case « sélection uniquement ») — appelé à l'ouverture et à chaque
    /// changement de ces réglages.
    fn build_export_dialog(
        &mut self,
        ctx: &egui::Context,
        format: crate::export::ExportFormat,
        selection_only: bool,
    ) -> Option<ExportPreviewDialog> {
        let (w, h, rgba) = self.render_export_region(ctx, selection_only)?;
        let bytes = crate::export::encode_to_bytes(w, h, &rgba, format, self.jpeg_quality).ok()?;
        let pixels: Vec<Color32> = rgba.as_chunks::<4>().0.iter().map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])).collect();
        let image = egui::ColorImage { size: [w as usize, h as usize], pixels };
        let texture = ctx.load_texture("export_preview", image, egui::TextureOptions::LINEAR);
        Some(ExportPreviewDialog { format, selection_only, w, h, bytes, texture })
    }

    /// Recalcule l'aperçu après un changement de format ou de la case
    /// « sélection uniquement » dans le dialogue déjà ouvert.
    pub fn refresh_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.export_dialog else { return };
        let (format, selection_only) = (d.format, d.selection_only);
        self.export_dialog = self.build_export_dialog(ctx, format, selection_only);
    }

    /// Valide le dialogue d'export : ouvre le sélecteur « Enregistrer » et
    /// écrit les octets déjà encodés (pas de second encodage).
    pub fn confirm_export_dialog(&mut self) {
        let Some(d) = self.export_dialog.take() else { return };
        match crate::export::save_dialog_bytes(d.format, &d.bytes) {
            Ok(p) => self.info(format!("{} {} : {}", d.format.label(), t("enregistré", "saved"), p.display())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export", "Export failed"))),
        }
    }

    /// Tailles cochées dans le panneau d'export par lots, en pixels, dérivées
    /// de `Document::size` (Sprint 7.3). Vide si rien n'est coché / saisi.
    fn batch_export_target_sizes(&self) -> Vec<(u32, u32)> {
        let (dw, dh) = self.doc.size;
        let mut sizes = Vec::new();
        let mut push = |w: u32, h: u32| {
            if w > 0 && h > 0 && !sizes.contains(&(w, h)) {
                sizes.push((w, h));
            }
        };
        let scale = |m: f32| {
            (((dw as f32) * m).round() as u32, ((dh as f32) * m).round() as u32)
        };
        if self.batch_export.scale_half {
            let (w, h) = scale(0.5);
            push(w, h);
        }
        if self.batch_export.scale_1 {
            push(dw, dh);
        }
        if self.batch_export.scale_2 {
            let (w, h) = scale(2.0);
            push(w, h);
        }
        if self.batch_export.scale_3 {
            let (w, h) = scale(3.0);
            push(w, h);
        }
        if self.batch_export.custom_enabled {
            if let Ok(w) = self.batch_export.custom_width.trim().parse::<u32>() {
                if w > 0 && dw > 0 {
                    let h = ((w as f32) * (dh as f32 / dw as f32)).round() as u32;
                    push(w, h);
                }
            }
        }
        sizes
    }

    /// Déclenche l'export par lots (Sprint 7.3) : rendu natif unique (§12.2),
    /// puis redimensionné (Lanczos3) vers chaque taille cochée — la base 1×
    /// n'est plus une capture d'écran, donc les tailles 2×/3× ne ré-agrandissent
    /// plus une image déjà sous-échantillonnée.
    pub fn request_batch_export(&mut self, ctx: &egui::Context) {
        let sizes = self.batch_export_target_sizes();
        if sizes.is_empty() {
            self.info(t("Aucune taille sélectionnée.", "No size selected."));
            return;
        }
        self.show_batch_export = false;
        let format = self.batch_export.format;
        let Some((w, h, rgba)) = self.render_for_export(ctx) else {
            self.fail(t("Échec du rendu à l'export.", "Export render failed."));
            return;
        };
        match crate::export::save_batch(w, h, &rgba, format, &sizes, self.jpeg_quality) {
            Ok(n) => self.info(format!("{n} {} ({}).", t("fichiers exportés", "files exported"), format.label())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export", "Export failed"))),
        }
    }

    /// Enregistre l'état courant du panneau d'export par lots comme profil
    /// nommé (Sprint L.8) — écrase silencieusement un profil du même nom,
    /// comme `save_brush_preset`.
    pub fn save_export_profile(&mut self, name: String) {
        if name.trim().is_empty() {
            self.info(t("Donne un nom au profil.", "Give the profile a name."));
            return;
        }
        let profile = crate::export::ExportProfile {
            name: name.trim().to_string(),
            format: self.batch_export.format,
            jpeg_quality: self.jpeg_quality,
            scale_half: self.batch_export.scale_half,
            scale_1: self.batch_export.scale_1,
            scale_2: self.batch_export.scale_2,
            scale_3: self.batch_export.scale_3,
            custom_enabled: self.batch_export.custom_enabled,
            custom_width: self.batch_export.custom_width.clone(),
        };
        self.export_profiles.retain(|p| p.name != profile.name);
        self.export_profiles.push(profile);
        crate::i18n::save_export_profiles(&self.export_profiles);
        self.info(t("Profil d'export enregistré.", "Export profile saved."));
    }

    /// Applique un profil d'export nommé au panneau d'export par lots.
    pub fn apply_export_profile(&mut self, profile: &crate::export::ExportProfile) {
        self.batch_export.format = profile.format;
        self.jpeg_quality = profile.jpeg_quality;
        self.batch_export.scale_half = profile.scale_half;
        self.batch_export.scale_1 = profile.scale_1;
        self.batch_export.scale_2 = profile.scale_2;
        self.batch_export.scale_3 = profile.scale_3;
        self.batch_export.custom_enabled = profile.custom_enabled;
        self.batch_export.custom_width = profile.custom_width.clone();
    }

    pub fn delete_export_profile(&mut self, name: &str) {
        self.export_profiles.retain(|p| p.name != name);
        crate::i18n::save_export_profiles(&self.export_profiles);
    }

    /// Export SVG vectoriel (opacité de calque correcte via `<g opacity>`).
    pub fn export_svg(&mut self) {
        self.encode_all_images();
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        match crate::svg::save_to_desktop(&self.doc, bg) {
            Ok(p) => self.info(format!("{} : {}", t("SVG enregistré", "SVG saved"), p.display())),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export SVG", "SVG export failed"))),
        }
    }

    /// Export PDF vectoriel (Sprint L.7) : même pipeline que `export_svg`,
    /// mais écrit de vrais opérateurs de dessin PDF au lieu de balises SVG —
    /// contrairement à `request_export(ExportFormat::Pdf)`, qui rastérise
    /// tout le document en une seule image JPEG.
    /// Impression (Sprint T, point 20) — version « PDF vers Aperçu » : le
    /// document est rendu en PDF vectoriel dans un fichier temporaire puis
    /// ouvert dans l'application PDF par défaut (Aperçu), où l'utilisateur
    /// imprime avec le vrai dialogue macOS. Choix assumé (voir CHANGELOG.md
    /// 0.20.0) : couvre l'usage sans dépendance objc2
    /// supplémentaire ; un `NSPrintOperation` natif reste possible plus
    /// tard. Note sandbox : `open` fonctionne aussi dans l'App Sandbox
    /// (LaunchServices), contrairement à un accès disque arbitraire.
    pub fn print_document(&mut self) {
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        match crate::pdf_vector::pdf_bytes(&self.doc, bg, self.jpeg_quality) {
            Ok(bytes) => {
                let path = std::env::temp_dir().join("quickpaint-impression.pdf");
                let opened = std::fs::write(&path, &bytes)
                    .and_then(|_| std::process::Command::new("open").arg(&path).status());
                match opened {
                    Ok(st) if st.success() => self.info(t(
                        "PDF ouvert dans Aperçu — imprime avec ⌘P depuis Aperçu.",
                        "PDF opened in Preview — print with ⌘P from Preview.",
                    )),
                    _ => self.fail(t(
                        "Impossible d'ouvrir le PDF d'impression.",
                        "Couldn't open the print PDF.",
                    )),
                }
            }
            Err(e) => self.fail(format!(
                "{} : {e}",
                t("Échec de préparation de l'impression", "Print preparation failed")
            )),
        }
    }

    pub fn export_pdf_vector(&mut self) {
        let bg = [self.bg.r(), self.bg.g(), self.bg.b()];
        match crate::pdf_vector::save_to_desktop(&self.doc, bg, self.jpeg_quality) {
            Ok(p) => self.info(format!("{} : {}", t("PDF vectoriel enregistré", "Vector PDF saved"), p.display())),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export PDF vectoriel", "Vector PDF export failed"))),
        }
    }
}
