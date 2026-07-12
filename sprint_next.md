# sprint_next.md — Suivi des sprints produit (suite de audit_next.md)

> Détail complet de chaque sprint : le journal git (messages de commit) et
> [CHANGELOG.md](CHANGELOG.md). Ce document ne garde que le statut et les
> décisions produit qui ne sont pas ailleurs.

Fait suite à [audit_next.md](audit_next.md) (audit du 5 juillet 2026, ~102
items, 17 ✅ / 3 🟡 / 0 ❌ sur les 20 premiers, reste couvert ci-dessous).

| Sprint | Contenu | Statut |
|---|---|---|
| G | Sélection : opérations d'ensemble (add/subtract/intersect), inversion, rognage des bords vides | ✅ Fait |
| H | Sélection : masque de pixels (feather/dilater/contracter) | ✅ Fait — décision produit : option 2 retenue (ajout du masque), confirmée par le porteur de projet le 2026-07-05 |
| I | Calques : remplissage, alignement/répartition, vignettes, recherche, code couleur, verrouillage granulaire | ✅ Fait intégralement |
| J | Dessin : aérographe, import de brosse, aperçu de contour, crayon dédié | ✅ Fait intégralement |
| K | Filtres : pixelisation, halftone, distorsions, flou radial, vignette, Canny, mixeur de canaux, auto-correction | ✅ Fait intégralement |
| L | Formats & export : export sélection, aperçu/poids, métadonnées, glisser-déposer, SVG, GIF animé, PDF vectoriel, profils nommés | ✅ Fait intégralement — décisions produit confirmées le 2026-07-05 : SVG en vectoriel éditable, GIF animé (pas seulement statique), PDF vectoriel ajouté |
| M | Extraction de palette, cisaillement | ✅ Fait |
| N | Rendu GPU via `wgpu` | ❌ **Non traité — décision d'architecture majeure, volontairement pas prise sans confirmation explicite du porteur de projet.** Changerait le backend UI `glow` → `wgpu` (packaging, compatibilité drivers, régressions visuelles à revalider). Le compositeur photo (`tiny-skia`) reste CPU quoi qu'il arrive, ce n'est pas concerné. |

Seul le sprint N reste ouvert, et seulement sur décision explicite.
