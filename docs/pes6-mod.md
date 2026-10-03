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
- **Prochaine étape :** Mehdi lance PES 6 avec le mod (critère de M2a) ; le journal dira si PES utilise `SetTransform` (pipeline fixe) ou des vertex shaders pour ses matrices, ce qui décide de M2b.
