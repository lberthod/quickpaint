//! Import de fichiers (image/PSD/SVG/tampon de brosse), presse-papiers, et
//! persistance de projet (sauvegarde/ouverture/récents) — extrait de `app`
//! en sous-module (sprint.md T3.3, suite de T3.1/T3.2) : ne partage que
//! `Document`/`History` avec le reste de `app`.

use super::{t, Document, History, PaintApp};

impl PaintApp {
    /// Importe une image comme tampon de brosse personnalisé (Sprint J.2) :
    /// le Pinceau pixel échantillonne ensuite sa luminance (blanc = pleine
    /// couverture, noir = aucune) au lieu de la formule de couverture
    /// circulaire habituelle. Réutilise le même dialogue de sélection/décodage
    /// que `import_image`.
    pub fn import_brush_stamp(&mut self) {
        match crate::project::import_image_dialog() {
            Some(Ok((w, h, rgba))) => {
                self.brush.custom_stamp = Some(std::sync::Arc::new(crate::tools::brush::BrushStamp::from_rgba(w, h, &rgba)));
                self.info(t("Tampon de brosse importé.", "Brush stamp imported."));
            }
            Some(Err(msg)) => {
                self.fail(format!("{} : {msg}", t("Image refusée", "Image rejected")));
            }
            None => {}
        }
    }

    /// Importe une image depuis un fichier.
    pub fn import_image(&mut self) {
        match crate::project::import_image_dialog() {
            Some(Ok((w, h, rgba))) => {
                self.place_image(w, h, rgba);
                self.info(t("Image importée — déplacez-la (outil Sélection).", "Image imported — move it (Select tool)."));
            }
            Some(Err(msg)) => {
                self.fail(format!("{} : {msg}", t("Image refusée", "Image rejected")));
            }
            None => {}
        }
    }

    /// Importe un fichier `.psd` (Sprint 8.3) : contrairement à
    /// `import_image` (une image posée dans le document courant), un PSD
    /// devient un **nouveau document** multi-calques, comme `open_project`.
    pub fn import_psd(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Photoshop (.psd)", &["psd"]).pick_file() else {
            return;
        };
        match crate::psd_import::import_psd(&path) {
            Ok(doc) => {
                self.apply_loaded(doc);
                self.info(t("Fichier PSD importé.", "PSD file imported."));
            }
            Err(msg) => {
                self.fail(format!("{} : {msg}", t("Impossible d'importer le PSD", "Couldn't import the PSD")));
            }
        }
    }

    /// Importe un fichier `.svg` comme nouveau document vectoriel éditable
    /// (Sprint L.5) — contrairement à `import_image` (qui poserait un rendu
    /// bitmap dans le document courant), un SVG devient un **nouveau
    /// document** multi-éléments éditables, comme `import_psd`.
    pub fn import_svg(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).pick_file() else {
            return;
        };
        match crate::svg_import::import_svg_file(&path) {
            Ok(doc) => {
                self.apply_loaded(doc);
                self.info(t("Fichier SVG importé.", "SVG file imported."));
            }
            Err(msg) => {
                self.fail(format!("{} : {msg}", t("Impossible d'importer le SVG", "Couldn't import the SVG")));
            }
        }
    }

    /// Colle une image depuis le presse-papiers (⌘V) — cœur du cas « comparer ».
    pub fn paste_image(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(img) => {
                let (w, h) = (img.width as u32, img.height as u32);
                // Bornage (ANALYSE.md §8.2) : le presse-papiers est une entrée
                // externe comme un fichier, à ne pas allouer sans limite.
                if let Err(e) = crate::model::image::check_dims(w, h) {
                    self.fail(format!("{} : {e}", t("Image du presse-papiers refusée", "Clipboard image rejected")));
                    return;
                }
                self.place_image(w, h, img.bytes.into_owned());
                self.info(t("Image collée depuis le presse-papiers.", "Image pasted from clipboard."));
            }
            Err(_) => {
                self.info(t("Aucune image dans le presse-papiers.", "No image in the clipboard."));
            }
        }
    }

    // --- Projet : sauvegarde / ouverture (idée 6) ---------------------------

    /// Encode (paresseusement) le PNG de toutes les images et du raster peint
    /// avant un export nécessitant les données encodées (projet, SVG) — pour
    /// la frame active ET les autres frames d'animation (Sprint L.6), sans
    /// quoi sauvegarder un projet animé perdrait silencieusement le
    /// raster/masque/pixels d'image des frames non actives.
    pub(super) fn encode_all_images(&mut self) {
        let all_layers = self.doc.layers.iter_mut().chain(self.doc.frames.iter_mut().flat_map(|f| f.layers.iter_mut()));
        for layer in all_layers {
            for im in &mut layer.images {
                im.ensure_encoded();
            }
            layer.ensure_raster_encoded();
            layer.ensure_mask_encoded();
        }
    }

    pub fn save_project(&mut self) {
        self.encode_all_images();
        if let Some(p) = crate::project::save_dialog(&self.doc) {
            crate::i18n::push_recent_project(&p.display().to_string());
            self.info(format!("{} : {}", t("Projet enregistré", "Project saved"), p.display()));
            // Le travail est maintenant en sécurité dans le fichier de
            // projet choisi par l'utilisateur : le brouillon de récupération
            // n'a plus lieu d'être.
            crate::project::clear_recovery();
        }
    }

    pub fn open_project(&mut self) {
        match crate::project::open_dialog() {
            Some((path, Ok(doc))) => {
                self.apply_loaded(doc);
                crate::i18n::push_recent_project(&path.display().to_string());
                self.info(t("Projet ouvert.", "Project opened."));
            }
            Some((_, Err(msg))) => {
                self.fail(format!("{} : {msg}", t("Impossible d'ouvrir le projet", "Couldn't open the project")));
            }
            None => {}
        }
    }

    /// Ouvre un projet depuis un chemin déjà connu (UX-4.3, menu **Fichier ›
    /// Ouvrir récent**) — pas de dialogue.
    pub fn open_recent_project(&mut self, path: &str) {
        match crate::project::open_path(std::path::Path::new(path)) {
            Ok(doc) => {
                self.apply_loaded(doc);
                crate::i18n::push_recent_project(path);
                self.info(t("Projet ouvert.", "Project opened."));
            }
            Err(msg) => {
                self.fail(format!("{} : {msg}", t("Impossible d'ouvrir le projet", "Couldn't open the project")));
            }
        }
    }

    pub(super) fn apply_loaded(&mut self, mut doc: Document) {
        doc.normalize_ids(); // répare les anciens projets (id manquants)
        // Reconstruit les pixels des images depuis leur PNG base64 — pour la
        // frame active (`layers`) ET les autres frames d'animation (Sprint
        // L.6), sans quoi rouvrir un projet animé laisserait les frames non
        // actives sans raster/masque/pixels d'image décodés.
        let all_layers = doc.layers.iter_mut().chain(doc.frames.iter_mut().flat_map(|f| f.layers.iter_mut()));
        for layer in all_layers {
            for im in &mut layer.images {
                im.decode();
            }
            layer.decode_raster();
            layer.decode_mask();
        }
        self.doc = doc;
        self.history = History::new();
        // La révision d'historique repart de 0 avec le nouvel historique :
        // resynchronise le compteur d'autosave pour ne pas rater le premier
        // changement réel de cette nouvelle session de document.
        self.autosave_last_rev = 0;
        self.cache.clear();
        self.image_textures.clear();
        self.erase_pending.clear();
        self.selection.clear();
        self.selection_mask = None;
        self.editing_text = None;
        self.reset_view();
        // Inclut traits, images ET textes : sinon `next_id` peut retomber sous
        // l'id d'un texte/image existant et provoquer des collisions d'ids.
        let max = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.each_z())
            .map(|(id, _)| id)
            .max()
            .unwrap_or(0);
        self.next_id = max + 1;
        // next_z au-dessus de tout élément chargé.
        let maxz = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.each_z())
            .map(|(_, z)| z)
            .fold(0.0_f64, f64::max);
        self.doc.next_z = maxz + 1.0;
    }
}
