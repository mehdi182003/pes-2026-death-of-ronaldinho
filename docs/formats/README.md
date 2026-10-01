# Formats de fichiers

Une page par format, tenue à jour selon la méthode du brief (`docs/BRIEF.md`, « Méthode de reverse engineering ») : documentation publique résumée, puis structure **vérifiée sur les vrais fichiers**, champs encore incertains et fichiers testés.

Règles :

- Aucun octet issu des jeux n'est recopié ici : on décrit des structures, des comptes et des noms de fichiers, jamais des dumps.
- Un champ non vérifié est signalé « HYPOTHÈSE » ici et `// HYPOTHÈSE:` dans le code.

| Format | Jeu | Page | État |
| --- | --- | --- | --- |
| IMG v1 + DIR | Vice City | [img.md](img.md) | Vérifié sur `models/gta3.img` |
| Flux RenderWare (chunks) | Vice City | [renderware.md](renderware.md) | Vérifié sur les 4617 DFF |
| DFF (modèles) | Vice City | [dff.md](dff.md) | Vérifié sur les 4617 DFF ; texture : une hypothèse |
| TXD (textures) | Vice City | [txd.md](txd.md) | Vérifié sur 1399 TXD ; ordre des couleurs des palettes à confirmer à l'œil |
| IFP (animations) | Vice City | [ifp.md](ifp.md) | Vérifié sur les 29 IFP ; liens de la variante 48 octets : hypothèse |
| SFX (sons) | Vice City | [sfx.md](sfx.md) | Vérifié sur la banque ; numéros des sons d'armes : à confirmer à l'oreille |

## Outils d'inspection

```sh
cargo run -p asset-tools -- img list --filter player      # contenu de models/gta3.img
cargo run -p asset-tools -- img extract player.dff        # copie dans le dossier de cache
cargo run -p asset-tools -- dump vc:player.dff             # dump annoté (arbre des chunks RenderWare)
cargo run -p asset-tools --bin viewer -- player            # visualiseur 3D (validation à l'œil)
cargo run -p asset-tools --bin viewer -- player --anim run_player   # animation
cargo run -p asset-tools --bin viewer -- --textures vc:player.txd     # textures à plat
```

Les fichiers extraits vont dans le dossier de cache de l'utilisateur (`%LOCALAPPDATA%\chaos-fc\extracted\` sous Windows), jamais dans le dépôt. On peut les ouvrir dans ImHex.
