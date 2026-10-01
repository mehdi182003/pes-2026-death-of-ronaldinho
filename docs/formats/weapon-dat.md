# weapon.dat (paramètres des armes de Vice City)

Fichier texte `data/weapon.dat` : une ligne par arme. Code : `crates/formats-rw/src/weapon_dat.rs`.

## Documentation publique

Wiki GTAMods, page « weapon.dat », tableau Vice City. Le fichier lui-même commence par un commentaire qui décrit les colonnes.

## Structure vérifiée

- Champs séparés par des espaces ou des tabulations (mélangés dans le vrai fichier).
- `#` commence un commentaire, jusqu'à la fin de la ligne ; les lignes vides sont ignorées.
- La lecture s'arrête à `ENDWEAPONDATA`.
- **37 armes, chacune sur exactement 26 champs** :

| N° | Champ | Exemple (Colt45) |
| --- | --- | --- |
| 1 | nom (liste fixe du jeu) | `Colt45` |
| 2 | type de tir | `INSTANT_HIT` |
| 3 | portée (m) | 30.0 |
| 4 | « firing rate » (sens non documenté) | 250 |
| 5 | rechargement (ms) | 450 |
| 6 | munitions par chargeur | 17 |
| 7 | dégâts | 25 |
| 8 – 11 | vitesse, rayon, durée de vie, dispersion | −1.0 … |
| 12 – 14 | décalage de la flamme du canon (x, y, z) | 0.30 0.0 0.09 |
| 15 | groupe d'animation (IFP de `gta3.img`) | `colt45` |
| 16 – 18 | animation de tir : début de boucle, fin de boucle, instant du tir | 11 18 14 |
| 19 – 21 | idem, accroupi | 11 18 12 |
| 22 | instant où l'on peut interrompre l'attaque | 99 |
| 23 – 24 | modèle de l'arme et d'un accessoire (identifiants IDE) | 274 −1 |
| 25 | drapeaux, en hexadécimal | `680C0` |
| 26 | emplacement d'arme (3 : pistolet) | 3 |

## Hypothèses et inconnues

- HYPOTHÈSE : les instants des colonnes 16 à 22 sont des images à 30 par seconde. Pour le Colt 45, 11, 18 et 14 tombent dans `colt45_fire` (images clés espacées de 1/30 s, durée 0,9 s). À valider à l'œil : le coup doit partir quand le bras finit son recul.
- Le décalage du canon est exprimé dans le repère de l'arme tenue en main, canon le long de +X : `colt45.dff` s'étend de x = 0 à 0,24 m et de z = −0,055 à 0,128, le point (0,30 ; 0 ; 0,09) tombe juste devant le bout du canon. **Confirmé à l'œil par Mokhmad** dans le jeu (la flamme et la traçante partent du canon).
- Le sens exact de « firing rate » (250 pour presque toutes les armes) n'est pas documenté.
- Les identifiants de modèle renvoient aux fichiers IDE, pas encore lus : pour l'instant, le modèle d'une arme est désigné par son nom (`colt45.dff`).

## Fichiers testés

Test `weapon_dat_describes_the_colt45` dans `crates/asset-bridge/tests/vice_city_files.rs` (ignoré si Vice City n'est pas configuré) : le fichier se lit, la ligne du Colt 45 a les valeurs attendues et son groupe d'animation désigne `colt45.ifp`.
