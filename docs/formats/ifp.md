# IFP (animations de Vice City, version 1 « ANPK »)

Les animations des personnages sont dans `anim/ped.ifp` (234 animations). `gta3.img` contient 28 autres IFP (armes, activités). Code : `crates/formats-rw/src/ifp.rs`.

## Documentation publique

Wiki GTAMods, page « IFP ». Elle décrit la version 1 (GTA III, Vice City) et la version 2 (`ANP3`, San Andreas, non concernée). Plusieurs points du wiki sont faux pour Vice City (voir plus bas).

## Structure vérifiée

Le fichier est une suite de **sections** : une étiquette de 4 caractères, une taille (u32, little-endian), puis les données. La taille ne compte pas le remplissage des chaînes : **la section suivante commence à la fin des données arrondie au multiple de 4**.

```text
ANPK                  taille = taille du fichier − 8
├── INFO              nombre d'animations (u32), nom du paquet (« ped »)
└── puis, pour chaque animation (à la suite de INFO, pas dedans) :
    ├── NAME          nom de l'animation, terminé par un zéro
    └── DGAN
        ├── INFO      nombre d'objets (u32), nom (« * »)
        └── CPAN × nombre d'objets
            ├── ANIM  voir ci-dessous (44 ou 48 octets)
            └── KR00, KRT0 ou KRTS : images clés
```

Section ANIM :

| Offset | Taille | Champ |
| --- | --- | --- |
| 0 | 28 | nom de l'os, terminé par un zéro |
| 28 | 4 | nombre d'images clés |
| 32 | 4 | toujours 0 |
| 36 | 4 | index de la dernière image (= nombre − 1, dans les 5031 objets) |
| 40 | 4 | **variante de 44 octets** (4322 objets) : identifiant d'os HAnim, ou −1 |
| 40 | 8 | **variante de 48 octets** (709 objets, 32 animations) : deux index de chaînage |

Le wiki appelle les champs 36 et 40 « suivant » et « précédent » : c'est faux pour la variante de 44 octets. L'identifiant d'os est celui du HAnim PLG des DFF : dans 4280 objets sur 4322, il correspond à l'os du même nom dans `player.dff`. Les 42 autres valent −1 (orteils, accessoires comme `chainsaw`) : on les associe par le nom. Les deux variantes ne se mélangent jamais dans une même animation.

Images clés, little-endian, f32 :

| Section | Octets par image | Contenu |
| --- | --- | --- |
| KR00 | 20 | quaternion (x, y, z, w), temps |
| KRT0 | 32 | quaternion, translation (x, y, z), temps |
| KRTS | 44 | quaternion, translation, échelle (x, y, z), temps |

Le wiki suggère `(taille − 4) / 16` images pour KR00 : c'est faux, **chaque image a son propre temps**. Constats sur tous les IFP :

- tous les quaternions sont unitaires ;
- les temps sont en secondes, croissants, et commencent à 0. `run_player` contient 21 images à 30 images par seconde, soit 0,667 s ; l'animation la plus longue dure 12,2 s ;
- **le quaternion stocké est l'inverse de la rotation de l'os** par rapport à son parent : il faut prendre son conjugué (−x, −y, −z, w). Sur la première image de chaque animation, le conjugué est le plus proche de la pose de liaison de `player.dff` pour 2100 os, contre 162 pour le quaternion brut ; pour Pelvis, Neck et Head de `run_player`, l'écart tombe sous 10°.

## Repère et déplacement

Les animations s'appliquent au squelette debout le long de **+Z** (le monde du jeu), alors que la pose de liaison des DFF est debout le long de +Y. Dans `run_player`, la translation de l'os `Root` avance de 4,3 m le long de +Y pendant l'animation : c'est le déplacement du personnage. Pour une boucle sur place, il faut retirer cette avance.

## Hypothèses et inconnues

- HYPOTHÈSE : les deux entiers de la variante de 48 octets sont des liens entre objets (wiki). Inutilisés par Chaos FC.
- Le champ à l'offset 32 vaut toujours 0 : son sens est inconnu.
- `anim/cuts.img` (animations des cinématiques) n'est pas lu.

## Fichiers testés

Tests dans `crates/asset-bridge/tests/vice_city_files.rs`, ignorés si Vice City n'est pas configuré :

- `anim/ped.ifp` et les 28 IFP de `gta3.img` se lisent ; quaternions unitaires et temps croissants partout (`every_ifp_parses`) ;
- `run_player` : 22 os, 0,667 s ; chaque identifiant d'os désigne la frame du même nom dans `player.dff` ; convention des quaternions (`run_player_drives_tommy_skeleton`).
