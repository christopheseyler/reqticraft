//! Corpus maison (`fixtures/thermostat`) : chaque variante de format doit donner
//! les résultats de `expected.json`.

use rc_core::rc_analysis::Analysis;
use rc_core::rc_model::Severity;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thermostat")
}

fn expected() -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join("expected.json")).unwrap()).unwrap()
}

fn analyze(project: &str) -> Analysis {
    rc_core::analyze_project(&root().join(project), &[])
        .unwrap_or_else(|e| panic!("{project} : {e}"))
        .analysis
}

fn doc_index(a: &Analysis, name: &str) -> usize {
    a.documents
        .iter()
        .position(|d| d.name == name)
        .unwrap_or_else(|| panic!("document {name} absent"))
}

fn idents(a: &Analysis, set: &std::collections::BTreeSet<(usize, usize)>) -> Vec<String> {
    let mut v: Vec<String> = set
        .iter()
        .map(|&(d, e)| a.documents[d].elements[e].ident.clone().unwrap_or_default())
        .collect();
    v.sort();
    v
}

fn check_full_project(project: &str) {
    let exp = expected();
    let a = analyze(project);
    let names = [
        ("specification", "Spécification"),
        ("design", "Conception"),
        ("tests", "Tests"),
    ];
    for (key, name) in names {
        let d = doc_index(&a, name);
        assert_eq!(
            a.summaries[d].requirements as u64,
            exp["documents"][key].as_u64().unwrap(),
            "{project} : exigences de {name}"
        );
    }
    for c in exp["coverage"].as_array().unwrap() {
        let covered = names.iter().find(|(k, _)| *k == c["covered"]).unwrap().1;
        let d = doc_index(&a, covered);
        assert_eq!(
            a.summaries[d].covered.map(|n| n as u64),
            c["covered_count"].as_u64(),
            "{project} : couverture de {covered}"
        );
        assert_eq!(
            a.summaries[d].requirements as u64,
            c["total"].as_u64().unwrap()
        );
    }
    let as_strings = |v: &Value| -> Vec<String> {
        let mut s: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect();
        s.sort();
        s
    };
    assert_eq!(
        idents(&a, &a.uncovered),
        as_strings(&exp["uncovered"]),
        "{project} : non couvertes"
    );
    assert_eq!(
        idents(&a, &a.derived),
        as_strings(&exp["derived"]),
        "{project} : dérivées"
    );
    let unknown: Vec<&str> = a
        .messages
        .iter()
        .filter(|m| m.rule == "coverUnknown")
        .map(|m| m.text.as_str())
        .collect();
    assert_eq!(unknown.len(), 1, "{project} : {unknown:?}");
    assert!(unknown[0].contains("THS_REQ99"));
    // Aucun autre problème que ceux attendus.
    let others: Vec<_> = a
        .messages
        .iter()
        .filter(|m| !["coverUnknown", "uncovered", "derived"].contains(&m.rule.as_str()))
        .collect();
    assert!(
        others.is_empty(),
        "{project} : messages inattendus {others:#?}"
    );
}

#[test]
fn text_projects() {
    check_full_project("text/thermostat.rqtf");
    check_full_project("text/thermostat-ini.rqtf");
}

#[test]
fn word_projects() {
    check_full_project("docx/thermostat.rqtf");
    check_full_project("docx/thermostat-ini.rqtf");
}

#[test]
fn spreadsheet_projects() {
    check_full_project("xlsx/thermostat.rqtf");
    check_full_project("xlsx/thermostat-ini.rqtf");
    check_full_project("xlsx/thermostat-xml.rqtf");
}

#[test]
fn pdf_projects() {
    check_full_project("pdf/thermostat.rqtf");
    check_full_project("pdf/thermostat-ini.rqtf");
}

#[test]
fn partial_projects() {
    let exp = expected();
    for (project, docs) in exp["partial"].as_object().unwrap() {
        let a = analyze(project);
        for (name, counts) in docs.as_object().unwrap() {
            let d = doc_index(&a, name);
            let counts: Vec<u64> = counts
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_u64().unwrap())
                .collect();
            assert_eq!(
                a.summaries[d].covered.map(|n| n as u64),
                Some(counts[0]),
                "{project} : {name}"
            );
            assert_eq!(
                a.summaries[d].requirements as u64, counts[1],
                "{project} : {name}"
            );
        }
        assert!(
            a.messages.iter().all(|m| m.severity != Severity::Error
                || ["coverUnknown", "uncovered"].contains(&m.rule.as_str())),
            "{project} : {:#?}",
            a.messages
        );
    }
}

#[test]
fn code_files_outside_the_patterns_are_ignored() {
    let a = analyze("code/thermostat.rqtf");
    let code = &a.documents[doc_index(&a, "Code")];
    let files: Vec<&str> = code
        .elements
        .iter()
        .filter(|e| e.type_name == "File")
        .filter_map(|e| e.ident.as_deref())
        .collect();
    assert_eq!(
        files,
        [
            "src/acquisition.c",
            "src/api/rest.py",
            "src/regulator.c",
            "src/sensor.h"
        ]
        .map(|f| f.trim_start_matches("src/"))
    );
}
