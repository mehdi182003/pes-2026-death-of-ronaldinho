# Textures de PES 6 (PC)

Signature `94 72 85 29`. Une texture PES 6 garde la forme de la version PlayStation 2 : une palette, puis des indices de couleur. Code : `crates/formats-pes/src/texture.rs`.

## Documentation publique

Aucune description trouvée pour PES 6. La structure ci-dessous vient des textures du jeu complet PC, et les formats de pixels suivent les conventions documentées du GS de la PlayStation 2 (PSMT8, PSMT4, ordre des palettes CSM1).

## En-tête de 128 octets : champs vérifiés

Little-endian.

| Offset | Type | Champ | Vérification |
| --- | --- | --- | --- |
| 0 | 4 octets | `94 72 85 29` | |
| 8 | u32 | taille du fichier | égale à la taille réelle |
| 16 | u16 | position des pixels | 1152 (8 bits) ou 128 (4 bits) |
| 18 | u16 | position de la palette | 128 (8 bits) ou 64 (4 bits) |
| 20, 22 | u16, u16 | largeur, hauteur | |
| 24 | u8 | 2, 4 ou 8 | sens inconnu |
| 25 | u8 | format : `0x13` PSMT8, `0x14` PSMT4 | |
| 26, 27 | u8, u8 | log2 de la largeur et de la hauteur, arrondis au-dessus | 64 × 48 : 6 et 6 |
| 40, 42 | u16, u16 | taille d'envoi des pixels (HYPOTHÈSE) | voir « mis de côté » |

Exemple : 66 688 octets = 128 (en-tête) + 1024 (palette de 256 couleurs) + 256 × 256 (pixels).

## Pixels et palette

- **PSMT8** : un octet par pixel, palette de 256 couleurs RGBA. Les couleurs sont rangées par blocs de 8, les deuxième et troisième blocs de chaque groupe de 32 étant échangés (bits 3 et 4 de l'index échangés). HYPOTHÈSE confirmée à l'œil sur une texture de visage (n° 1943) ; dans le jeu, les palettes remises dans l'ordre ont des dégradés plus réguliers que dans l'ordre stocké.
- **PSMT4** : un demi-octet par pixel, palette de 16 couleurs dans l'ordre. HYPOTHÈSE : le premier pixel est dans la moitié basse de l'octet.
- **Alpha** : HYPOTHÈSE, convention PS2 (0x80 = opaque, valeur doublée) quand aucune couleur de la palette ne dépasse 0x80, ce qui est le cas de 7550 des 7552 palettes. Les autres utilisent toute la plage et sont prises telles quelles.

## Mis de côté (signalés, pas devinés)

| Cas | Nombre | Hypothèse |
| --- | --- | --- |
| Dimensions nulles | 19 | palettes seules |
| Fichier qui s'arrête avant ses pixels (128 ou 1152 octets) | environ 1500 | variantes de couleur : palette pour les pixels de la texture précédente du même conteneur |
| Position de la palette = position des pixels | environ 2000 | palette dans un autre fichier (textures de visage de l'éditeur ; section « palettes » de la carte) |
| Taille d'envoi = moitié de la taille (offsets 40 et 42) | 48 | pixels rangés comme une image 32 bits (« swizzle » PS2), pas encore pris en charge |

Bilan dans `0_text.afs` : 5165 textures décodées, 3173 mises de côté.

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` : `texture_headers_give_sizes_and_their_logarithms` et `every_texture_decodes_or_is_reported` (toutes les textures de `0_text.afs`), `player_body_loads_textured_at_a_plausible_size` (chasuble n° 296/1/0, 128 × 128).

Pour voir une texture : `cargo run -p asset-tools -- afs extract 0_text 1943`, puis la texture `.tex` ; dans le visualiseur, sur un modèle (`--pes-texture`).
