# TXD (dictionnaires de textures de Vice City, PC)

Un TXD est un flux RenderWare (voir [renderware.md](renderware.md)) qui regroupe des textures au format natif Direct3D 8. Les DFF y renvoient par le nom de texture de leurs matériaux. Code : `crates/formats-rw/src/txd.rs`.

## Documentation publique

Wiki GTAMods : pages « Texture Dictionary (RW Section) » et « Raster (RW Section) ».

## Arbre des chunks

```text
Texture Dictionary (0x16)
├── Struct          nombre de textures (u32)
├── Raster (0x15) × nombre
│   ├── Struct      en-tête natif + palette + niveaux d'image
│   └── Extension   vide
└── Extension
```

## Structure vérifiée

En-tête natif, little-endian, **88 octets** (le wiki annonce 86, mais ses propres champs font 88, comme les vrais fichiers) :

| Champ | Type | Valeurs constatées |
| --- | --- | --- |
| plateforme | u32 | 8 (Direct3D 8) partout |
| filtrage et adressage | u32 | même u32 que dans les textures des DFF (`0x1106` le plus souvent) |
| nom, nom du masque | 2 × char[32] | terminés par un zéro |
| format de raster | u32 | voir ci-dessous |
| a de l'alpha | u32 | 0 ou 1 |
| largeur, hauteur | 2 × u16 | multiples de 4 |
| profondeur (bits par pixel) | u8 | 8 (palette), 16 (DXT), 32 |
| nombre de niveaux de mipmap | u8 | |
| type de raster | u8 | 4 (texture) |
| compression | u8 | 0 aucune, 1 DXT1, 3 DXT3 |

Suivent, si le format contient `PAL8` (`0x2000`), une palette de 256 couleurs de 4 octets, puis pour chaque niveau : sa taille (u32) et ses octets. Aucun remplissage de lignes n'a été constaté. La structure consomme exactement la taille de son chunk dans tous les fichiers.

## Formats rencontrés

Sur 1399 TXD (les 1368 de `gta3.img` et 31 fichiers isolés de `models/` et `txd/`), soit 12 286 textures dont 12 284 hors `INTRO.TXD` :

| Format | Profondeur | Compression | Textures | Décodage |
| --- | --- | --- | --- | --- |
| 565 (`0x200`), avec ou sans mipmaps | 16 | DXT1 | 10 781 | BC1, alpha forcé à 255 |
| 1555 (`0x100`), avec ou sans mipmaps | 16 | DXT1 | 212 | BC1 avec alpha 1 bit |
| 4444 (`0x300`), avec ou sans mipmaps | 16 | DXT3 | 1 241 | BC2 |
| PAL8 + 888 (`0x2600`) | 8 | aucune | 47 | palette, 4ᵉ octet ignoré |
| PAL8 + 8888 (`0x2500`) | 8 | aucune | 2 | palette avec alpha |
| 888 (`0x600`) | 32 | aucune | 1 | B, G, R, inutilisé |

Le DXT est décodé par la crate texpresso. Dans les palettes sans alpha (888), le 4ᵉ octet contient des valeurs quelconques : ce n'est pas de l'alpha. Les autres formats (palettes 4 bits, couleurs 16 bits non compressées, DXT5…) n'apparaissent pas et ne sont pas gérés.

## Hypothèses et inconnues

- HYPOTHÈSE : les couleurs des palettes sont dans l'ordre R, G, B, A (wiki). À confirmer à l'œil, par exemple sur un écran de chargement (`txd/LOADSC0.TXD`).
- HYPOTHÈSE : les textures 32 bits sont en B, G, R, inutilisé (wiki). Une seule texture concernée (`Alumplat64` dans `metal.txd`), non vérifiée à l'œil.
- `txd/INTRO.TXD` est en RenderWare 3.1.0.0, avec un raster d'une autre forme : non géré (inutile au projet).
- Seul le premier niveau de mipmap est décodé pour l'instant.

## Fichiers testés

Tests dans `crates/asset-bridge/tests/vice_city_files.rs`, ignorés si Vice City n'est pas configuré :

- tous les TXD de `gta3.img`, de `models/` et de `txd/` (sauf `INTRO.TXD`) se lisent et toutes leurs textures se décodent (`every_txd_parses_and_decodes`) ;
- `player.txd` : une texture `player` de 256 × 256 en DXT1 avec 9 niveaux, entièrement opaque (`player_txd_holds_tommy_texture`).
