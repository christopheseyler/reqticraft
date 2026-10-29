# Corpus de test maison

Tous ces fichiers sont générés par `tools/gen_fixtures.py` (bibliothèque standard Python,
sortie reproductible à l'octet près). Ne pas les modifier à la main : modifier le script puis
relancer :

```bash
python -I tools/gen_fixtures.py
```

## `thermostat/`

Un même scénario sur trois niveaux, décliné en quatre formats (`text/`, `docx/`, `xlsx/`, `pdf/`).
Chaque dossier contient aussi le projet au format Reqtify XML (`thermostat.rqtf`) et INI
(`thermostat-ini.rqtf`).

| Document | Exigences | Couvre |
|---|---|---|
| `specification` | THS_REQ1 à THS_REQ6 | — |
| `design` | DES_REQ1 à DES_REQ7 | la spécification |
| `tests` | TST_REQ1 à TST_REQ6 | la conception |

Cas volontairement présents (détail dans `thermostat/expected.json`) :

- `THS_REQ5` et `DES_REQ7` ne sont pas couvertes ;
- `DES_REQ6` et `DES_REQ7` sont dérivées (elles ne couvrent rien) ;
- `DES_REQ5` référence `THS_REQ99`, qui n'existe pas ;
- couverture attendue : spécification 5/6 (83 %), conception 6/7 (86 %).

Projets complémentaires :

| Projet | Contenu | Attendu |
|---|---|---|
| `xlsx/thermostat-xml.rqtf` | mêmes tableurs, type `ExcelXML` (arbres d'analyse) | comme ci-dessus |
| `code/thermostat.rqtf` | conception couverte par `code/src` (C, en-tête, Python ; `notes.txt` ignoré) | conception 5/7 |
| `json/thermostat.rqtf` | spécification en JSON couverte par la conception | spécification 5/6 |
| `composite/thermostat.rqtf` | sous-projet « Fournisseur » dont la spécification est remplacée par celle du projet | spécification 5/6 |

Le PDF n'utilise que du texte positionné et des signets : aucun style de paragraphe. Les lignes y
sont fusionnées par les extracteurs naïfs, ce qui teste la reconstitution des paragraphes.
