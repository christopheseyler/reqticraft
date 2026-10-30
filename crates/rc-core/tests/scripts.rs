//! Scripts Python : vérifications appliquées au projet du thermostat.

use rc_model::Severity;
use std::path::{Path, PathBuf};

fn project(name: &str, script: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rc-scripts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    let text = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thermostat/text");
    for f in [
        "thermostat.rqtf",
        "specification.txt",
        "design.txt",
        "tests.txt",
    ] {
        std::fs::copy(text.join(f), dir.join(f)).unwrap();
    }
    std::fs::write(dir.join("scripts").join("regles.py"), script).unwrap();
    dir.join("thermostat.rqtf")
}

const SCRIPT: &str = r#"
from reqticraft import check, Problem

@check(severity="error", message="exigence non couverte par la conception")
def sans_conception(req):
    if req.document.name == "Spécification":
        return not req.covered_by(document="Conception")

@check(on="document", severity="information")
def compte(doc):
    print("ignoré : la sortie standard des scripts va vers stderr")
    return f"{len(doc.requirements)} exigences"

@check
def plante(req):
    return 1 / 0
"#;

#[test]
fn scripts_add_problems() {
    let path = project("run", SCRIPT);
    let mut run = rc_core::analyze_project(&path, &[]).unwrap();
    let before = run.analysis.messages.len();
    let report = rc_core::apply_scripts(&mut run, true);
    let added = &run.analysis.messages[before..];
    if added
        .iter()
        .any(|m| m.rule == "scriptError" && m.text.contains("Python introuvable"))
    {
        eprintln!("Python absent : test ignoré");
        return;
    }
    assert!(report.ran);
    assert_eq!(report.scripts, ["regles.py"]);
    assert_eq!(report.checks, 3);

    // THS_REQ5 n'est couverte par aucun élément de conception.
    let missing: Vec<_> = added
        .iter()
        .filter(|m| m.rule == "sans_conception")
        .collect();
    assert_eq!(missing.len(), 1, "{added:#?}");
    let m = missing[0];
    assert_eq!(m.severity, Severity::Error);
    assert_eq!(m.document.as_deref(), Some("Spécification"));
    let doc = &run.analysis.documents[0];
    assert_eq!(
        doc.elements[m.element.unwrap()].ident.as_deref(),
        Some("THS_REQ5")
    );
    assert!(m.line.is_some());

    // Une information par document.
    let counts: Vec<_> = added.iter().filter(|m| m.rule == "compte").collect();
    assert_eq!(counts.len(), 3);
    assert_eq!(counts[0].severity, Severity::Information);
    assert!(counts[0].text.ends_with("exigences"), "{}", counts[0].text);

    // Une seule erreur pour la vérification qui lève une exception, avec sa ligne.
    let errors: Vec<_> = added.iter().filter(|m| m.rule == "scriptError").collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].text.contains("plante"), "{}", errors[0].text);
    assert!(
        errors[0].text.contains("regles.py, ligne 16"),
        "{}",
        errors[0].text
    );
    assert!(errors[0].text.contains("ZeroDivisionError"));
    assert_eq!(report.errors, 1);
    assert_eq!(report.problems, 4);
}

#[test]
fn syntax_errors_are_reported() {
    let path = project(
        "syntax",
        "from reqticraft import check\n\n@check\ndef f(req)\n    pass\n",
    );
    let mut run = rc_core::analyze_project(&path, &[]).unwrap();
    rc_core::apply_scripts(&mut run, true);
    let last = run.analysis.messages.last().unwrap();
    assert_eq!(last.rule, "scriptError");
    if !last.text.contains("Python introuvable") {
        assert!(last.text.contains("SyntaxError"), "{}", last.text);
    }
}

#[test]
fn untrusted_scripts_do_not_run() {
    let path = project("untrusted", SCRIPT);
    let mut run = rc_core::analyze_project(&path, &[]).unwrap();
    let report = rc_core::apply_scripts(&mut run, false);
    // Le répertoire temporaire n'est jamais approuvé.
    assert!(!report.trusted);
    assert!(!report.ran);
    let last = run.analysis.messages.last().unwrap();
    assert_eq!(last.rule, "scriptsNotTrusted");
    assert_eq!(last.severity, Severity::Information);
    assert!(last.text.contains("regles.py"));
}
