# explication_artboards_smart_objects.md — Artboards & Smart Objects, et pourquoi c'est une question de positionnement (30 août 2026)

Contexte : `audit_100_features.md` liste « Plans de travail multiples
(artboards) » et « Objets dynamiques (Smart Objects) » comme absents de
QuickPaint, en notant que les intégrer rapprocherait le produit d'un
« outil de PAO professionnel » plutôt que d'un « outil de dessin/retouche
rapide ». Ce document explique concrètement ce que sont ces deux notions,
en quoi elles diffèrent du modèle actuel de QuickPaint, et pourquoi ce
n'est pas un simple ajout de fonctionnalité mais un choix qui engage
l'architecture et l'identité du produit.

---

## 1. Plans de travail multiples (Artboards)

### C'est quoi, concrètement ?

Dans Illustrator, Figma ou Canva, un **artboard** (ou « plan de travail »)
est une zone de dessin rectangulaire indépendante à l'intérieur d'un même
fichier/document. Un designer peut avoir, dans un seul fichier :

- Artboard 1 : une affiche A4
- Artboard 2 : une story Instagram (1080×1920) reprenant les mêmes éléments
- Artboard 3 : une bannière LinkedIn (1200×627)

Tous les artboards partagent le même espace de travail (on peut faire
glisser un élément d'un artboard à l'autre), mais chacun a sa **propre
taille, son propre fond, et s'exporte indépendamment**. C'est l'outil de
base de la déclinaison multi-format d'un même visuel — exactement le cas
d'usage que couvre le « Redimensionnement magique » de Canva (`audit_100_features.md`
#95), qui n'existe que parce que les artboards existent en amont.

### Comment QuickPaint fonctionne aujourd'hui

`Document` a **une seule taille de canevas** (`doc.size: (u32, u32)`).
La « galerie de modèles » (`ui/toolbar.rs::templates()`) ne fait que
proposer des **dimensions prédéfinies au moment de créer un nouveau
document** — une fois choisi, on a un fichier, une taille, un export. Pour
décliner un visuel en 3 formats aujourd'hui dans QuickPaint, il faut soit
recommencer 3 documents séparés, soit redimensionner le canevas d'un même
document (perdant la version précédente), soit dupliquer les calques et
composer manuellement 3 documents.

### Ce que ça coûterait d'ajouter des artboards

Ce n'est pas un ajout de bouton — ça change la donnée de base :
- `Document` devrait porter **plusieurs tailles de canevas simultanées**,
  chacune avec son propre fond, sa propre zone d'export, potentiellement
  ses propres calques ou des calques partagés avec offset.
- L'undo/redo (`history.rs`), le compositeur (`render/compositor.rs`),
  l'export (`export.rs`, `pdf_vector.rs`, `svg.rs`) devraient tous
  raisonner en « quel artboard » plutôt qu'en « le document ».
- L'UI (canevas, règles, panneau calques) devrait afficher/naviguer entre
  plusieurs zones plutôt qu'une seule — un changement de navigation
  fondamental, pas un panneau en plus.

C'est un chantier de plusieurs semaines, pas de plusieurs jours, parce
qu'il touche le modèle de données central utilisé par quasiment tous les
autres modules.

---

## 2. Objets dynamiques (Smart Objects)

### C'est quoi, concrètement ?

Dans Photoshop, un **Smart Object** est un conteneur qui référence un
contenu (une image importée, ou même un autre document Photoshop) **sans
le rasteriser immédiatement**. Deux propriétés clés :

1. **Transformation non destructive** : redimensionner, faire pivoter ou
   déformer un Smart Object ne dégrade jamais sa qualité — le contenu
   d'origine est conservé intact, seule une transformation est stockée
   « par-dessus » ; on peut revenir en arrière ou re-transformer sans
   perte cumulative.
2. **Liaison/duplication intelligente** : si le même Smart Object est
   utilisé à 3 endroits du document (ou dans 3 documents, en mode
   « lié »), modifier le contenu source **répercute le changement partout
   où il est utilisé**, en une seule opération.

C'est le mécanisme qui permet, par exemple, de remplacer le logo d'une
maquette et de voir toutes ses occurrences se mettre à jour d'un coup.

### Comment QuickPaint fonctionne aujourd'hui

Une image importée (`ImageItem`) est un objet avec ses pixels et ses
transformations, mais :
- Redimensionner une image dans QuickPaint **rééchantillonne ses pixels**
  (comme n'importe quel logiciel raster classique) — répéter l'opération
  plusieurs fois dégrade la qualité, il n'y a pas de « retour à la source »
  automatique au-delà de l'undo classique.
- Dupliquer une image (`duplicate_layer`, copier-coller) crée une **copie
  indépendante** — modifier l'originale n'affecte pas les copies. C'est le
  modèle « chaque calque est son propre monde », cohérent avec le reste de
  l'architecture (calques, historique par delta).

### Ce que ça coûterait d'ajouter des Smart Objects

- Il faudrait un nouveau concept dans le modèle de données : une
  **référence** (pas une copie) vers un contenu source, plus une pile de
  transformations appliquées à la volée au rendu plutôt que cuites dans
  les pixels — un changement du pipeline de composition (`render/compositor.rs`)
  qui aujourd'hui rasterise chaque calque une fois pour toutes (avec cache
  par hash, voir `audit_aout.md` §8).
- Il faudrait gérer la **propagation** : quand la source change, retrouver
  et invalider toutes les occurrences qui la référencent, potentiellement
  à travers plusieurs documents (Smart Objects « liés » vs « embarqués »).
- Le format de sauvegarde (`.json` du projet) devrait représenter des
  références entre objets plutôt que des copies indépendantes — un
  changement de sérialisation qui affecte la compatibilité des projets
  existants.

---

## 3. Pourquoi c'est une question de positionnement, pas juste de fonctionnalités

QuickPaint, tel qu'il existe aujourd'hui, est conçu autour d'un modèle
simple et cohérent : **un document = une taille = des calques
indépendants = des transformations qui se figent dans les pixels/tracés**.
C'est ce qui permet une session de dessin/retouche courte et directe :
ouvrir, dessiner, retoucher, exporter, refermer — sans concept de
« projet » qui vivrait au-delà d'un seul document.

Les artboards et les Smart Objects appartiennent à une autre famille
d'outils : ceux pensés pour des **projets de production** qui durent (une
identité visuelle déclinée en 10 formats, une maquette dont le logo
changera 3 fois avant la version finale). Ce sont des outils qui
supposent qu'on retravaille le même contenu dans le temps et qu'on en
tire plusieurs livrables — le territoire d'Illustrator/InDesign/Figma
plus que celui d'un Paint tactile rapide.

**Ce n'est pas un jugement de valeur** — les deux approches sont
légitimes. La question à trancher est : est-ce que QuickPaint veut rester
un outil de session courte et focalisée, ou évoluer vers un outil de
production multi-livrables ? La réponse change complètement la priorité
(et la faisabilité à effort raisonnable) de ces deux fonctionnalités par
rapport au reste de la liste des 38 items non-✅ de `audit_100_features.md`.
