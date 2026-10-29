# ReqtiCraft

Traçabilité des exigences entre documents hétérogènes (Word, Excel, PDF, code source, outils ALM),
avec une interface moderne et un mode ligne de commande pour l'intégration continue.

ReqtiCraft capture les exigences et leurs références directement dans les documents sources, calcule
la couverture et l'analyse d'impact, contrôle des règles de cohérence et produit des rapports. Il sait
importer les projets Reqtify existants.

> État : connecteurs **P0** disponibles en ligne de commande (texte, Word docx, tableurs, PDF, code
> source, JSON, XML, projets composites). L'interface graphique n'est pas encore branchée.

## Utilisation

```bash
cargo run -p rc-cli -- analyze fixtures/thermostat/docx/thermostat.rqtf
```

```bash
cargo run -p rc-cli -- analyze projet.rqtf --details --json resultat.json --fail-on error
```

```bash
cargo run -p rc-cli -- import projet.rqtf
```

```bash
cargo run -p rc-cli -- types test exigences.docx
```

```bash
cargo run -p rc-cli -- convert exigences.pdf
```

Pour réutiliser une bibliothèque de types existante (par exemple le répertoire `config` d'une
installation Reqtify), ajouter `--config <répertoire>`.

## Documentation

- [Cahier des charges fonctionnel](docs/functional-spec.md)
- [Formats d'interopérabilité](docs/interop-formats.md)
- [Proposition d'architecture et plan de MVP](docs/architecture-proposal.md)
- [Contribuer (règles clean room)](CONTRIBUTING.md)

ReqtiCraft est un projet indépendant, sans lien avec Dassault Systèmes. Reqtify est une marque de
Dassault Systèmes.
