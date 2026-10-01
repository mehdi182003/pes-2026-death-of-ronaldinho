# Modèles 3D de PES 6 (PC)

Signature `20 05 04 20`. Un modèle se compose de parties de sommets, d'une bande de triangles et d'un petit programme de dessin. Code : `crates/formats-pes/src/model.rs` ; conversion vers les types neutres : `crates/asset-bridge/src/pes6.rs`.

## Documentation publique

Aucune description trouvée pour PES 6 (les outils publics visent les PES récents, au format `WESYS`). Tout ce qui suit vient des 9647 modèles du jeu complet PC : 9546 se lisent en entier, avec les vérifications ci-dessous.

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

Formats vus dans le jeu (taille, drapeaux : nombre de parties) : (40, 2) 6028, (40, 3) 5304, (32, 0) 4397, (36, 1) 3250, (24, 0) 2502, (40, 4) 2110, (24, 0x20) 1868, (32, 0x20) 54, (28, 1) 40, (36, 0x20) 5, (44, 2) 3, (44, 4) 2. HYPOTHÈSE : le bit 0x20 (parties de stade) ne change pas l'ordre des champs.

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

Juste après le programme de dessin (offset donné par le u32 à l'offset 4) : un nombre d'os (u32), puis par os 6 × f32 (trois angles en radians, autour de X, Y et Z, puis une translation), puis les parents (i16 par os, −1 pour la racine). Vérifié sur les modèles du jeu complet : la plupart n'ont pas d'os ; les 594 corps de joueurs et d'arbitres en ont 19, avec les parents `−1, 0, 1, 1, 2, 3, 0, 6, 6, 7, 8, 6, 4, 5, 9, 10, 11, 14, 15`.

Chaque os donne le passage de l'espace du modèle à celui de l'os : un point `p` du modèle est en `R·p + t` pour l'os. L'articulation est donc en `−Rᵀ·t`.

HYPOTHÈSE : `R = Rz·Ry·Rx` (X d'abord). Avec cet ordre, les articulations des bras (épaules à ±80,8, coudes à ±205,8, poignets à ±297,3, toutes à 602,7 de haut) et de la tête (670,6) tombent où le maillage les attend, et une tête posée sur l'os 16 regarde du côté où pointent les orteils (+Z). L'ordre `Ry·Rz·Rx`, essayé d'abord, place aussi bien les articulations mais tourne la tête vers l'arrière (constaté à l'œil, puis vérifié par le test `player_body_gets_its_head_on_the_shoulders`). Celles des jambes et du dos ne tombent pas encore juste, quel que soit l'ordre : à reprendre au jalon J6.

## Emplacements de texture d'un corps de joueur

L'instruction `02` du programme de dessin choisit l'emplacement de texture. **Les numéros changent d'une famille de corps à l'autre** (au moins huit numérotations parmi les 581 corps à 19 os) : le rôle de chaque emplacement se déduit de sa géométrie (`asset_bridge::pes6::PlayerSlot::classify`), dans cet ordre :

| Règle | Rôle |
| --- | --- |
| tous les sommets sous 70 unités | chaussures |
| tous les sommets au bout des bras (\|x\| > 300) | peau (mains) |
| moins de 60 sommets | marquage (numéro, nom, écusson) : pas encore affiché |
| toutes les coordonnées u au bord droit (u > 0,9) | tenue (col, poignets) |
| grande zone : celle qui a le plus de sommets au milieu du torse | tenue |
| autre grande zone qui atteint les bras (\|x\| > 200) ou les jambes (sous 300) | peau |
| autre grande zone, cantonnée au tronc | marquage (les numéros du corps n° 1010) |

Vérifié sur les corps 995, 1010 et 1064 (test `body_slots_get_their_role_from_the_geometry`), et à l'œil sur le n° 1010. HYPOTHÈSE pour les autres corps.

**Familles de corps** : avec une même tenue, tous les corps ne donnent pas un joueur. Le n° 1064 est un corps de staff (seuls les survêtements et costumes y sont symétriques), le n° 995 porte un pantalon long (gardien). Le n° 1010 est un joueur de champ : avec la tenue n° 419, deux manches jaunes, un short bleu et des chaussettes jaunes.

## Repère et échelle

- Y vers le haut, pieds à Y = 0 ; les bras d'un personnage en T s'étendent le long de X.
- HYPOTHÈSE : 420 unités par mètre (`retarget::PES6_UNITS_PER_METRE`), pour qu'un joueur mesure environ 1,80 m : le corps n° 1064 va de −18 (semelles) à 673,8 unités (cou), la tête posée monte jusqu'à environ 750 unités, soit 768 unités (1,83 m) ; une tête fait environ 90 unités (21 cm). Le ballon (11 unités de diamètre) est mis à l'échelle par le jeu et ne sert pas de référence.
- HYPOTHÈSE : le premier triangle d'une bande est dans le sens direct (à vérifier à l'œil : faces arrière masquées).

## Modèles repérés

| Numéro | Contenu |
| --- | --- |
| 995 à 1116 environ | corps en T, sans tête, 19 os, plusieurs niveaux de détail : 1010 joueur de champ, 995 gardien (pantalon long), 1064 staff |
| 288 à 297 | accessoires du terrain d'entraînement : 17 petits modèles et 24 textures (pas des joueurs) |
| 409 à 434 | textures de tenues lisibles, 512 × 256 : maillot, short et chaussettes (419 : jaune et bleu ; 420 : rouge et bleu ; 421 : bleu et blanc ; 426 : blanc...) |
| 5322 et voisins (chaussures) | 5322/0/0 : chaussure (tige et semelle), 256 × 256 |
| 1891 à 2937 (visages) | tête : 2 modèles (la même tête en deux niveaux de détail) et une texture de visage ; se place sur l'os 16 du corps |
| 4448 à 5316 (coiffures) | un modèle de cheveux et deux textures (la seconde plus petite), dans le repère de la tête |

Les grandes textures de maillots sont chiffrées (voir [afs.md](afs.md)).

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` : `models_parse_with_consistent_draws` (tous les modèles de `0_text.afs`) et `player_body_loads_textured_at_a_plausible_size`.

Test `player_body_gets_its_head_on_the_shoulders` : la tête se place au-dessus du cou.

Pour voir un joueur à côté de Tommy :

```sh
cargo run -p asset-tools --bin viewer -- player --pes 0_text:1010 --pes-texture 0_text:419 --pes-boots 0_text:5322/0/0 --pes-head 0_text:1943 --pes-hair 0_text:4570
```
