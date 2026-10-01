# DFF (modèles RenderWare de Vice City)

Un DFF contient un **Clump** : une hiérarchie de frames (le squelette pour un personnage), des géométries et des atomics qui attachent chaque géométrie à une frame. Code : `crates/formats-rw/src/dff.rs`. Le découpage en chunks est décrit dans [renderware.md](renderware.md).

## Documentation publique

Wiki GTAMods : pages RpClump, Frame List, Node Name, HAnim PLG, Geometry List, RpGeometry, Material List, RpMaterial, Texture, String, Atomic, Skin PLG, Bin Mesh PLG. Les points où le wiki est faux ou incomplet sont signalés ci-dessous.

## Arbre des chunks

Ordre constaté dans **tous** les DFF de `gta3.img` :

```text
Clump
├── Struct                    compteurs
├── Frame List
│   ├── Struct                frames
│   └── Extension × frames    Node Name, HAnim PLG
├── Geometry List
│   ├── Struct                nombre de géométries
│   └── Geometry ×
│       ├── Struct            sommets, triangles…
│       ├── Material List
│       │   ├── Struct        indices des matériaux
│       │   └── Material ×    Struct, Texture (Struct, String nom, String masque, Extension), Extension
│       └── Extension         Bin Mesh PLG, Skin PLG, Morph PLG
├── Atomic ×                  Struct, Extension
├── (Struct, Light) ×         lumières (130 fichiers), non décodées
└── Extension
```

## Structures vérifiées

Tout est en little-endian. « Vérifié » signifie : la structure consomme exactement la taille de son chunk sur tous les fichiers concernés de `gta3.img`, et les valeurs sont cohérentes (voir les tests).

### Struct du Clump

| Champ | Type | Présence |
| --- | --- | --- |
| Nombre d'atomics | u32 | toujours |
| Nombre de lumières | u32 | après 3.3.0.0 |
| Nombre de caméras | u32 | après 3.3.0.0 (toujours 0) |

12 octets dans tous les fichiers 3.3.0.2 et 3.4.0.3, 4 octets dans les cinq 3.2.0.0. Le parser se fie à la taille.

### Frame List

Un u32 (nombre de frames), puis par frame **56 octets** :

| Champ | Type |
| --- | --- |
| right, up, at | 3 × vec3 f32 (matrice de rotation) |
| position | vec3 f32 |
| parent | i32 (−1 pour la racine) |
| drapeaux de matrice | u32 (sens inconnu, non utilisé) |

Le wiki annonce `0x44` octets par frame, mais ses propres champs font 56 octets (0x38), comme les vrais fichiers. Le parent est toujours une frame antérieure. Une matrice s'applique à des vecteurs colonnes : `monde = parent × local`, où les colonnes sont right, up, at et position.

Chaque frame a une Extension, dans le même ordre, avec :

- **Node Name** (`0x0253F2FE`) : le nom, sans zéro final (taille du chunk = longueur du nom).
- **HAnim PLG** (`0x11E`) : version (`0x100`), identifiant d'os (−1 si la frame n'est pas un os), nombre de nœuds. Sur l'os racine seulement, ce nombre est non nul et suivi de : drapeaux (0), taille d'une image clé (36), puis par nœud `(identifiant, index, drapeaux)` en 3 × u32.

**L'ordre des nœuds HAnim diffère de l'ordre des frames.** L'os `i` du Skin PLG est le nœud `i` de la hiérarchie, retrouvé parmi les frames par son identifiant.

### Geometry

| Champ | Type | Présence |
| --- | --- | --- |
| format | u32 : drapeaux, et nombre de jeux d'UV dans les bits 16-23 | toujours |
| nombre de triangles, de sommets, de morph targets | 3 × u32 | toujours |
| ambiant, spéculaire, diffus | 3 × f32 | **avant 3.4** seulement |
| couleurs précalculées | RGBA u8 par sommet | drapeau PRELIT (`0x08`) |
| coordonnées de texture | jeux × sommets × (u, v) f32 | voir ci-dessous |
| triangles | 4 × u16 par triangle | toujours (pas de géométrie native sur PC) |
| par morph target : sphère englobante, a des sommets, a des normales, sommets, normales | 4 f32, 2 u32, vec3 f32 par sommet | toujours 1 morph target |

Nombre de jeux d'UV : les bits 16-23 du format ; s'ils valent 0, 1 si TEXTURED (`0x04`), 2 si TEXTURED2 (`0x80`). 16 géométries n'ont aucune UV.

**Triangles.** Un triangle est stocké `(sommet 2, sommet 1, matériau, sommet 3)` selon les noms du wiki. Remis dans l'ordre `(sommet 1, sommet 2, sommet 3)`, 1341 des 1355 triangles de `player.dff` tournent dans le sens antihoraire autour de leurs normales : c'est l'ordre des faces avant pour Bevy. Les 14 autres sont de petits détails (test `player_triangles_face_their_normals`).

4174 des 6510 géométries n'ont **pas de normales** (objets de la ville, pré-éclairés : 4185 ont des couleurs précalculées) : il faut les calculer pour l'affichage.

### Material List, Material, Texture

- Material List : u32 (nombre d'entrées), puis un i32 par entrée : −1 pour un nouveau matériau (un chunk Material suit), sinon l'index d'un matériau précédent dont c'est une instance.
- Struct d'un Material, 28 octets dans tous les fichiers : drapeaux (inutilisé), couleur RGBA u8, u32 inutilisé (valeurs quelconques), « texturé » u32, puis ambiant, spéculaire, diffus (3 × f32, présents après 3.4.0.0, donc toujours ici).
- Texture : Struct d'un u32 (filtrage et adressage), deux String (nom, puis nom du masque, souvent vide ; terminées par un zéro et complétées à 4 octets), Extension (Sky Mipmap Val).

### Atomic

Struct de 16 octets : index de frame, index de géométrie, drapeaux (`0x01` collisions, `0x04` rendu), u32 inutilisé. Extension : Right To Render, Material Effects.

### Skin PLG

| Champ | Type |
| --- | --- |
| nombre d'os, d'os utilisés, de poids max par sommet, remplissage | 4 × u8 |
| os utilisés | u8 × os utilisés |
| index d'os par sommet | 4 × u8 par sommet |
| poids par sommet (somme = 1) | 4 × f32 par sommet |
| par os : marqueur `0xDEADDEAD` (si version < 3.7 et poids max = 0), puis matrice inverse de liaison | u32, 16 × f32 |
| fin des fichiers 3.4 : limite d'os, groupes, remappages | 3 × u32, tous nuls |

Deux variantes, vérifiées sur les 275 modèles skinnés :

- 3.2 et 3.3 (161 fichiers, dont `player.dff`) : aucun os utilisé listé, poids max 0, marqueur avant chaque matrice, pas de fin.
- 3.4 (114 fichiers) : os utilisés listés, poids max 3 ou 4, pas de marqueur, 12 octets nuls à la fin.

Une matrice est stockée en quatre lignes (right, up, at, position) de 4 flottants. **Le 4ᵉ flottant de chaque ligne est du remplissage** : il contient des valeurs parasites (par exemple 1e34). Validation : pour 270 modèles sur 275, (matrice inverse) × (matrice monde de la frame de l'os) donne l'identité à 1e-7 près. La lecture transposée, elle, échoue.

## Hypothèses et inconnues

- HYPOTHÈSE : dans le u32 de la Texture, filtrage dans les bits 0-7, adressage U dans les bits 8-11 et V dans les bits 12-15 (wiki). `player.dff` donne `0x1106` (trilinéaire, répétition), ce qui est plausible ; à valider à l'œil en J2 avec les textures.
- Les drapeaux de matrice des frames (`0x20003` sur la racine de `player.dff`) ne sont pas interprétés.
- Les lumières des Clumps, le Bin Mesh PLG (triangles en bandes, redondants avec la liste de triangles), le Morph PLG, les Material Effects et Right To Render sont lus comme chunks mais pas décodés.
- **Pour J2 (animation)** : dans 5 modèles skinnés, les frames ne sont pas dans la pose de liaison. `player6.dff` et `fireman.dff` ont tout le squelette tourné de 90° par rapport aux matrices inverses ; `CScamj.dff` a deux os d'yeux décalés. Il faudra partir des matrices inverses, pas des frames, pour la pose de liaison.

## Fichiers testés

Tests dans `crates/asset-bridge/tests/vice_city_files.rs`, ignorés si Vice City n'est pas configuré :

- les 4617 DFF de `gta3.img` se lisent sans erreur (`every_dff_parses`) ;
- `player.dff` (Tommy) : 25 frames, 1 géométrie de 1153 sommets et 1355 triangles, texture `player`, skin de 24 os (`player_dff_has_the_expected_structure`) ; pose de liaison cohérente avec les frames (`player_bind_pose_matches_its_frames`) ; orientation des triangles (`player_triangles_face_their_normals`).
