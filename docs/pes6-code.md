# Code de PES6.exe — carte du code décompilé

Jalon R1 de la route « tout en Rust » (`docs/BRIEF.md`). Le code décompilé lui-même reste hors du dépôt : projet Ghidra `~/pes6-decomp/PES6.gpr`, export complet dans `~/pes6-decomp/export` (`functions.tsv` : adresse, nom, taille, appelants, appelés, chaînes ; `decomp_NNNN.c` : 500 fonctions par fichier), produit par `~/pes6-decomp/scripts/ExportAll.java`. Adresses absolues (PES6.exe se charge toujours à `0x400000`).

Une partie du code saute dans les sections de SecuROM (`.rld`, adresses `0x044xxxxx`, par exemple les sauts finaux `(*_DAT_044600d8)()`) : on ne la décompile pas au travers de la protection ; ce qu'elle fait est reproduit d'après les enregistrements du mod (`pes6_mod::record`).

## Données en mémoire

| Adresse | Contenu | Source |
| --- | --- | --- |
| `0x00BCCE94` | Pointeur vers la structure du ballon (`PTR_DAT_00bcce94`) | FUN_005a2a7b, FUN_00478020, FUN_004a3020 |
| `0x03BDC980` | Tableau de 23 joueurs de 0x240 octets (0 : l'arbitre ; 1–11 : équipe 0 ; 12–22 : équipe 1) | FUN_005a2a7b |
| `0x03BCF5A8` | Caractéristiques des joueurs, `(équipe * 0x20 + numéro) * 0x348` | FUN_005a2a7b |
| `0x00B8B874` | Table de coefficients par type de frappe (0x28 octets par entrée), dont un amortissement | FUN_004a3020 |
| `0x03BE12AC` | Facteur d'échelle appliqué aux vitesses au lancement du ballon | FUN_004a3020 |

### Ballon (`*0x00BCCE94`)

| Champ | Type | Sens | Source |
| --- | --- | --- | --- |
| `+0x20`, `+0x24`, `+0x28` | 3 × f32 | Position (unités logiques, 256,5 par mètre, Y vers le bas) | mod, M3 |
| `+0x30` | f32 | Vitesse verticale au lancement | FUN_004a3020 |
| `+0x50` | f32 (écrit par un appel) | Vitesse horizontale au lancement | FUN_004a3020, FUN_00478020 |
| `+0x58` | u16 | Direction au sol (angle sur 16 bits) | FUN_004a3020, FUN_00478020 |

### Joueur (`0x03BDC980 + i * 0x240`)

| Champ | Type | Sens | Source |
| --- | --- | --- | --- |
| `+0x00` | u8 | Identifiant | FUN_00478020 |
| `+0x11` | u8 | Numéro dans l'équipe | FUN_005a2a7b |
| `+0x12` | u8 | Équipe (0 ou 1) | FUN_00478020 |
| `+0xE0`, `+0xE4`, `+0xE8` | 3 × f32 | Position | FUN_00478020, FUN_005a2a7b |
| `+0xF0`, `+0xF8` | f32 | Vitesse en X et en Z | FUN_005a2a7b |

## Fonctions

| Adresse | Rôle | Notes |
| --- | --- | --- |
| `0x005A2A7B` | Calcul d'une passe (choix du receveur, puissance) | Lit les positions et vitesses des joueurs, la position visée du ballon |
| `0x00478020` | Donne le ballon à un joueur (dernier toucher) | Copie identifiant et position du joueur ; écrit `+0x50`, `+0x58` du ballon |
| `0x004A3020` | Lancement du ballon depuis une structure de frappe | Boucle « vitesse += accélération » amortie par `0x00B8B874[type]` ; écrit `+0x30`, `+0x50`, `+0x58` |

## À faire (R3)

- Trouver la mise à jour du ballon à chaque image : gravité, frottement au sol, rebond, effet. Point de départ : les fonctions qui lisent `+0x30`, `+0x50`, `+0x58` du ballon et écrivent `+0x20..+0x28`.
- Rejouer l'enregistrement `chaos-fc-record-1791039364.cfrc` (12 953 images) pour valider le portage en Rust.
