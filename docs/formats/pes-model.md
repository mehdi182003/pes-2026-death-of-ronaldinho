# Modèles 3D de PES 6 (PC)

Signature `20 05 04 20`. Un modèle se compose de parties de sommets, d'une bande de triangles et d'un petit programme de dessin. Code : `crates/formats-pes/src/model.rs` ; conversion vers les types neutres : `crates/asset-bridge/src/pes6.rs`.

## Documentation publique

Aucune description trouvée pour PES 6 (les outils publics visent les PES récents, au format `WESYS`). Tout ce qui suit vient des 762 modèles de la démo PC : 751 se lisent en entier, avec les vérifications ci-dessous.

## En-tête

u32 little-endian à partir de l'offset 4 :

| Offset | Champ |
| --- | --- |
| 4 | fin du programme de dessin |
| 8, 12 | inconnus |
| 16 | section des sommets |
| 20 | bande d'indices (début) |
| 24 | début du programme : toujours `0x48` |
| 28 | bande d'indices (fin) |
| 32 à 40 | inconnus |

## Sommets

La section commence par le nombre de parties (u32), puis une position par partie (u32, depuis le début de la section). Chaque partie : nombre de sommets (u16), taille d'un sommet (u8), drapeaux (u8), 4 octets inutilisés, puis les sommets.

Les 4 bits bas des drapeaux donnent le **nombre d'os par sommet** (0 à 4). Un sommet se lit dans cet ordre, chaque champ n'étant présent que si la taille le permet :

| Champ | Taille | Présent si |
| --- | --- | --- |
| position | 3 × f32 | toujours |
| poids des os | 4 × u8 (somme 255) | 2 à 4 os |
| numéros des os | 4 × u8 | 1 à 4 os |
| normale | 3 × f32 | il reste 12 ou 16 octets |
| couleur | 4 × u8 | il reste 4 ou 16 octets |
| coordonnées de texture | 2 × f32 | toujours |

Formats vus sur la démo (taille, drapeaux) : (32, 0) 512 parties, (40, 2) 212, (40, 3) 206, (24, 0) 139, (36, 1) 123, (40, 4) 88, (24, 0x20) 30, (32, 0x20) 24, (28, 1) 11, (36, 0x20) 5. HYPOTHÈSE : le bit 0x20 (parties de stade) ne change pas l'ordre des champs.

Les normales n'ont pas toujours une longueur de 1 (8 dans les visages) : la conversion les ramène à 1.

## Bande d'indices

Une suite de blocs : un nombre (u16) puis autant d'indices (u16), jusqu'à un nombre nul ou la fin de la section. Les indices sont **locaux à la partie** dessinée. Les dessins se raccordent par des triangles dégénérés (indices répétés).

## Programme de dessin

Instructions alignées sur 2 octets ; le premier octet est le code :

| Code | Taille | Sens |
| --- | --- | --- |
| `00` | 2 | rien (remplissage) |
| `01`, `0d` | 4 | inconnus, un argument u16 |
| `02` | 4 | emplacement de texture des dessins suivants (0 à 10 sur un joueur) |
| `03` | 3 + n, aligné | une table de n octets (`03 00 n` puis les octets) ; sens inconnu, peut-être les os utilisés |
| `04` | 4 | partie de sommets des dessins suivants (`04 drapeaux u16`, les drapeaux reprenant ceux de la partie) |
| `07` | 10 | dessin : `07 mode`, puis nombre d'indices, premier sommet, nombre de sommets, nombre de triangles |
| `0a` | 4 | os des dessins suivants |

Vérifications faites à chaque lecture, vraies sur les 751 modèles : les indices d'un dessin restent dans ses sommets ; le nombre de triangles non dégénérés est celui annoncé ; toute la bande est utilisée. Les modes `05`, `06` et `07` (bits bas ; les bits hauts suivent le nombre d'os) sont tous des bandes de triangles.

11 modèles utilisent les codes `08` ou `11`, encore inconnus : ils sont refusés avec une erreur.

## Squelette

Juste après le programme de dessin (offset donné par le u32 à l'offset 4) : un nombre d'os (u32), puis par os 6 × f32 (trois angles en radians, autour de X, Y et Z, puis une translation), puis les parents (i16 par os, −1 pour la racine). Vérifié sur les 762 modèles : 700 n'ont pas d'os ; les 26 corps de joueurs et d'arbitres en ont 19, avec les parents `−1, 0, 1, 1, 2, 3, 0, 6, 6, 7, 8, 6, 4, 5, 9, 10, 11, 14, 15`.

Chaque os donne le passage de l'espace du modèle à celui de l'os : un point `p` du modèle est en `R·p + t` pour l'os. L'articulation est donc en `−Rᵀ·t`.

HYPOTHÈSE : `R = Ry·Rz·Rx`. Avec cet ordre, les articulations des bras (épaules à ±80,8, coudes à ±205,8, poignets à ±297,3, toutes à 602,7 de haut) et de la tête (670,6) tombent où le maillage les attend, et une tête se fixe à l'endroit. Celles des jambes et du dos ne tombent pas encore juste, quel que soit l'ordre : à reprendre au jalon J6.

## Repère et échelle

- Y vers le haut, pieds à Y = 0 ; les bras d'un personnage en T s'étendent le long de X.
- HYPOTHÈSE : 420 unités par mètre (`retarget::PES6_UNITS_PER_METRE`), pour que l'arbitre (756 unités, tête comprise) mesure 1,80 m. Un corps sans tête fait environ 690 unités (1,64 m) et une tête environ 90 (21 cm) : cohérent. Le ballon (11 unités de diamètre) est mis à l'échelle par le jeu et ne sert pas de référence.
- HYPOTHÈSE : le premier triangle d'une bande est dans le sens direct (à vérifier à l'œil : faces arrière masquées).

## Modèles repérés (démo)

| Numéro | Contenu |
| --- | --- |
| 431 | arbitre complet (corps et tête), 1796 sommets ; texture n° 432 (512 × 256), variantes 433 et 434 |
| 1064, 1104, 1105, 1108, 1110, 1111 | corps de joueur en T, sans tête, 19 os, 39 parties, 5 niveaux de détail. Les maillots de la démo sont chiffrés ; la chasuble d'entraînement 296/1/0 (512 × 256 : chasuble, short, chaussettes, chaussures) s'y applique |
| 1943 et voisins | tête : 2 modèles (tête, cheveux ?) et une texture de visage ; se place sur l'os 16 du corps |
| 417 à 419 | présentatrice des menus (veste, visage, cheveux) |

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` : `models_parse_with_consistent_draws` (tous les modèles de `0_text.afs`) et `referee_loads_textured_at_a_plausible_size`.

Test `player_body_gets_its_head_on_the_shoulders` : la tête se place au-dessus du cou.

Pour voir un joueur à côté de Tommy :

```sh
cargo run -p asset-tools --bin viewer -- player --pes 0_text:1064 --pes-texture 0_text:296/1/0 --pes-head 0_text:1943
```
