# ReqtiCraft

Traçabilité des exigences entre documents hétérogènes (Word, Excel, PDF, code source, outils ALM),
avec une interface moderne et un mode ligne de commande pour l'intégration continue.

ReqtiCraft capture les exigences et leurs références directement dans les documents sources, calcule
la couverture et l'analyse d'impact, contrôle des règles de cohérence et produit des rapports. Il sait
importer les projets Reqtify existants.

> État : connecteurs **P0, P1 et P2** disponibles en ligne de commande. L'interface graphique n'est
> pas encore branchée.
>
> - Documents : texte, Word docx, tableurs, PDF, code source, JSON, XML, HTML, Markdown, AsciiDoc,
>   RTF, OpenDocument, PowerPoint, FrameMaker MIF, ReqIF, Visio, MS Project, UML/SysML (XMI),
>   médias, propriétés de fichiers.
> - Accès distants : ZIP, HTTP(S)/FTP, Git, GitHub, Bitbucket, Subversion, CVS, Perforce, Google
>   Drive, OneDrive, commande externe.
> - Outils : Jira, Redmine, Bugzilla, codebeamer, Jama, Polarion, DOORS Next, OSLC, bases de données
>   (Access, ODBC, Oracle, SQLite), plugins JSON-RPC.

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

### Sources distantes et outils

Le chemin d'un document peut désigner une source distante : `https://…`, `zip:archive.zip!doc.docx`,
`git+https://serveur/depot.git#main:docs/spec.md`, `github:org/depot@main/docs/spec.md`, `svn+https://…`,
`gdrive:<identifiant>`, `onedrive:<chemin>`. Pour un outil ALM, c'est une requête : JQL pour Jira, par
exemple. Les identifiants viennent d'un profil de connexion, `reqticraft-profiles.ini` à côté du
projet ou `~/.reqticraft/profiles.ini`. Le profil désigne la variable d'environnement qui contient le
secret ; il ne contient jamais le secret lui-même :

```ini
[jira]
server=https://jira.example.com
user=alice
tokenEnv=JIRA_TOKEN
```

```bash
cargo run -p rc-cli -- convert "project = THS" --type Jira --param profile=jira
```

Le transport HTTP utilise `curl` ; Git, Subversion, CVS, Perforce, `sqlplus`, `sqlite3`, `7z` et
`mediainfo` sont utilisés s'ils sont installés. Les connecteurs propriétaires se branchent par le
[protocole de plugins](docs/plugin-protocol.md).

## Documentation

- [Cahier des charges fonctionnel](docs/functional-spec.md)
- [Formats d'interopérabilité](docs/interop-formats.md)
- [Proposition d'architecture et plan de MVP](docs/architecture-proposal.md)
- [Protocole de plugins](docs/plugin-protocol.md)
- [Contribuer (règles clean room)](CONTRIBUTING.md)

ReqtiCraft est un projet indépendant, sans lien avec Dassault Systèmes. Reqtify est une marque de
Dassault Systèmes.
