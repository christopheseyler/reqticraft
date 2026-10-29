# ReqtiCraft — Proposition d'architecture

> Statut : brouillon v0.1 — 2026-10-28. Document de travail, à arbitrer.
> Entrées : `functional-spec.md` (besoins), `interop-formats.md` (formats à lire).

## 1. Principes directeurs

1. **Cœur sans interface** : tout le métier (import, conversion, capture, analyse, règles, filtres,
   instantanés, rapports) vit dans une bibliothèque testable, pilotable en CLI. L'interface graphique
   n'est qu'un client.
2. **Pipeline en deux temps conservé** : *convertisseur* (natif → intermédiaire texte ou XML), puis
   *moteur de capture* piloté par un type d'analyse. C'est la condition pour réutiliser les `.types`
   des clients.
3. **Formats natifs ouverts** : configuration lisible et versionnable (diffs Git propres). Lecture et
   écriture des formats Reqtify en import, et en export pour les fichiers de configuration texte.
4. **Analyse incrémentale et déterministe** : même entrée, même résultat. Cache par empreinte des
   documents sources.
5. **Connecteurs en plugins** : les connecteurs P0 sont intégrés, les autres passent par un protocole
   de plugin isolé.
6. **Clean room vérifiable** : chaque décision de format renvoie à une preuve d'`interop-formats.md`.
   Aucune dépendance au binaire d'origine.

## 2. Stack proposée

| Couche | Choix | Raisons | Alternatives écartées |
|---|---|---|---|
| Cœur | **Rust** (workspace Cargo) | Performance sur de gros corpus, sûreté mémoire, binaire unique, portable ; écosystème solide pour XML, ZIP, regex et SQLite | TypeScript seul : plus lent sur l'analyse massive et les regex PCRE ; C++ : coût de maintenance |
| Regex | **`pcre2`** (crate de liaison PCRE2), avec `fancy-regex` en repli pur Rust | Dialecte identique à celui des types existants (lookbehind, `(?:)`, paresseux, `\d` Unicode) | `regex` seul : pas de lookaround ni de backreferences |
| XML | `quick-xml` (lecture en flux) et `roxmltree` (arbres d'analyse) | Rapide, sans dépendance native | — |
| Formats Office | `zip` + `quick-xml` pour **docx**, `calamine` pour **xlsx/xls/ods**, `csv` | Conversion **sans Word ni Excel**, multiplateforme | Automatisation COM : Windows seul, fragile |
| PDF | **`pdf-extract` + `lopdf`** (Rust pur) : glyphes avec position et taille, signets, mot de passe. Reconstitution des paragraphes dans notre propre code. Passage à **`pdfium-render`** si la robustesse sur des PDF réels l'exige (police, graisse, couleur, tableaux). | Aucun binaire à télécharger ni à embarquer ; suffisant pour le texte positionné | MuPDF : licence AGPL |
| Stockage | Projet natif en **TOML/JSON** ; cache d'analyse et instantanés en **SQLite** (`rusqlite`) | Configuration lisible et diffable ; résultats interrogeables, comparaisons d'instantanés en SQL | Binaire maison : opaque |
| Application de bureau | **Tauri 2** | Léger (WebView système), cœur Rust en processus, multiplateforme, mise à jour signée | Electron : lourd ; Qt : interface moins moderne et licence |
| Interface | **TypeScript + React** (ou SolidJS), **Vite**, **TanStack Table/Virtual**, **React Flow + elkjs** (diagrammes et vue graphique), **Monaco** (éditeur et testeur de regex), **Tailwind** + Radix (composants accessibles) | Grilles virtualisées pour 100 000 lignes ; diagrammes avec mise en page automatique ; accessibilité | — |
| Rapports | Moteur de gabarits **MiniJinja** sur un modèle de requête ; sorties HTML, **XLSX** (`rust_xlsxwriter`), **DOCX** (`docx-rs`), CSV, PDF (via **Typst** embarqué), AsciiDoc/Markdown | Formats équivalents aux `doc_templates` livrés | — |
| Script et règles | Règles **déclaratives** (expression sur le modèle, voir §5.4) ; scripts utilisateur en **Rhai** (embarqué, sandboxé) | Remplace le langage de script propriétaire sans le copier | Python embarqué : distribution lourde |
| Plugins de connecteurs | **Sous-processus JSON-RPC sur stdio** (n'importe quel langage), et à terme **WASM** (`wasmtime`, WASI) | Isolation des API tierces (Jira, Polarion…), plugins écrits en Python, Java ou .NET possibles | Bibliothèques dynamiques : ABI fragile |
| Serveur (plus tard) | `axum` exposant le même cœur et la même interface web en lecture | Équivalent de l'accès web | — |

## 3. Découpage en modules

```text
reqticraft/
├─ crates/
│  ├─ rc-model        # types de données (§4), sérialisation native
│  ├─ rc-interop      # lecture/écriture .rqtf (XML+INI), .types (XML+INI), .filters, .marks,
│  │                  #   ModificationDocument, Reviewer XML ; rapport d'import
│  ├─ rc-convert      # convertisseurs de fichiers : text, docx, xlsx/csv, pdf, code, xml, json,
│  │                  #   html, markdown, rtf, odt, pptx, mif, reqif, vsdx, mspdi, xmi, médias, propriétés
│  ├─ rc-connect      # types d'accès (zip, http, git, svn…), outils ALM, journaux de versions, bases
│  ├─ rc-capture      # moteur de capture : regex (texte) et arbres d'analyse (XML) → éléments
│  ├─ rc-analysis     # résolution des références, couverture, impact, règles, filtres
│  ├─ rc-snapshot     # instantanés SQLite, comparaison, historique d'élément
│  ├─ rc-report       # requêtes de rapport, gabarits, rendus
│  ├─ rc-plugin       # hôte de plugins (JSON-RPC, WASM)
│  ├─ rc-core         # façade : chargement, conversion, capture, analyse ; bibliothèque de types intégrée
│  └─ rc-cli          # binaire `reqticraft`
├─ app/               # Tauri : src-tauri (commandes → cœur) + ui (React/TS)
├─ plugins/           # connecteurs hors cœur (jira, polarion, reqif…)
├─ fixtures/          # corpus de test **maison** (aucun fichier Dassault versionné)
└─ docs/
```

Flux d'une analyse :

```text
 .rqtf / projet natif ──► rc-interop ──► Project + DocumentTypes
                                             │
 sources ──► rc-convert (cache SHA-256) ──► Intermédiaire (texte | XML)
                                             │
                                  rc-capture (type d'analyse)
                                             │
                            Éléments par document (+ ModificationDocuments)
                                             │
                rc-analysis : résolution ► couverture ► règles ► filtres
                                             │
                AnalysisResult ──► UI / CLI / rc-report / rc-snapshot
```

## 4. Modèle de données

Rust en notation simplifiée. Les identifiants internes sont des `u32` denses (arènes), et les noms
logiques restent des chaînes.

```rust
struct Project {
    name: String,
    properties: Vec<(String, String)>,      // Feature, boostPurgeTypes, … (multivaluées)
    nodes: Vec<ProjectNode>,                // documents, dossiers, sous-projets
    channels: Vec<Channel>,
    path_replacements: Vec<PathReplacement>,// ReplacingPath (composites)
    annotations: Vec<Annotation>,
    categories: Categories,                 // documents et canaux
    active_rules: RuleSelection,
    layout: Layout,                         // positions éditeur + cadres vue graphique
}

enum ProjectNode {
    Document(DocumentDecl),
    Folder { name: String, default_type: Option<TypeRef>, children: Vec<NodeId> },
    SubProject { name: String, path: PathBuf, visible_docs: Vec<String> },
}

struct DocumentDecl {
    name: String, doc_type: TypeRef, path: PathBuf, absolute_path: Option<PathBuf>,
    category: Option<String>, params: Vec<(String, String)>,   // variables du type
    modification_docs: Vec<ModificationDocRef>, ignored_types: Vec<String>,
    keep_intermediate: bool, block_reload: bool, ignore_structure: bool, rich_text: bool,
}

enum ChannelKind { Cover, Mirror { compare: MirrorCompare }, Dependency }
struct Channel {
    kind: ChannelKind,
    covered: NodeId,                // amont (outputDocument)
    covering: Vec<NodeId>,          // aval ; len > 1 ⇒ combinaison OU
    category: Option<String>, via_port: Option<PortRef>,
}

struct DocumentType {
    name: String, folder: String, origin: TypeOrigin,     // bibliothèque | projet | importé
    kind: TypeKind,                                       // Text | Xml | AddedElements
    converter: String, edit_tool: Option<String>, post_processor: Option<String>,
    file_selection: FileSelection,                        // extensions, motifs, exclusions, profondeur
    encoding: Option<Encoding>, variables: Vec<VariableDecl>,
    options: TypeOptions,                                 // homonymes, GUID, CRC, exclusions, suppression
    element_types: Vec<ElementType>, terminals: Vec<TerminalType>,
}

enum ElementClass { Section, MacroRequirement, Requirement, Entity, Reference,
                    Attribute, ReferenceAttribute, Link, Value }

struct ElementType {
    name: String, class: ElementClass,
    rule: CaptureRule,              // Regex { re, sub, end, roles } | XmlTree(TreePattern)
    parents: Vec<String>, targets: Vec<String>,
    display: Option<String>, text_display: Option<String>,
    ident_format: Option<String>, guid_format: Option<String>,
    flags: ElementFlags,            // visible, ignored, multiple, nested, inverse, boolean, external…
    enum_values: Vec<EnumValue>,
}

struct Element {                    // résultat de capture
    id: ElemId, doc: DocId, etype: ETypeId, class: ElementClass,
    ident: Option<String>, guid: Option<String>, label: Option<String>,
    text: RichText, parents: SmallVec<[ElemId; 1]>,       // plusieurs parents possibles
    attributes: Vec<AttrValue>, source: SourceSpan, crc: u64,
    origin: Origin,                                       // capturé | document de modification | ajouté
}

struct Reference {                  // couverture ou lien
    id: RefId, kind: RefKind,       // Cover | Link(type) ; inverse
    source: ElemId, target_ident: String, target_doc: Option<String>,
    attributes: Vec<AttrValue>, resolved: SmallVec<[ElemId; 1]>,
}

struct AnalysisResult {
    elements: Arena<Element>, references: Arena<Reference>,
    coverage: Vec<ChannelCoverage>,   // par canal : couverts / total, détail par exigence
    combined: Vec<CombinedCoverage>,  // OU et ET (vue de gestion)
    violations: Vec<Violation>,       // rule_id, sévérité, éléments, message
    derived: BitSet,                  // exigences dérivées
    changes: Vec<ElementChange>,      // par rapport à l'analyse précédente
}

struct Filter { name: String, combine: AndOr, scope: Display | Analysis,
                conditions: Vec<Condition>, params: Vec<FilterParam>,
                hide_children: bool, hide_link_if_target_hidden: bool, show_in_list: bool }
struct Mark { name: String, color: Rgb, label: String }
struct Snapshot { id: Uuid, name: String, taken_at: DateTime, result: SnapshotStore /* SQLite */ }
struct Checklist { guid: Uuid, name: String, items: Vec<ChecklistItem> }
struct ReviewResult { item: Uuid, element: ElementKey, status: ReviewStatus, cars: Vec<Car>, audit: Audit }
```

Points de conception :

- **`ElementKey` stable** (document + type + identifiant ou GUID) pour rattacher marques, revues et
  ajouts aux éléments entre deux analyses. C'est ce qui manque visiblement dans le format Reviewer
  d'origine (`object` = entier opaque).
- Comparaison des identifiants **insensible à la casse** par défaut et des GUID sensible à la casse,
  avec option de projet [UG « Requirement »].
- **Projet natif** `*.rcproj.toml` qui référence les types (`types/*.toml`), les filtres et les
  marques. Les éléments ajoutés restent dans des documents de modification XML **au format Reqtify**
  (§7.2 d'`interop-formats.md`), ce qui permet un aller-retour sans perte.

## 5. Composants clés

### 5.1 Convertisseurs (`rc-convert`)

```rust
trait Converter {
    fn id(&self) -> &str;                        // "WordWithStyles", "ExcelWithColNumbers", "text"…
    fn accepts(&self, path: &Path) -> bool;
    fn convert(&self, input: &Input, params: &Params) -> Result<Intermediate>;  // Text | Xml
}
```

- Une table d'alias associe les `convertTool` Reqtify aux implémentations ReqtiCraft. Un
  convertisseur absent déclenche la règle `unknownConvertMethod`, et le rapport d'import le signale.
- Les convertisseurs docx, xlsx et PDF émettent le **même format de ligne** que celui documenté
  (`interop-formats.md` §5), validé par des fichiers de référence (§7).
- Le convertisseur PDF fonctionne en trois passes : extraction des glyphes (PDFium), regroupement en
  lignes puis en paragraphes selon des règles paramétrables (police, gras, italique, couleur, taille,
  interligne, retrait, phrases entières), puis détection des tableaux, images, signets (titres) et
  annotations. Chaque règle est une option du type, comme dans l'outil d'origine.
- Cache : `sha256(source) + version du convertisseur + paramètres`, puis intermédiaire en cache.
  L'intermédiaire peut être affiché dans l'interface (débogage des types).

### 5.2 Moteur de capture (`rc-capture`)

- **Texte** : parcours ligne à ligne, avec des fenêtres multilignes pour les expressions contenant
  `\n` ou `[\s\S]`. Pile de contextes ouverts (section, macro-exigence, exigence, entité) avec
  expressions de fin et règle du « prochain élément de rang supérieur ou égal ». Rattachement des
  terminaux (texte, tableau, image) à l'élément courant. Sous-expressions, préfixes, formats
  d'identifiant, exclusions de texte, expressions de suppression.
- **XML** : compilation de l'`analysisTree` en motif d'arbre (tags et attributs = conditions regex,
  rôles marqués, nœuds négatifs, nœud de référence, variables `${…}`), puis correspondance sur un DOM
  `roxmltree`.
- **Testeur intégré** : même moteur, résultats en direct dans Monaco (groupes colorés par rôle).

### 5.3 Analyse (`rc-analysis`)

- Index des identifiants (casse normalisée) et des GUID, puis résolution des références : cibles
  multiples si homonymes, références inverses, propagation depuis les macro-exigences.
- Validation de chaque référence par rapport aux canaux (`traceabilityGraphViolation`).
- Couverture selon les formules d'UG « Understanding the Coverage Ratios » : par canal, OU combinée,
  ET (vue de gestion), avec et sans filtre d'analyse.
- Impact : parcours de graphe orienté (amont et aval, profondeur illimitée, détection de cycles).
- Calcul parallèle par document avec `rayon`. Seuls les documents dont l'empreinte change sont
  recapturés.

### 5.4 Règles

- Les règles intégrées reprennent les **comportements** listés dans le cahier des charges §3.8,
  réimplémentés à partir de leur description, avec les mêmes **identifiants publics**
  (`uncovered`, `coverUnknown`…) pour faciliter la migration.
- Règles utilisateur déclaratives, par exemple :
  `rule "high-priority-tested" severity=error: requirement where attr("Priority")=="High" and not covered_by(category="Tests")`.
- Scripts Rhai pour les cas avancés, avec une API en lecture seule sur `AnalysisResult`.

### 5.5 Interface (Tauri + React)

- Commandes Tauri fines et typées (`open_project`, `analyze`, `query_elements`, `coverage_of`,
  `impact_of`, `apply_filter`…), et événements de progression en flux.
- Les grandes collections sont **paginées et virtualisées** côté cœur : l'interface ne reçoit jamais
  tout le modèle.
- Écrans : Accueil, Tableau de bord, Couverture, Impact, Graphe, Détails des exigences, Matrice des
  liens, Texte, Problèmes (règles), Éditeur de projet (diagramme), Éditeur de types (avec testeur),
  Filtres, Instantanés (comparaison côte à côte), Rapports, Revues.
- Navigation : historique, palette de commandes (Ctrl+K), recherche plein texte (index `tantivy`),
  ouverture du document source à l'élément (protocole par éditeur : Word, VS Code `file:line`…).
- Surveillance des fichiers (`notify`) : bandeau « document modifié, recharger ? ».

### 5.6 CLI

```text
reqticraft import   <projet.rqtf> [--to native-dir] [--report import.md]
reqticraft analyze  <projet> [--filter NAME] [--fail-on error|warning] [--json out.json]
reqticraft report   <projet> --template matrix --format xlsx -o matrix.xlsx
reqticraft snapshot <projet> create|list|diff <A> [<B>]
reqticraft types    test <type> <fichier>    # testeur de regex en ligne de commande
reqticraft convert  <fichier> --converter docx   # affiche l'intermédiaire
```

## 6. Sécurité et conformité

- Les plugins tournent en sous-processus ou en WASM, avec des permissions déclarées (réseau, fichiers).
- Les identifiants de connexion aux serveurs ALM sont stockés dans le trousseau du système (`keyring`),
  jamais dans le projet.
- Signature des exécutables et mise à jour signée (Tauri updater).
- Clean room : aucun fichier Dassault dans le dépôt (exemples, types, gabarits, PDF). Les tests
  d'interopérabilité sur le corpus d'exemples Reqtify s'exécutent **localement**, à partir d'un chemin
  configuré (`REQTICRAFT_REQTIFY_EXAMPLES`), et sont ignorés en CI publique. Les types par défaut de
  ReqtiCraft sont **réécrits** à partir des conventions documentées (par exemple `[Covers: …]`,
  `REQxx : libellé`), et non copiés.

## 7. Stratégie de test

| Niveau | Contenu |
|---|---|
| Unitaire | Parseurs INI et XML, moteur regex, formules de couverture (cas des figures du User Guide : 25 % / 50 % / 75 %, filtres 90 %) |
| Fichiers de référence | Paires *document maison → intermédiaire attendu*, obtenues en boîte noire avec l'option « Intermediate File » de Reqtify |
| Interop locale | Pour chacun des 50 projets d'exemple : import sans erreur, puis comparaison des exigences, références et taux avec les valeurs **affichées** par Reqtify (relevées manuellement ou par export de rapport) |
| Acceptation | Parcours P1 du tutoriel : 6 exigences, 83 %, 1 erreur, puis 100 % après modification, puis impact sur 3 niveaux et erreur `PS_AO_REQ199` |
| Performance | Corpus synthétique de 50 000 exigences sur 20 documents : analyse complète en moins de 10 s, incrémentale en moins de 1 s |
| Interface | Playwright sur la WebView ; tests de composants (Vitest) |

## 8. Plan de MVP par étapes

Chaque étape produit un livrable démontrable. Durées indicatives pour 1 à 2 développeurs.

**M0 — Fondations (1 semaine).** Workspace Cargo, CI (fmt, clippy, tests), squelette Tauri,
conventions, `docs/interop-evidence/`, corpus de fixtures maison (texte, docx, xlsx).

**M1 — Lecture des projets (2 semaines).** `rc-model` et `rc-interop` : `.rqtf` XML et INI,
`.types` XML et INI, `.filters` XML et INI, `.marks`, documents de modification. Commande
`reqticraft import` avec rapport d'import.
*Critère* : les 50 `.rqtf` d'exemple et les 89 `.types` se chargent sans erreur de syntaxe.

**M2 — Moteur texte et analyse (3 semaines).** Convertisseur `text`, moteur de capture regex
complet (sections, exigences, macro-exigences, références avec sous-expressions, attributs, texte,
tableaux), résolution, couverture (par canal, ET, OU), impact, règles principales (`uncovered`,
`coverUnknown`, `traceabilityGraphViolation`, dérivées, `requirementMultipleTimesDefined`,
`coversItself`, `sameIdentAndParent`, `badParentType`, `openingWithoutClosing`/`closingWithoutOpening`).
CLI `analyze --json`.
*Critère* : `EX\Coupling\Text\Text demo.rqtf` donne les mêmes résultats que Reqtify (REQ1-5 ;
LL-REQ1 couvre REQ1 ; LL-REQ2 couvre REQ1, REQ2 et REQ3 ; couverture 3/5 = 60 %).

**M3 — Convertisseurs documents (5 semaines).** docx (styles de paragraphe, numérotation, tableaux,
styles de caractères, images), xlsx/csv (feuilles, colonnes), **PDF** (paragraphes, titres par
signets, tableaux, images, annotations, pages exclues, mot de passe), code source (C, C++, Java, C#).
Fichiers de référence.
*Critères* : le parcours P1 du tutoriel fonctionne en CLI avec des documents **docx maison** reproduisant
la structure décrite dans le guide (6 exigences, 83 %). Le même tutoriel exporté en **PDF** donne les
mêmes exigences et la même couverture avec le type PDF par défaut.

**M4 — Interface v1 (4 semaines).** Ouverture et import de projet, tableau de bord, vues Couverture et
Impact, Détails des exigences, panneau Problèmes, ouverture du document source, rechargement à chaud.
*Critère* : parcours P1 complet dans l'interface.

**M5 — Édition et filtres (3 semaines).** Éditeur de projet (diagramme : documents, canaux, combinaisons,
catégories), éditeur de types avec testeur de regex, filtres (affichage et analyse, paramètres),
enregistrement au format natif et export `.rqtf` XML.

**M6 — Instantanés et rapports (3 semaines).** Instantanés SQLite, comparaison, historique d'élément ;
rapports Matrice de traçabilité, Résultats d'analyse, Description du projet et Contrôle des règles
(HTML, XLSX, DOCX, CSV).
*Critère* : parcours P4 et P5 (CI avec `--fail-on`).

**MVP livrable à la fin de M6, soit environ 21 semaines.**

Après le MVP :

- **M7** : éléments ajoutés depuis l'interface (liens, attributs) écrits en documents de modification,
  marques, Copier pour…, annuler et rétablir ;
- **M8** : vue graphique, matrice des liens interactive, types XML (arbres d'analyse) et convertisseur JSON ;
- **M9** : protocole de plugins, puis ReqIF, Jira, Polarion, Git ;
- **M10** : revues (checklists, résultats, CAR), projets composites, serveur web en lecture.

## 9. Risques

| Risque | Impact | Parade |
|---|---|---|
| Écarts fins entre nos intermédiaires docx/xlsx et ceux de Reqtify, qui cassent les regex clients | Élevé | Fichiers de référence en boîte noire ; mode « compatibilité » par convertisseur ; affichage de l'intermédiaire dans l'interface |
| Reconstitution des paragraphes PDF (mises en page multicolonnes, tableaux sans bordures, PDF scannés) | Élevé | Règles paramétrables par type ; corpus PDF varié dans les tests ; OCR hors MVP (PDF scannés signalés par une règle) |
| Dialecte regex (PCRE 16 bits, `\w` Unicode, multilignes) | Moyen | PCRE2 avec options alignées ; tests sur toutes les expressions de la bibliothèque |
| Syntaxes non observées (dossiers, miroir, Booleans…) | Moyen | Protocole boîte noire Q1-Q8 (`interop-formats.md` §10), à mener **avant M5** |
| Pas d'import de l'historique `.rqtfimage` | Moyen (attente client) | Le documenter ; reconstituer l'historique depuis Git |
| Connecteurs propriétaires (DOORS, CATIA, ENOVIA) | Élevé pour certains clients | Plugins tiers ; priorité aux formats d'échange (ReqIF, XML d'export) |
| Propriété intellectuelle et marques | Élevé | Clean room documentée, aucun fichier Dassault dans le dépôt, nom et interface originaux |

## 10. Avancement (2026-10-29)

| Étape | État |
|---|---|
| M0 Fondations | Fait |
| M1 Lecture des projets | Fait : `.rqtf`, `.types`, `.filters`, `.marks`, documents de modification, dans les deux formats ; 50/50 projets d'exemple lus |
| M2 Moteur et analyse | Fait : capture par expressions (sections, profondeur, macro-exigences, exigences, entités, références, attributs, textes, exclusions, suppressions) ; couverture par canal, ET, OU ; dérivées ; règles principales ; CLI `analyze` |
| M3 Convertisseurs P0 | Fait : texte, docx, xlsx/xls/ods/csv (texte et XML), PDF, code (répertoires), JSON, XML ; arbres d'analyse XML ; projets composites |
| Connecteurs P1 et P2 | Fait (avant M4, à la demande) : voir ci-dessous et `interop-formats.md` §12 |
| M4 à M6 | À faire (interface, édition, instantanés, rapports) |

Écarts par rapport au plan : le PDF utilise `pdf-extract` plutôt que PDFium (voir §2) ; un crate
`rc-core` regroupe l'orchestration commune à la CLI et à l'application. Limites connues du P0 : pas de
`.doc` binaire, pas d'analyse syntaxique du code (fonctions), pas d'images ni de tableaux restitués,
filtres lus mais pas encore appliqués.

**Connecteurs P1 et P2.**

| Famille | Réalisé | Vérifié par |
|---|---|---|
| Documents | HTML, Markdown, AsciiDoc, RTF, OpenDocument Texte, PowerPoint `.pptx`, FrameMaker MIF | Corpus maison complet (mêmes résultats que les formats P0) ; exemples Reqtify AsciiDoc, OpenOffice Writer et Calc avec leurs types |
| Formats structurés | ReqIF/ReqIFz, Visio `.vsdx`, MS Project XML, UML/SysML XMI et `.mdzip`, médias (MediaInfo ou lecteur intégré), propriétés de fichiers | Corpus maison (ReqIF complet ; autres en sources couvrant la spécification) |
| Types d'accès | ZIP (7-Zip par `7z`), HTTP(S)/FTP, Git, GitHub, Bitbucket, Subversion, CVS, Perforce, Google Drive (Docs, Sheets, Slides exportés), OneDrive, commande externe (ClearCase, Synergy, PVCS, Documentum, ProjectWise, DesignSync) | ZIP, HTTP, Google Drive et Git testés (serveur HTTP simulé, dépôt Git réel) ; Subversion, CVS, Perforce et OneDrive non testés faute d'outil ou de serveur |
| Outils ALM | Jira, Redmine, Bugzilla, codebeamer, Jama, Polarion, DOORS Next (API de publication), OSLC Change Management | Jira et Redmine contre un serveur simulé (pagination, authentification) ; tous en mode fichier sur le corpus maison ; aucun serveur réel |
| Historiques | Git, Subversion, Perforce (forme `svn log --xml`) | Git sur un dépôt réel ; Perforce sur une sortie type |
| Bases de données | Access et ODBC (PowerShell, Windows), Oracle (`sqlplus`), SQLite (`sqlite3`) | Exemple Reqtify Access : 5 + 5 exigences, couverture 100 % ; Oracle et SQLite non testés |
| Plugins | Protocole JSON-RPC sur stdio (`docs/plugin-protocol.md`), découverte dans `plugins/` | Plugin de démonstration lancé par les tests |

Écarts : les connecteurs P1/P2 sont intégrés au cœur plutôt qu'en plugins (principe 5) : ce sont des
lecteurs de formats ouverts ou de simples clients REST, sans dépendance propriétaire. Le protocole de
plugins reste la voie des connecteurs propriétaires. Le registre crates.io étant injoignable pendant
le développement, aucune nouvelle bibliothèque n'a été ajoutée hormis `html5ever` (déjà en cache) :
transport HTTP par `curl`, ODBC par PowerShell, Markdown, RTF et MIF lus par du code maison.
