# ReqtiCraft — Formats d'interopérabilité Reqtify

> Statut : brouillon v0.1 — 2026-10-28
> But : permettre à ReqtiCraft de **lire les projets Reqtify existants**. Ce document décrit les
> formats de fichiers *sur disque*. Il ne décrit aucun algorithme interne.

## 0. Méthode et niveaux de preuve

Chaque conclusion porte une étiquette de preuve :

| Étiquette | Signification |
|---|---|
| **[DOC]** | Énoncé explicite de la documentation officielle (chapitre cité). |
| **[EX]** | Observé dans un fichier d'exemple ou de configuration livré (chemin cité). |
| **[EX×]** | Recoupement : le même projet existe dans les deux formats (ancien et nouveau), ce qui donne la correspondance champ par champ. |
| **[INF]** | Inférence plausible, non confirmée : **à vérifier** par un test en boîte noire (enregistrer depuis Reqtify, puis observer le fichier). |
| **[REA]** | Observation faite avec REA. |

Corpus étudié :

- `C:\Program Files\Reqtify v2026x Refresh1\config\types` : 63 fichiers `.types` ;
- `C:\Users\Public\Documents\Reqtify\2026x Refresh1\Examples` (noté `EX\` ci-dessous) : 50 `.rqtf`
  (33 ancien format, 17 nouveau format), 49 `.rqtfimage`, 26 `.types`, 4 `.filters`, 3 `.marks`,
  des documents de modification XML et des fichiers Reviewer ;
- un script d'inventaire du vocabulaire XML et des clés INI de tous ces fichiers (scratchpad de session,
  non versionné).

**Utilisation de REA.** Une tentative a porté sur `bin.w64\reqtify.exe` (12,5 Mo, x86-64), limitée à une
**recherche de chaînes** (noms de clés des formats). L'import et l'analyse Ghidra ont dépassé le délai de
330 s, et aucune chaîne n'a été exploitée. Une recherche brute d'octets a ensuite montré que les noms de
clés des formats (`GVPosition`, `outputDocument`, `Booleans`…) **n'apparaissent pas** dans l'exécutable,
ni en ASCII ni en UTF-16 **[REA/OBS]**. La logique de sérialisation ne vit donc pas sous forme de
littéraux dans le binaire. Les fichiers d'exemple suffisent pour tous les formats texte. Aucun
désassemblage ni pseudocode n'a été produit ou consulté.

## 1. Fichiers d'un projet

| Fichier | Rôle | Format | Preuve |
|---|---|---|---|
| `<projet>.rqtf` | Configuration du projet (documents, canaux, propriétés) | XML (format 2019 FD01 et après) **ou** INI (ancien format) | [DOC] UG « Project Files » ; « File Menu » ; [EX] |
| `<projet>.types` et `*.types` | Types d'analyse du projet (tous les `.types` du répertoire sont chargés) | XML ou INI | [DOC] UG « Types Definition » ; [EX] |
| `<projet>.filters` et `*.filters` | Filtres, filtre courant, état des filtres d'affichage | XML ou INI | [DOC] UG « Filter Definition File » ; [EX] |
| `<projet>.marks` | Définition des marques | XML | [EX] `EX\Plugin\Reviewer Demo\*.marks` |
| `rules.br` | Règles utilisateur (langage de script propriétaire) | Script | [DOC] UG « Handling Rules ». **Non pris en charge.** |
| `<doc>-modif.xml`… | Documents de modification / éléments ajoutés | XML | [EX] |
| `<projet>.review\` | Revues : `Checklist_{GUID}.xml`, `Association_{GUID}.xml` | XML | [EX] |
| `doc_models\*.xml` | Modèles de rapport | XML | [DOC] UG « Report Templates » ; [EX] |
| `doc_templates\*` | Gabarits de style (rtf, docx, html, xlsx, csv, txt, adoc, mif, pdf) | Natif | [DOC] ; [EX] `config\doc_templates` |
| `<projet>.rqtfimage` | Résultats d'analyse | Binaire propriétaire | [DOC] UG « Analysis Results File » |
| `snapshots\<projet>_YYMMDDHHNNSSZ_<nom>.rqtfimage` | Instantanés (avec un `.marks` et un `.review\` homonymes) | Binaire | [DOC] ; [EX] |

Règles transverses :

- **Encodage UTF-8** pour tous les fichiers de configuration. Un enregistrement en ANSI peut casser le
  chargement **[DOC]** UG « Note 2 ». Exception observée : les fichiers Reviewer de l'exemple
  déclarent `encoding="ISO-8859-1"` **[EX]**. Il faut donc respecter la déclaration XML.
- Tous les fichiers XML générés commencent par `<?xml version="1.0" encoding="UTF-8"?>`, suivi du
  commentaire `<!-- Generator: reqtify -->` **[EX]**.
- Fins de ligne CRLF **[EX]** (`file` : « with CRLF line terminators »).
- Une copie de sauvegarde `<fichier>~` est créée à l'enregistrement **[DOC]** UG « Note 3 ».
- Le guide Getting Started écrit `MyTutorial.rtqf` : c'est une coquille. L'extension réelle est
  `.rqtf` **[EX]** (les 50 projets).
- **Détection du format** : un fichier dont le premier caractère non blanc est `<` est en XML, sinon il
  est en INI **[EX]** (`Tutorial.rqtf` commence directement par `[Files]`).
- **Répertoires de configuration** : ordre de recherche `config` d'installation, puis répertoires
  supplémentaires (`Config="dir1,dir2"`), puis répertoire du projet et ses sous-répertoires
  `doc_models`, `doc_templates`. Les sous-dossiers de `config\types` deviennent des dossiers du
  navigateur de types **[DOC]** UG « Overview of the Product Work Files » ; « Command Line Options ».

## 2. Projet `.rqtf` — nouveau format (XML)

Vocabulaire complet observé sur 17 fichiers **[EX]** :

```text
Project
├─ Property            @name @value            (répétable, même nom possible)
├─ Document            @name @type @path @positionX @positionY [@category] [@absolutePath] [@withRichText]
│   ├─ Property        @name @value            (paramètres propres au type)
│   ├─ ModificationDocument @type @path
│   ├─ IgnoredType     @name
│   ├─ DisplayInformation @graphicsL @graphicsT @graphicsR @graphicsB [@documentName] [@sortAttribute]
│   └─ Column          @name @width            (colonnes de la vue détails)
├─ ProjectReference    @name @type="_ProjectReference" @path(.rqtfimage) @positionX @positionY
│   └─ DisplayInformation …
├─ Cover               @outputDocument @positionX @positionY
│   └─ InputDocument   @name                   (1..n)
└─ ReplacingPath
    ├─ Component @name                          (chemin du document remplaçant)
    └─ ReplacedPath                             (1..n)
        └─ Component @name                      (chemin : sous-projet / document)
```

Sémantique :

- **`Cover`** : `outputDocument` est le document **couvert** (amont) et chaque `InputDocument` est un
  document **couvrant** (aval). **[EX×]** `EX\Use Cases\UC2-Multi_Levels\Multi_Levels.rqtf` contient
  `Cover outputDocument="Specifications"` avec `InputDocument "Design"`, ce qui correspond à
  « Design couvre Specifications » dans le guide GS p.16. `positionX/Y` est la position du point de
  jonction du canal dans le diagramme.
- **Plusieurs `<Cover>`** avec le même `outputDocument` correspondent à une couverture **ET**
  (indépendante). **Un `<Cover>` avec plusieurs `<InputDocument>`** correspond à une couverture **OU**
  (combinée) **[INF]**. C'est cohérent avec UG « Covering a Document by Combination » (point milieu
  partagé), mais aucun exemple livré n'a plusieurs `InputDocument` dans un même `Cover`. *À confirmer.*
- **`Document@type`** : nom d'un `DocumentType` de l'ensemble des `.types` chargés. **`@path`** est
  relatif au répertoire du projet (séparateur `\`). `@absolutePath` est optionnel (option « Save
  absolute paths ») **[DOC]** UG p.67 ; **[EX]**.
- **`Property` du projet** observées : `Feature` (liste de jetons séparés par des espaces, par exemple
  `F_WORD`, `F_COMPOSITE_PROJECT`, `F_FILTER F_REPORT F_RULES` : fonctions de licence requises),
  `boostPurgeTypes`. **`Property` de document** observées : `textFormat`, `withCharacterStyles`,
  `withImages`, `tabCellsFolding`, `encoding`, `hasTitles`, `separators`, `textQuote`, `icon`,
  `guidHierarchy`, `requirementsPackage` et `previousReqsPackage` (répétées). Ce sont les
  **variables** déclarées par le type (`Variable@name`, voir §4) **[EX]**. Quand une même `Property`
  porte plusieurs valeurs, elle est répétée.
- **`ProjectReference`** : sous-projet référencé par son `.rqtfimage` **[EX]** `EX\Coupling\Composite Project\CompositeProject1\CompositeProject1.rqtf`.
- **`ReplacingPath`** : le document `Specification` du parent remplace `SubContractor1/Specification`
  et `SubContractor2/Specification` **[EX]** (même fichier) ; **[DOC]** CPP.
- **Absences notables** : aucun exemple XML ne contient de **dossier**, de canal **miroir** ou
  **dépendance**, d'**annotation**, de catégorie de canal ni de mot de passe. Leur syntaxe est **inconnue**
  et doit être établie en boîte noire (voir §10).

## 3. Projet `.rqtf` — ancien format (INI)

Clés observées sur 33 fichiers **[EX]** :

```ini
[Files]
Names=<doc1>,<doc2>,...            ; liste ordonnée des documents (noms = sections suivantes)
Variable<n>Name=<propriété>        ; propriétés du projet (ex. Feature, boostPurgeTypes)
Variable<n>Value=<valeur>

[<docN>]
Type=<nom du type d'analyse>
Path=<chemin relatif>
AbsolutePath=<chemin absolu>       ; optionnel
Category=<catégorie>
GraphicPosition=<x>@<y>
GVPosition=<L>,<T>,<R>,<B>         ; cadre dans la vue graphique
Cover<n>=<doc couvrant>            ; ce document est COUVERT par <doc couvrant>
Cover<n>Position=<x>@<y>
ModificationDocument<n>=<chemin>
ModificationDocument<n>Type=<type>
UseIntermediateFile=1
WithRichText=1
DisplayedVariable=<nom>            ; ex. _category, withImages, testDate
RepositoryTool=Boost|SQLite
AttributeOrders=<a>,<b>
ColumnWidths=<attr>=<px>,...
SortAttributeName=<attr>
RemoteConfig=<outil>               ; ex. SVN
```

**Correspondance INI ↔ XML** **[EX×]** : le même tutoriel existe dans les deux formats,
`EX\Use Cases\Getting Started\Tutorial.rqtf` (INI) et `EX\Plugin\Reviewer Demo\Reviewer Tutorial.rqtf` (XML) :

| INI (section `[Product Specification]`) | XML |
|---|---|
| `GraphicPosition=705@370` | `Document positionX="705" positionY="370"` |
| `GVPosition=500,820,3860,5580` | `DisplayInformation graphicsL="500" graphicsT="820" graphicsR="3860" graphicsB="5580"` |
| `Cover1=Design Specification` | `Cover outputDocument="Product Specification"` › `InputDocument name="Design Specification"` |
| `Cover1Position=1007@710` | `Cover positionX="1007" positionY="710"` |
| `Type=Word`, `Path=ProductSpec.doc` | `@type="Word"`, `@path="ProductSpec.doc"` |
| `[Files] Variable1Name=boostPurgeTypes / Variable1Value=1` | `<Property name="boostPurgeTypes" value="1"/>` |

Sens de `Cover<n>` : il est porté par le **document couvert** et nomme le **couvrant** **[EX×]**.
Les paramètres de document (`withImages`…) semblent stockés comme `Variable<n>Name/Value` dans la
section du document **[INF]**, par analogie avec `[Files]`.

## 4. Types d'analyse `.types`

Les deux formats coexistent, même dans la bibliothèque livrée : par exemple `Office\word.types` est en
INI et `Office\excel.types` en XML **[EX]**. Sur 89 fichiers lisibles : 42 XML, 47 INI. L'éditeur de
types sait convertir de l'un à l'autre (menu « Old format ») **[DOC]** CG « Contextual Menu ».

### 4.1 Format XML

Vocabulaire complet, avec le nombre d'occurrences dans le corpus **[EX]** :

```text
Types [@coupling]
└─ DocumentType @name @convertTool [@editTool] [@postprocessTool] [@image] [@kind="xml"]
   │            [@description] [@guid] [@isFileBased] [@textFormat] [@intermediateFileEncoding]
   │            [@useGuidsForComparison] [@mergeOrReplaceHomonymousParagraphs]
   │            [@areAttributesIdentifiedByValue] [@reDelete] [@printExpression]
   ├─ FileExtension @value          ├─ MatchingFileName @value   ├─ ExcludedFileName @value
   ├─ ExcludingAT @value            ├─ OldName @value            ├─ Property @name @value
   ├─ Variable @name @label [@type] [@method]
   ├─ ElementType @name @elementClass
   │   [@regularExpression] [@subExpression] [@endExpression]      ← types « texte »
   │   [@analysisTree]                                              ← types XML (@kind="xml")
   │   [@printExpression] [@printTextExpression] [@image] [@guid]
   │   [@isVisible] [@isIgnored] [@isMultiple] [@isNested] [@isInverse] [@isExternal] [@isBoolean]
   │   [@index1..4] [@identFormat] [@guidFormat] [@identifyChildren] [@inheritsTerminals]
   │   [@computationExpression]
   │   ├─ ParentType @name   (0..n : hiérarchie autorisée)
   │   ├─ TargetType @name
   │   ├─ EnumValue @value [@image] [@label1]
   │   └─ Property @name @value
   └─ TerminalType @name @elementClass @regularExpression [@columnIndex] [@reRow]
```

Valeurs d'`elementClass` observées : `Section` (247), `Attribute` (302), `Requirement` (133), `CoverLink`
(référence, 107), `Link` (46), `Text` (28), `MacroRequirement` (19), `Picture` (16), `EntityStrict`
(entité, 15), `LinkAttribute` (attribut de référence, 13), `Value` (13), `Table` (2) **[EX]**.
La correspondance avec les concepts **[DOC]** CG « Left Area » est directe.

Convertisseurs (`convertTool`) observés : `text`, `BasicConnector`, `WordWithStyles`, `RTFwithStyles`,
`ExcelWithColNumbers`, `ExcelXML`, `PowerPoint`, `CScan`, `CScanCPP`, `CScanJava`, `DoorsXML`,
`DoorsBaselineSet`, `DoorsNG`, `DoorsNGView`, `JamaRest`, `JamaFilter`, `JamaBaseline`, `Polarion`,
`CodeBeamer`, `JIRA`, `Redmine`, `MSProject`, `CCM`, `Oracle`, `PaDaFix`, `Reqtify`,
`ReqtifyComparison`, `MagicDraw`, `TeamworkCloud`, `Simulink`, `ReqCentral`, ainsi que les variantes
CATIA, ENOVIA et SolidWorks **[EX]**. ReqtiCraft doit traiter un convertisseur inconnu comme la règle
`unknownConvertMethod` **[DOC]**.

Exemple minimal (extrait de `config\types\Office\text.types`) **[EX]** :

```xml
<DocumentType name="Text" convertTool="BasicConnector" editTool="TextEditor" ...>
  <ElementType name="Heading1" elementClass="Section" regularExpression="^(\d+)\.[ \t]+(.*)$"/>
  <ElementType name="Heading2" elementClass="Section" regularExpression="^(\d+\.\d+)\.[ \t]+(.*)$">
    <ParentType name="Heading1"/>
  </ElementType>
  <ElementType name="Requirement" elementClass="Requirement"
               regularExpression="^[ \t]*(\S*REQ\d+)(?:[ \t]*\:[ \t]*(.+)|)$"/>
  <ElementType name="Covers Reference" elementClass="CoverLink" printExpression="[Covers: $i]"
               regularExpression="\[Covers[ \t]*:[ \t]*([^\]]*)" subExpression="([^\,\;\s]+)"/>
  <ElementType name="Priority" elementClass="Attribute" regularExpression="...Priority...(\S+)">
    <EnumValue value="High"/><EnumValue value="Medium"/><EnumValue value="Low"/>
  </ElementType>
  <TerminalType name="Text" elementClass="Text" regularExpression="\&lt;\&lt;([\s\S]+?)\>\>"/>
  <TerminalType name="Table" elementClass="Table" regularExpression="^#TABLE$" reRow="^#TR"/>
</DocumentType>
```

Sémantique documentée, à implémenter **[DOC]** CG « Defining Element Types for Text Files » :

- la capture s'applique **ligne à ligne sur le fichier intermédiaire**, ou sur plusieurs lignes si
  l'expression contient `\n` ou `[\s\S]` ;
- `regularExpression` puis `subExpression` : capture en deux temps (la liste, puis chaque identifiant) ;
- `endExpression` : fin explicite, qui permet l'imbrication. Si l'expression capture un groupe, il
  doit égaler l'identifiant ouvert. Sinon, l'élément se termine au prochain élément de même rang ou de
  rang supérieur ;
- un texte (`TerminalType Text`) se rattache à l'élément capturé juste avant ;
- `printExpression` : symboles d'affichage `$i $j $l $t $R $T $e ${Ident} ${Label} ${Type} ${Text} ${<Attribut>}` ;
- `identFormat` / `guidFormat` : symboles `\d` (nom du document), `\f` (nom de fichier), `\j` / `\k`
  (identifiant du parent), `\l` (GUID du parent), `\n` (n-ième capture), `\p` (préfixe), `\H`
  (hachage du texte) ;
- `isInverse` : la référence est portée par l'exigence amont (« est couvert par ») ;
- `EnumValue@label1` : libellé localisé.

**Dialecte d'expressions régulières** : syntaxe de type PCRE, avec groupes non capturants `(?:…)`,
quantificateurs paresseux et assertions **[DOC]** CG « Regular Expressions » ; « Assertions ». Le
répertoire `bin.w64` contient `pcre2-16.dll` et `libpcre-0.dll` **[OBS]** (liste des fichiers). Choix
proposé : un moteur compatible PCRE2.

**`index1..index4`** (XML) et **`Indexes=a,b,c,d`** (INI) : affectation des groupes de capture aux
rôles (identifiant, libellé, texte, GUID, parent, profondeur) **[DOC]** CG « Defining element type ».
Lecture retenue **[EX fort]** : le k-ième nombre est le **rôle du k-ième groupe**, avec les codes
0 aucun, 1 identifiant, 2 libellé, 3 texte, 6 profondeur. Preuves :

- `Office\word.types`, type `Word` : `Section1=^[Hh]eading (\d+)\t(?:([\d\.]+)[ \t](.*)|([^\d\n].+))$`
  avec `Section1Indexes=6,1,2,1`. Le groupe 1 est le niveau du titre (profondeur = 6), le groupe 2 le
  numéro (identifiant = 1), le groupe 3 le titre (libellé = 2), le groupe 4 le titre sans numéro
  (identifiant = 1). C'est le seul découpage cohérent de cette expression.
- `Indexes=1,2,1,0` sur `^Heading 1\t(?:([\d\.]+)[ \t](.+)|([^\d\n].+))$` : numéro, titre, titre
  sans numéro ; le 4e code (0) vise un groupe inexistant.
- `index1="1" index2="3"` sur `(REQ\w+)\n(.+)` : identifiant puis texte sur la ligne suivante.

Les codes 4 (GUID) et 5 (parent) sont supposés suivre l'ordre du menu de l'éditeur, sans exemple
**[INF]**. Une section capturée avec une profondeur s'imbrique sous la dernière section ouverte de
profondeur inférieure ; c'est ainsi que ReqtiCraft retrouve la hiérarchie des titres du type `Word`
de Reqtify (vérifié sur l'exemple Word, voir §11).

### 4.2 Arbres d'analyse XML (`@analysisTree`)

Pour `kind="xml"`, `analysisTree` contient un **motif XML échappé** **[EX]** `config\types\Office\excel.types` :

```xml
<Row><Cell col='^1$'>$i^(REQ\d+)$</Cell><Cell col='^2$'>$l$o</Cell><Cell col='^3$'>$t$o</Cell></Row>
```

Ce que l'on lit dans les exemples et le guide **[DOC]** CG « Analysis Tab » (XML) :

- valeurs d'attributs et de texte = expressions régulières (conditions) ;
- marqueurs de rôle : `$i` identifiant ou valeur, `$l` libellé ou type, `$t` texte ou document, et,
  selon le guide, GUID, parent ou source, image, cible, document cible ;
- variables `${<var>}`, `${Type}`, `${DocName}` et `${<section>.<clé>}` (clé du `.ini`) ;
- options : « Negative » (le sous-arbre doit être absent), « Reference » (point de départ pour
  plusieurs correspondances), « Ignore tag nesting ».

Recensement des marqueurs dans les `analysisTree` de la bibliothèque livrée **[EX]**, et sens retenu
d'après leur contexte **[INF]** (sauf `$i`, `$l`, `$t`, documentés) :

| Marqueur | Occurrences | Contexte typique | Sens retenu |
|---|---|---|---|
| `$i` | 487 | `name='$i'`, `<value>$i</value>` | identifiant, valeur ou cible |
| `$u` | 167 | `externalId='$u'`, `uuid='$u'` | GUID |
| `$l` | 164 | `referenceName='$l'` | libellé |
| `$t` | 74 | `<field isContent='true'><value>$t…` | texte |
| `$o` | 63 | `<Attribute …>$o</Attribute>` | nœud facultatif |
| `$p` | 47 | `<Relation targetUuid='$p' …>` | parent ou source |
| `$g` | 13 | `<PNGImage>$g</PNGImage>` | image |
| `$r` | 10 | `<Implemented m1Id='$i'>$r</Implemented>` | point de départ « Reference » |
| `$h` | 5 | `<value>$t$h</value>` | contenu riche (HTML) |
| `$c` | 2 | `<h2>$l$c</h2>` | inconnu |
| `$!` | — | `<value>$!<div>…</div></value>` | sous-arbre « Negative » |

ReqtiCraft implémente `$i $l $t $u $p $o $!` et accepte les autres sans effet. Les noms de balises
préfixés (`ds:artifact`, `rrm:identifier`) sont comparés sur leur nom local, sans déclaration
d'espace de noms. Un `subExpression` découpe la valeur capturée d'une référence en plusieurs cibles.

### 4.3 Format INI des types

```ini
[Types]
Names=<type1>,<type2>
Coupling=<connecteur>

[<type1>]
Tool=<convertTool>          ToolEdit=<editTool>       Image=<icône>
FileExtension=<ext>         MatchingFileNames=...     IsFileBased=1
IntermediateFileEncoding=UTF8   TextFormat=3   MergeHomonymousEntities=1
MergeOrReplaceHomonymousParagraphs=1   Type=automatic
Variable<n>Name/Label/Type/EnumValuesMethod
<Classe><n>Name=<nom de l'élément>
<Classe><n>=<regex>                ; Section, Requirement, Reference, Attribute, Link, Entity,
<Classe><n>End=<regex de fin>      ; MacroRequirement, LinkAttribute, Value, Picture, Table,
<Classe><n>Sub=<sous-regex>        ; Diagram, GraphicalElement
<Classe><n>Parents=<a>,<b>         ; ParentType (le nom du type = racine du document)
<Classe><n>Print=<affichage>       <Classe><n>Image=  <Classe><n>Tree=<analysisTree>
<Classe><n>Indexes=a,b,c,d         <Classe><n>Guid=   <Classe><n>IdentFormat=
<Classe><n>IsMultiple/IsIgnored/IsVisible/IsBoolean/IdentifyChildren=1
<Classe><n>EnumValues=High|ko,Medium|nt,Low|ok    ; valeur|icône
<Classe><n>Value<k>Key=<clé>  <Classe><n>Value<k>Value=<valeur>   ; → Property
Text<n>Name=  Text<n>RegularExpression=  Text<n>ColumnIndex=  Text<n>EscapeCharacter=
Picture<n>Name=  Picture<n>RegularExpression=
Table<n>Name=  Table<n>RegularExpression=  Table<n>Row=
ExcludingText<n>=<regex>
```

Source : inventaire de 47 fichiers INI **[EX]** ; `EX\Use Cases\AddedElements\AddedElements.types`
pour un type d'éléments ajoutés. Les commentaires commencent par `;` **[EX]**.

**Correspondance INI → XML** **[INF fort]** : identité des noms (`Tool`→`convertTool`,
`ToolEdit`→`editTool`, `<Classe><n>`→`ElementType@regularExpression`, `…End`→`endExpression`,
`…Sub`→`subExpression`, `…Print`→`printExpression`, `…Tree`→`analysisTree`, `…Parents`→`ParentType`,
`Text<n>`→`TerminalType elementClass=Text`), avec `Reference`→`CoverLink`, `Entity`→`EntityStrict`,
`LinkAttribute`→`LinkAttribute`. *À valider par conversion en boîte noire* : charger un type INI dans
l'éditeur, le basculer en « nouveau format », puis comparer les deux fichiers.

Types réservés à ne pas supprimer : `_ProjectReference`, `Text`, `LostType`, `InternalType` **[DOC]** UG « Types Definition ».

## 5. Fichiers intermédiaires (sortie des convertisseurs)

Ils sont essentiels : ReqtiCraft doit **réimplémenter les convertisseurs** pour produire des
intermédiaires compatibles, faute de quoi les expressions des types existants ne s'appliqueront pas.

| Convertisseur | Format de ligne | Preuve |
|---|---|---|
| Texte (`text`, `BasicConnector`) | Le fichier lui-même | [EX] `EX\Coupling\Text\*.txt` avec `Office\text.types` |
| Word (`WordWithStyles`) | `<style Word><TAB><numérotation><séparateur><texte>`. Tableaux : `#TABLE`, puis des lignes `#TR|<col> <style> <texte>|<col> …`, plusieurs paragraphes d'une cellule sur la même ligne. Images : `<Picture<HEX…>Picture>`. Styles de caractères : ligne supplémentaire `<style car.> <texte>`. Annotations : `Annot_<user>_<n>/…/End_Annot_…`. Le nom de style est le nom anglais d'origine (pas l'alias). Les espaces insécables deviennent des espaces : le document de l'exemple Word, annoncé conforme au type par défaut, sépare `REQ2` de `:` par U+00A0, que l'expression du type (`[ \t]*\:`) refuse. | [DOC] Word Coupling Notes « Intermediate Txt File » ; [EX] `Office\word.types` (`^Heading 1\t…`, `^Requirement_ID\t…`) ; [EX] `Examples\Coupling\Word\Word demo doc.docx` |
| Excel (`ExcelWithColNumbers`) | `Worksheet : <nom>`, puis par ligne `|<col> <texte>` répété (séparés par une tabulation), `BeginTextZone_<id>:` … `EndTextZone_<id>`, fin : `End_of_Conversion`. Les dates antérieures à 1970 sont omises. ReqtiCraft écrit la référence de cellule (`|A2 REQ1`) : le type livré accepte `\|\w+ ` et son attribut `Position` capture `\|([A-Z]+\d+) ` ; d'autres expressions acceptent aussi un numéro seul (`\|(?:D\d+|4) `) **[INF]**. | [DOC] Excel Coupling Notes ; [EX] `Office\excel.types` |
| PDF (`PaDaFix`) | Proche de l'intermédiaire Word : `<style> <texte>` quand l'option « styles » est active (nom de police sans préfixe de sous-ensemble, par exemple `Calibri-Italic`), `Heading X` pour les titres issus des signets. Tableaux : `#TABLE`, puis `#TR|<col> <texte>|<col> …`. Images : pseudo-style `Picture` suivi de `<Picture<<doc>_data\<image>.png>Picture>`. Annotations : `Annot_<user>/<contenu>/<texte surligné>/End_Annot_<user>/…`. Paragraphes reconstitués et césures supprimées. Intermédiaire écrit dans le sous-répertoire `intermediate` du projet. L'ancien convertisseur `PdfCvTxt` (types « PDF Old ») utilise une syntaxe différente. Variante « propriétés » : intermédiaire XML. | [DOC] PDF Coupling Notes « Intermediate File », « From PdfCvTxt to PaDaFix », « Capturing PDF Annotations » ; [EX] `Office\PaDaFix.types` (`^(?:[^\t]+\t|)(REQ\d+)`, `^Annot[^/]+/(REQ[\w\-]+)…/End_Annot`) |
| Excel XML (`ExcelXML`) | `<Worksheet name=…><Row><Cell col=…>…` | [DOC] ; [EX] |
| Multitype | Lignes `BEGIN_DIRECTORY: <label>:<chemin>`, `BEGIN_FILE:`, `END_FILE:`, `END_DIRECTORY:` (texte) ; `<Dir label path>` et `<File label path>` (XML) | [DOC] CG « Customizing Multitypes » ; [EX] `word.types` (Multi Word) |
| JSON | Transformé en XML, puis traité par arbre d'analyse | [DOC] JSON Coupling Notes « Transformation from JSON to XML » |

Point critique : le détail exact (numérotation, séparateurs, échappements, ordre des colonnes,
encodage des images) se valide **en boîte noire**. Dans l'outil d'origine, l'option « Intermediate
File » du projet **conserve les intermédiaires** **[DOC]** Word et Excel Coupling Notes. En la
cochant sur nos propres documents de test (pas sur ceux de Dassault), on obtient des **fichiers de
référence** (« golden files ») pour les tests de non-régression de nos convertisseurs.

## 6. Filtres `.filters`

### 6.1 XML **[EX]** `EX\Use Cases\UC2-Multi_Levels\Multi_Levels.filters`

```xml
<Filters>
  <Filter name="Tests Passed">
    <Condition applicationClass="AbstractRequirement" operator="equal" value="OK"
               receiverClass="Attribute" receiverDocumentType="My_Tests" receiverElementType="Test Result"/>
  </Filter>
</Filters>
```

### 6.2 INI **[EX]** `EX\Use Cases\UC4-Tests_Log\Use of Tests Log.filters`, UC3, UC6

```ini
[Filters]
Names=Software reqs,Tests passed,...
Current=(no filter)                 ; filtre actif

[Non tested yet]
Booleans=1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1   ; 16 drapeaux
IsAnd=1                                    ; ET (absent : OU)
Cond1=Attribute.Tests Log.OK,isAbsent,a value
Cond2=Attribute.Tests Log.KO,isAbsent,a value
```

`Cond<n>` = `<receiverClass>.<receiverDocumentType>.<receiverElementType>,<operator>,<value>` **[EX×]**
(mêmes notions qu'en XML). Opérateurs observés : `equal`, `isPresent`, `isAbsent`. Les autres sont
listés en langage naturel dans **[DOC]** UG « Defining Filters », mais leurs **identifiants** sont
inconnus **[INF]**. La valeur `a value` sert de valeur fictive pour les opérateurs unaires **[INF]**.

**`Booleans` (16 drapeaux)** : signification **[INF]**. Hypothèse : options du filtre et bascules
d'affichage (le guide indique que le fichier contient « show/hide requirements, show/hide empty
sections… » **[DOC]** UG « Filter Definition File »). *À établir en boîte noire*, en basculant chaque
option une à une puis en comparant les fichiers.

Le receveur `*.<élément>` est indépendant du type de document **[DOC]**.

## 7. Marques, éléments ajoutés et revues

### 7.1 `.marks` **[EX]**

```xml
<Marks version="3">
  <Mark name="approved" color="4509764" text="Approved"></Mark>
</Marks>
```

`color` est un entier décimal. Hypothèse **[INF]** : COLORREF Windows `0x00BBGGRR`.
4509764 = `0x44D044`, soit R=0x44, G=0xD0, B=0x44 : un vert, cohérent avec « Approved ». L'association
marque → élément n'apparaît pas dans ce fichier. Elle est probablement stockée dans le `.rqtfimage`
**[INF]**.

### 7.2 Documents de modification **[EX]**

Racine `<ModificationDocument>`. La forme courte à racine unique `<Requirement>` existe aussi
(`EX\Use Cases\AddedElements\modif.xml`).

```xml
<ModificationDocument>
  <_Value name="textFormat">4</_Value>
  <Section type="Part" ident="Part1">                       <!-- chemin de sections imbriquées -->
    <Reference type="Implements" ident="REQ6" label="Position of Whole1"></Reference>
  </Section>
  <Requirement type="EDS_REQ" ident="EDS_Req9">
    <Attribute type="Status" ident="Rejected"></Attribute>  <!-- attribut valué -->
  </Requirement>
  <Requirement type="Requirement" ident="REQ_Design_1">
    <Attribute type="Mandatory"></Attribute>                <!-- attribut booléen : présence -->
  </Requirement>
</ModificationDocument>
```

Sources : `EX\Coupling\CATIA V5\implements.xml`, `EX\Coupling\Modelica\SRS_CC-modif.xml`,
`EX\Use Cases\UC6-MultiLevels-XML-Word-Soft\Design_additions.xml`.

- `type` = nom d'`ElementType` dans le **type pour éléments ajoutés** (déclaré par
  `ModificationDocument<n>Type` dans le `.rqtf`) ; `ident` = identifiant de l'élément visé, ou valeur
  pour un attribut ou une référence.
- Ce format est **essentiel** : c'est là que vivent les liens et attributs créés dans l'outil. ReqtiCraft
  doit le lire **et l'écrire**.

### 7.3 Reviewer **[EX]**

- `Checklist_{GUID}.xml` : `<Checklist xmlns="http://reqtify.com/XMLSchema-instance/Reviewer" guid name [description]>`
  contenant des `<items guid name applyTo="Project|Requirement|…" [description]/>`. Il existe
  `config\reviewer` (bibliothèque, par exemple DO-178B) et `<projet>.review\` (projet).
- `Association_{GUID}.xml` : `<Association object="<projet|nom de document>" checklist="{GUID}" guid>`
  contenant des `<results guid item="{GUID item}" object="<entier>" status="open|approved|rejected" creator modifier creationDate modificationDate>`
  (dates en secondes Unix), eux-mêmes contenant `<cars guid label priority reporter creationDate modificationDate>`.
- **Limite** : `results@object` est un entier (par exemple `729336682`) qui désigne un élément du projet
  sans référence stable visible. Sa résolution vers une exigence est **inconnue** (probablement un
  identifiant interne du `.rqtfimage`). L'import des résultats de revue au niveau de l'exigence est
  donc hors MVP. L'import des checklists, lui, est simple.

## 8. Résultats et instantanés `.rqtfimage`

- **[DOC]** UG « Analysis Results File » : fichier binaire propriétaire, recréé par réanalyse s'il est
  supprimé. Il sert aussi d'instantané et de format d'échange en mode « visionneuse ».
- **[EX]** (inspection d'en-tête, sans outil de rétro-ingénierie) : un préambule ASCII
  `TniBoost <version> [n] s7:deflate s4:utf8 `, suivi d'un flux zlib. Une fois décompressé, on obtient
  la **sérialisation d'un graphe d'objets** internes (chaînes préfixées par leur longueur, `sN:`).
  Les versions 2 et 3 du préambule sont observées.
- **Décision** : **ne pas lire ce format.** Il reflète la structure interne du produit, ce qui sort du
  cadre clean room et le rend fragile d'une version à l'autre. Il est inutile pour la configuration,
  puisque l'on réanalyse les sources. Conséquence : l'**historique des instantanés** et les
  **associations marques→éléments** de Reqtify ne sont pas importés. La documentation d'import doit
  le dire clairement. Pour conserver un historique, il faut reconstituer des instantanés ReqtiCraft en
  analysant d'anciennes versions des documents (par exemple depuis Git).

## 9. Modèles de rapport `doc_models\*.xml` (information)

**[EX]** `config\doc_models\Library Reports.xml` : `<DocumentList name1..4>` › `<Document name1..4 type receiverClass>`
› `DocParameter`, `DataMethodCallLoop @receiver @variable @method @condition @sortMethod`, `Structure @type`
(title, sectionTitle, paragraph, table, titleRow, columnTitle…), `DataMethodCallText`, `DataTerminalText text1..4`
(libellés en 4 langues, dans l'ordre eng, fra, chn, jpn).
Ces modèles invoquent des **méthodes de l'API objet** du produit (`coveredDocuments`, `coveredRatio`,
`leafRequirements`…), documentées dans le Programming Guide. **Décision** : pas d'import automatique
en v1. ReqtiCraft fournit ses propres rapports équivalents (§3.13 du cahier des charges). Un
convertisseur partiel pourra être étudié plus tard à partir du **Programming Guide** (documentation
publique), et non du binaire.

## 10. Questions ouvertes et protocole boîte noire

Elles se tranchent en utilisant Reqtify comme un utilisateur (avec une licence valide), sur des
**documents de test créés par nous**, puis en observant les fichiers produits. Aucune analyse de binaire
n'est nécessaire.

| # | Question | Expérience |
|---|---|---|
| Q1 | Syntaxe XML et INI des **dossiers**, canaux **miroir** et **dépendance**, **annotations**, catégories de canal, mot de passe | Créer chaque objet dans l'éditeur de projet, enregistrer dans les deux formats, comparer. |
| Q2 | Couverture **OU** combinée : un `Cover` avec plusieurs `InputDocument` ? | Créer un canal combiné, enregistrer. |
| Q3 | Codes 4 et 5 de `Indexes` / `index1..4` (1, 2, 3 et 6 établis, voir §4.1) | Affecter les rôles GUID et parent dans le testeur d'expressions, enregistrer, comparer. |
| Q4 | Marqueurs d'`analysisTree` (`$o`, GUID, parent, cible…) | Cocher chaque rôle dans l'éditeur XML, enregistrer. |
| Q5 | Sens des 16 `Booleans` et identifiants des opérateurs de filtre | Basculer chaque option et chaque opérateur, enregistrer. |
| Q6 | Format exact des intermédiaires Word et Excel (numérotation, séparateur, échappements) | Option « Intermediate File » sur un corpus maison couvrant tous les cas. |
| Q6b | Intermédiaire PDF : séparateur exact entre style et texte, nommage des images, effet de chaque règle de paragraphe | Option « Intermediate File » sur des PDF maison (titres avec signets, tableaux, images, annotations, césures, colonnes). |
| Q7 | Paramètres de document en INI | Enregistrer un projet avec `withImages=1` en ancien format. |
| Q8 | Correspondance INI ↔ XML des `.types` | Bascule « Old format » dans l'éditeur de types, puis comparaison. |
| Q9 | Représentation d'un **type d'accès** (ZIP, HTTP, Git, Subversion…) et de ses paramètres (profil, branche, révision) dans le `.rqtf` | Créer un document de chaque type d'accès, enregistrer dans les deux formats, comparer. En attendant, ReqtiCraft lit des URI dans le chemin (§12.2). |
| Q10 | Intermédiaire PowerPoint : le dernier `NOTES` est-il suivi d'un marqueur (l'expression `NOTES([\s\S]*?)SLIDE` du type livré ne capture pas les notes de la dernière diapositive) ? Texte des masques (`MASTER`, `CUSTOM_LAYOUT`) | Option « Intermediate File » sur une présentation maison. |
| Q11 | Signification des `id` de `<Property>` du type File Properties (`id="1"`, `id="2"`, `id="10"`) | Option « Intermediate File » sur un fichier aux propriétés connues. |
| Q12 | Forme exacte des intermédiaires HTML (balise `A`, classe sur `P`), Visio (`<Property>` sous `<Shape>` ou sous `<Page>`) et MS Project (`TaskDependency to` : nom ou identifiant) | Option « Intermediate File » sur des documents maison. |

Les résultats seront consignés dans `docs/interop-evidence/` (fichiers produits + note datée), avec
l'étiquette **[BB]** (boîte noire).

## 11. Validation de l'implémentation (2026-10-28)

État de `rc-interop` et du moteur, mesuré sur le corpus livré (test local
`crates/rc-core/tests/reqtify_examples.rs`, avec `--config` sur le répertoire `config` de Reqtify) :

- **Lecture** : les 50 `.rqtf` (33 INI, 17 XML), les 89 `.types` (y compris `CsvXML Demo.types`,
  XML invalide avec des `<` bruts dans un attribut, que l'outil d'origine accepte), les `.filters`,
  `.marks` et documents de modification se lisent sans erreur.
- **Analyse avec les types de Reqtify** :

  | Exemple | Résultat ReqtiCraft | Attendu (contenu des documents) |
  |---|---|---|
  | `Coupling/Text/Text demo.rqtf` | 5 exigences, couverture 3/5 (60 %) | REQ1 à REQ5 ; REQ1, REQ2, REQ3 couvertes |
  | `Coupling/Word/Word Demo.rqtf` (docx) | 5 exigences avec attributs, titres imbriqués | 5 exigences annoncées |
  | `Coupling/Excel/Excel Demo.rqtf` | 5 exigences | 5 |
  | `Coupling/Excel/ExcelXML Demo.rqtf` | 5 exigences | 5 |
  | `Coupling/PDF/PDF Demo.rqtf` | 5 exigences | 5 |

- **Hors périmètre P0** (message explicite, pas d'échec) : documents Word 97-2003 `.doc`
  (`conversionProblem`, la majorité des exemples « Use Cases »), et convertisseurs propriétaires
  (`CScan` pour l'analyse syntaxique C/Java, `DoorsXML`, `FrameMaker`, `CSVtoXML`, `Word2Xml`…,
  règle `unknownConvertMethod`).

## 12. Connecteurs P1 et P2 (2026-10-29)

Les connecteurs P1 et P2 suivent le même principe que les P0 : un convertisseur produit un intermédiaire,
qu'un type d'analyse capture. Pour réutiliser les types des clients, ReqtiCraft produit, chaque fois que
c'est possible, **l'intermédiaire de la forme attendue par les types livrés**. Cette forme se lit dans les
expressions et arbres d'analyse de ces types : c'est la preuve principale de cette section **[EX]**. Les
Coupling Notes **[DOC]** la complètent. Aucun convertisseur d'origine n'a été exécuté ni inspecté.

### 12.1 Formats de documents (`rc-convert`)

| Format | Intermédiaire | Preuve |
|---|---|---|
| HTML | Lignes `H1`…`H6`, `P`, `LI`, `PRE` + tabulation + texte ; `.classe` après la balise ; `#TABLE`, `#TR`, `\|<col> TD<TAB>texte` ; `<Picture<src>` | [DOC] HTML Coupling Notes « Supported HTML tags » ; [EX] `Office/html.types` (`^H(\d)(?:\.[^\t]+)?\t(.+)`, `Section1Indexes=6,1,3,0`) |
| Markdown | Même forme que HTML (connecteur nouveau, sans équivalent d'origine) | — |
| AsciiDoc | Texte brut (aucun convertisseur) | [EX] `Office/AsciiDoc.types` : pas de `Tool`, expressions sur le texte source (`^= (.+)`) ; validé sur `EX\Coupling\AsciiDoc` (3 exigences) |
| RTF | Lignes stylées comme Word : `<style><TAB><numéro><séparateur><texte>`, tableaux `\|<col><style> texte` | [DOC] RTF Coupling Notes « Basics », « RTF Tables » ; [EX] `Office/rtf.types` (`^heading (\d+)\t…`) |
| OpenDocument Texte | Lignes stylées, nom affiché du style (`Heading 1`, `Requirement_ID`) | [DOC] OpenOffice Writer Coupling Notes ; [EX] `Office/openOfficeWriter.types`, `LibreOfficeWriter.types` ; validé sur `EX\Coupling\OpenOffice Writer` (5 exigences) |
| OpenDocument Classeur | Comme Excel (P0, via `calamine`) | [EX] `Office/openOfficeCalc.types` ; validé sur `EX\Coupling\OpenOffice Calc` (5 exigences) |
| PowerPoint `.pptx` (et Google Slides exporté) | `SLIDE <n>`, `SHAPES`, une ligne par paragraphe, `NOTES`, `COMMENTS`, `PICTURE <fichier>` | [DOC] PowerPoint Coupling Notes « Encoding » ; [EX] `Office/powerpoint.types`, `googleSlide.types` |
| FrameMaker (MIF) | Lignes stylées `<balise><TAB><numéro><texte>` ; tableaux à leur ancre | [DOC] FrameMaker Coupling Notes ; [EX] `Office/framemaker.types` (`^Heading1\t([\d\.*]+)\s+(.+)`). Les `.fm` et `.book` binaires exigent FrameMaker. |
| ReqIF / ReqIFz | XML ReqtiCraft résolu : `<Specification>`, `<Object ident kind label text>`, `<Attribute>`, `<Link>` | Norme OMG ReqIF 1.2 ; pas de type d'origine (connecteur nouveau) |
| Visio `.vsdx` | `<Document><Page name><Shape name text><Property name value/></Shape><Connector sourceName targetName/></Page>` | [DOC] Visio Coupling Notes « Intermediate File » (exemple) ; [EX] `Office/visio.types`. Place des `<Property>` : Q12 |
| MS Project (XML MSPDI) | `<Task name notes text1 text2 percentcomplete hyperlink><Resource name emailaddress/><TaskDependency to/></Task>` | [DOC] Project Coupling Notes (champs extraits) ; [EX] `Project/msproject.types`. Le `.mpp` binaire exige MS Project |
| UML/SysML (XMI, `.mdzip`) | XMI d'origine, suivi d'un `<Summary>` ReqtiCraft (stéréotypes et relations résolus) | [EX] `UML/MagicDraw.types` (arbres sur le XMI brut : `packagedElement xmi:type=…`) ; normes OMG XMI, UML, SysML |
| Médias | `<MediaInfo><media ref><track type="General"><Format/><Duration/><Comment/></track>…` ; sortie de l'outil MediaInfo s'il est installé | [DOC] MediaInfo Coupling Notes ; [EX] `Office/MediaInfo.types` |
| Propriétés de fichiers | `<FolderItem name path><Property id name value/>` | [EX] `Project/fileProperties.types` ; numérotation des `id` : Q11 |

### 12.2 Types d'accès (`rc-connect::access`)

**[DOC]** Plusieurs Coupling Notes décrivent un même mécanisme : ZIP, HTTP, Git, GitHub, Bitbucket,
Subversion, CVS, Perforce, PVCS, Synergy, ClearCase, Documentum, ProjectWise, DesignSync, Google Drive et
OneDrive. On choisit un « access type » pour un document ; l'outil en obtient une **copie locale**
(« intermediate access file »), puis l'analyse avec le type d'analyse du document. Règles reprises :

- **ZIP** : pour un type multitype (répertoire), toute l'archive est extraite. Sinon, seul le premier
  fichier ayant l'extension attendue est extrait, ou à défaut le premier fichier **[DOC]** ZIP Coupling
  Notes « Selecting the ZIP/7-Zip Access Type ». Les archives 7-Zip passent par `7z.exe`.
- **Git, GitHub, Bitbucket** : clone complet, une copie par branche (`dépôt.git_branche`), modifications
  locales conservées à la mise à jour **[DOC]** Git Coupling Notes « Specific Parameters ».
- **Subversion et CVS** : copie de travail ; paramètre `Revision` pour Subversion, `Tag` et `CVSROOT` pour
  CVS **[DOC]**.

La représentation d'un type d'accès dans le `.rqtf` n'est pas connue (Q9). En attendant, ReqtiCraft
accepte un paramètre `access`, ou un **URI dans le chemin du document** :
- `https://…` (téléchargement) ;
- `zip:a.zip!b.docx` (fichier d'une archive) ;
- `git+https://…#branche:chemin` et `github:propriétaire/dépôt@branche/chemin` ;
- `svn+…`, `cvs:…`, `p4://…` ;
- `gdrive:<id>`, `onedrive:<chemin>` ;
- `command:…` (commande externe).

Les outils sans connecteur dédié passent par cette commande, déclarée dans le profil de connexion :
ClearCase, Synergy, PVCS, Documentum, ProjectWise et DesignSync.

### 12.3 Outils ALM (`rc-connect::alm`)

| Outil | Requête du document | Intermédiaire | Preuve de la forme |
|---|---|---|---|
| Jira | JQL (API REST `search`) ; un export XML (RSS) est repris tel quel | `<issues key><fields summary description><priority name/>…` | [EX] `Project/JIRA.types` (2ᵉ type : `<issues key='$i'><fields summary='$l' description='$t'>`) ; validé sur `EX\Coupling\JIRA` (export XML, 2 exigences) |
| Redmine | Paramètres de `issues.xml` | XML natif de l'API (`<issue><id/><subject/><priority name/>`) | [EX] `Project/Redmine.types` |
| Bugzilla | Paramètres de `/rest/bug` | `<Bug><id/><summary/><comment/><whiteboard/>…` | [EX] `Project/bugzilla.types` |
| codebeamer | cbQL (`/api/v3/items/query`) | `<items id name description><parent id/><status name/>` | [EX] `Project/codeBeamer.types` |
| Jama | `project=<id>` ou `filter=<id>` | `<Item id name itemType description parent><Field label value/><Upstreamrelationship from type/>` | [EX] `Requirement Tools/JAMA.types` |
| Polarion | Requête Lucene et paramètre `project` (API REST) | `<WorkItem id title type status … linkedWorkItems="rôle={projet}ID,…"><Hyperlink uri/>` | [EX] `Requirement Tools/Polarion.types` (`linkedWorkItems='$iverifies=[^,]+\}([^,]+)'`) |
| DOORS Next | URL de l'API de publication (`/rm/publish/…`), XML repris tel quel | `ds:artifact`, `rrm:identifier`… | [EX] `Requirement Tools/DoorsNG.types` |
| Gestion des modifications (OSLC CM) | URL d'une requête OSLC | `<WorkItem url title description shortId severity status priority type>` | [EX] `Project/CCM.types` ; spécification OSLC CM 3.0 |
| DOORS classique | Exports ReqIF ou tableur | Le XML de la passerelle d'origine (`<Object number><Attribute name value>…`) n'est produit que par DOORS | [EX] `Requirement Tools/DOORS.types` |

**Identifiants.** Ils viennent d'un profil de connexion, un fichier INI hors du projet. Ce profil ne
contient pas le secret : il nomme la variable d'environnement qui le contient (`tokenEnv`,
`passwordEnv`). Le transport HTTP(S)/FTP passe par l'outil `curl`, qui reçoit le secret par l'entrée
standard.

**Mode hors ligne.** Une réponse d'API enregistrée dans un fichier est convertie sans connexion, ce qui
sert au corpus de test et au travail hors ligne.

### 12.4 Historiques de versions

| Source | Intermédiaire | Preuve |
|---|---|---|
| Subversion | `svn log --xml -v` tel quel : `<log><logentry revision><author/><date/><msg/><paths/>` | [DOC] Subversion Coupling Notes « Intermediate File » (révision, auteur, date, message) ; [EX] `Project/svn.types` (`<logentry revision="$i"><msg>$t</msg>`, `ForceProcessingAsDirectory=1`) |
| Git, Perforce | Même forme que Subversion (choix ReqtiCraft, pour réutiliser les mêmes types) | — |

### 12.5 Bases de données

**[DOC]** Selon les Access Coupling Notes (« Intermediate XML File with Access Objects »), l'intermédiaire
a la forme suivante :
- `<table name="…">` pour la table ;
- un `<record>` par ligne ;
- `<cell name="colonne">valeur</cell>` par cellule.

Le pilote ODBC « Microsoft Access Driver (*.mdb, *.accdb) » suffit à la conversion. Oracle passe par
`sqlplus` et un profil (serveur, base, utilisateur) **[DOC]** Oracle Coupling Notes.

ReqtiCraft produit cette forme pour :
- Access et toute source ODBC, via PowerShell sous Windows ;
- Oracle, via `sqlplus` ;
- SQLite, via `sqlite3`.

**Validation.** Sur `EX\Coupling\Access\Access_Demo.rqtf`, avec les types d'origine, on obtient
5 exigences de haut niveau et 5 de bas niveau, avec une couverture de 100 %, conforme au contenu de la
base.

### 12.6 Hors de portée

Les éléments suivants n'ont ni format ouvert ni outil disponible. Ils restent à traiter par le protocole
de plugins (`docs/plugin-protocol.md`) ou par une commande externe :
- formats binaires `.doc`, `.ppt`, `.vsd`, `.mpp`, `.fm` et `.book` ;
- convertisseurs d'analyse syntaxique `CScan`, `VhdlScan`, `Ada`, `Dymola` ;
- RequisitePro (`RequisiteProXml`), passerelle DOORS (`DoorsXML`, `DoorsBaselineSet`), PTC Integrity,
  Teamwork Cloud, SolidWorks ;
- l'écosystème Dassault (P3).
