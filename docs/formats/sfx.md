# SFX (banque de sons de Vice City, PC)

Les bruitages de Vice City (armes, pas, véhicules, voix des passants) sont dans une banque : `Audio/sfx.SDT` (la table) et `Audio/sfx.RAW` (les sons). Les musiques des radios (`.adf`) et les ambiances (`.mp3`, `.wav`) sont des fichiers à part. Code : `crates/formats-rw/src/sfx.rs`.

## Documentation publique

Wiki GTAMods, page « SFX » : format de la table pour GTA 2, GTA III et Vice City (PS2 et PC), en-tête WAV à ajouter pour écouter un son extrait, et **liste des sons de Vice City** par numéro.

## Structure vérifiée

`sfx.SDT` : une suite d'entrées de **20 octets**, sans en-tête, little-endian :

| Offset | Type | Champ |
| --- | --- | --- |
| 0 | u32 | position du son dans `sfx.RAW`, en octets |
| 4 | u32 | taille du son, en octets |
| 8 | u32 | fréquence d'échantillonnage (Hz) |
| 12 | i32 | début de boucle (0 pour presque tous) |
| 16 | i32 | fin de boucle (−1 : fin du son) |

`sfx.RAW` : les sons bout à bout, en **PCM 16 bits signé, mono**.

Constats sur la copie de Mokhmad :

- 9941 sons ; la taille de la table (198 820 octets) vaut exactement 9941 × 20 ;
- les sons se suivent sans trou, le premier commence à 0 et le dernier finit exactement à la fin de `sfx.RAW` (340 245 502 octets) ;
- toutes les tailles sont paires (échantillons de 2 octets) ;
- fréquences de 2000 à 44 100 Hz, surtout 12 000 et 16 000 Hz ;
- 133 sons ont une boucle définie (début non nul ou fin différente de −1), les autres se jouent une fois.

## Sons des armes

D'après la liste du wiki (à confirmer à l'oreille) :

| Sons | Description |
| --- | --- |
| 50 – 51 | tir de pistolet (Colt 45) |
| 54 – 55 | tir d'Uzi |
| 74 – 75 | tir de M4 / Ruger |
| 77 | rechargement du pistolet |
| 156 – 157 | douille qui tombe |

Le jeu associe arme et son dans son code, qu'on ne décompile pas : la confirmation se fait à l'oreille. **Son 50 = tir du Colt 45 : confirmé par Mokhmad.** HYPOTHÈSE pour les autres (54 pour l'Uzi, 74 pour le Ruger), à confirmer de la même façon.

## Outil

```sh
cargo run -p asset-tools -- sfx list --from 48 --count 10
cargo run -p asset-tools -- sfx export 50 51     # WAV dans %LOCALAPPDATA%\chaos-fc\extracted\vice_city\sfx\
```

## Fichiers testés

Test `sound_bank_is_consistent` dans `crates/asset-bridge/tests/vice_city_files.rs` (ignoré si Vice City n'est pas configuré) : table et fichier RAW cohérents, lecture des sons 50 et 51.
