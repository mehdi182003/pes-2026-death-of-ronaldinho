# IMG version 1 (GTA III, Vice City)

Archive principale de Vice City : `models/gta3.img` (les données) et `models/gta3.dir` (le répertoire). Code : `crates/formats-rw/src/img.rs`.

## Documentation publique

Wiki GTAMods, page « IMG archive » : le `.dir` n'a pas d'en-tête et contient une suite d'entrées de 32 octets ; le `.img` n'a pas de structure propre, il contient les fichiers bout à bout, alignés sur des secteurs de 2048 octets. La version 2 (San Andreas) fusionne les deux fichiers et ne nous concerne pas.

## Structure vérifiée

Entrée du `.dir`, 32 octets, little-endian :

| Offset | Taille | Type | Champ |
| --- | --- | --- | --- |
| 0 | 4 | u32 | Position du fichier dans le `.img`, en secteurs de 2048 octets |
| 4 | 4 | u32 | Taille du fichier, en secteurs |
| 8 | 24 | char[24] | Nom, terminé par un octet nul |

Constats sur `gta3.dir` / `gta3.img` de Vice City PC (copie de Mokhmad, 1er octobre 2026) :

- 6043 entrées ; la taille du `.dir` (193 376 octets) est exactement 6043 × 32. Pas d'en-tête.
- Les entrées sont rangées par position croissante, sans trou ni chevauchement ; la dernière se termine exactement à la fin du `.img` (159 906 secteurs).
- Les noms font au plus 23 caractères, tous ASCII. **Après l'octet nul, le champ contient souvent des octets parasites** (3964 entrées) : il faut couper au premier nul.
- **La casse varie** (`player.dff`, mais aussi `Pga.dff`, `Generic.txd`) : la recherche par nom ignore la casse.
- **11 noms apparaissent deux fois**, avec des contenus différents (`chef.dff`, `Pga.dff`, `Pgb.dff`, `camera.dff`, `Generic.txd`…).
- Contenu : 4617 `.dff`, 1368 `.txd`, 30 `.col`, 28 `.ifp` (animations d'armes et d'activités ; `ped.ifp` n'y est pas, il est dans `anim/`).
- La taille en secteurs inclut le remplissage final : un DFF se termine plus tôt, sa vraie taille est donnée par son chunk racine (voir la future page RenderWare).

## Hypothèses et inconnues

- HYPOTHÈSE : pour un nom en double, on prend la **première** entrée. On ne sait pas laquelle le jeu charge ; à vérifier en jeu si un de ces fichiers sert un jour.

## Fichiers testés

- `models/gta3.dir` / `gta3.img` en entier (test `gta3_img_directory_is_consistent` dans `crates/asset-bridge/tests/vice_city_files.rs`, ignoré si Vice City n'est pas configuré).
- Extraction de `player.dff` (88 064 octets, 43 secteurs).
