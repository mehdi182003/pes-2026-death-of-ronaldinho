# Mod du vrai PES 6 — plan et journal

Plan de modding au sens du skill `game-recon` (universal-modder), et journal au sens de `MODLOG.md`. Contexte et jalons : `docs/BRIEF.md`, section « Route mod du vrai PES 6 ».

## Plan

- **Installation :** `C:\Users\mehdi\OneDrive\Documents\KONAMI` (clé `pes6` de `config.toml`). PES6.exe : 21 880 832 octets, horodatage PE 2006-09-09 11:32:42.
- **Moteur :** propriétaire Konami, natif x86 32 bits, Direct3D 9, DirectInput 8.
- **Anti-triche / en ligne :** aucun anti-triche. Protection SecuROM : on ne la contourne pas, pas de débogueur attaché.
- **Sauvegardes :** `KONAMI\Pro Evolution Soccer 6\save` ; réglages : `settings.dat` à côté. Sauvegarde faite : `~/.universal-modder/backups/pes6-saves/20261003-144812.zip` (restauration : `um backup restore`).
- **Route :** proxy `dinput8.dll` (crate `pes6-mod`) puis accroches de fonctions. Pas de chargeur communautaire utilisé pour l'instant.
- **Outils (hors dépôt) :** Ghidra 12.1.4 dans `~/tools`, JDK Temurin 21, x64dbg. Projets Ghidra et sorties de décompilation dans `~/pes6-decomp`, jamais dans le dépôt.

## Utilisation

```sh
cargo build -p pes6-mod --release --target i686-pc-windows-msvc
cargo run -p asset-tools -- mod install      # copie dinput8.dll à côté de PES6.exe
cargo run -p asset-tools -- mod uninstall    # le retire
```

L'installation refuse de remplacer un `dinput8.dll` qui ne vient pas de Chaos FC.

## Journal

- **3 octobre 2026 (M1).** `um scan` : moteur inconnu, x86, aucun anti-triche. Sections de PES6.exe : code à `0x1000` (entropie 6,8, prologues de fonctions lisibles, point d'entrée `0x1a3ce` dans cette section), puis `.rdata`, `.data`, `.data1`, `.rsrc`, et les sections de SecuROM `age`, `agis`, `quod`, `.rld`. Chaînes `SecuROM`, `paul.dll`, `Direct3DCreate9`, `DirectInput8Create`.
- Sur i686, une fonction `extern "system"` est exportée `_DirectInput8Create@20` : `dinput8.def` l'exporte sous son nom simple (vérifié avec `dumpbin /exports`).
- **M1 validé** par Mehdi : match joué, commandes normales, journal complet.
- **M2a.** PES6.exe importe `d3d8.dll`, pas `d3d9.dll` (table d'imports lue sur le fichier ; tables de noms séparées présentes pour chaque DLL). Le mod remplace l'emplacement de `Direct3DCreate8`, puis les entrées de vtable `IDirect3D8::CreateDevice` (15) et `IDirect3DDevice8::Present` (15), `Reset` (14), `BeginScene` (34), `SetTransform` (37), `CreateVertexShader` (75), `SetVertexShaderConstant` (79). Indices et constantes vérifiés dans `d3d8.h` et `d3d8types.h` de Wine (même disposition que les en-têtes de Microsoft). Le bandeau est dessiné dans `Present`, dans sa propre scène, avec sauvegarde et restauration de tout l'état (bloc d'état `D3DSBT_ALL`).
- Ghidra : analyse complète de PES6.exe terminée (`~/pes6-decomp/PES6.gpr`).
- **M2a validé** par Mehdi. Journal : `Direct3DCreate8(220)`, `CreateDevice(adaptateur 0, type 1 (HAL), drapeaux 0x80, 640x480)`, deux `Reset` au démarrage ; par image 1 scène, 1 `VIEW`, 1 `PROJECTION`, 1 `WORLD`, 2 appels `SetVertexShaderConstant` et 0 vertex shader : pipeline fixe. Caméra relevée : à 2236 unités de l'origine, plongée d'environ 63°, qu'elle vise exactement (vérifié par un test de `scene`).
- **M2b.** La caméra du terrain est celle qui dessine le plus de primitives dans l'image (accroches de `DrawPrimitive`, `DrawIndexedPrimitive` et de leurs variantes `UP`). Le mod dessine avec elle, dans `Present`, un repère à l'origine : croix jaune de 10 m au sol, poteau rouge côté +Y, poteau bleu côté −Y (HYPOTHÈSE : 51,3 unités par mètre, l'échelle des stades), sans test de profondeur. Le journal donne toutes les 10 s les images par seconde, le nombre de caméras et la caméra principale.
- **Essai M2b (Mehdi) :** le repère est bien au centre du terrain, mais « par-dessus » le jeu (pas de test de profondeur). Journal : pendant le match, une seule caméra à 60 images/s (12 000 à 15 000 primitives), caméras de présentation à 30 images/s avant. Dans les caméras du match, le +Y du monde finit en bas de l'écran : **le monde de PES est Y vers le bas**, comme ses fichiers de stade (HYPOTHÈSE à confirmer : le poteau bleu doit monter vers le ciel).
- **Correction :** test de profondeur contre le tampon de profondeur de PES (`ZENABLE`, `LESSEQUAL`, `ZBIAS` 2), croix posée au-dessus du gazon ; le bandeau 2D reste toujours devant.
- **Essai (Mehdi) :** le repère apparaissait aussi sur l'écran titre (PES y pose une caméra 3D pour son fond animé, 8 à 10 primitives). Capture : le poteau **bleu** monte et le rouge descend, ce qui confirme le monde Y vers le bas. Correction : le repère n'est dessiné que si la caméra dessine au moins 5 000 primitives (HYPOTHÈSE, en attendant l'état du match lu en mémoire, M3).
