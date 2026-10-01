# Chaos FC — instructions pour Claude Code

Le brief de référence est [`docs/BRIEF.md`](docs/BRIEF.md). **Le relire avant chaque jalon** et le mettre à jour (dans la section concernée) dès qu'une décision de design est prise ou change.

## Règles non négociables (légal)

1. **Aucun asset dans le dépôt.** Rien qui provienne de GTA Vice City ou de PES (modèles, textures, animations, sons, fichiers extraits, dumps) ne doit être commité. Le `.gitignore` bloque les extensions concernées et la CI échoue si l'une d'elles est suivie par Git.
2. **Chemins configurables.** Les jeux sont lus depuis les installations du joueur, déclarées dans `config.toml` (non versionné ; modèle : `config.example.toml`). Sans chemins valides, le jeu refuse de démarrer avec un message clair.
3. **Pas de code d'origine.** Interdiction d'utiliser ou de s'inspirer ligne à ligne de reVC, re3, ou du code source de GTA V qui a fuité.
4. **Pas de décompilation des exécutables.** Le comportement des jeux est reproduit par observation. La documentation publique des formats est autorisée.
5. **Références open source autorisées** pour comprendre les formats (wiki GTAMods, librw, OpenRW, outils de modding PES), en respectant leurs licences.
6. **Tests sans assets commités.** Les tests qui lisent de vrais fichiers sont ignorés automatiquement si les chemins des jeux ne sont pas configurés.

## Conventions de travail

- **Un jalon à la fois** (feuille de route dans le brief). Annoncer le plan, implémenter par petites étapes, **un commit par étape**, puis demander à Mokhmad de valider le critère du jalon sur sa machine.
- **Branches** : chaque jalon est développé sur une branche `jN`, fusionnée dans `main` après validation par Mokhmad.
- **Qualité** : avant chaque commit, `cargo fmt --all` et `cargo clippy --workspace --all-targets -- -D warnings` sans avertissement, `cargo test --workspace` vert. Chaque parser a des tests.
- **Formats** : jamais de champ inventé. Tout champ incertain est marqué `// HYPOTHÈSE:` dans le code et listé dans `docs/formats/<format>.md`.
- **Vérifications visuelles** : l'IA ne juge pas un rendu ni un ressenti. Demander à Mokhmad de lancer le build et de décrire ou capturer le résultat.
- **Dépendances** : versions figées (`=x.y.z`) dans `[workspace.dependencies]` du `Cargo.toml` racine. Avant d'écrire du code Bevy, bevy_rapier3d ou bevy_egui, consulter la documentation **de la version exacte figée** (sources dans le registre Cargo ou docs.rs à cette version), jamais la mémoire.
- **Architecture** : la crate `game` n'importe jamais `formats-rw` ni `formats-pes` ; elle passe par `asset-bridge`.
- **Langue** : code, identifiants et commentaires en anglais (sauf le marqueur `// HYPOTHÈSE:`) ; documentation, messages affichés au joueur, messages de commit et échanges en français.

## Commandes utiles

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p game                               # lance le jeu (lit ./config.toml)
cargo run -p asset-tools -- check-config        # vérifie les chemins sans compiler Bevy
cargo run -p asset-tools -- img list --filter player   # contenu de models/gta3.img de Vice City
```

La variable d'environnement `CHAOS_FC_CONFIG` permet d'utiliser un autre fichier de configuration que `./config.toml`.

Les tests qui lisent les vrais fichiers des jeux commencent par `let Some(dir) = asset_bridge::testing::game_dir(Game::ViceCity) else { return; };` : ils sont ignorés quand le jeu n'est pas configuré (CI).
