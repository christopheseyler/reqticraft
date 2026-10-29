# ReqtiCraft — Protocole de plugins de connecteurs

> Statut : v1 — 2026-10-29. Implémentation : `crates/rc-plugin`.

Un plugin ajoute des convertisseurs que la bibliothèque intégrée ne connaît pas : outils
propriétaires, référentiels internes, formats maison. C'est un programme écrit dans n'importe quel
langage. Il dialogue en **JSON-RPC 2.0** sur son entrée et sa sortie standard, à raison d'un message
JSON par ligne (UTF-8).

## Déclaration

Chaque plugin a un répertoire qui contient un fichier `plugin.ini` :

```ini
[plugin]
name=referentiel-maison
command=python3 plugin.py
tools=Referentiel,ReferentielBaseline
```

- `command` : programme et arguments, séparés par des espaces (les guillemets regroupent un argument).
  Un programme relatif est d'abord cherché dans le répertoire du plugin, et la commande est lancée
  depuis ce répertoire.
- `tools` : valeurs de `convertTool` (types d'analyse) prises en charge, séparées par des virgules ; la
  casse n'est pas significative.

Les répertoires de plugins sont cherchés, dans cet ordre :

1. `plugins/` à côté du projet ;
2. `~/.reqticraft/plugins/` ;
3. les répertoires de la variable `REQTICRAFT_PLUGINS`, séparés par `;` sous Windows et par `:` ailleurs.

Pour chaque emplacement, ReqtiCraft lit son `plugin.ini` s'il en a un, sinon celui de chaque
sous-répertoire. Il ne s'adresse à un plugin que si aucun convertisseur intégré ni aucun connecteur ne
correspond au `convertTool` du type.

## Échanges

Le plugin est lancé une fois par document à convertir. ReqtiCraft lui envoie la requête suivante :

```json
{"jsonrpc": "2.0", "id": 1, "method": "convert",
 "params": {"tool": "Referentiel", "location": "PROJ-12", "params": {"profile": "prod"},
            "baseDir": "C:\\projets\\thermostat", "workDir": "C:\\projets\\thermostat\\.reqticraft\\work\\Spec"}}
```

Les champs de `params` :

- `location` : le chemin du document tel qu'il est écrit dans le projet, qui peut aussi être une
  requête ou une URL ;
- `params` : les paramètres du document ;
- `baseDir` : le répertoire du projet ;
- `workDir` : un répertoire réservé au document, que le plugin peut créer.

Le plugin répond sur une ligne, avec le même `id` :

```json
{"jsonrpc": "2.0", "id": 1, "result": {"format": "xml", "content": "<items>…</items>"}}
```

`format` vaut `xml` ou `text` : c'est la forme de l'intermédiaire, que le type d'analyse capture
ensuite. En cas d'échec, le plugin renvoie une erreur JSON-RPC
(`{"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "…"}}`). Son message devient une
règle `conversionProblem` du document.

ReqtiCraft envoie ensuite `{"jsonrpc": "2.0", "id": 2, "method": "shutdown"}`, puis ferme l'entrée
standard du plugin. Le plugin doit alors se terminer. Les lignes que le plugin écrit sur sa sortie
standard et qui ne sont pas la réponse attendue sont ignorées. Sa sortie d'erreur est reprise dans le
message si aucune réponse n'arrive.

## Sécurité

Un plugin s'exécute avec les droits de l'utilisateur. Ne déclarer que des plugins de confiance. Les
secrets doivent venir des variables d'environnement, comme pour les profils de connexion, et jamais
des paramètres du document.

## Exemple

`crates/rc-plugin/src/bin/reqticraft-plugin-demo.rs` est un plugin complet d'une cinquantaine de
lignes : il transforme chaque ligne `ID : texte` en `<req id="ID">texte</req>`. En Python :

```python
import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if msg["method"] == "shutdown":
        break
    p = msg["params"]
    with open(f'{p["baseDir"]}/{p["location"]}', encoding="utf-8") as f:
        xml = "".join(f'<req id="{a.strip()}">{b.strip()}</req>'
                      for a, b in (l.split(":", 1) for l in f if ":" in l))
    print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                      "result": {"format": "xml", "content": f"<reqs>{xml}</reqs>"}}), flush=True)
```
