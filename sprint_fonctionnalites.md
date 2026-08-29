# sprint_fonctionnalites.md — Sprints de complétion (19 fonctionnalités restantes)

> Issu de l'analyse du 20 juillet 2026 : liste de 100 fonctionnalités
> « concurrent de Paint/GIMP/Canva/Photoshop/Illustrator », croisée avec le
> code — 81 ✅ / 10 🟡 / 9 ❌. Ce document planifie les 19 items non ✅,
> regroupés par zone de code (un sprint = une zone = un contexte de travail),
> ordonnés par rapport impact/effort. Suite des Sprints G-N (voir
> [CHANGELOG.md](CHANGELOG.md), versions 0.16.0-0.19.0).

Effort : **S** (< ½ jour) · **M** (½–2 jours) · **L** (> 2 jours ou décision
produit préalable).

## Vue d'ensemble

| Sprint | Zone de code | Items | Effort total | Statut |
|---|---|---|---|---|
| O | Transformations & sélection | #66 flip, #52 lasso polygonal, #50 symétrie miroir, #60 fourmis | S+M+M+M | ✅ Fait (20 juillet 2026) |
| P | Calques & compositing | #30 fusion complète, #23 modes de fusion, damier (bonus) | M+M+S | ✅ Fait (20 juillet 2026) — damier écarté (voir sprint P) |
| Q | Texte | #82 italique, #83 interlignage/crénage | M+M | ✅ Fait (20 juillet 2026) |
| R | UI & navigation macOS | #96 mode sombre, #95 guides manuels, #93 rotation canevas, #97 raccourcis ⌘, #92 gestes multi-touch | M+M+M+M+L | ✅ Fait (20 juillet 2026) — #93 livré après réestimation L ; #92 écran tactile non engagé (décision) |
| S | Ajustements | #73 courbes multi-points par canal | M | ✅ Fait (20 juillet 2026) |
| T | Import/export & impression | #20 impression, #100a export vidéo, #10 export PSD, #38 .abr, #7 WebP lossy | M+L+L+L+L | ✅ Clos (20 juillet 2026) — #20 et #100a (APNG) livrés ; #10/#38/#7 non-support documenté (décisions produit) |
| U | Animation | #100b onion skin | M | ✅ Fait (20 juillet 2026) |

Ordre recommandé : **O → P → Q → S → R → U → T**. O/P/Q/S sont des gains
rapides sans décision produit ; R contient un chantier lourd (gestes) ; T
concentre tous les items nécessitant une décision (dépendances système,
licences).

---

## Sprint O — Transformations & sélection *(app/transform.rs, app/selection.rs, canvas_overlay.rs)* — ✅ Fait

| Item | Statut | Réalisation |
|---|---|---|
| #66 Retourner horizontal/vertical | ✅ | `Document::flip_content` (traits + ancres de plume + dégradés + textes repositionnés + pixels d'images/raster/masques réellement inversés via `RasterLayer::flipped`/`ImageItem::flip_pixels`), commande `PaintApp::flip_document` annulable (`push_doc_snapshot`), menu Image. Choix : flip du **document entier** (référence Paint) plutôt que de la sélection — un flip de sélection reste possible plus tard via l'échelle signée. Textes : boîte repositionnée en miroir, glyphes non inversés (resteraient illisibles), rotation négée — limite documentée dans le code. |
| #52 Lasso polygonal | ✅ | `SelectMode::PolyLasso` : clic = sommet, double-clic ou clic près du premier sommet (seuil 8 px/zoom, comme la plume) = fermeture, Échap = annulation. Réutilise `select_in_lasso` + masque de sélection pixel. Aperçu : sommets carrés + segment élastique vers le curseur (`paint_marquee`). |
| #50 Symétrie miroir axial | ✅ | `SymmetryMode { Radial, MirrorH, MirrorV, MirrorBoth }` + sélecteur dans la barre d'options (le réglage d'axes ne s'affiche qu'en Radial). Vraie réflexion (orientation inversée), pas une rotation. **Non fait** : l'aperçu live des copies pendant le geste (toujours appliqué à la validation, comme le mode radial existant). |
| #60 Fourmis en marche | ✅ | `selection_mask::contours()` (marching squares : arêtes orientées chaînées en boucles), cache par hash de contenu (`selection_ants`), rendu trait blanc + tirets noirs à offset animé (`dashed_line_with_offset`), repaint ~12 fps seulement quand un masque existe. La teinte semi-transparente existante est conservée en complément. |

*Tests : 308 passent (6 nouveaux — flip involutif strokes/raster, flip vertical des pixels d'image, contours carré/2 régions, miroir H en une commande d'undo, flip annulable).*

## Sprint P — Calques & compositing *(app/layers_ops.rs, render/compositor.rs, model/document.rs)* — ✅ Fait

| Item | Statut | Réalisation |
|---|---|---|
| #30 Fusion de calques complète | ✅ | `RasterLayer::composite_over` (alpha-over épars, tuile par tuile) : `merge_down` et `flatten` composent désormais le raster peint du calque source dans la cible — il était silencieusement perdu. L'**opacité** et le **masque** du calque source sont « cuits » dans les pixels fusionnés (ces attributs disparaissent avec le calque). Limite documentée dans le code : les éléments vectoriels restent transférés éditables tels quels, leur masquage éventuel n'est pas cuit (il faudrait les rasteriser). Annulable (le `SetLayers` existant clone les calques raster compris). |
| #23 Modes de fusion supplémentaires | ✅ | `BlendMode` passe de 6 à 12 modes : + Lumière tamisée (Soft Light), Lumière crue (Hard Light), Différence, Exclusion, Densité couleur − (Color Dodge), Densité couleur + (Color Burn) — tous rendus nativement par tiny-skia (`map_blend`), sélecteur UI automatique (itère `ALL`). Mapping PSD étendu : ces 6 modes ne retombent plus sur Normal à l'import. |
| Bonus : damier de transparence | ❌ Écarté | Non pertinent dans ce modèle : le fond du document est **opaque par conception** et cuit à l'export (`render_to_rgba_bakes_an_opaque_background`) — il n'existe pas d'état « document transparent » qu'un damier représenterait honnêtement. À revisiter seulement si un fond transparent devient une fonctionnalité (alpha sur `bg` + export sans fond). |

*Tests : 313 passent (5 nouveaux — composite_over opacité+masque, fond vide, merge_down garde le raster + annulable, flatten compose bas→haut, mapping PSD des 6 modes ; 1 test existant mis à jour : HardLight ne doit plus retomber sur Normal).*

## Sprint Q — Texte *(model/text.rs, render/text.rs, ui/toolbar.rs)* — ✅ Fait

| Item | Statut | Réalisation |
|---|---|---|
| #82 Italique | ✅ | `TextItem::italic` + bouton 𝐼. `FontManager::ensure_loaded` enregistre désormais **deux** familles egui par police système : le romain et la variante italique (`<famille>#italic`, vraie fonte italique trouvée via `fontdb::Style::Italic` ; repli sur les octets romains si la famille n'en a pas — jamais de famille egui inconnue, qui ferait paniquer le layout). `render::text::family()` bascule sur la famille italique. Écart vs plan : **pas de faux-italique par cisaillement** — egui ne sait pas incliner un galley (il faudrait shear les deux chemins de rendu) ; l'italique est donc effectif seulement avec une police système ayant une vraie variante (tooltip explicite), sans effet sur Sans/Mono intégrées. |
| #83 Interlignage / crénage | ✅ | `TextItem::line_height` (multiple de la taille, défaut 1.25 = l'ancienne valeur codée en dur) et `letter_spacing` (px document, négatif accepté) ; portés par le `TextFormat` egui (`line_height`/`extra_letter_spacing`) dans `render::text::layout` — donc identiques dans le painter live et le compositeur CPU. Deux sliders dans la barre d'options texte ; `approx_bounds` (sélection/cadre) suit ; le texte sur courbe applique l'espacement à l'avance angulaire (`char_angles`). Propagés au presse-papiers de style (copier/coller de style). Limite : non propagés aux writers SVG/PDF vectoriel, qui ne gèrent déjà ni multi-ligne ni espacement (texte mono-run minimal). |

*Tests : 314 passent (1 nouveau — la boîte englobante suit interligne/espacement ; 3 tests `char_angles` mis à jour pour le paramètre d'espacement).*

## Sprint S — Ajustements *(tools/filter.rs, ui/layers.rs)* — ✅ Fait

| Item | Statut | Réalisation |
|---|---|---|
| #73 Courbes libres par canal | ✅ | Nouvelle variante `Adjustment::CurvesFree { master, r, g, b }` : points de contrôle libres (jusqu'à 16 par canal), spline **monotone Fritsch–Carlson** (lisse, jamais d'oscillation entre points), LUT 256 composée `canal[master[entrée]]`. Éditeur interactif dans le panneau de calque d'ajustement : sélecteur RVB/R/V/B, canevas 200×150 (clic = ajouter, glisser = déplacer, clic droit = retirer ; les abscisses restent strictement ordonnées). État transitoire (canal actif, glissé) en mémoire temporaire egui, pas dans le document. Migration : l'ancien `Curves` à 3 points est **conservé tel quel** pour rouvrir les projets existants (plutôt qu'une conversion), les nouveaux calques créent `CurvesFree`. Effet de bord assumé : `Adjustment` n'est plus `Copy` (les courbes portent des `Vec`), 4 sites d'emprunt ajustés. |

*Tests : 317 passent (3 nouveaux — LUT identité, monotonie de la spline, canal rouge isolé).*

## Sprint R — UI & navigation macOS *(app/mod.rs, canvas_input.rs, canvas_overlay.rs, keybindings.rs)*

| Item | Effort | Approche |
|---|---|---|
| #96 Mode sombre | ✅ | `UiTheme { System, Light, Dark }` persisté dans `settings.json`, appliqué via la préférence **native** d'egui 0.29 (`ThemePreference` — `System` suit le thème macOS remonté par winit, bascule automatique). Le `Visuals::light()` forcé au démarrage est retiré. Sélecteur dans le menu Vue. Les règles graduées suivent le mode ; le pasteboard sombre et les guides magenta/cyan restent lisibles dans les deux modes. |
| #95 Guides manuels | ✅ | `Document::guides: Vec<ManualGuide>` (persisté avec le projet). Glisser depuis la règle du haut = guide horizontal, depuis celle de gauche = vertical ; l'outil Sélection peut saisir un guide (±4 px) pour le déplacer, le relâcher hors du document/sur une règle le supprime. Lignes cyan + aperçu pendant le glissé. Magnétisme : `guides::snap()` étendu avec des candidats mono-axe (`extra_x`/`extra_y`) — un guide vertical n'attire que l'axe X, contrairement à une boîte dégénérée qui polluerait l'axe Y. |
| #93 Rotation du canevas | ✅ | Engagé sur décision produit (20 juillet 2026) après réestimation L. `ViewTransform` porte un `angle` (rotation autour de l'origine, chemin historique inchangé à 0°) ; `set_view_angle` compense le pan pour pivoter visuellement autour du **centre du document**. Composite/teinte de sélection/onion skin dessinés en **quadrilatères texturés** (`paint_doc_quad`), fond+ombre en polygones, images via le maillage tourné existant ; cadre de sélection et marquee (rect/ellipse) construits en coordonnées document puis projetés point à point. UI : menu Vue (⟲/⟳ 15°, remise à 0). Limites assumées, affichées dans le menu : règles+guides manuels masqués/inactifs hors 0° (graduations axis-aligned), pot de peinture et détourage refusés hors 0° (ils échantillonnent les pixels *affichés*). |
| #97 Raccourcis ⌘ personnalisables | ✅ | `CommandAction { Export, Duplicate, InvertSelection, ZoomIn/Out/Reset }` rebindables (touche libre, ⌘ implicite, ⇧ capturé avec) — section dédiée dans le panneau Raccourcis, échange en cas de collision, reset commun. Les conventions macOS (⌘Z/⌘C/⌘V/⌘X/⌘S/⌘O/⌘N, ⌘[/⌘]) restent fixes **et refusées comme cible** de rebind. Compatibilité : ⌘= accepté pour le zoom avant comme avant. |
| #92 Gestes multi-touch | ✅/🟡 | **Constat corrigé : le trackpad était déjà couvert** — `canvas_input.rs` branche `zoom_delta()` (pinch) et `smooth_scroll_delta` (pan 2 doigts) depuis longtemps ; l'audit visait l'écran tactile. Reste ouvert uniquement le multi-touch d'un **écran tactile** (NSEvent natifs hors winit, même chantier lourd que la pression stylet — ARCHITECTURE.md §3), qui demande une décision produit. |

## Sprint T — Import/export & impression *(export.rs, project.rs — décisions produit requises)*

| Item | Effort | Approche / décision à trancher |
|---|---|---|
| #20 Impression ⌘P | ✅ | Version M retenue : `print_document()` rend le PDF **vectoriel** (texte net à l'impression) dans un fichier temporaire et l'ouvre dans Aperçu, qui fournit le vrai dialogue d'impression macOS. Menu Fichier + ⌘P (ajouté aux conventions fixes, refusé comme cible de rebind). Le `NSPrintOperation` natif reste une amélioration possible (L, objc2). |
| #100a Export vidéo | ✅ | **Décision produit (20 juillet 2026) : APNG plutôt que MP4** (aucune dépendance système). `save_animated_apng`/`encode_animated_apng` via la crate `png` (déjà dans l'arbre — API `set_animated`/`set_frame_delay` que `image` ne ré-exporte pas), boucle infinie, délai par frame, couleurs 24 bits + alpha là où le GIF plafonne à 256 couleurs. Bouton dédié dans le panneau Animation. MP4 : non-support documenté dans le README. |
| #10 Export PSD | ✅ Clos | **Décision produit (20 juillet 2026) : non-support documenté** (README, section « Not supported by design »). L'import PSD reste. |
| #38 Import .abr | ✅ Clos | **Décision produit (20 juillet 2026) : non-support documenté** (README). Le tampon-depuis-image couvre le besoin. |
| #7 WebP lossy | ✅ Clos | Décision existante **confirmée** (20 juillet 2026) : non, dépendance `libwebp` refusée. |

## Sprint U — Animation *(app/animation.rs, ui/toolbar.rs)* — ✅ Fait

| Item | Statut | Réalisation |
|---|---|---|
| #100b Onion skin | ✅ | Case « Pelure d'oignon » dans le panneau Animation : frames N−1 (teinte rouge) et N+1 (verte) rendues en fantôme **sur fond transparent** (seul le contenu apparaît) sous la frame active. Cache de textures par frame invalidé par révision d'historique (les frames inactives ne changent que par des opérations annulables) ; compositeur jetable à chaque recalcul — les frames partagent les ids de calques, passer par le compositeur principal corromprait ses caches par calque. |

*Sprints R+U : 321 tests passent (4 nouveaux — accroche sur guide manuel mono-axe, défauts ⌘ historiques, refus des touches réservées, échange en cas de collision — isolés du vrai `settings.json` via le pattern `home_env_lock` existant).*

---

## Décisions produit — toutes tranchées le 20 juillet 2026

1. **Export vidéo** : APNG retenu (livré) ; MP4 = non-support documenté.
2. **Export PSD** : non-support documenté (README).
3. **Import .abr** : non-support documenté (README).
4. **WebP lossy** : refus existant confirmé.
5. **Gestes multi-touch** : trackpad déjà couvert ; écran tactile **non engagé** (chantier NSEvent écarté à ce stade).
6. **Rotation du canevas (#93)** : engagée malgré la réestimation L — livrée (voir Sprint R).

**Bilan final : 18 des 19 fonctionnalités livrées** (la 19ᵉ — gestes écran tactile — est un non-engagement décidé, pas un reste à faire). 325 tests, 0 avertissement.
