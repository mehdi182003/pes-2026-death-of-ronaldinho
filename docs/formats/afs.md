# AFS et fichiers internes (PES 6, PC)

Les données de PES 6 sont dans des archives AFS du dossier `dat`. La plupart des fichiers qu'elles contiennent sont compressés en zlib derrière un en-tête de 32 octets, et beaucoup sont des conteneurs de sous-fichiers. Code : `crates/formats-pes/src/afs.rs`, `packed.rs`, `content.rs` ; carte et ouverture de l'installation : `crates/asset-bridge/src/pes6.rs`.

## Documentation publique

- Format AFS (archives de CRI Middleware, courant sur PS2 et PC) : signature `AFS\0`, nombre de fichiers, table (position, taille), puis un couple (position, taille) vers un répertoire de noms de 48 octets par fichier (nom sur 32 octets, date, taille).
- Carte communautaire de `0_text.afs` (« MAP 0_text.afs PES6 », blog obipes6) : plages de numéros des ballons, visages, coiffures, chaussures, maillots, numéros, sons, panneaux...

## Copie testée

Démo PC de PES 6 (`PES6 DEMO\dat`) : trois archives.

| Archive | Taille | Emplacements | Utilisés | Contenu |
| --- | --- | --- | --- | --- |
| `0_sound.afs` | 136 585 216 | 536 | 536 | sons ADX uniquement |
| `0_text.afs` | 93 739 008 | 7152 | 1117 | modèles, textures, sons, données |
| `e_text.afs` | 3 309 568 | 421 | 88 | données encore non identifiées |

HYPOTHÈSE : le jeu complet a les mêmes archives (et d'autres) avec la même structure. À revérifier sur une copie complète.

## Archive AFS : structure vérifiée

Little-endian.

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | 4 octets | `AFS\0` |
| 4 | u32 | nombre d'emplacements *n* |
| 8 | *n* × (u32, u32) | position et taille de chaque fichier, en octets |
| 8 + 8*n* | u32, u32 | position et taille du « répertoire de noms » |

Constats sur la démo :

- tous les fichiers commencent sur une frontière de 2048 octets, sans chevauchement, et finissent dans l'archive ;
- **emplacements vides** : taille 0 (6035 sur 7152 dans `0_text.afs`, 333 sur 421 dans `e_text.afs`). Ce sont sans doute les fichiers du jeu complet retirés de la démo. On les garde à leur place : le jeu désigne ses fichiers par numéro ;
- **pas de noms de fichiers** : le couple qui suit la table a la bonne taille (48 × *n* octets), mais sa position tombe au milieu des données d'un autre fichier (n° 338 de `0_text.afs`), et les octets y sont quelconques. Le code n'utilise ce répertoire que s'il est en dehors de toutes les données (`AfsTable::has_directory`), ce qui n'arrive jamais sur la démo.

## Fichier compressé : en-tête de 32 octets

Devant la plupart des fichiers de `0_text.afs` et `e_text.afs`, et devant certains sous-fichiers des conteneurs.

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | u8 | 0 |
| 1 | u8 | type : 0, 1, 2, 4, 5, 6 ou 14 sur la démo |
| 2 | u8 | 1 : données zlib ; 0 : données stockées telles quelles |
| 3 | u8 | 0 |
| 4 | u32 | taille des données après l'en-tête |
| 8 | u32 | taille une fois décompressé |
| 12 – 31 | | souvent nuls (pas toujours : 49 fichiers compressés de `0_text.afs`) |
| 32 | | données |

Les signatures les plus fréquentes sont donc `00 06 01 00` (type 6, zlib) et `00 06 00 00` (type 6, stocké).

Constats :

- les données remplissent le fichier, à un octet de remplissage près pour les fichiers de l'archive (un seul cas), et **0 à 15 octets** pour les sous-fichiers des conteneurs, alignés sur 16 octets ;
- **803 des 820 fichiers compressés** de `0_text.afs` (et les 29 de `e_text.afs`) se décompressent à la taille exacte annoncée, comme les 746 sous-fichiers compressés ;
- **17 fichiers illisibles** dans `0_text.afs` : 16 ne sont pas du zlib (octets d'aspect aléatoire dès l'octet 16, entropie proche de 8 bits par octet, sans motif répétitif) et sont tous dans la plage des maillots (n° 5481 à 5562, par groupes de trois avec deux petits fichiers lisibles) ; 1 (n° 7051) commence bien par un en-tête zlib, mais la suite ne se décompresse pas. Ils sont signalés, pas devinés.

Inconnues :

- HYPOTHÈSE : l'octet 1 indique un type de contenu (le type 6 est presque toujours un conteneur), mais son sens exact n'est pas documenté ;
- pour les fichiers stockés, le champ de l'offset 8 n'est pas toujours nul (34 cas) : sens inconnu ;
- le sens des octets 12 à 31 quand ils ne sont pas nuls.

## Conteneur : structure vérifiée

Après décompression, 876 des 881 fichiers de type 6 sont des conteneurs :

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | u32 | nombre de sous-fichiers *n* |
| 4 | u32 | position de la table : toujours 8 |
| 8 | *n* × u32 | position de chaque sous-fichier, croissante |

Un sous-fichier finit là où commence le suivant, le dernier à la fin des données. Les positions sont alignées sur 16 octets. Un sous-fichier peut être lui-même compressé (même en-tête de 32 octets) ou un autre conteneur.

## Contenus reconnus

| Signature | Contenu | Certitude |
| --- | --- | --- |
| `80 00`, puis la position (big-endian) des données audio, précédée de `(c)CRI` | son ADX (CRI) | vérifié : 536 dans `0_sound.afs`, 34 dans `0_text.afs` |
| `RIFF` … `WAVE` | son WAV | vérifié (15) |
| `WEPLDATA` | base des joueurs | 1 fichier (n° 66) ; structure non lue |
| `20 05 04 20` | modèle 3D | HYPOTHÈSE (voir ci-dessous) |
| `94 72 85 29` | texture | HYPOTHÈSE (voir ci-dessous) |

HYPOTHÈSE modèles et textures, fondée sur la carte communautaire :

- chaque fichier des plages « visages » et « coiffures » est un conteneur de **deux modèles et une texture** (par exemple n° 1943 : 38 192 et 4560 octets pour les modèles, 11 392 pour la texture) ;
- les plages « maillots », « numéros et polices » et « palettes » ne contiennent que des textures ;
- les en-têtes de texture contiennent des dimensions en puissances de deux (512 × 512...).

Leur structure interne sera lue au jalon J5 ; c'est l'affichage d'un modèle qui validera ces hypothèses.

Encore inconnus (signature : nombre dans `0_text.afs`) : `07 12 01 20` (923 sous-fichiers), `18 39 84 29` (264), `aPDT` (22 fichiers), `10 00 00 00` (83 fichiers de type 14), quelques petites tables dont le premier entier est un petit nombre, et les fichiers de `e_text.afs` (36 commencent par `01 00 00 00`). Les trois « PNG » (n° 644 à 646) ne font que 6 octets : des fichiers vidés de la démo.

## Carte de `0_text.afs`

Numéros à partir de 0, tirés de la carte communautaire (`asset_bridge::pes6::section`). HYPOTHÈSE : établie sur le jeu complet. Vérifiée sur la démo pour les visages, coiffures, maillots, numéros, palettes et sons ADX, dont le contenu a la signature attendue (test `text_archive_contents_match_the_community_map`). Les autres plages sont vides ou presque dans la démo.

| Numéros | Section | Fichiers dans la démo |
| --- | --- | --- |
| 0 – 48 | ballons | 3 |
| 431 – 446 | arbitres | 16 |
| 535 – 536 | drapeaux et emblèmes | 0 |
| 1891 – 2937 | visages | 102 |
| 2938 – 3404 | visages (éditeur) | 142 |
| 4448 – 4902, 4922 – 5316 | coiffures (éditeur) | 81 |
| 5322 – 5338 | chaussures | 5 |
| 5339 – 5443 | chaussures (éditeur) | 0 |
| 5444 – 5455 | palettes | 4 |
| 5456 – 5472 | numéros et polices | 8 |
| 5473 – 6831 | maillots | 48, dont 16 illisibles |
| 6872 – 6912 | sons ADX | 41 |
| 6913 – 6914 | foule | 2 |
| 6915 – 6939 | panneaux publicitaires | 25 |

640 fichiers de la démo sont hors de ces plages.

## Outil

```sh
cargo run -p asset-tools -- afs summary                    # toutes les archives, par contenu
cargo run -p asset-tools -- afs list 0_text --tree         # fichiers, sous-fichiers, sections
cargo run -p asset-tools -- afs list 0_text --kind texture # filtre sur le contenu
cargo run -p asset-tools -- afs extract 0_text 1943        # %LOCALAPPDATA%\chaos-fc\extracted\pes6\0_text\
```

L'extraction écrit le fichier décompressé et chacun de ses sous-fichiers (`0_text_01943.bin`, `0_text_01943_0.mdl`, `_1.mdl`, `_2.tex`). Les extensions `.mdl` et `.tex` sont choisies par Chaos FC : le jeu n'a pas de noms. Elles sont bloquées par le `.gitignore` et la CI, comme `.adx`.

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` (ignoré si PES 6 n'est pas configuré) : toutes les archives du dossier `dat` s'ouvrent, `0_sound.afs` ne contient que des sons ADX, au moins 98 % des fichiers de `0_text.afs` se décompressent, et le contenu des sections vérifiées correspond à la carte.
