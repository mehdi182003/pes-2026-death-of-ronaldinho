# AFS et fichiers internes (PES 6, PC)

Les données de PES 6 sont dans des archives AFS du dossier `dat`. La plupart des fichiers qu'elles contiennent sont compressés en zlib derrière un en-tête de 32 octets, et beaucoup sont des conteneurs de sous-fichiers. Code : `crates/formats-pes/src/afs.rs`, `packed.rs`, `content.rs` ; carte et ouverture de l'installation : `crates/asset-bridge/src/pes6.rs`.

## Documentation publique

- Format AFS (archives de CRI Middleware, courant sur PS2 et PC) : signature `AFS\0`, nombre de fichiers, table (position, taille), puis un couple (position, taille) vers un répertoire de noms de 48 octets par fichier (nom sur 32 octets, date, taille).
- Carte communautaire de `0_text.afs` (« MAP 0_text.afs PES6 », blog obipes6) : plages de numéros des ballons, visages, coiffures, chaussures, maillots, numéros, sons, panneaux...

## Copie testée

Jeu complet PC, version française (`KONAMI\dat`, exécutable `PES6.exe` d'octobre 2006). Quatre archives et un fichier `opmov` (vidéo d'introduction, non étudiée).

| Archive | Taille | Fichiers | Contenu |
| --- | --- | --- | --- |
| `0_text.afs` | 444 692 480 | 9806 | modèles, textures, sons, données |
| `0_sound.afs` | 569 624 576 | 536 | sons ADX uniquement |
| `f_sound.afs` | 354 930 688 | 13 127 | sons ADX uniquement (commentaires en français ?) |
| `f_text.afs` | 24 838 144 | 421 | données encore non identifiées (textes et menus en français ?) |

Aucun emplacement n'est vide. La version de démonstration, utilisée au début du jalon, a moins de fichiers (7152 dans `0_text.afs`, dont 6035 vides) et des numéros décalés hors des plages de la carte : seuls les chiffres du jeu complet sont donnés ici.

## Archive AFS : structure vérifiée

Little-endian.

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | 4 octets | `AFS\0` |
| 4 | u32 | nombre d'emplacements *n* |
| 8 | *n* × (u32, u32) | position et taille de chaque fichier, en octets |
| 8 + 8*n* | u32, u32 | position et taille du répertoire de noms |

Constats :

- tous les fichiers commencent sur une frontière de 2048 octets, sans chevauchement, et finissent dans l'archive ;
- **le répertoire de noms est illisible** : il a la bonne taille (48 × *n* octets), et dans `0_text.afs` il est bien en dehors des données (offset 0x8c90400), mais ses octets sont d'aspect aléatoire, comme les fichiers chiffrés ; dans les trois autres archives, sa position tombe au milieu d'un autre fichier. Le code n'utilise donc pas de noms : les fichiers sont désignés par archive et numéro (`0_text:1943`).

## Fichier compressé : en-tête de 32 octets

Devant 9711 des 9806 fichiers de `0_text.afs`, et devant certains sous-fichiers des conteneurs.

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | u8 | 0 |
| 1 | u8 | type (6 pour presque tous les conteneurs) |
| 2 | u8 | 1 : données zlib ; 0 : données stockées telles quelles |
| 3 | u8 | 0 |
| 4 | u32 | taille des données après l'en-tête |
| 8 | u32 | taille une fois décompressé |
| 12 – 31 | | souvent nuls |
| 32 | | données |

Les signatures les plus fréquentes sont donc `00 06 01 00` (type 6, zlib) et `00 06 00 00` (type 6, stocké).

Constats sur `0_text.afs` :

- 8815 fichiers compressés et 896 stockés ; les données remplissent le fichier, à 15 octets de remplissage près (les sous-fichiers des conteneurs sont alignés sur 16 octets) ;
- **7663 fichiers compressés se décompressent** à la taille exacte annoncée ;
- **1152 ne se décompressent pas** : 1151 ont la forme d'un fichier compressé ordinaire (en-tête normal), mais leurs données ne sont pas du zlib et ont l'aspect d'octets aléatoires. Ils sont tous dans deux zones : **un fichier sur trois des maillots** (n° 5473, 5476, 5479... jusqu'à 6829, la grande texture de chaque tenue, 453 fichiers) et **le bloc n° 1193 à 1890** (698 fichiers, contenu inconnu). Le dernier, n° 9467, commence comme du zlib mais ne se décompresse pas ;
- ces fichiers sont signalés, pas devinés. **Ce sont des fichiers protégés par le jeu** : Chaos FC ne cherche pas à les déchiffrer (voir `docs/BRIEF.md`, « Reste à faire (J4) »).

Inconnues : le sens exact de l'octet 1 ; pour les fichiers stockés, le champ de l'offset 8 n'est pas toujours nul ; le sens des octets 12 à 31 quand ils ne sont pas nuls.

## Conteneur : structure vérifiée

Après décompression, la plupart des fichiers de type 6 sont des conteneurs :

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | u32 | nombre de sous-fichiers *n* |
| 4 | u32 | position de la table : toujours 8 |
| 8 | *n* × u32 | position de chaque sous-fichier, croissante |

Un sous-fichier finit là où commence le suivant, le dernier à la fin des données. Les positions sont alignées sur 16 octets. Un sous-fichier peut être lui-même compressé (même en-tête de 32 octets) ou un autre conteneur. `0_text.afs` contient 7703 conteneurs.

## Contenus reconnus

| Signature | Contenu | Nombre (`0_text.afs`) |
| --- | --- | --- |
| `20 05 04 20` | modèle 3D (voir [pes-model.md](pes-model.md)) | 9622 sous-fichiers et 25 fichiers |
| `94 72 85 29` | texture (voir [pes-texture.md](pes-texture.md)) | 8289 sous-fichiers et 49 fichiers |
| `80 00`, puis la position (big-endian) des données audio, précédée de `(c)CRI` | son ADX (CRI) | 34 (plus 536 dans `0_sound.afs` et 13 127 dans `f_sound.afs`) |
| `RIFF` … `WAVE` | son WAV | 15 |
| `WEPLDATA` | base des joueurs | 1 (structure non lue) |

La forme de la signature des modèles fait penser à une date (2005-04-20), comme celle de plusieurs formats encore inconnus.

Encore inconnus (signature : nombre de sous-fichiers dans `0_text.afs`), à reprendre aux jalons J5 et J6 :

| Signature | Nombre | Indices |
| --- | --- | --- |
| `07 12 01 20` | 963 | par séries dans des conteneurs dont le premier sous-fichier est une petite table commençant par leur nombre ; demi-flottants valant 1,0 (`00 3c`) et flottants comme 450,0 ou 600,0. HYPOTHÈSE à tester en J6 : des animations |
| `19 11 01 20` | 623 | signature en forme de date ; non étudiée |
| premier octet `1a`, `1b`, `5a`, `9a`, `9b`... | plus de 1000 | en-tête d'environ 28 octets puis de longues suites d'entiers de 16 bits qui varient doucement ; structure pas assez régulière pour conclure |
| petits entiers (`14 00 00 00`, `0c 00 00 00`, `03 00 00 00`, `06 00 00 00`...) | plusieurs milliers | petites tables ; non étudiées |
| `18 39 84 29` | 264 | petites tables de positions (nombre à l'offset 8) |
| `57 45 39 00`, `57 45 38 49` (« WE9 », « WE8I ») | 6 fichiers | en-têtes au nom de Winning Eleven, la version japonaise ; non étudiés |
| `aPDT` | 22 fichiers | non compressés, tailles multiples de 2048 |
| `10 00 00 00` (type 14) | 433 fichiers | non étudiés |

## Carte de `0_text.afs`

Numéros à partir de 0, tirés de la carte communautaire (`asset_bridge::pes6::section`). **Vérifiée sur le jeu complet** : chaque plage a le contenu attendu (test `text_archive_contents_match_the_community_map`).

| Numéros | Section | Contenu constaté |
| --- | --- | --- |
| 0 – 48 | ballons | 24 modèles et 24 textures |
| 431 – 446 | arbitres | 16 textures |
| 535 – 536 | drapeaux et emblèmes | 2 fichiers non identifiés |
| 1891 – 2937 | visages | 1047 conteneurs : 2 modèles (même tête en deux niveaux de détail) et une texture de visage |
| 2938 – 3404 | visages (éditeur) | 464 conteneurs de textures |
| 4448 – 4902, 4922 – 5316 | coiffures (éditeur) | 848 conteneurs : modèle et textures |
| 5322 – 5338 | chaussures | 17 conteneurs de textures |
| 5339 – 5443 | chaussures (éditeur) | 105 conteneurs de textures |
| 5444 – 5455 | palettes | 12 conteneurs de textures |
| 5456 – 5472 | numéros et polices | 17 conteneurs de textures |
| 5473 – 6831 | maillots | 453 tenues × 3 fichiers : 906 conteneurs de textures lisibles, 453 grandes textures chiffrées |
| 6872 – 6912 | sons | 34 sons ADX et 7 fichiers inconnus |
| 6913 – 6914 | foule | 2 conteneurs de textures |
| 6915 – 6939 | panneaux publicitaires | conteneurs de textures et de données |

Hors carte, les fichiers n° 1193 à 1890 sont chiffrés, et de nombreux fichiers restent à situer (corps de joueurs vers les n° 1060, ensembles d'entraînement vers les n° 288 à 297...).

## Outil

```sh
cargo run -p asset-tools -- afs summary                    # toutes les archives, par contenu
cargo run -p asset-tools -- afs list 0_text --tree         # fichiers, sous-fichiers, sections
cargo run -p asset-tools -- afs list 0_text --kind texture # filtre sur le contenu
cargo run -p asset-tools -- afs extract 0_text 1943        # %LOCALAPPDATA%\chaos-fc\extracted\pes6\0_text\
cargo run -p asset-tools -- afs extract 0_text --all       # toute l'archive (38 285 fichiers)
```

L'extraction écrit le fichier décompressé et chacun de ses sous-fichiers (`0_text_01943.bin`, `0_text_01943_0.mdl`, `_1.mdl`, `_2.tex`). Les extensions `.mdl` et `.tex` sont choisies par Chaos FC : le jeu n'a pas de noms lisibles. Elles sont bloquées par le `.gitignore` et la CI, comme `.adx`.

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` (ignoré si PES 6 n'est pas configuré) : toutes les archives du dossier `dat` s'ouvrent, `0_sound.afs` ne contient que des sons ADX, les fichiers illisibles de `0_text.afs` sont tous dans les zones chiffrées connues, le contenu des sections correspond à la carte, et chaque en-tête de texture donne des dimensions cohérentes.
