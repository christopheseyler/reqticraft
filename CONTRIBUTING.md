# Contribuer à ReqtiCraft

## Règles clean room (non négociables)

ReqtiCraft est un outil original, interopérable avec Reqtify (Dassault Systèmes). Pour rester
indépendant de ce produit :

1. **Fonctionnalités** : elles se déduisent uniquement de la documentation officielle, des fichiers
   de configuration et d'exemple livrés, et du comportement visible. Chaque comportement implémenté
   doit pouvoir citer sa source (`docs/functional-spec.md`).
2. **Pas de rétro-ingénierie de la logique** : ne pas décompiler `reqtify.exe` ni ses scripts
   (`config\otscript`) pour en reproduire le fonctionnement. Ne recopier aucun code, pseudocode ou
   structure interne.
3. **Formats** : toute conclusion sur un format de fichier est consignée dans
   `docs/interop-formats.md` avec sa preuve (doc, fichier d'exemple, observation en boîte noire).
4. **Aucun fichier Dassault dans le dépôt** : ni exemples, ni types, ni gabarits, ni PDF de
   documentation, ni `.rqtfimage`. Les types par défaut de ReqtiCraft sont réécrits à partir des
   conventions documentées. Le script `tools/check-clean-room.sh` le vérifie en CI.
5. **Fixtures** : uniquement des documents créés par nous (`fixtures/`, générés par
   `tools/gen_fixtures.py`).
6. **Tests sur le corpus Reqtify** : ils s'exécutent en local seulement, à partir du chemin donné par
   la variable `REQTICRAFT_REQTIFY_EXAMPLES`, et sont ignorés quand elle est absente.
7. **Preuves en boîte noire** : un fichier enregistré par Reqtify à partir de *nos* documents de test
   peut être versionné dans `docs/interop-evidence/`, avec une note datée qui décrit l'expérience.

## Organisation

| Dossier | Contenu |
|---|---|
| `crates/rc-model` | Modèle de données |
| `crates/rc-interop` | Formats Reqtify (lecture, écriture) |
| `crates/rc-convert` | Convertisseurs : natif → intermédiaire |
| `crates/rc-capture` | Moteur de capture piloté par les types |
| `crates/rc-analysis` | Couverture, impact, règles, filtres |
| `crates/rc-snapshot` | Instantanés |
| `crates/rc-report` | Rapports |
| `crates/rc-plugin` | Plugins de connecteurs |
| `crates/rc-cli` | Binaire `reqticraft` |
| `app/` | Application de bureau (Tauri 2 + React) |
| `fixtures/` | Corpus de test maison |
| `docs/` | Spécifications et preuves d'interopérabilité |

## Commandes

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo run -p rc-cli -- --help
```

```bash
npm --prefix app run tauri dev
```

Prérequis : Rust stable, Node.js 22 ou plus. Sous Windows, les outils de compilation C++ de
Visual Studio (MSVC) ; sous Linux, les paquets WebKitGTK listés dans `.github/workflows/ci.yml`.

## Commits

Messages en français, à l'impératif, préfixés par la zone (`docs:`, `model:`, `interop:`, `app:`,
`ci:`…).
