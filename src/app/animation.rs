//! Animation (Sprint L.6) : chaque frame est un instantané complet de la
//! pile de calques (`AnimationFrame`), pas une timeline de keyframes par
//! calque — le choix le plus simple des deux options envisagées (Sprint
//! L.6), cohérent avec le reste de l'app (pas de nouveau système d'undo dédié
//! : chaque opération de frame passe par `push_doc_snapshot`, donc annulable
//! comme n'importe quel redimensionnement de document).
//!
//! Invariant : `doc.frames` vide = document statique (comportement
//! historique inchangé, `doc.layers` est la seule image). Dès qu'il y a au
//! moins une frame, `doc.layers` reflète le contenu de la frame active
//! (`doc.active_frame`) — les autres frames vivent dans `doc.frames`, mais
//! la frame active elle-même n'y est resynchronisée qu'au moment de changer
//! de frame ou d'exporter (voir `synced_frames`).

use super::PaintApp;
use crate::i18n::t;
use crate::model::AnimationFrame;

impl PaintApp {
    /// Ajoute une nouvelle frame après la frame active, dupliquant son
    /// contenu (façon « dupliquer la frame » d'un éditeur d'animation
    /// classique — un point de départ à retoucher plutôt qu'une frame
    /// vierge). Première frame ajoutée : convertit le document statique en
    /// animation à 2 frames (frame 0 = contenu actuel, frame 1 = sa copie).
    pub fn add_animation_frame(&mut self) {
        let mut after = self.doc.clone();
        if after.frames.is_empty() {
            after.frames.push(AnimationFrame::new(after.layers.clone()));
        } else {
            after.frames[after.active_frame].layers = after.layers.clone();
        }
        after.frames.push(AnimationFrame::new(after.layers.clone()));
        after.active_frame = after.frames.len() - 1;
        self.push_doc_snapshot(after, t("Ajouter une frame", "Add frame"));
    }

    /// Bascule l'édition sur une autre frame : sauvegarde d'abord la frame
    /// quittée (les traits/calques modifiés depuis le dernier changement de
    /// frame vivent dans `doc.layers`, pas encore dans `doc.frames`).
    pub fn switch_animation_frame(&mut self, index: usize) {
        if index >= self.doc.frames.len() || index == self.doc.active_frame {
            return;
        }
        let mut after = self.doc.clone();
        after.frames[after.active_frame].layers = after.layers.clone();
        after.layers = after.frames[index].layers.clone();
        after.active_frame = index;
        self.push_doc_snapshot(after, t("Changer de frame", "Switch frame"));
    }

    /// Supprime une frame. Refuse de vider la dernière frame restante (une
    /// « animation » à 0 frame n'a pas de sens — repasser à un document
    /// statique est une action distincte, pas un effet de bord accidentel).
    pub fn remove_animation_frame(&mut self, index: usize) {
        if self.doc.frames.len() <= 1 || index >= self.doc.frames.len() {
            return;
        }
        let mut after = self.doc.clone();
        after.frames[after.active_frame].layers = after.layers.clone();
        after.frames.remove(index);
        after.active_frame = if after.active_frame >= after.frames.len() {
            after.frames.len() - 1
        } else if index < after.active_frame {
            after.active_frame - 1
        } else {
            after.active_frame
        };
        after.layers = after.frames[after.active_frame].layers.clone();
        self.push_doc_snapshot(after, t("Supprimer la frame", "Delete frame"));
    }

    /// Déplace une frame d'un cran (voisin immédiat) — même granularité que
    /// `move_active_layer` pour les calques, pas de glisser-déposer pour
    /// cette première version.
    pub fn move_animation_frame(&mut self, index: usize, delta: i32) {
        let new_index = index as i32 + delta;
        if new_index < 0 || new_index as usize >= self.doc.frames.len() || index >= self.doc.frames.len() {
            return;
        }
        let new_index = new_index as usize;
        let mut after = self.doc.clone();
        after.frames[after.active_frame].layers = after.layers.clone();
        after.frames.swap(index, new_index);
        after.active_frame = if after.active_frame == index {
            new_index
        } else if after.active_frame == new_index {
            index
        } else {
            after.active_frame
        };
        self.push_doc_snapshot(after, t("Réordonner les frames", "Reorder frames"));
    }

    /// Change le délai d'affichage d'une frame (millisecondes) — édition
    /// continue (glissière), pas de cran d'annulation dédié, comme
    /// `layer.opacity`.
    pub fn set_frame_delay(&mut self, index: usize, delay_ms: u32) {
        if let Some(f) = self.doc.frames.get_mut(index) {
            f.delay_ms = delay_ms.max(20);
        }
    }

    /// Copie des frames avec la frame active resynchronisée depuis
    /// `doc.layers` — nécessaire avant tout usage qui parcourt `doc.frames`
    /// en entier (export), sans quoi les modifications les plus récentes de
    /// la frame en cours d'édition seraient absentes du résultat.
    fn synced_frames(&self) -> Vec<AnimationFrame> {
        let mut frames = self.doc.frames.clone();
        if let Some(active) = frames.get_mut(self.doc.active_frame) {
            active.layers = self.doc.layers.clone();
        }
        frames
    }

    /// Exporte l'animation en APNG (Sprint T, point 100a) : mêmes rendus par
    /// frame que le GIF, mais en PNG animé — couleurs 24 bits + alpha, lu par
    /// tous les navigateurs et Aperçu. Décision produit du 20 juillet 2026 :
    /// retenu à la place du MP4 (qui exigerait une dépendance système).
    pub fn export_animated_apng(&mut self, ctx: &egui::Context) {
        if !self.doc.is_animated() {
            self.info(t(
                "Ajoute au moins 2 frames pour exporter une animation.",
                "Add at least 2 frames to export an animation.",
            ));
            return;
        }
        let frames_data = self.synced_frames();
        let bg = self.bg;
        let mut apng_frames = Vec::with_capacity(frames_data.len());
        let (mut ow, mut oh) = (0, 0);
        for f in &frames_data {
            let mut temp = self.doc.clone();
            temp.layers = f.layers.clone();
            let Some((w, h, rgba)) = self.compositor.render_to_rgba(ctx, &temp, bg) else {
                self.fail(t("Échec du rendu d'une frame.", "Frame render failed."));
                return;
            };
            (ow, oh) = (w, h);
            apng_frames.push((f.delay_ms.max(20), rgba));
        }
        match crate::export::save_animated_apng(&apng_frames, ow, oh) {
            Ok(p) => self.info(format!("{} : {}", t("APNG animé enregistré", "Animated APNG saved"), p.display())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export APNG", "APNG export failed"))),
        }
    }

    /// Exporte l'animation en GIF (Sprint L.6) : chaque frame est rendue
    /// séparément via le compositeur (même chemin que l'export statique),
    /// puis encodée avec son propre délai. Refuse s'il y a moins de 2 frames
    /// (rien à animer).
    pub fn export_animated_gif(&mut self, ctx: &egui::Context) {
        if !self.doc.is_animated() {
            self.info(t(
                "Ajoute au moins 2 frames pour exporter une animation.",
                "Add at least 2 frames to export an animation.",
            ));
            return;
        }
        let frames_data = self.synced_frames();
        let bg = self.bg;
        let mut gif_frames = Vec::with_capacity(frames_data.len());
        for f in &frames_data {
            let mut temp = self.doc.clone();
            temp.layers = f.layers.clone();
            let Some((w, h, rgba)) = self.compositor.render_to_rgba(ctx, &temp, bg) else {
                self.fail(t("Échec du rendu d'une frame.", "Frame render failed."));
                return;
            };
            let Some(buf) = image::RgbaImage::from_raw(w, h, rgba) else { continue };
            let delay = image::Delay::from_numer_denom_ms(f.delay_ms.max(20), 1);
            gif_frames.push(image::Frame::from_parts(buf, 0, 0, delay));
        }
        match crate::export::save_animated_gif(gif_frames) {
            Ok(p) => self.info(format!("{} : {}", t("GIF animé enregistré", "Animated GIF saved"), p.display())),
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => self.info(t("Export annulé.", "Export cancelled.")),
            Err(e) => self.fail(format!("{} : {e}", t("Échec de l'export du GIF animé", "Animated GIF export failed"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_one_stroke() -> PaintApp {
        let mut app = PaintApp::default();
        let id = app.next_id;
        app.next_id += 1;
        let mut s = crate::model::Stroke::new([255, 0, 0, 255], 2.0, crate::model::Tool::Brush);
        s.id = id;
        s.points.push(crate::model::StrokePoint { pos: (0.0, 0.0), width: 2.0 });
        app.doc.layers[0].strokes.push(s);
        app
    }

    #[test]
    fn add_animation_frame_turns_a_static_document_into_two_frames() {
        let mut app = app_with_one_stroke();
        assert!(app.doc.frames.is_empty());
        app.add_animation_frame();
        assert_eq!(app.doc.frames.len(), 2);
        assert_eq!(app.doc.active_frame, 1);
        // Les deux frames démarrent identiques (duplication).
        assert_eq!(app.doc.frames[0].layers[0].strokes.len(), 1);
        assert_eq!(app.doc.frames[1].layers[0].strokes.len(), 1);
    }

    #[test]
    fn switch_animation_frame_preserves_edits_made_on_each_frame() {
        let mut app = app_with_one_stroke();
        app.add_animation_frame(); // frame 0 (1 trait) + frame 1 (copie, 1 trait), actif = 1
        // Ajoute un second trait uniquement sur la frame active (1).
        let id2 = app.next_id;
        app.next_id += 1;
        let mut s2 = crate::model::Stroke::new([0, 255, 0, 255], 2.0, crate::model::Tool::Brush);
        s2.id = id2;
        s2.points.push(crate::model::StrokePoint { pos: (5.0, 5.0), width: 2.0 });
        app.doc.layers[0].strokes.push(s2);
        assert_eq!(app.doc.layers[0].strokes.len(), 2);

        app.switch_animation_frame(0);
        assert_eq!(app.doc.active_frame, 0);
        assert_eq!(app.doc.layers[0].strokes.len(), 1, "la frame 0 ne doit pas avoir le second trait");

        app.switch_animation_frame(1);
        assert_eq!(app.doc.layers[0].strokes.len(), 2, "la frame 1 doit garder le second trait après retour");
    }

    #[test]
    fn remove_animation_frame_refuses_to_empty_the_last_frame() {
        let mut app = app_with_one_stroke();
        app.add_animation_frame();
        app.remove_animation_frame(0);
        assert_eq!(app.doc.frames.len(), 1);
        // Refuse de descendre à 0 frame.
        app.remove_animation_frame(0);
        assert_eq!(app.doc.frames.len(), 1);
    }

    #[test]
    fn move_animation_frame_swaps_neighbors_and_tracks_the_active_frame() {
        let mut app = app_with_one_stroke();
        app.add_animation_frame();
        app.add_animation_frame();
        assert_eq!(app.doc.frames.len(), 3);
        assert_eq!(app.doc.active_frame, 2);
        app.move_animation_frame(2, -1);
        assert_eq!(app.doc.active_frame, 1, "la frame active doit suivre son contenu après déplacement");
    }

    #[test]
    fn saving_and_reloading_a_project_keeps_raster_content_on_inactive_frames() {
        // Régression : `encode_all_images`/`apply_loaded` n'itéraient à
        // l'origine que `doc.layers` (la frame active), pas `doc.frames` —
        // le raster peint sur une frame non active aurait été perdu (encodage
        // manquant) ou resterait vide après rechargement (décodage manquant).
        let mut app = app_with_one_stroke();
        app.add_animation_frame(); // frame 0 (contenu initial) + frame 1 (copie), actif = 1
        app.switch_animation_frame(0);
        // Peint du raster sur la frame 0 (désormais non active après le
        // retour sur la frame 1 ci-dessous).
        app.doc.layers[0].raster.stamp(5.0, 5.0, 4.0, 1.0, [10, 20, 30, 255], false, None);
        app.switch_animation_frame(1);
        assert_eq!(app.doc.active_frame, 1);
        assert!(!app.doc.frames[0].layers[0].raster.is_empty(), "le raster peint doit être sauvegardé dans la frame 0 lors du changement de frame");

        app.encode_all_images();
        let json = serde_json::to_string(&app.doc).expect("serialize");
        let mut reloaded: crate::model::Document = serde_json::from_str(&json).expect("deserialize");
        // Simule `apply_loaded` sans dépendre du reste de son effet de bord
        // (reset d'historique, etc.) : décode raster/masque/images comme le
        // ferait l'ouverture réelle d'un projet.
        let all_layers = reloaded.layers.iter_mut().chain(reloaded.frames.iter_mut().flat_map(|f| f.layers.iter_mut()));
        for layer in all_layers {
            layer.decode_raster();
        }
        assert!(!reloaded.frames[0].layers[0].raster.is_empty(), "le raster de la frame 0 doit survivre à un aller-retour JSON");
        assert_eq!(reloaded.frames[0].layers[0].raster.get_pixel(5, 5)[3], 255);
    }

    #[test]
    fn set_frame_delay_clamps_to_a_minimum() {
        let mut app = app_with_one_stroke();
        app.add_animation_frame();
        app.set_frame_delay(0, 1);
        assert_eq!(app.doc.frames[0].delay_ms, 20);
        app.set_frame_delay(0, 200);
        assert_eq!(app.doc.frames[0].delay_ms, 200);
    }
}
