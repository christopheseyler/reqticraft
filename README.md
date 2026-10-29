# ReqtiCraft

Traçabilité des exigences entre documents hétérogènes (Word, Excel, PDF, code source, outils ALM),
avec une interface moderne et un mode ligne de commande pour l'intégration continue.

ReqtiCraft capture les exigences et leurs références directement dans les documents sources, calcule
la couverture et l'analyse d'impact, contrôle des règles de cohérence et produit des rapports. Il sait
importer les projets Reqtify existants.

> État : **application de bureau** avec édition de projet, de types et de filtres (M4 et M5), et
> connecteurs **P0, P1 et P2**. Les instantanés et les rapports arrivent avec M6.
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

## Application de bureau

L'application ouvre un projet `.rqtf` et affiche :

- le **tableau de bord** : documents, exigences, couverture, dérivées et problèmes ;
- la vue **Couverture** : éléments de chaque document, avec un filtre (non couvertes, dérivées,
  modifiées…), une recherche et le panneau **Détails** (texte, attributs, « couvre », « couvert par »,
  problèmes) ;
- la vue **Impact** : ce que l'élément couvre en amont, et ce qui dépend de lui en aval, sur toute la
  profondeur ;
- la vue **Problèmes** : messages par sévérité et par règle, avec un lien vers l'élément.

Quand un document change sur le disque, un bandeau propose de recharger. Après le rechargement, les
éléments nouveaux ou modifiés sont signalés.

Elle permet aussi de modifier le projet :

- **Projet** : diagramme des documents et des canaux, ajout et retrait de documents, type, chemin et
  catégorie de chaque document ;
- **Types** : duplication d'un type de la bibliothèque, édition de ses éléments (classe, expression,
  sous-expression, rôle des groupes, parents) et **testeur** sur un fichier, avec l'intermédiaire ;
- **Filtres** : conditions combinées en ET ou en OU, paramètres `$nom`, application à l'affichage
  (masquage) ou à l'analyse (taux recalculés).

Un projet s'enregistre en `.rqtf` (avec `<projet>.types` et `<projet>.filters`, lisibles par Reqtify)
ou au format natif `.rcproj` (un seul fichier JSON).

```bash
cd app && npm ci && npx tauri dev
```

Pour travailler sur l'interface dans un navigateur, le serveur local expose les mêmes requêtes que
l'application de bureau, sur 127.0.0.1 seulement :

```bash
cargo run -p rc-cli -- serve fixtures/thermostat/text/thermostat.rqtf --ui app/dist
```

## Documentation

- [Cahier des charges fonctionnel](docs/functional-spec.md)
- [Formats d'interopérabilité](docs/interop-formats.md)
- [Proposition d'architecture et plan de MVP](docs/architecture-proposal.md)
- [Protocole de plugins](docs/plugin-protocol.md)
- [Contribuer (règles clean room)](CONTRIBUTING.md)

ReqtiCraft est un projet indépendant, sans lien avec Dassault Systèmes. Reqtify est une marque de
Dassault Systèmes.
