# ReqtiCraft — Cahier des charges fonctionnel

> Statut : brouillon v0.1 — 2026-10-28
> Cadre : **clean room**. Les fonctionnalités ci-dessous viennent uniquement de la documentation
> officielle livrée avec Reqtify 2026x Refresh1 (`doc\*.pdf`), des fichiers de configuration et
> d'exemple livrés, et du comportement visible décrit dans ces documents. Aucune logique interne de
> `reqtify.exe` n'a été examinée ni reproduite. Les références de type `[UG §…]` renvoient aux
> sources listées en fin de document.
>
> ReqtiCraft est un **outil original** : il reprend le *problème métier* (traçabilité des exigences
> entre documents hétérogènes) et assure l'**interopérabilité** avec les projets existants, mais il
> ne copie ni l'interface, ni le code, ni la marque d'origine.

---

## 1. Vision et périmètre

ReqtiCraft relie des exigences définies dans des documents et des bases (Word, Excel, code source,
outils ALM…) aux documents de conception et de vérification qui les couvrent. Il calcule la
**couverture**, l'**analyse d'impact**, contrôle des **règles de cohérence**, compare des **instantanés**
et produit des **rapports** [GS « Product Overview »].

Principe clé, à conserver : l'outil **ne stocke pas les exigences**. Il les **capture** dans les
documents sources, qui restent la référence. Le projet ne décrit que *quels* documents analyser,
*comment* les lire (types d'analyse) et *quelles relations* sont attendues entre eux (canaux)
[UG « Capture and Analysis Process »].

Objectifs de modernisation (propres à ReqtiCraft) :

- interface rapide et moderne (thème clair/sombre, recherche globale, palette de commandes, raccourcis) ;
- multiplateforme (Windows, macOS, Linux), sans dépendance à COM ou à Office pour les formats ouverts (docx, xlsx) ;
- mode **sans interface** (CLI) pour l'intégration continue : analyse, contrôle des règles, rapports ;
- formats natifs ouverts et lisibles, versionnables dans Git ;
- **import fidèle** des projets Reqtify existants (`.rqtf`, `.types`, `.filters`, `.marks`, documents de modification).

Hors périmètre de la v1 : l'édition des documents sources dans l'outil, l'hébergement multi-utilisateur
temps réel, la lecture des résultats binaires `.rqtfimage` (voir `interop-formats.md` §8).

## 2. Processus de capture et d'analyse

Le pipeline documenté comporte cinq étapes [UG p.10 ; CG « Capturing Information from Interfaced Tools »] :

1. **Conversion** : un *convertisseur* transforme le document natif (Word, Excel, DOORS…) en un
   **fichier intermédiaire**, texte ou XML.
2. **Capture** : le *type d'analyse* du document (expressions régulières pour un intermédiaire texte,
   arbres d'analyse pour un intermédiaire XML) identifie les éléments de traçabilité.
3. **Analyse** : la configuration du projet (documents et canaux) permet de résoudre les références,
   de calculer les taux de couverture et de contrôler les règles. Le résultat est un « résultat d'analyse ».
4. **Filtrage** : des filtres d'affichage ou d'analyse restreignent les résultats.
5. **Restitution** : vues, rapports, exports vers les outils tiers.

ReqtiCraft **garde cette architecture en deux temps** (convertisseur, puis capture pilotée par le type).
C'est elle qui rend réutilisables les milliers de lignes de types d'analyse déjà écrites par les clients.

## 3. Concepts

### 3.1 Projet

- Un projet contient des **documents**, des **dossiers** (regroupements hiérarchiques de documents),
  des **références de sous-projets** (projets composites), des **canaux** entre documents ou dossiers,
  des **annotations** graphiques, des **propriétés** (variables) et des **catégories** de documents et
  de canaux [UG « Project Configuration »].
- L'éditeur de projet est un **diagramme** : chaque document est un nœud positionné, chaque canal une
  flèche du document couvrant vers le document couvert [GS p.14-16].
- Options de projet : police, mot de passe de projet, variables d'environnement, sélection des règles
  actives, profils de connexion aux serveurs [GS p.11 ; UG « Project Options »].
- Contrôle d'accès : verrouillage de l'enregistrement par utilisateur, gestion des accès concurrents
  [GS p.8 ; UG « Configuration Window »].

### 3.2 Document

Propriétés visibles [GS p.14 ; exemples `.rqtf`] :

- nom, **type d'analyse**, chemin du fichier ou du répertoire (relatif au projet, avec chemin absolu en option) ;
- catégorie (par exemple Requirements, Specification, Design, Tests) ;
- paramètres propres au type (images, texte riche, styles de caractères, encodage…) ;
- **documents de modification** attachés (voir §3.9) ;
- options : conserver le fichier intermédiaire, bloquer le rechargement, ignorer la structure.

Document **multitype** : un répertoire analysé comme un ensemble de fichiers du même type, avec
l'arborescence répertoire et fichier restituée [CG « Customizing Multitypes »].

### 3.3 Types d'analyse

Un type d'analyse définit comment sélectionner les fichiers (extensions, filtres, exclusions), quel
convertisseur et quel outil d'édition utiliser, et comment interpréter le contenu en éléments
[GS p.13 ; CG « Types Editor »].

Trois familles [CG « Defining Document Types »] :

- **type fichier**, dont l'intermédiaire est du texte : capture par expressions régulières ;
- **type XML**, dont l'intermédiaire est du XML : capture par *arbres d'analyse* (motif d'arbre XML
  avec marqueurs d'identifiant, de libellé, de texte, de GUID, de parent, de cible…) ;
- **type pour éléments ajoutés** : définit les exigences, références, liens et attributs que
  l'utilisateur peut créer depuis l'outil (attributs booléens, valeurs, énumérations).

Bibliothèque : les types livrés sont regroupés en dossiers (Code, Database, Design, Office, Project,
Requirement Tools, Test, UML) et sont en lecture seule. Les types propres à un projet sont stockés
dans le répertoire du projet [UG « Overview of the Product Work Files »].

Options avancées d'un type : fusion ou remplacement des homonymes, utilisation des GUID pour la
comparaison, profondeur de répertoire, expressions d'exclusion de texte, expression de suppression
d'élément, contrôle CRC du texte non capturé, sous-arbres XML exclus, variables d'initialisation
[CG « Advanced Options Tab »].

Fonctions de l'éditeur de types : créer, dupliquer, copier-coller, trier, convertir en multitype,
passer de l'ancien au nouveau format de fichier, **testeur d'expressions régulières** et **testeur de
syntaxe XML** sur un fichier, un répertoire ou un intermédiaire [CG « Testing Regular Expressions » ; « Testing XML Syntaxes »].

### 3.4 Éléments de traçabilité

[UG « Traceability Elements » ; CG « Left Area »]

| Élément | Rôle |
|---|---|
| **Section** | Structure hiérarchique du document (titres Word, feuilles Excel, paquetages UML…). |
| **Macro-exigence** | « Super-exigence » qui contient des exigences et leur transmet ses propriétés (attributs, texte, liens, couvertures), sans duplication : l'élément ajouté a plusieurs parents. Peut être hiérarchique. |
| **Exigence** | Besoin ou contrainte, avec un identifiant unique ou un GUID. Identifiants comparés sans tenir compte de la casse, GUID en tenant compte de la casse (paramétrable). |
| **Exigence dérivée** | Exigence d'un document couvrant qui ne couvre aucune exigence amont. Le terme « dérivé » est renommable. |
| **Exigence feuille** | Exigence ou macro-exigence qui ne contient aucune exigence. |
| **Entité** | Élément qui *doit* couvrir une exigence (par exemple pour détecter du code mort). Ne peut pas être référencée. |
| **Référence** | Information de couverture qui pointe vers une exigence, une macro-exigence ou une exigence dérivée. Direction ascendante (le bas couvre le haut) ou descendante (« est couvert par », option *inverse*). Plusieurs exigences homonymes : une référence vers chacune. |
| **Attribut** | Complète l'exigence (priorité, criticité, maturité…). Booléen, valué, multivalué, énuméré, date. Attributs *génériques* : le nom vient de la capture. |
| **Attribut de référence** | Qualifie une couverture (partielle, provisoire…). |
| **Lien** | Relation hors couverture (« vérifié par », « alloué à »…), typée. |
| **Texte** | Corps d'un élément, attaché à l'élément capturé juste au-dessus. Texte riche : couleurs, gras, italique, listes, tableaux, images, objets OLE, hyperliens. |
| **Image** / **Tableau** | Éléments terminaux capturés pour l'affichage (BMP, PNG, JPEG, WMF, EMF, SVG). |

Propriétés de capture communes : identifiant, libellé, texte, GUID, parent, profondeur, chacune
affectée à un groupe de l'expression ; affichage paramétrable par symboles (`$i` identifiant, `$l`
libellé, `$t` type, `$T` texte, `${Attribut}`…) ; formats de reconstruction d'identifiant (préfixe du
nom de document, nom de fichier, identifiant du parent, hachage du texte…) [CG « Display Symbols » ; « Formatting »].

### 3.5 Canaux (relations entre documents)

[UG « Channels » ; « Covering a Document by Combination of Several Others » ; « Using Folders »]

- **Couverture** (*cover*) : toutes les exigences du document couvert doivent être couvertes par des
  éléments du ou des documents couvrants.
- **Miroir** (*mirror*) : toutes les exigences amont doivent être dupliquées en aval, avec les mêmes
  identifiants. Comparaison optionnelle du libellé, du texte et des attributs. Une variante « OU » existe.
- **Dépendance** : relation informative, sans contrainte.
- **Combinaisons** :
  - *ET* : canaux indépendants vers plusieurs documents. L'exigence n'est couverte que si elle est
    référencée dans **tous** les documents couvrants (par exemple conçue ET testée) ;
  - *OU* : canal combiné. L'exigence est couverte dès qu'**au moins un** document couvrant la référence
    (par exemple logiciel OU matériel).
- **Dossiers** : canaux possibles entre dossiers, entre un dossier et un document, entre documents d'un
  même dossier, et via des **ports** pour relier un document interne à un document externe.
- **Catégories de canaux**, configurables.

Une référence qui ne suit aucun canal déclaré déclenche la règle *Traceability Graph Violation*
[CG « Project Definition »].

### 3.6 Couverture

Formules documentées [UG « Understanding the Coverage Ratios »] :

- **Cas général** (par canal) : nombre d'exigences du document couvert référencées par le couvrant,
  divisé par le nombre d'exigences du document couvert. Si le document couvert ne contient aucune
  exigence, le taux vaut 0 %.
- **Couverture combinée (OU)** : exigences référencées dans au moins un couvrant, divisées par le
  nombre d'exigences du document couvert.
- **Vue de gestion (ET)** : exigences référencées dans **tous** les couvrants, divisées par le total.
- **Avec filtre d'analyse** : numérateur et dénominateur sont calculés après filtrage. Un filtre
  d'affichage ne modifie pas les taux.
- Indicateur de **qualité globale** : pourcentage d'exigences et d'entités feuilles sans erreur ni
  avertissement (observé dans `config\presentations\management.xml`).

### 3.7 Analyse d'impact

- **Vue couverture** : pour l'élément sélectionné, un niveau amont (N-1) et un niveau aval (N+1), avec
  le taux de couverture du document [GS p.9, p.19-23].
- **Vue impact** : tous les niveaux amont (N-m) et aval (N+p), avec distinction entre exigences
  dérivées et non dérivées [GS p.24-29].
- Navigation : double-clic pour suivre un lien, historique Précédent/Suivant, ouverture du document
  source à l'emplacement de l'élément [GS p.22 ; UG « Edit Menu »].
- Détection des **éléments modifiés** depuis la dernière analyse (icône de changement), rechargement
  proposé quand un document source change sur disque [GS p.23 ; UG « Detection of Requirement Changes »].

### 3.8 Contrôle des règles

Les règles sont classées par sévérité : erreur (rouge), avertissement (orange), information (verte,
sans couleur, ou masquée) [UG « Handling Rules »]. On peut activer ou désactiver chaque règle par
projet, et un explorateur de règles navigue entre les violations [UG « Rule Check Explorer »].

Règles documentées, avec leur nom interne [UG « Errors listed in Rules Check », pages 123-135] :

- **Couverture et traçabilité** : `uncovered`, `coverUnknown` (lien vers une exigence non définie),
  `linkToUnknown`, `unknownCoveringEntity`, `traceabilityGraphViolation`, `coversItself`,
  `coveringNothing` (entité non couvrante), `inverseReferenceWithoutRequirement`,
  `coverLinkSourceNotFound`, `linkSourceNotFound`, `documentNotFound`.
- **Miroir** : `missingMirrorRequirement`, `mirrorRequirementWithoutMain`, `badMirrorAttributeValue`.
- **Unicité et structure** : `requirementMultipleTimesDefined`, `macroRequirementMultipleTimesDefined`,
  `multipleUseOfSameGUID`, `sameIdentAndParent`, `cyclicGraph`, `badParentType`, `badEntityLevel`,
  `entityFoundInMacro`, `openingWithoutClosing`, `closingWithoutOpening`, `unknownParentElement`,
  `unknownChildElement`, `badAttributeValue`.
- **Configuration et technique** : `analyseError`, `conversionProblem`, `unknownConvertMethod`,
  `unreadableDocument`, `fileAccessError`, `intermediateFileAccessError`, `unknownCoveringDocument`,
  `unknownEmbeddedDocument`, `unknownReplacingDocument`, `unknownDocumentType`, `unknownElementType`,
  `unknownDeletedElement`, `tooManyMatchesFound`, `postProcessingError`, `postAnalyzingError`,
  `postDownloadingError`, `ruleException`, `comError`.
- **Avertissements** : exigence dérivée, attribut ou lien défini plusieurs fois, mauvais niveau de
  section, élément modifié absent du document d'origine, attribut de référence sans référence, parent
  défini plusieurs fois, etc.

**Règles utilisateur** : l'outil d'origine les écrit dans un langage de script propriétaire. ReqtiCraft
proposera à la place des règles déclaratives (requêtes sur le modèle) et un langage de script
généraliste (voir l'architecture). L'import des règles de script existantes est hors périmètre.

### 3.9 Création d'informations depuis l'outil

[UG « Creating Information » ; CG « Defining Element Types for Added Elements »]

- Ajout de liens de couverture, de liens, d'attributs et d'attributs de référence depuis les vues,
  sans modifier le document source. Ces informations sont stockées dans des **documents de
  modification** (XML) rattachés au document.
- Suppression de liens, annuler et rétablir.
- **Copier pour…** : copie dans le presse-papiers la syntaxe de référence attendue par le type du
  document couvrant (par exemple `[Covers: REQ_1]`), pour l'insérer dans le document source.
- Documents de modification « de revue » : fiches qui complètent, modifient ou remplacent le
  contenu d'une version de document.

### 3.10 Marques

Étiquettes colorées que l'utilisateur pose sur des éléments (par exemple « Approuvé »). La mise en
évidence des modifications et des résultats de recherche est automatique et persistante. Les marques
restent visibles sur les éléments supprimés et sont utilisables dans les filtres. Raccourcis de pose
[UG « Adding Marks »].

### 3.11 Filtres

[UG « Filters Usage and Advanced Analysis »]

- Un filtre est un ensemble de **conditions** combinées en ET ou en OU.
- Il s'applique à l'**affichage** (masquage seul) ou à l'**analyse** (les éléments filtrés sont
  ignorés et les taux recalculés).
- Options : masquer les enfants des éléments filtrés, masquer un lien dont la cible est masquée,
  enregistrer la disposition graphique dans le filtre, afficher ou non le filtre dans la liste.
- **Paramètres** (`$param`), demandés à l'application du filtre ; variables d'environnement.
- Les filtres peuvent servir de conditions dans d'autres filtres. Filtres de bibliothèque partagés.
- Classes de condition : exigence/macro-exigence, entité, section, référence, document,
  exigence/entité/section.
- Opérateurs : contient / ne contient pas (sensible ou non à la casse), est marqué par, est de ce
  type, vérifie la règle, le texte contient, a des attributs, la hiérarchie contient, égal / différent,
  présent / absent, nom de document contient, a des références entrantes ou sortantes, est couvert /
  non couvert / partiellement couvert, présent en entrant, ≥ ≤ > <, vide / non vide, correspond à
  l'expression, catégorie égale, vrai / faux.
- Bascules d'affichage rapides : exigences sans erreur, dérivées, non définies, non couvertes ;
  entités ; attributs ; liens ; sections vides ; informations ajoutées [UG « Toolbar »].

### 3.12 Instantanés (snapshots)

[UG « Using Snapshots » ; « Overview of the Product Work Files »]

- Capture datée et nommée des résultats d'analyse.
- Comparaison de l'état courant avec un instantané, ou de deux instantanés : éléments ajoutés,
  modifiés ou supprimés, évolution de la couverture.
- **Historique d'un élément** à travers les instantanés.
- Ouverture d'un instantané en lecture seule, ce qui permet l'échange entre équipes.

### 3.13 Rapports

[GS « Generating Reports » ; CG « Customizing Reports »]

Rapports de bibliothèque à fournir :

1. **Matrice de traçabilité** : liens amont→aval et aval→amont, avec taux de couverture par paire.
2. **Résultats d'analyse** : synthèse de couverture du projet.
3. **Description du projet** : documents, types, canaux.
4. **Impact amont** et 5. **Impact aval** pour une sélection.
6. **Synthèse des informations ajoutées** : attributs, références, textes, liens.
7. **Contrôle des règles**.

Personnalisation : un éditeur de modèles de rapport (structure : paragraphe, section, tableau,
division, texte, image, liste, champ, inclusion de fichier, URL, renvoi, ancre) alimenté par des
requêtes sur le modèle (boucles sur documents, exigences, couvertures). Le style dépend du format de
sortie (modèles RTF, DOCX, HTML, XLSX, CSV, TXT, PDF, AsciiDoc ; voir `config\doc_templates`).
Paramètres demandés à la génération. Rapports multilingues. Un filtre actif s'applique au rapport,
avec confirmation.

### 3.14 Revues (module Reviewer)

[RP ; `config\reviewer\Checklist_*.xml`]

- **Listes de contrôle** (*checklists*) : éléments à vérifier, applicables au projet, à un document
  ou à une exigence. Modèles livrés : DO-178B (revue des exigences de haut niveau, etc.).
- **Association** d'une liste de contrôle à un objet (projet ou document).
- **Résultats** par élément et par objet : statut (ouvert, approuvé, rejeté…), auteur, modificateur, dates.
- **Demandes d'action corrective** (CAR) : libellé, priorité, rapporteur, correction.
- Métriques et rapports de revue.

### 3.15 Projets composites

[CPP ; RCN]

- Insertion d'un sous-projet, créé ou existant, comme un document du projet parent, avec choix des
  documents visibles.
- **Remplacement de chemin** : un document du parent remplace un document de même rôle dans les
  sous-projets (par exemple une spécification commune).
- Intérêts annoncés : performance (un sous-projet déjà analysé est réutilisé), sécurité (périmètre
  limité), organisation (sous-traitants).

### 3.16 Accès web et intégration système

- **Accès web** : publication d'un projet en lecture via un serveur, avec filtrage, rapports, vues
  couverture, impact, détails, texte et graphique [WA].
- Prévisualisation dans l'explorateur de fichiers et la messagerie, ouverture par double-clic, création
  de projet par « Ouvrir avec », dépôt de fichiers sur la fenêtre [UG « Windows Integration »].
- **Ligne de commande** : langue, répertoires de configuration multiples (configuration partagée
  d'entreprise), enregistrement de serveur COM [UG « Appendix: Command Line Options »].
- API d'automatisation : l'outil d'origine expose une API COM et un langage de script. ReqtiCraft
  proposera une CLI et une API locale (voir l'architecture).

## 4. Vues de l'espace de travail

[GS « Project Workspace » ; UG « Main Window »]

| Vue | Contenu | Évolution moderne proposée |
|---|---|---|
| Accueil | Projets récents, exemples | + glisser-déposer, assistant de création |
| **Tableau de bord** (gestion) | Documents, couverture par document, qualité globale, compteurs d'erreurs | Cartes et graphiques, tendances depuis les instantanés |
| **Couverture** | Arbre de sélection, colonne amont N-1, colonne aval N+1, onglet messages | Panneau de détails latéral, fil d'Ariane |
| **Impact** | Amont et aval sur tous les niveaux | Arbre dépliable et vue « graphe focalisé » |
| **Graphique** | Documents en arbres reliés par des traits, zoom, masquage, « graphe de la sélection » | Canvas WebGL, mise en page automatique (ELK), mini-carte |
| **Détails des exigences** | Tableau exigences × attributs, tri, colonnes, coller | Grille virtualisée, filtres par colonne, export |
| **Détails des liens** | Matrice couvrant × couvert | Matrice interactive avec cellules cliquables |
| **Texte** | Document restitué avec ses éléments | Rendu riche, surlignage des captures |
| Explorateur de règles | Violations par règle | Panneau « Problèmes » façon IDE |

## 5. Parcours utilisateur

### P1 — Créer un projet et mesurer une couverture (tutoriel de référence)

Reproduit le parcours du guide Getting Started, qui sert de **test d'acceptation** de la v1 :

1. Créer un projet `MyTutorial`.
2. Ajouter « Product Specification » (type Word, `ProductSpec.doc`).
3. Ajouter « Design Specification » et tracer un canal Design → Product.
4. Valider : le tableau de bord affiche 2 documents, 6 exigences, **83 %** de couverture et 1 erreur
   (exigence non couverte).
5. Dans la vue couverture, sélectionner `PS_USB_REQ2` : non couverte, en rouge.
6. Modifier le document source et enregistrer : l'outil propose le rechargement, la couverture passe à
   100 % et l'élément apparaît comme modifié.
7. Ajouter « Test Specification », tracer le canal Test → Design, puis la vue impact montre la chaîne
   PS → DS → TS, les dérivées `DS_USB1_REQ1/2` et l'erreur « exigence `PS_AO_REQ199` introuvable ».
8. Générer le rapport « Description du projet ».

### P2 — Administrateur : définir un type d'analyse

Dupliquer un type de bibliothèque, modifier l'expression d'exigence (par exemple `(LL-REQ_\d+)`),
définir une référence en deux temps (expression puis sous-expression), tester sur un document réel
dans le testeur, puis appliquer le type à un document [CG p.15-24].

### P3 — Importer un projet Reqtify existant

Ouvrir un `.rqtf` (ancien ou nouveau format). ReqtiCraft charge les `.types`, `.filters` et `.marks`
du projet, résout les types de bibliothèque, signale les convertisseurs non pris en charge, analyse,
puis affiche un **rapport d'import** (éléments repris, approximations, écarts).

### P4 — Ingénieur qualité : suivre l'évolution entre deux jalons

Créer un instantané « V1 », faire évoluer les documents, comparer l'état courant à V1 (exigences
ajoutées, modifiées ou supprimées, couvertures perdues), puis exporter la matrice de traçabilité.

### P5 — Intégration continue

`reqticraft analyze projet.rqtf --fail-on error --report matrix.xlsx` dans un pipeline. Le code de
sortie est non nul si une règle de sévérité « erreur » est violée.

### P6 — Revue

Associer la checklist « DO-178B HLR » au document de spécification, passer les exigences en revue, rejeter
une exigence avec une demande d'action corrective, puis générer le rapport de revue.

## 6. Connecteurs, classés par priorité

L'inventaire vient des 70 Coupling Notes et des dossiers `config\types` ; la priorité est une
proposition de ReqtiCraft. Un connecteur = un convertisseur (natif → intermédiaire) + un ou plusieurs
types d'analyse par défaut.

**P0 — MVP** (formats ouverts, sans application tierce, avec un fort volume d'usage) :

| Connecteur | Source | Notes |
|---|---|---|
| Texte brut | Text Coupling Notes ; `Office/text.types` | Base du moteur. |
| Word `.docx` | Word Coupling Notes ; `Office/word.types` | Réimplémenté en lisant directement l'OOXML, sans Word. Intermédiaire compatible (style, tabulation, texte ; tableaux `#TABLE`/`#TR`). `.doc` binaire : hors MVP. |
| Excel `.xlsx` / CSV | Excel Coupling Notes ; `Office/excel.types` | Intermédiaire texte `Worksheet :` et `|<col> texte`, plus variante XML. |
| PDF | PDF Coupling Notes ; `Office/PaDaFix.types` | Extraction de texte sans Acrobat. Reconstitution des paragraphes selon des règles (police, gras, italique, couleur, taille, interligne, retrait, phrases entières), suppression des césures. Titres déduits des signets (`Heading X`) si l'option « styles » est active. Pages exclues, texte barré ignoré, texte riche, images, tableaux (`#TABLE`/`#TR`), annotations (`Annot_…/End_Annot`), PDF protégés par mot de passe. Le PDF est en lecture seule : la navigation ouvre le visualiseur à la page de l'élément. Variante « propriétés du PDF » (métadonnées seules, en multitype). |
| Code source (C, C++, C#, Java, Ada, VHDL, Verilog) | Code Coupling Notes ; `Code/*.types` | Capture dans les commentaires. |
| XML / JSON génériques | JSON Coupling Notes ; types XML | JSON transformé en XML, puis arbres d'analyse. |
| Projet ReqtiCraft/Reqtify (sous-projet) | Reqtify Coupling Notes ; Composite Project Plugin | Projets composites. |

**P1 — après le MVP** (outils ALM et formats courants) :

- ReqIF (absent de Reqtify, mais c'est le standard d'échange : différenciateur) ;
- HTML, AsciiDoc, Markdown (nouveau), RTF, OpenOffice/LibreOffice Writer et Calc, PowerPoint ;
- Jira, Polarion, codebeamer, Jama, DOORS Next (OSLC/REST), Redmine, Bugzilla ;
- Git, GitHub, Bitbucket, Subversion (lecture de dépôts et d'historiques) ;
- Google Docs, Sheets, Slides et Drive, OneDrive (API cloud) ;
- ZIP (archives de documents).

**P2 — à la demande** :

- DOORS classique (export XML/DXL), RequisitePro, PTC Integrity, MS Access, Oracle, MS Project, Visio, FrameMaker ;
- Magic Cyber Systems Engineer / Cameo / Teamwork Cloud (SysML), IDE, HTTP, FileProperties, MediaInfo ;
- Perforce, CVS, ClearCase, Synergy, PVCS, Documentum, ProjectWise, DesignSync, SolidWorks, Change Management.

**P3 — écosystème Dassault et outils de niche** (forte dépendance à des API propriétaires) :

- CATIA V5, CATIA Functional & Logical Design, CATIA Product Structure Design, ENOVIA (Requirements,
  Engineering BOM, Logical Structures, Project Management, Generic Connector), ENOVIAvpm ;
- AUTOSAR Builder, ControlBuild, logiCAD, Modelica/Dymola, SIMULIA V&V.

## 7. Exigences non fonctionnelles

- **Performance** : analyser un projet de 50 000 exigences en moins de 10 s (hors conversion). Analyse
  incrémentale : seuls les documents modifiés sont reconvertis (CRC / empreinte).
- **Fiabilité de l'import** : sur le corpus d'exemples livré avec Reqtify (50 projets), chaque projet
  dont le connecteur est pris en charge doit retrouver les mêmes exigences, références et taux que
  ceux affichés par l'outil d'origine (comparaison en boîte noire).
- **Encodage** : tous les fichiers en UTF-8. Lecture des encodages d'entrée UTF-8, UCS-2 (LE et BE),
  Shift_JIS et ISO-8859-1 [CG « Analysis Tab » ; UG « Note 2 »].
- **Sauvegarde** : copie de sécurité avant écrasement (Reqtify utilise le suffixe `~`) [UG « Note 3 »].
- **i18n** : anglais, français, japonais, chinois.
- **Accessibilité** : navigation complète au clavier, contraste conforme WCAG AA.
- **Traçabilité normative** : l'outil doit permettre de répondre aux objectifs de traçabilité des
  normes DO-178C/DO-254, ISO 26262, IEC 61508, EN 50128, ECSS, CMMI, 21 CFR 820/11 [UG « Compliance with Quality Standards »].

## 8. Sources

| Code | Document |
|---|---|
| GS | `doc\Getting Started.pdf` |
| UG | `doc\User Guide.pdf` |
| CG | `doc\Customization Guide.pdf` |
| RP | `doc\Reviewer Plugin.pdf` |
| CPP | `doc\Composite Project Plugin.pdf` |
| RCN | `doc\Reqtify Coupling Notes.pdf` |
| WA | `doc\Web Access.pdf` |
| — | `doc\* Coupling Notes.pdf` (70 documents) |
| — | `config\types\**\*.types`, `config\doc_models`, `config\doc_templates`, `config\presentations`, `config\reviewer` |
| — | Exemples : `C:\Users\Public\Documents\Reqtify\2026x Refresh1\Examples` (50 projets `.rqtf`) |
