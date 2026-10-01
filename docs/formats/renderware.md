# Flux binaires RenderWare (DFF, TXD)

Base commune des modèles (`.dff`) et des dictionnaires de textures (`.txd`) de Vice City. Code : `crates/formats-rw/src/rw.rs`.

## Documentation publique

Wiki GTAMods : pages « RenderWare binary stream file », « RenderWare » (versions) et « List of RW section IDs ». Le dépôt officiel `electronicarts/RenderWare3Docs` est une référence possible ; on n'utilise **pas** le code source de RenderWare qui circule (`rw37`).

## Structure vérifiée

Un flux est une suite de chunks. Chaque chunk commence par un en-tête de 12 octets, little-endian :

| Offset | Taille | Type | Champ |
| --- | --- | --- | --- |
| 0 | 4 | u32 | Type du chunk |
| 4 | 4 | u32 | Taille du contenu, en-tête exclu |
| 8 | 4 | u32 | Tampon de bibliothèque (version et build de RenderWare) |

Le contenu est soit des données, soit une suite de chunks enfants. Il n'y a **pas d'alignement** entre chunks : un nom de 6 octets donne un chunk de 18 octets, et le suivant commence juste après.

### Version

Si les 16 bits de poids fort du tampon ne sont pas nuls : `version = ((tampon >> 14) & 0x3FF00) + 0x30000 | ((tampon >> 16) & 0x3F)` et `build = tampon & 0xFFFF`. Sinon (avant 3.1.0.1), `version = tampon << 8`.

Constats sur les 4617 DFF de `models/gta3.img` :

| Tampon | Version | Fichiers |
| --- | --- | --- |
| `0x1003FFFF` | 3.4.0.3 | 4348 |
| `0x0C02FFFF` | 3.3.0.2 | 264, dont `player.dff` |
| `0x0800FFFF` | 3.2.0.0 | 5 |

Le wiki associe Vice City PC à 3.4.0.3, mais un fichier sur quatorze est plus ancien, dont le modèle de Tommy : **les parsers doivent gérer les trois versions**.

### Types de chunks rencontrés

Conteneurs (contenu = chunks enfants), vérifiés sur tous les DFF : Extension `0x03`, Texture `0x06`, Material `0x07`, Material List `0x08`, Frame List `0x0E`, Geometry `0x0F`, Clump `0x10`, Light `0x12`, Atomic `0x14`, Geometry List `0x1A`.

Données : Struct `0x01`, String `0x02`, Right To Render `0x1F`, Morph PLG `0x105`, Sky Mipmap Val `0x110`, Skin PLG `0x116`, HAnim PLG `0x11E`, Material Effects PLG `0x120`, Bin Mesh PLG `0x50E`, Node Name `0x0253F2FE` (extension Rockstar).

Le contenu de chaque type est décrit dans la page du format qui l'utilise ([dff.md](dff.md)).

## Outil

```sh
cargo run -p asset-tools -- dump vc:player.dff             # arbre des chunks, 64 octets de données par chunk
cargo run -p asset-tools -- dump vc:player.dff --bytes 0   # toutes les données
cargo run -p asset-tools -- dump fichier.bin --raw --offset 4096 --bytes 512
```

## Fichiers testés

Tous les DFF de `models/gta3.img` se parcourent sans erreur : la taille de chaque chunk tient dans son parent, et le reste de l'entrée IMG après le chunk racine n'est que du remplissage nul.
