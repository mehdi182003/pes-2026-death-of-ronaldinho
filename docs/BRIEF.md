# Chaos FC — Brief projet pour Claude Code

Oct 1, 2026 · @Mokhmad

## Vision

Chaos FC est un match de football façon PES 6 dans lequel Tommy Vercetti (GTA Vice City) débarque armé, en électron libre, et peut abattre n'importe quel joueur pendant que le match continue. Le projet est écrit en Rust et développé intégralement en vibe coding avec Claude Code.

Le modèle technique est celui du mashup MW2 × Minecraft × Skate 3 de Chasm : on réécrit les moteurs et la logique, et on charge les assets originaux depuis les copies des jeux que possède le joueur. Rien n'est distribué.

Ce document est le brief de référence. Claude Code le relit avant chaque jalon et le met à jour quand une décision change.

## Contraintes légales (non négociables)

Aucun fichier issu de GTA Vice City ou de PES ne doit jamais entrer dans le dépôt Git. Le jeu lit tout depuis les installations du joueur.

1. **Aucun asset dans le dépôt.** Modèles, textures, animations, sons, fichiers extraits et sorties de décompilation restent hors du repo. Le dossier de cache d'extraction est dans le `.gitignore` dès le premier commit.
2. **Chemins configurables.** Le joueur indique l'emplacement de ses copies de Vice City et de PES 6 dans un fichier de configuration (`config.toml`). Le jeu refuse de démarrer avec un message clair si un chemin manque.
3. **Références open source autorisées** pour comprendre les formats : la documentation du wiki GTAMods, librw, OpenRW, les outils de la scène de modding PES. Respecter leurs licences si du code est repris.
4. **Tests sans assets commités.** Les tests qui lisent de vrais fichiers sont ignorés automatiquement si les chemins des jeux ne sont pas configurés.

**Décision de Mehdi (3 octobre 2026) :** les anciennes règles « pas de code d'origine (reVC, re3, code de GTA V qui a fuité) » et « pas de décompilation des exécutables » sont supprimées, ainsi que l'interdiction de déchiffrer les fichiers chiffrés de PES. Objectif : retrouver la physique, les modèles et la jouabilité réelles des deux jeux par reverse engineering complet (skills universal-modder : `reverse-engineering`, `mashup-mods`). Cette décision s'applique au fork de Mehdi (`mehdi182003/pes-2026-death-of-ronaldinho`), pas au dépôt de Mokhmad (`MokhmadGUIRIEV/chaos-fc`). Risque connu : Take-Two a poursuivi les auteurs de re3 et reVC ; les sorties de décompilation restent hors du dépôt (règle 1).

**Mise en œuvre (J0) :**

- Le `.gitignore` bloque `config.toml`, les dossiers de cache locaux et les extensions de fichiers des jeux (`.img`, `.dir`, `.dff`, `.txd`, `.ifp`, `.col`, `.sfx`, `.sdt`, `.raw`, `.adf`, `.afs`, `.bin`, `.str`, `.dump`, et depuis J4 `.adx`, `.mdl`, `.tex`). Un job de CI échoue si un fichier portant l'une de ces extensions est suivi par Git.
- `config.toml` est cherché dans le dossier courant, ou à l'emplacement donné par la variable d'environnement `CHAOS_FC_CONFIG`. Section `[paths]`, clés `vice_city` et `pes6`. Un chemin relatif part du dossier du fichier de configuration. Modèle : `config.example.toml`.
- Un dossier n'est accepté que s'il ressemble à l'installation attendue. Vice City : `models/gta3.img` et `models/gta3.dir` présents. PES 6 : au moins une archive `.afs` dans un sous-dossier `dat` (HYPOTHÈSE à confirmer sur la copie de Mokhmad). Toutes les erreurs sont listées d'un coup, avec la clé fautive.
- `cargo run -p asset-tools -- check-config` vérifie les chemins sans compiler Bevy.

## Règles du jeu

Le match continue quoi qu'il arrive, Tommy tire sans limite de munitions, et seul l'arbitre tente de l'arrêter en le plaquant.

| Règle | Décision |
| --- | --- |
| Joueur abattu | Reste au sol jusqu'à la fin du match, en ragdoll. Son corps est un obstacle physique pour le ballon et les joueurs. |
| Arbitre | Réagit aux tirs et poursuit Tommy pour le plaquer au sol. |
| Déroulement | Le match continue normalement, quel que soit le nombre de joueurs encore debout. |
| Munitions | Illimitées. |
| Tommy | Électron libre : il n'appartient à aucune équipe et peut toucher le ballon comme tirer sur tout le monde. |

### Questions ouvertes

- [ ] L'arbitre est-il invincible, ou remplacé par les assistants puis les stadiers s'il est abattu ?
- [ ] Que se passe-t-il quand l'arbitre réussit un plaquage : Tommy au sol quelques secondes, désarmé, ou autre ?
- [ ] Quel est l'objectif de Tommy : score de chaos, survie, marquer dans les deux buts ?
- [ ] Que se passe-t-il quand une équipe n'a plus aucun joueur debout ?
- [ ] Durée d'un match en temps réel (proposition : 10 minutes pour 90 minutes de jeu).

Tant qu'une question est ouverte, Claude Code implémente la version la plus simple et la rend configurable.

## Stack technique

Tout est en Rust stable : Bevy pour le moteur, Rapier pour la physique, binrw pour lire les formats binaires.

| Besoin | Choix | Pourquoi |
| --- | --- | --- |
| Moteur de jeu | Bevy (ECS) | Rendu 3D, meshes skinnés, animation squelettique, audio et entrées intégrés. |
| Physique | bevy\_rapier3d | Ballon, collisions, ragdolls des joueurs abattus. |
| Lecture des formats binaires | binrw | Structures binaires décrites de façon déclarative, faciles à corriger quand une hypothèse sur un format est fausse. |
| Décompression des textures DXT | texpresso, ou une autre crate BCn | Les textures de Vice City sont souvent compressées en DXT1/DXT3. |
| Configuration | serde + toml | Chemins des jeux, options de règles. |
| Erreurs | thiserror (crates de formats), anyhow (binaires) | Erreurs de parsing précises avec offsets. |
| Logs | tracing | Traces de parsing activables par crate. |
| Outil de debug | bevy\_egui | Inspecteur en jeu : squelettes, états de l'IA, physique. |
| Outils en ligne de commande | clap | Extraction et inspection des archives. |

**Version de Bevy :** l'API de Bevy change beaucoup entre versions. Claude Code fige une version précise dans `Cargo.toml` au démarrage, et vérifie la documentation de cette version exacte avant d'écrire du code Bevy, au lieu de se fier à sa mémoire. Même règle pour bevy\_rapier3d et bevy\_egui, dont les versions doivent être compatibles avec celle de Bevy.

**Versions figées (J0, 1er octobre 2026) :** toutes les dépendances sont déclarées une seule fois dans `[workspace.dependencies]` avec une version exacte (`=x.y.z`), et `Cargo.lock` est versionné.

| Crate | Version | Remarque |
| --- | --- | --- |
| bevy | 0.19.1 | Dernière version stable (0.20 encore en release candidate). |
| bevy\_rapier3d | 0.36.0 | Dépend de bevy ^0.19. |
| bevy\_egui | 0.42.0 | Dépend de bevy ^0.19. |
| binrw | 0.15.2 | |
| texpresso | 2.0.2 | |
| serde / toml | 1.0.229 / 1.1.6 | |
| thiserror / anyhow | 2.0.21 / 1.0.104 | |
| tracing | 0.1.44 | Côté jeu, Bevy installe lui-même le subscriber. |
| clap | 4.6.7 | |
| tempfile | 3.27.0 | Tests uniquement. |

La toolchain Rust est épinglée dans `rust-toolchain.toml` pour que fmt et clippy donnent le même résultat en local et en CI. La version minimale de Rust est la 1.95, celle exigée par Bevy 0.19. En debug, les dépendances sont compilées en `opt-level = 3` et nos crates en `opt-level = 1`, sinon Bevy est inutilisable.

**Prérequis Windows :** rustup, plus les Build Tools de Visual Studio avec la charge de travail « Développement Desktop en C++ » (linker MSVC et SDK Windows). VS Code ne suffit pas, mais peut servir d'éditeur.

## Architecture du workspace

Trois couches : deux crates de formats lisent les fichiers des jeux, asset-bridge les convertit en types neutres, et l'application Bevy ne manipule que ces types.

&#91;embedded content: architecture du workspace · 6 crates\]

La crate game n'importe jamais formats-rw ni formats-pes directement. Changer de version de PES ne touche donc que formats-pes et asset-bridge. Le cache d'extraction est stocké dans le dossier de cache de l'utilisateur, hors du dépôt.

**Décisions (J0) :** la lecture et la validation des chemins des jeux vivent dans `asset-bridge` (module `config`), partagées par `game` et `asset-tools`. Le binaire du jeu s'appelle `chaos-fc` (`cargo run -p game`).

**Décisions (J1) :**

- `asset-bridge` produit des types neutres (`asset_bridge::model` : nœuds, maillages par matériau, squelette), sans dépendre de Bevy. Ils restent **dans le repère et les unités du jeu d'origine** (Vice City : mètres ; un personnage en pose de liaison est debout le long de +Y). Les changements d'axes sont laissés au consommateur, puis à `retarget` (J6).
- `asset-tools` contient deux binaires : la CLI (`asset-tools`) et le visualiseur Bevy (`viewer`). Leur entrée est un fichier, ou `vc:<nom>` pour une entrée de `models/gta3.img`.
- Les fichiers extraits vont dans `%LOCALAPPDATA%\chaos-fc\extracted\<jeu>` (équivalents macOS/Linux dans `asset_bridge::cache`).
- Les tests sur vrais fichiers vivent dans `crates/asset-bridge/tests/` et commencent par `asset_bridge::testing::game_dir(...)`, qui les ignore si le jeu n'est pas configuré.

**Décisions (J2) :**

- Un modèle utilise le TXD du même nom (`player.dff` → `player.txd`). La vraie association est décrite dans les fichiers IDE du jeu ; on ne les lira que si un modèle s'en écarte.
- Les animations neutres (`asset_bridge::model::Animation`) contiennent des rotations **locales** : le parser IFP fournit le quaternion stocké, et `asset-bridge` prend son conjugué (voir `docs/formats/ifp.md`). Une piste retrouve son os par identifiant HAnim, sinon par nom.
- Les animations de Vice City mettent le personnage debout le long de +Z, alors que la pose de liaison est le long de +Y ; le visualiseur tourne l'affichage en conséquence. Le déplacement de la racine (4,3 m par cycle de `run_player`) est retiré pour une boucle sur place.
- La pose de liaison des os vient de l'inverse des matrices inverses de liaison du skin, pas des frames, car quelques modèles ont des frames hors pose de liaison.
- Seul le premier niveau de mipmap des textures est utilisé pour l'instant.

**Décisions (J3) :**

- Nouvelle crate **`bevy-bridge`** (7ᵉ crate) : elle instancie dans Bevy les types neutres d'`asset-bridge` (modèles texturés et skinnés, calques d'animation, sons). Le visualiseur et le jeu la partagent ; `game` dépend d'`asset-bridge` et de `bevy-bridge`, jamais des crates de formats.
- Les animations se superposent par **calques** : un calque n'anime que les os qu'il contient (par exemple `colt45_fire`, qui ne touche que le bras droit, par-dessus la pose de repos).
- Les sons de la banque SFX sont joués via la feature `wav` de Bevy : chaque son est emballé en WAV en mémoire.
- Les paramètres des armes viennent de `data/weapon.dat` (portée, boucle de tir, instant du coup, point de sortie), pas de constantes inventées. Le modèle et le numéro de son de chaque arme sont décrits dans `asset_bridge::vice_city` (`COLT45`, `UZI`, `M4`) en attendant la lecture des fichiers IDE.
- Le jeu repère le monde en Y vers le haut (Bevy) ; les modèles de Vice City sont tournés par `retarget::VICE_CITY_TO_Y_UP`. L'arme est fixée telle quelle au nœud « R Hand » du personnage.
- Visée comme dans GTA : le curseur désigne un point (rayon depuis la caméra), le tireur pivote vers lui et la balle part du canon **vers ce point**, pas dans l'axe du bras animé (qui pointe vers le haut dans `colt45_fire`). Le tir est un lancer de rayon contre le sol et des cibles en boîtes ; la physique (Rapier) arrivera avec le foot (J7).
- L'arme est fixée telle quelle au nœud « R Hand » : confirmé à l'œil.
- Armes jouables en J3 : Colt 45, Uzi et Ruger (touches 1, 2, 3). Vice City n'a pas d'AK47 : le Ruger le remplace (choix de Mokhmad). **Le bazooka** (arme à projectile : roquette, explosion, pose `IDLE_ROCKET`) est reporté à J9.

```text
chaos-fc/
├── Cargo.toml            (workspace)
├── config.example.toml
├── crates/
│   ├── formats-rw/       IMG, DFF, TXD, IFP, SFX
│   ├── formats-pes/      AFS, décompression, modèles, animations
│   ├── asset-bridge/     types neutres + cache d'extraction
│   ├── bevy-bridge/      types neutres → Bevy (modèles, animations, sons)
│   ├── retarget/         squelettes, échelles, axes
│   ├── asset-tools/      CLI dump et extraction + visualiseur
│   └── game/             application Bevy
└── docs/
    ├── BRIEF.md
    └── formats/
```

## Formats de fichiers à lire

Côté Vice City, les formats sont documentés depuis vingt ans et il suffit de les implémenter. Côté PES 6, une partie reste à déchiffrer, surtout les animations. Les détails ci-dessous sont des points de départ : Claude Code vérifie chacun sur les vrais fichiers avant de s'y fier.

### GTA Vice City (RenderWare, version PC)

| Format | Contenu | Notes |
| --- | --- | --- |
| IMG + DIR | Archive principale (`models/gta3.img` et son répertoire `gta3.dir`) | Entrées de 32 octets : offset et taille en secteurs de 2048 octets, nom sur 24 octets. |
| DFF | Modèles 3D (Tommy, armes) | Flux binaire RenderWare en chunks : en-tête de 12 octets (type, taille, version). Clump → liste de frames (squelette), géométries, atomics. Les personnages utilisent l'extension Skin PLG pour le skinning. |
| TXD | Textures | Dictionnaire de textures natives PC, souvent compressées en DXT ou palettisées. |
| IFP | Animations | Format ANPK, celui de GTA III et Vice City. Les animations des personnages sont dans `anim/ped.ifp`. |
| SFX | Sons des armes | Banque audio dans le dossier `audio/`. Format exact à vérifier. |

Références : le wiki GTAMods (pages RenderWare, DFF, TXD, IFP, IMG), librw et OpenRW pour comparer une implémentation existante.

**Constats (J1)**, détaillés dans `docs/formats/` : les 4617 DFF de `gta3.img` mélangent trois versions de RenderWare (3.4.0.3, 3.3.0.2, 3.2.0.0) ; le modèle de Tommy (`player.dff`) est en 3.3.0.2, pas en 3.4.0.3. Certaines pages du wiki sont fausses (taille d'une frame) : seule la vérification sur les vrais fichiers fait foi.

**Constats (J2)** : les TXD sont en DXT1/DXT3 pour l'essentiel (plus quelques palettes 8 bits), avec un en-tête de 88 octets. `anim/ped.ifp` contient 234 animations, dont `run_player`. Le wiki se trompe aussi sur l'IFP : la section INFO ne contient pas les animations, chaque image clé a son temps, et le 4ᵉ entier de la section ANIM est l'identifiant d'os.

### PES 6 (version PC)

| Élément | État des connaissances | Notes |
| --- | --- | --- |
| Archives AFS | Vérifié (J4) | En-tête `AFS`, nombre de fichiers, table offset/taille. Jeu complet : `0_text.afs`, `0_sound.afs`, `f_text.afs`, `f_sound.afs` ; noms de fichiers illisibles. |
| Compression interne | Vérifiée (J4) | En-tête de 32 octets (type, drapeau zlib, tailles) puis zlib ; beaucoup de fichiers sont des conteneurs de sous-fichiers. |
| Modèles joueurs, ballon, stade | Outillé par la scène de modding | Corps, visages, cheveux, maillots en textures. S'appuyer sur la documentation des outils de modding PES 6. |
| Animations | Peu documentées | Principal chantier de reverse engineering du projet. |

**Constats (J4)**, détaillés dans `docs/formats/afs.md`, sur le **jeu complet** PC en français (`0_text.afs` : 9806 fichiers, `0_sound.afs`, `f_sound.afs`, `f_text.afs`) : le répertoire de noms des archives est illisible, les fichiers sont donc identifiés par leur signature et situés grâce à la carte communautaire de `0_text.afs`, qui correspond au jeu complet. Un visage est un conteneur de deux modèles (la même tête en deux niveaux de détail) et d'une texture. **1151 fichiers sont chiffrés** : la grande texture de chaque tenue (453 maillots) et le bloc n° 1193 à 1890 (698 fichiers).

**Décisions (J4) :**

- `formats-pes` lit les archives (`afs`), l'en-tête compressé (`packed`, avec flate2) et reconnaît les contenus (`content`) ; `asset-bridge::pes6` ouvre le dossier `dat` et porte la carte de `0_text.afs`. Les fichiers sont désignés par archive et numéro (`0_text:1943`), faute de noms.
- Les contenus non reconnus sont listés avec leurs premiers octets, jamais devinés.
- Les tests sur les fichiers de PES vérifient des propriétés (contenus des sections, fichiers illisibles cantonnés aux zones chiffrées), pas des nombres exacts.
- **Les fichiers chiffrés ne sont pas déchiffrés.** C'est une protection du jeu ; la contourner pose une question juridique (mesures techniques de protection) qui dépasse les règles du projet. Chaos FC s'en passe, sauf décision contraire de Mokhmad et Mehdi, à consigner ici. **Levée par Mehdi le 3 octobre 2026 sur son fork** (voir « Contraintes légales »).

**Reste à faire (J4)** : validé avec réserves. À reprendre :

- **Formats inconnus** (détails dans `docs/formats/afs.md`) : `07 12 01 20` (963 sous-fichiers, peut-être des animations : à tester en J6), `19 11 01 20` (623), la famille `1a`/`9a`... (plus de 1000), de nombreuses petites tables, `18 39 84 29` (264), les fichiers « WE9 » et « WE8I », `aPDT` (22), le type 14 (433) et les fichiers de `f_text.afs`.
- **Fichiers chiffrés** : 453 grandes textures de maillots et le bloc n° 1193 à 1890 ; le n° 9467 ne se décompresse pas.
- **Hors carte** : beaucoup de fichiers restent à situer (corps de joueurs, ensembles d'entraînement, stades...).

**Constats (J5)**, détaillés dans `docs/formats/pes-texture.md` et `docs/formats/pes-model.md` : les textures gardent les formats de la PlayStation 2 (palette de 256 couleurs réordonnée, ou 16 couleurs). Les modèles sont faits de parties de sommets (jusqu'à 4 os par sommet : prêts pour le skinning), d'une bande de triangles et d'un petit programme de dessin qui choisit partie, texture et os. Les grandes textures de maillots des équipes étant chiffrées, le joueur affiché porte une **tenue lisible** (n° 419, maillot jaune et short bleu) : corps de joueur de champ n° 1010, chaussures n° 5322/0/0, tête et visage n° 1943 et cheveux n° 4570 placés sur l'os de la tête du squelette (19 os, 594 corps dans le jeu). Une texture par emplacement (tenue, chaussures, peau), le rôle des emplacements étant déduit de la géométrie ; numéros et nom pas encore affichés.

**Décisions (J5) :**

- `formats-pes` : modules `texture` (décodage en RGBA) et `model` (parties, bande, programme de dessin, avec vérifications). `asset-bridge::pes6` les convertit vers les types neutres ; un fichier se désigne par `PesFile` (`0_text:431`, `0_text:1943/2`).
- Les modèles neutres restent en unités PES ; la mise à l'échelle est dans `retarget::PES6_UNITS_PER_METRE` (420, HYPOTHÈSE : l'arbitre mesure 1,80 m). PES 6 est déjà Y vers le haut.
- Un joueur s'assemble avec `Pes6::load_player(&PlayerParts { body, kit, boots, head, hair })` ; le rôle de chaque emplacement de texture du corps est déduit de sa géométrie (`PlayerSlot::classify` : tenue, chaussures, peau, marquages non affichés), les numéros d'emplacement changeant d'une famille de corps à l'autre.
- Le visualiseur affiche un modèle PES seul ou à côté d'un modèle de Vice City (`--pes`, `--pes-texture`, `--pes-boots`, `--pes-head`, `--pes-hair`), pieds à la même hauteur ; `--yaw` choisit l'angle de la caméra. `--capture <fichier.png>` enregistre une image de la fenêtre puis ferme : Claude peut ainsi voir le rendu avant de demander une validation.
- Le squelette des corps est lu (`formats_pes::model::Bone`) ; seule l'articulation de la tête sert en J5. HYPOTHÈSE sur l'ordre des rotations, à confirmer en J6 (les jambes et le dos ne tombent pas encore juste).

**Constats (J6)**, détaillés dans `docs/formats/pes-model.md` : les numéros d'os des sommets passent par la **table des os** de l'instruction `03` du programme de dessin (une par corps, vérifiée sur les 573 corps à 19 os). Avec elle, tout le squelette en T tombe juste (l'ordre `Rz·Ry·Rx` est confirmé) : bassin, hanches, genoux, chevilles, colonne, clavicules, épaules, coudes, poignets, cou, tête. Tommy en pose de liaison et un corps PES sont dans le **même repère** : Y en haut, face vers +Z, gauche vers +X, bras à l'horizontale. Les animations de Vice City, elles, tiennent le personnage le long de +Z et le font avancer vers +Y.

**Décisions (J6) :**

- Un corps PES devient un modèle skinné (`asset_bridge::pes6::convert_model`) : un nœud par os, nommé d'après `BODY_BONES` (« pelvis », « left thigh »…), et les poids des sommets ; la tête et les cheveux suivent le nœud de l'os de la tête.
- Le retargeting (`retarget::retarget`) donne à chaque os cible la rotation de son os source relative à sa pose de liaison, dans le repère commun ; le bassin suit le déplacement du bassin source, mis à l'échelle de la hauteur des jambes, et peut jouer sur place. Carte des os : `retarget::PES6_FROM_VICE_CITY` (l'os « hips » de PES, sans équivalent, garde sa pose par rapport au bassin ; HYPOTHÈSE : « spine » suit Spine1). Repère des animations de Vice City : `retarget::VICE_CITY_ANIMATION_TO_COMMON`. `glam` (la version de Bevy) est figé pour ces calculs hors de Bevy.
- Le visualiseur fait jouer une animation de Vice City à Tommy et, retargetée, au joueur PES placé à côté (`--anim run_player --pes …`).
- Les animations propres à PES (format `07 12 01 20`, HYPOTHÈSE) ne sont pas encore lues : le critère de J6 est rempli avec une animation de Vice City. Elles restent à étudier pour J7 et J8 (course, passe, tir des joueurs PES).

**Constats (J7)**, détaillés dans `docs/formats/pes-stadium.md` : les stades sont dans `0_text.afs` au-delà de la carte communautaire (groupes de 11 fichiers à partir du n° 6940). Le n° 6949 est le dôme de Sapporo complet : pelouse et motif de tonte, lignes, panneaux, tribunes, toit, écran géant, bancs, photographes et staff. Un modèle nomme ses textures par numéro (table à l'offset 12), les conteneurs ont des emplacements vides (position 0), les stades sont Y vers le bas à 51,3 unités par mètre. Les textures des ballons sont rangées en swizzle PS2 (décodé). Les buts et les filets de PES n'ont pas été trouvés.

**Décisions (J7) :**

- `cargo run -p game` lance le match ; la scène de tir de J3 devient `cargo run -p game -- tir`. `--capture <fichier.png>` enregistre une image puis ferme.
- Le décor vient de PES (`Pes6::load_scenery`, stade 0_text:6949), affiché sans éclairage ajouté. Les buts sont faits aux dimensions réglementaires, habillés de la texture de filet du stade, en attendant ceux de PES. HYPOTHÈSE : masque de tonte atténué à 20 %.
- Physique avec bevy_rapier3d : ballon de 22 cm et 430 g, traînée, effet (Magnus) et frottement du gazon calculés à part ; poteaux, barres, filets et murs invisibles aux panneaux.
- Joueur contrôlé au clavier ou à la manette, caméra latérale serrée comme celle de PES ; animations de Vice City retargetées en attendant celles de PES.
- Un but compte quand le ballon a entièrement franchi la ligne entre les poteaux et sous la barre ; score à l'écran, puis engagement au centre. Le « BUT ! » affiché est provisoire : les célébrations de PES viendront avec ses animations.
- **Demande de Mehdi (2 octobre 2026)** : tout ce qu'on voit dans un match de PES doit venir de PES (entrée des joueurs, célébrations, public, sons...). Les animations PES passent donc avant le 3 contre 3 (jalon J7b).

**Plan de repli pour les animations PES :** si elles restent illisibles après un effort raisonnable, utiliser des animations libres de droits (par exemple Mixamo) retargetées sur le squelette PES. Le jalon correspondant ne doit pas bloquer tout le projet.

## Méthode de reverse engineering

Chaque format suit le même cycle : hypothèse, parser, validation visuelle, documentation. Claude Code n'invente jamais un champ de format qu'il n'a pas vérifié sur un vrai fichier.

1. **Lire la documentation existante** (wiki GTAMods, outils de modding PES) et la résumer dans `docs/formats/<format>.md`.
2. **Inspecter les octets.** Un outil `cargo run -p asset-tools -- dump <fichier>` affiche un dump hexadécimal annoté. Pour un format inconnu, Mokhmad peut aussi ouvrir le fichier dans ImHex et partager le dump.
3. **Comparer des fichiers proches** (deux joueurs, deux armes, deux animations) pour isoler les octets qui changent : nombres d'éléments, offsets, tableaux de flottants.
4. **Écrire le parser binrw** avec une hypothèse explicite en commentaire pour chaque champ incertain.
5. **Valider visuellement** dans le visualiseur d'assets. Un modèle qui s'affiche correctement valide l'hypothèse. Un modèle déformé indique un champ mal lu.
6. **Documenter le résultat** dans `docs/formats/<format>.md` : structure confirmée, champs encore inconnus, fichiers testés.

Pour le comportement des jeux (vitesse de course, cadence de tir, rebonds du ballon), on procède par observation : vidéos de gameplay, mesures en jeu, puis réglage de constantes dans un fichier de tuning. Depuis le 3 octobre 2026, la décompilation des exécutables est aussi permise (voir « Contraintes légales »).

L'IA ne sait pas juger si un rendu ou un ressenti est correct. À chaque validation visuelle, Claude Code demande à Mokhmad de lancer le build et de confirmer ce qu'il voit, captures d'écran à l'appui.

## Spécifications gameplay

Le cœur du projet est un moteur de foot crédible. Tommy, l'arbitre et les corps au sol se greffent dessus.

### Moteur de foot

- **Ballon** : corps physique Rapier avec rebond, frottement au sol et effet (rotation qui courbe la trajectoire).
- **Joueurs** : déplacement, contrôle, passe courte, passe longue, tir, tacle. Animations enchaînées selon la vitesse et l'action.
- **IA d'équipe** : positionnement selon une formation, pressing sur le porteur, appels de balle, gardien. Machine à états par joueur.
- **État « panique »** : quand Tommy tire à proximité, les joueurs proches fuient ou se dispersent avant de reprendre leur poste.
- **Terrain encombré** : l'IA contourne les corps au sol, qui restent des obstacles physiques pour le ballon.
- **Règles de base** : buts, touches, corners, mi-temps, chrono. Pas de hors-jeu dans un premier temps.

### Tommy Vercetti

- Contrôlé par le joueur, vue à la troisième personne.
- **Deux modes** basculés par une touche : mode foot (courir, passer, tirer au but) et mode arme (viser, tirer).
- Armes de Vice City (par exemple Colt 45, Uzi, M4), avec leurs modèles, sons et animations de tir. Munitions illimitées.
- Tir par lancer de rayons (raycast), avec recul et cadence propres à chaque arme.

### Arbitre

- Suit le jeu normalement tant que Tommy ne tire pas.
- Dès un tir, il se met à poursuivre Tommy pour le plaquer au sol.
- Conséquence du plaquage : voir les questions ouvertes.

### Joueurs abattus

- Passage immédiat en ragdoll, impulsion dans la direction du tir.
- Le corps reste sur le terrain jusqu'à la fin du match.
- Prévoir un plafond de performance : au-delà d'un certain nombre de corps, les ragdolls passent en corps statiques.

## Feuille de route

On valide d'abord que les assets des deux jeux sont lisibles (J1 à J5), avant d'écrire la moindre ligne d'IA de foot. Un jalon n'est terminé que lorsque son critère est validé par Mokhmad sur sa machine.

**Installations disponibles chez Mokhmad (1er octobre 2026) :** GTA Vice City (PC) installé ; PES 6 pas encore. **Chez Mehdi (1er octobre 2026) :** GTA Vice City (PC) et PES 6 (PC, jeu complet en français ; J4 a été commencé sur la démo, puis repris sur le jeu complet).

| Jalon | Livrable | Critère de réussite |
| --- | --- | --- |
| J0 | Workspace Cargo, `config.toml`, `.gitignore`, CI (fmt, clippy, tests) | `cargo test` passe ; le jeu refuse de démarrer sans chemins valides. **Validé le 1er octobre 2026.** |
| J1 | Lecture IMG/DIR et DFF de Vice City | Tommy s'affiche en T-pose, géométrie correcte, dans le visualiseur. **Validé le 1er octobre 2026** (Tommy et le Colt 45 vérifiés par Mokhmad). |
| J2 | TXD + IFP | Tommy texturé joue son animation de course en boucle. **Validé le 1er octobre 2026** (Tommy texturé court en boucle, vérifié par Mokhmad). |
| J3 | Armes Vice City | Tommy tient une arme, tire dans une scène vide, avec le son d'origine. **Validé le 1er octobre 2026** (Colt 45, Uzi et Ruger : prise en main, visée, rythme et sons vérifiés par Mokhmad). |
| J4 | Lecture AFS et décompression PES 6 | Liste complète des fichiers internes extraits et identifiés. **Validé le 1er octobre 2026 par Mehdi, avec réserves** (revu sur le jeu complet ; voir « Reste à faire (J4) »). |
| J5 | Modèle joueur PES 6 | Un joueur PES texturé s'affiche à côté de Tommy, à la bonne échelle. **Validé le 2 octobre 2026 par Mehdi, avec réserves** (tenue lisible n° 419, les maillots d'équipes étant chiffrés ; numéros et nom pas affichés ; petites taches de peau sur les manches). |
| J6 | Squelettes et retargeting | Une même animation joue correctement sur Tommy et sur un joueur PES. **Validé le 2 octobre 2026 par Mehdi** (`run_player` sur Tommy et sur le corps n° 1010 ; les animations propres à PES restent à lire). |
| J7 | Foot minimal | Terrain, ballon physique, un joueur contrôlé, buts qui comptent. |
| J7b | Animations PES | Les animations de PES (format `07 12 01 20`) lues et jouées : course, passe, tir, célébration, entrée des joueurs. |
| J8 | 3 contre 3 avec IA basique | Un match jouable de bout en bout, sans Tommy. |
| J9 | Tommy dans le match | Bascule foot/arme, joueurs abattus en ragdoll, corps persistants. Bazooka (roquette, explosion). |
| J10 | Arbitre | L'arbitre poursuit et plaque Tommy après un tir. |
| J11 | 11 contre 11, stade PES, réglages | Match complet stable, ressenti validé par Mokhmad. |

### Route « mod du vrai PES 6 » (décision de Mehdi, 3 octobre 2026)

Mehdi veut la jouabilité réelle de PES, pas une réécriture. Décision : **modder d'abord le vrai PES6.exe**, et garder le moteur Rust (J7 à J11) comme repli et comme source des parsers (Tommy, armes, sons de Vice City). Le mod est la crate `pes6-mod` : une DLL 32 bits nommée `dinput8.dll`, posée à côté de PES6.exe (qui importe `DINPUT8.dll`), qui transmet `DirectInput8Create` à la DLL du système. Plan et journal : `docs/pes6-mod.md`.

**Méthode (décision de Mehdi, 3 octobre 2026) :** pour le mod du vrai PES 6, toute structure en mémoire vient du code décompilé (Ghidra), jamais de recherches heuristiques.

**Constats (M2a)**, journal du mod : PES 6 dessine en **pipeline fixe** (aucun vertex shader créé) ; une matrice `VIEW` et une `PROJECTION` par image via `SetTransform`. Le périphérique est créé en 640×480 chez Mehdi (réglage de PES), en traitement de sommets mixte (`0x80`). La projection inverse l'axe Y.

**Constats (M1) :** PES6.exe (21,9 Mo, compilé le 9 septembre 2006) est natif x86, sans anti-triche, et protégé par SecuROM (sections renommées `age`, `agis`, `quod`, `.rld`). Sa section de code est lisible statiquement (Ghidra). On ne touche pas à SecuROM et on évite les débogueurs (contrôles anti-débogage) : analyse statique, puis lecture de la mémoire depuis le mod. Il importe **`d3d8.dll`** (`Direct3DCreate8` : PES 6 est un jeu **Direct3D 8**) et `DINPUT8.dll`, appelés par des sauts `jmp [emplacement]` vers sa table d'imports (emplacements `0x77d3a8` et `0x77d01c`). Les chaînes `d3d9`/`Direct3DCreate9` sont dans les sections de SecuROM, pas dans les imports du jeu.

| Jalon | Livrable | Critère de réussite |
| --- | --- | --- |
| M1 | Mod chargé dans PES6.exe (proxy `dinput8.dll`, `asset-tools mod install`/`uninstall`) | PES 6 se lance et se joue normalement ; `chaos-fc-mod.log` apparaît à côté de PES6.exe. **Validé le 3 octobre 2026 par Mehdi** (match joué, commandes normales ; journal : chargement, DirectInput 8 système, déchargement). |
| M2a | Accroche Direct3D 8 (`Direct3DCreate8` → `CreateDevice` → `Present`) | Un bandeau « CHAOS FC » s'affiche en haut à gauche, par-dessus les menus et le match, sans gêner le jeu ; le journal indique comment PES envoie ses matrices. **Validé le 3 octobre 2026 par Mehdi** (bandeau visible dans les menus et en match, jeu normal). |
| M2b | Objet dans le monde | Un objet de test dessiné dans le vrai match, à un point fixe du terrain. **Validé le 3 octobre 2026 par Mehdi** (repère au point central, caché par les joueurs, absent des menus ; poteau bleu vers le ciel : monde Y vers le bas). |
| M3 | Ballon et joueurs en mémoire (Ghidra + lecture depuis le mod) | Positions du ballon et des 22 joueurs journalisées, cohérentes avec l'écran. Étapes : (1) relever les matrices WORLD de chaque objet dessiné en match ; (2) chercher ces valeurs dans la mémoire de PES depuis le mod ; (3) confirmer la structure d'un joueur avec Ghidra. **Ballon validé le 3 octobre 2026 par Mehdi** (épingle sur le ballon). Joueurs : tableau de 23 × 0x240 octets à `0x03BDC980`, lu dans le code décompilé (emplacement 0 l'arbitre, 1–11 équipe 0, 12–22 équipe 1 ; voir `docs/pes6-mod.md`). **Validé le 3 octobre 2026 par Mehdi** (un fanion sur chaque joueur, une couleur par équipe). |
| M4a | Tommy au bord du terrain | Tommy (modèle et textures de Vice City) dessiné immobile hors du terrain, sur la ligne de touche, dans l'image de PES. |
| M4b | Animations de Tommy | Tommy joue ses animations de Vice City (repos, course). |
| M4c | Bascule vers Tommy | Une touche fait passer le joueur sur Tommy : les deux équipes passent sous le contrôle de l'ordinateur (code de PES décompilé), PES ne reçoit plus les commandes du joueur ; la même touche rend l'équipe. |
| M4d | Intégration | Tommy caché par les joueurs qui passent devant, ombre, taille juste. |
| M5 | Tirs | Armes de Vice City, sons d'origine ; un joueur touché tombe et sort du jeu. |

**Déroulement voulu (Mehdi, 3 octobre 2026) :** le match commence normalement, le joueur contrôle son équipe et Tommy attend hors du terrain, sur la ligne de touche. Une touche fait passer le joueur sur Tommy : les deux équipes sont alors jouées par l'ordinateur (comme un match ordinateur contre ordinateur de PES), et le joueur peut tirer sur tous les joueurs ; un joueur touché tombe et sort du match. La même touche rend l'équipe au joueur.

### Route « tout en Rust depuis le code décompilé » (décision de Mehdi, 3 octobre 2026)

Mehdi veut finalement **tout dans le moteur Rust** (crate `game`), porté depuis le code décompilé des deux jeux, plutôt qu'un mod du vrai PES. Le mod `pes6-mod` reste, comme **instrument de mesure** : il enregistre ce que fait le vrai PES image par image, et chaque sous-système porté en Rust doit reproduire ces enregistrements. Une partie du code de PES saute dans les sections de SecuROM : on ne contourne pas cette protection, ces parties sont reproduites d'après les enregistrements.

| Jalon | Livrable | Critère de réussite |
| --- | --- | --- |
| R1 | Carte des deux exécutables | Toutes les fonctions de PES6.exe et de gta-vc.exe décompilées et exportées hors du dépôt ; sous-systèmes repérés (physique du ballon, déplacement des joueurs, IA, règles, commandes, animations), consignés dans `docs/pes6-code.md` et `docs/vc-code.md`. |
| R2 | Enregistreur | Le mod enregistre un match réel image par image (ballon, 23 joueurs, commandes) dans un fichier. |
| R3 | Physique du ballon de PES en Rust | Portée du code décompilé ; rejouée sur les enregistrements, la trajectoire du ballon Rust suit celle du vrai. |
| R4+ | Joueurs, animations PES, IA, règles, puis Tommy (code de Vice City) | Chaque sous-système validé contre les enregistrements, puis par Mehdi à la manette. |

### Vision longue (Mehdi, 3 octobre 2026), après la route « mod du vrai PES 6 »

- **Le stade PES dans Vice City :** le stade de PES est ajouté à la carte de Vice City comme une extension. Tommy y entre, le menu « Match rapide » de PES s'ouvre (choix des équipes, etc., le déroulement normal de PES), puis le match avec Tommy (route ci-dessus).
- **Les joueurs PES dans les rues de Vice City :** chaque joueur PES (par exemple Ronaldinho) apparaît dans Vice City sous forme d'image plate (PNG, rendue depuis son modèle PES), et Tommy peut aussi lui tirer dessus en dehors du stade.
- Vice City a déjà un chargeur ASI installé chez Mehdi : c'est la porte d'entrée côté GTA. Le détail sera planifié une fois M5 validé.

## Conventions de travail pour Claude Code

Petites étapes, un commit par étape, et toujours une vérification par Mokhmad avant de passer au jalon suivant.

- **Au démarrage** : copier ce brief dans `docs/BRIEF.md` et créer un `CLAUDE.md` qui résume les règles non négociables et pointe vers le brief.
- **Un jalon à la fois.** Annoncer le plan du jalon, l'implémenter par petites étapes, puis demander une validation.
- **Qualité** : `cargo fmt` et `cargo clippy` sans avertissement avant chaque commit. Chaque parser a des tests.
- **Formats** : jamais de champ inventé. Un champ incertain est marqué `// HYPOTHÈSE:` dans le code et listé dans `docs/formats/`.
- **Vérifications visuelles** : quand un résultat se juge à l'œil (modèle, animation, ressenti), demander à Mokhmad de lancer le build et de décrire ou capturer le résultat.
- **Décisions** : toute décision de design prise en cours de route est ajoutée à `docs/BRIEF.md`, dans la section concernée.
- **Dépendances** : versions figées dans `Cargo.toml`, documentation de la version exacte consultée avant usage.
- **Langue** : code et identifiants en anglais, documentation et échanges en français.
- **Précisions (J0)** : les commentaires de code sont en anglais (sauf le marqueur `// HYPOTHÈSE:`) ; les messages affichés au joueur et les messages de commit sont en français. Chaque jalon est développé sur une branche `jN`, fusionnée dans `main` après validation par Mokhmad.
