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
| weapon.dat (armes) | Vice City | [weapon-dat.md](weapon-dat.md) | Vérifié sur les 37 armes ; unités des instants : hypothèse |
| AFS, compression, conteneurs | PES 6 | [afs.md](afs.md) | Vérifié sur le jeu complet ; 1151 fichiers chiffrés |
| Textures | PES 6 | [pes-texture.md](pes-texture.md) | 5165 décodées ; palettes externes et « swizzle » : non pris en charge |
| Modèles 3D | PES 6 | [pes-model.md](pes-model.md) | 9546 modèles sur 9647 lus ; squelette et échelle : hypothèses |

## Outils d'inspection

```sh
cargo run -p asset-tools -- img list --filter player      # contenu de models/gta3.img
cargo run -p asset-tools -- img extract player.dff        # copie dans le dossier de cache
cargo run -p asset-tools -- dump vc:player.dff             # dump annoté (arbre des chunks RenderWare)
cargo run -p asset-tools --bin viewer -- player            # visualiseur 3D (validation à l'œil)
cargo run -p asset-tools --bin viewer -- player --anim run_player   # animation
cargo run -p asset-tools --bin viewer -- --textures vc:player.txd     # textures à plat
cargo run -p asset-tools --bin viewer -- player --pes 0_text:1064 --pes-texture 0_text:419 --pes-boots 0_text:5322/0/0 --pes-head 0_text:1943   # joueur PES 6 à côté de Tommy
```

Les fichiers extraits vont dans le dossier de cache de l'utilisateur (`%LOCALAPPDATA%\chaos-fc\extracted\` sous Windows), jamais dans le dépôt. On peut les ouvrir dans ImHex.
