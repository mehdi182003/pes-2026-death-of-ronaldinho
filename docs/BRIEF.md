# Chaos FC — Brief projet pour Claude Code

Oct 1, 2026 · @Mokhmad

## Vision

Chaos FC est un match de football façon PES 6 dans lequel Tommy Vercetti (GTA Vice City) débarque armé, en électron libre, et peut abattre n'importe quel joueur pendant que le match continue. Le projet est écrit en Rust et développé intégralement en vibe coding avec Claude Code.

Le modèle technique est celui du mashup MW2 × Minecraft × Skate 3 de Chasm : on réécrit les moteurs et la logique, et on charge les assets originaux depuis les copies des jeux que possède le joueur. Rien n'est distribué.

Ce document est le brief de référence. Claude Code le relit avant chaque jalon et le met à jour quand une décision change.

## Contraintes légales (non négociables)

Aucun fichier issu de GTA Vice City ou de PES ne doit jamais entrer dans le dépôt Git. Le jeu lit tout depuis les installations du joueur.

1. **Aucun asset dans le dépôt.** Modèles, textures, animations, sons et fichiers extraits restent hors du repo. Le dossier de cache d'extraction est dans le `.gitignore` dès le premier commit.
2. **Chemins configurables.** Le joueur indique l'emplacement de ses copies de Vice City et de PES 6 dans un fichier de configuration (`config.toml`). Le jeu refuse de démarrer avec un message clair si un chemin manque.
3. **Pas de code d'origine.** Interdiction d'utiliser ou de s'inspirer ligne à ligne de reVC, re3, ou du code source de GTA V qui a fuité. Ces projets ont fait l'objet de poursuites par Take-Two.
4. **Pas de décompilation des exécutables pour copier du code.** Le comportement des jeux est reproduit par observation (vidéos, mesures en jeu). La documentation publique des formats de fichiers est autorisée.
5. **Références open source autorisées** pour comprendre les formats : la documentation du wiki GTAMods, librw, OpenRW, les outils de la scène de modding PES. Respecter leurs licences si du code est repris.
6. **Tests sans assets commités.** Les tests qui lisent de vrais fichiers sont ignorés automatiquement si les chemins des jeux ne sont pas configurés.

**Mise en œuvre (J0) :**

- Le `.gitignore` bloque `config.toml`, les dossiers de cache locaux et les extensions de fichiers des jeux (`.img`, `.dir`, `.dff`, `.txd`, `.ifp`, `.col`, `.sfx`, `.sdt`, `.raw`, `.adf`, `.afs`, `.bin`, `.str`, `.dump`). Un job de CI échoue si un fichier portant l'une de ces extensions est suivi par Git.
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
| Archives AFS | Format simple et connu | En-tête `AFS`, nombre de fichiers, table offset/taille. Noms exacts des archives à relever sur la copie installée. |
| Compression interne | Partiellement connue | Beaucoup de fichiers seraient compressés en zlib derrière un petit en-tête propre au jeu. À confirmer. |
| Modèles joueurs, ballon, stade | Outillé par la scène de modding | Corps, visages, cheveux, maillots en textures. S'appuyer sur la documentation des outils de modding PES 6. |
| Animations | Peu documentées | Principal chantier de reverse engineering du projet. |

**Plan de repli pour les animations PES :** si elles restent illisibles après un effort raisonnable, utiliser des animations libres de droits (par exemple Mixamo) retargetées sur le squelette PES. Le jalon correspondant ne doit pas bloquer tout le projet.

## Méthode de reverse engineering

Chaque format suit le même cycle : hypothèse, parser, validation visuelle, documentation. Claude Code n'invente jamais un champ de format qu'il n'a pas vérifié sur un vrai fichier.

1. **Lire la documentation existante** (wiki GTAMods, outils de modding PES) et la résumer dans `docs/formats/<format>.md`.
2. **Inspecter les octets.** Un outil `cargo run -p asset-tools -- dump <fichier>` affiche un dump hexadécimal annoté. Pour un format inconnu, Mokhmad peut aussi ouvrir le fichier dans ImHex et partager le dump.
3. **Comparer des fichiers proches** (deux joueurs, deux armes, deux animations) pour isoler les octets qui changent : nombres d'éléments, offsets, tableaux de flottants.
4. **Écrire le parser binrw** avec une hypothèse explicite en commentaire pour chaque champ incertain.
5. **Valider visuellement** dans le visualiseur d'assets. Un modèle qui s'affiche correctement valide l'hypothèse. Un modèle déformé indique un champ mal lu.
6. **Documenter le résultat** dans `docs/formats/<format>.md` : structure confirmée, champs encore inconnus, fichiers testés.

Pour le comportement des jeux (vitesse de course, cadence de tir, rebonds du ballon), on procède par observation : vidéos de gameplay, mesures en jeu, puis réglage de constantes dans un fichier de tuning. Jamais de décompilation de l'exécutable.

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

**Installations disponibles chez Mokhmad (1er octobre 2026) :** GTA Vice City (PC) installé ; PES 6 pas encore.

| Jalon | Livrable | Critère de réussite |
| --- | --- | --- |
| J0 | Workspace Cargo, `config.toml`, `.gitignore`, CI (fmt, clippy, tests) | `cargo test` passe ; le jeu refuse de démarrer sans chemins valides. **Validé le 1er octobre 2026.** |
| J1 | Lecture IMG/DIR et DFF de Vice City | Tommy s'affiche en T-pose, géométrie correcte, dans le visualiseur. **Validé le 1er octobre 2026** (Tommy et le Colt 45 vérifiés par Mokhmad). |
| J2 | TXD + IFP | Tommy texturé joue son animation de course en boucle. **Validé le 1er octobre 2026** (Tommy texturé court en boucle, vérifié par Mokhmad). |
| J3 | Armes Vice City | Tommy tient une arme, tire dans une scène vide, avec le son d'origine. |
| J4 | Lecture AFS et décompression PES 6 | Liste complète des fichiers internes extraits et identifiés. |
| J5 | Modèle joueur PES 6 | Un joueur PES texturé s'affiche à côté de Tommy, à la bonne échelle. |
| J6 | Squelettes et retargeting | Une même animation joue correctement sur Tommy et sur un joueur PES. |
| J7 | Foot minimal | Terrain, ballon physique, un joueur contrôlé, buts qui comptent. |
| J8 | 3 contre 3 avec IA basique | Un match jouable de bout en bout, sans Tommy. |
| J9 | Tommy dans le match | Bascule foot/arme, joueurs abattus en ragdoll, corps persistants. Bazooka (roquette, explosion). |
| J10 | Arbitre | L'arbitre poursuit et plaque Tommy après un tir. |
| J11 | 11 contre 11, stade PES, réglages | Match complet stable, ressenti validé par Mokhmad. |

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
