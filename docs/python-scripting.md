# ReqtiCraft — Scripts Python

> Statut : v1 (prototype) — 2026-10-30. Implémentation : `crates/rc-script`.

Les scripts personnalisent ce que le moteur ne sait pas faire seul : règles de qualité propres à un
projet, vérifications, attributs calculés, rapports et intégrations. Ils s'écrivent en Python, avec
une API conçue pour ReqtiCraft. Ce n'est pas une reprise d'OTScript : seule la documentation publique
de Reqtify a servi à cerner les besoins (personnaliser, définir des règles, des vérifications).

La v1 ne fournit que les **vérifications** (`@check`). Les attributs calculés (`@attribute`) et les
rapports (`@report`) suivront sur le même protocole.

## Exemple

`scripts/regles.py`, à côté du projet :

```python
from reqticraft import check

@check(severity="error", message="exigence couverte par aucun test")
def non_testee(req):
    # Seulement pour les documents de spécification.
    if req.document.type == "Spécification":
        return not req.covered_by(doc_type="Tests")

@check(on="element", types=["Exigence"])
def texte_vague(req):
    vagues = [m for m in ("rapidement", "environ", "si possible") if m in req.text.lower()]
    if vagues:
        return f"terme vague : {', '.join(vagues)}"
```

Chaque problème apparaît dans la vue **Problèmes**, avec pour règle le nom de la vérification
(`non_testee`), et dans la sortie de `reqticraft analyze`.

## Emplacement des scripts

Tous les fichiers `*.py` du répertoire `scripts/` placé à côté du projet, par ordre alphabétique.
Un script peut importer ses voisins (le répertoire `scripts/` est ajouté au chemin d'import) et toute
bibliothèque installée dans l'interpréteur.

## Exécution

Après chaque analyse, ReqtiCraft lance **un processus Python** qui :

1. reçoit le modèle analysé, en lecture seule ;
2. exécute les scripts, qui enregistrent leurs vérifications ;
3. applique chaque vérification aux éléments visés et renvoie les problèmes.

Les problèmes s'ajoutent aux messages de l'analyse : vue Problèmes, compteurs, `--fail-on`, rapports.
Un script qui plante n'arrête ni l'application ni l'analyse : l'erreur devient un message
`scriptError`.

### Interpréteur

La variable `REQTICRAFT_PYTHON` (chemin d'un exécutable) si elle est définie ; sinon le premier
programme trouvé parmi `python` puis `py -3` sous Windows, `python3` puis `python` ailleurs. Python 3.9 ou plus récent. Une distribution embarquée
(Python « embeddable » sous Windows) est prévue avec l'installateur, sans changer ce protocole.

### Confiance

Un script est du code exécuté avec les droits de l'utilisateur. Il ne s'exécute que si le répertoire
du projet est **approuvé** :

- dans l'interface, la vue Problèmes signale les scripts présents et propose de les approuver ;
- l'approbation est enregistrée dans `~/.reqticraft/trusted-projects.txt` (un répertoire par ligne) ;
- en ligne de commande, `reqticraft analyze --scripts` les exécute sans approbation (CI).

Sans approbation, un message d'information `scriptsNotTrusted` indique les scripts ignorés.

### Délai

Le processus est arrêté au-delà de 120 s (variable `REQTICRAFT_SCRIPT_TIMEOUT`, en secondes) ; un
message `scriptError` le signale.

## API (module `reqticraft`)

### `@check`

```python
@check(name=None, severity="warning", on="requirement", types=None, doc_types=None, message=None)
```

| Paramètre | Rôle |
|---|---|
| `name` | Identifiant de la règle ; par défaut le nom de la fonction. |
| `severity` | `"error"`, `"warning"` ou `"information"`. |
| `on` | Ce que reçoit la fonction : `"requirement"` (exigences et macro-exigences), `"element"` (tout élément de l'arbre : sections, exigences, entités), `"document"` ou `"project"`. |
| `types` | Types d'éléments retenus (nom ou liste de noms). |
| `doc_types` | Types d'analyse des documents retenus. |
| `message` | Texte du problème quand la fonction renvoie `True`. |

La fonction renvoie :

- `None`, `False`, `""` ou une liste vide : pas de problème ;
- `True` : un problème, de texte `message` (ou le nom de la règle) ;
- une chaîne : un problème de ce texte ;
- une liste (ou un générateur) de chaînes : un problème par chaîne ;
- `Problem(text, target=None, severity=None)` (seul ou en liste) : pour viser un autre élément ou
  document, ou changer la sévérité.

Le problème est rattaché à l'objet reçu, sauf `target` contraire.

### `Element`

| Attribut / méthode | Valeur |
|---|---|
| `id`, `label` | Identifiant et libellé capturés (`None` si absents). |
| `name` | Nom affiché (identifiant et libellé). |
| `type` | Type d'élément (`"Exigence"`, `"Section"`…). |
| `kind` | Classe : `section`, `requirement`, `macro_requirement`, `entity`. |
| `text` | Texte de l'élément. |
| `attributes` | Dictionnaire nom → valeur (`None` pour un attribut sans valeur). |
| `attr(nom, défaut=None)` | Valeur d'un attribut. |
| `status` | `covered`, `uncovered`, `derived` ou `none`, comme dans la vue Couverture. |
| `is_requirement` | Exigence ou macro-exigence. |
| `document`, `parent`, `children` | Position dans l'arbre. |
| `ancestors()`, `descendants()` | Parcours de l'arbre. |
| `covers(doc_type=None, document=None)` | Éléments ou documents que l'élément couvre. |
| `covered_by(doc_type=None, document=None)` | Éléments ou documents qui le couvrent. |
| `line` | Ligne dans le fichier intermédiaire. |

### `Document`

`name`, `type`, `text`, `attributes`, `elements` (tous), `roots`, `requirements`, `find(id)`,
`covers(...)`, `covered_by(...)` (liens qui visent le document entier), `project`.

### `Project`

`name`, `dir`, `documents`, `document(nom)`, `elements()` (tous les éléments de tous les documents).

## Protocole

Le même que les plugins de connecteurs (`docs/plugin-protocol.md`) : JSON-RPC 2.0, un message par
ligne en UTF-8 sur l'entrée et la sortie standard. ReqtiCraft lance
`python -X utf8 -m reqticraft._host` depuis le répertoire du projet, avec le paquet `reqticraft` en
tête de `PYTHONPATH` (le paquet est intégré à l'exécutable et écrit dans un répertoire temporaire).

Requête :

```json
{"jsonrpc": "2.0", "id": 1, "method": "check",
 "params": {"project": {"name": "thermostat", "dir": "C:\\projets\\thermostat"},
            "scripts": ["C:\\projets\\thermostat\\scripts\\regles.py"],
            "documents": [{"name": "Spec", "type": "Spécification", "text": "", "attributes": [],
                           "roots": [0],
                           "elements": [{"type": "Exigence", "kind": "requirement", "id": "REQ1",
                                         "label": null, "text": "…", "attributes": [["Priorité", "1"]],
                                         "parent": null, "children": [], "line": 3,
                                         "status": "uncovered"}]}],
            "links": [[1, 4, 0, 0]]}}
```

`links` : un lien de couverture par quadruplet `[document couvrant, élément couvrant, document
couvert, élément couvert]`, l'élément valant `null` pour un document entier.

Réponse :

```json
{"jsonrpc": "2.0", "id": 1,
 "result": {"checks": 2,
            "problems": [{"rule": "non_testee", "severity": "error", "text": "…",
                          "document": 0, "element": 0}],
            "errors": [{"text": "regles.py, ligne 12 : ZeroDivisionError: …",
                        "document": null, "element": null}]}}
```

La sortie standard des scripts (`print`) est redirigée vers la sortie d'erreur, ce qui garde la
sortie standard pour la réponse. Une vérification qui lève une exception est arrêtée après sa première
erreur, pour ne pas produire une erreur par élément.

## Suite prévue

- `@attribute(nom)` : attributs calculés, visibles dans les vues, les filtres et les rapports ;
- `@report(fichier)` : rapports et exports libres (Excel, Jira…) ;
- distribution de Python avec l'installateur Windows ;
- vérifications rattachées à un utilisateur (`~/.reqticraft/scripts/`).
