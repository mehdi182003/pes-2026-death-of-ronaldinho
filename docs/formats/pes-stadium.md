# Stades de PES 6 (PC)

Un stade est un ensemble de modèles et de textures de `0_text.afs`, au-delà de la carte communautaire (qui s'arrête au n° 6939). Code : `asset_bridge::pes6::Pes6::load_scenery` ; voir aussi [pes-model.md](pes-model.md) et [pes-texture.md](pes-texture.md).

## Documentation publique

Aucune pour PES 6. Tout ce qui suit vient des fichiers du jeu complet PC.

## Fichiers

À partir du n° 6940, `0_text.afs` contient des groupes de 11 fichiers qui se répètent (n° 6941 à 6951, 6952 à 6962...) :

| Dans le groupe | Contenu |
| --- | --- |
| 1 à 4 | une tribune d'un bout du terrain, en quatre niveaux de détail (164 à 2519 sommets) |
| 5 à 8 | trois modèles chacun : l'autre bout et les deux côtés, en quatre niveaux de détail |
| 9 | le **stade complet** (n° 6949, 466 000 octets) : conteneur de textures, de modèles et de tables |
| 10, 11 | tables non étudiées |

HYPOTHÈSE : chaque groupe est un stade ou une variante (jour, nuit, pluie...) ; les tribunes séparées servent au loin ou aux ralentis.

### Le n° 6949, vérifié

| Sous-fichier | Contenu |
| --- | --- |
| `/0` | 12 emplacements, 9 textures : pelouse (0/0), éléments des lignes (0/1), bord de pelouse (0/2), terre (0/4), ombres (0/5, 0/7)... |
| `/1` | 46 emplacements, 30 textures : tribunes, toit, panneaux KONAMI, écran géant, photographes et staff (1/22), filet (1/43), masque de tonte (1/0)... |
| `/2` | 27 emplacements, 15 modèles : sol extérieur (n° 1), pelouse (4), masque de tonte (5), lignes, terre et ombres (6), tribunes (8 à 11), toit et structure (13), dôme (22), panneaux publicitaires (23 à 26) |
| `/3`, `/4` | tables non étudiées (`14 00 00 00`, `0c 00 00 00`...) |

Les emplacements vides ont la position 0 dans la table du conteneur (voir [afs.md](afs.md)).

## Textures des modèles

Un modèle désigne ses textures **par numéro** : sa table de textures (offset donné par le u32 à l'offset 12 du modèle) donne, pour chaque emplacement de l'instruction `02`, le numéro écrit à l'offset 12 de l'en-tête de la texture. Exemple : le modèle n° 6 a la table `0x2711 0x2714 0x2715 0x2717 0x2738 0x274e` ; son emplacement 0 prend la texture 0x2711, celle des lignes (0/1). Les 15 modèles du n° 6949 trouvent ainsi toutes leurs textures, sauf un emplacement (0x2752).

## Repère et échelle

- **Y vers le bas** (convention PlayStation 2) : les tribunes montent vers −Y. Le jeu les tourne de 180° autour de X (`retarget::PES6_STADIUM_TO_Y_UP`) ; le terrain reste le long de X.
- **51,3 unités par mètre** (`retarget::PES6_STADIUM_UNITS_PER_METRE`) : les lignes du n° 6949 sont à leurs distances réglementaires de la ligne de but (2693 unités, 52,5 m) : surface de réparation à 1848, surface de but à 2412, point de penalty à 2130, ligne de touche à 1746 (34 m). HYPOTHÈSE : la même échelle pour tous les stades.
- Les couleurs de sommet portent l'éclairage du stade (pleine plage 0 à 255) : le décor s'affiche sans lumière ajoutée.

## Transparence et ordre de dessin

- Les lignes sont blanches avec un alpha de 18 à 32 sur 128 (environ 25 % après doublement) : elles sont mélangées à la pelouse. Les ombres sont noires et peu opaques.
- Les couches du sol sont dans le même plan : elles se dessinent dans l'ordre du fichier (`Material::layer`, puis décalage de profondeur dans Bevy).
- HYPOTHÈSE : le masque de tonte (texture entièrement noire, alpha de 180 à 254, troué en grille) assombrit la pelouse à 20 % de son alpha. Pris tel quel, il noircit le terrain, ce que le jeu ne montre pas : sur PlayStation 2, il servait sans doute à alterner deux tons de gazon. À comparer à l'œil avec le jeu.

## Fichiers testés

`crates/asset-bridge/tests/pes6_files.rs` : `a_stadium_loads_with_the_textures_its_models_name` (39 textures, lignes mélangées au-dessus de la pelouse, terrain de 105 m).

Pour voir un stade :

```sh
cargo run -p asset-tools --bin viewer -- --pes-scenery 0_text:6949 --yaw 0 --zoom 0.22 --look-at 0,0,0
```
