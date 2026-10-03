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
- **Prochaine étape :** Mehdi installe le mod et lance PES 6 (critère de M1). En parallèle : import de PES6.exe dans Ghidra.
