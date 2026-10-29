//! Analyse des exemples livrés avec Reqtify, avec les types de Reqtify, en local
//! uniquement (voir CONTRIBUTING.md). Sans `REQTICRAFT_REQTIFY_EXAMPLES` et
//! `REQTICRAFT_REQTIFY_CONFIG`, les tests ne vérifient rien.
//!
//! Valeurs attendues : contenu des documents d'exemple (lus dans leur outil) et
//! résultat annoncé par leur `readme.txt` ou par la documentation.

use std::path::PathBuf;

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

fn setup() -> Option<(PathBuf, Vec<PathBuf>)> {
    let examples = env_dir("REQTICRAFT_REQTIFY_EXAMPLES")?;
    let config = env_dir("REQTICRAFT_REQTIFY_CONFIG")?;
    Some((examples, vec![config]))
}

/// (projet, document, exigences attendues, couverture attendue).
const CASES: &[(&str, &str, usize, Option<usize>)] = &[
    // requirements.txt : REQ1 à REQ5 ; Coverage.txt couvre REQ1, REQ2 et REQ3.
    ("Coupling/Text/Text demo.rqtf", "Requirements", 5, Some(3)),
    // Le document annonce cinq exigences conformes au type Word par défaut.
    ("Coupling/Word/Word Demo.rqtf", "Requirements", 5, None),
    ("Coupling/Excel/Excel Demo.rqtf", "Requirements", 5, None),
    ("Coupling/Excel/ExcelXML Demo.rqtf", "Requirements", 5, None),
    ("Coupling/PDF/PDF Demo.rqtf", "Requirements", 5, None),
];

#[test]
fn p0_examples_match_their_documented_content() {
    let Some((examples, config)) = setup() else {
        eprintln!("variables REQTICRAFT_REQTIFY_* absentes : test ignoré");
        return;
    };
    for &(project, document, requirements, covered) in CASES {
        let run = rc_core::analyze_project(&examples.join(project), &config)
            .unwrap_or_else(|e| panic!("{project} : {e}"));
        let a = &run.analysis;
        let d = a.documents.iter().position(|x| x.name == document).unwrap();
        assert_eq!(a.summaries[d].requirements, requirements, "{project}");
        assert_eq!(a.summaries[d].covered, covered, "{project}");
        let blocking: Vec<_> = a
            .messages
            .iter()
            .filter(|m| {
                [
                    "unknownConvertMethod",
                    "conversionProblem",
                    "analyseError",
                    "unknownDocumentType",
                ]
                .contains(&m.rule.as_str())
            })
            .collect();
        assert!(blocking.is_empty(), "{project} : {blocking:#?}");
    }
}

#[test]
fn every_example_project_is_analyzed_without_crashing() {
    let Some((examples, config)) = setup() else {
        return;
    };
    let mut stack = vec![examples];
    let mut count = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rqtf") {
                rc_core::analyze_project(&path, &config)
                    .unwrap_or_else(|e| panic!("{} : {e}", path.display()));
                count += 1;
            }
        }
    }
    assert!(count >= 50, "{count} projets");
}
