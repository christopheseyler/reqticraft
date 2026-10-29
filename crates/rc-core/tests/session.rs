//! Session de l'interface sur le corpus maison : vue d'ensemble, pages
//! d'éléments, détails, impact et rechargement après modification.

use rc_core::rc_model::ElementClass;
use rc_core::session::{call, Change, ElementQuery, Session, Status};
use serde_json::json;
use std::path::{Path, PathBuf};

fn text_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thermostat/text")
}

fn find(s: &Session, doc: usize, ident: &str) -> usize {
    s.run.analysis.documents[doc]
        .elements
        .iter()
        .position(|e| e.ident.as_deref() == Some(ident))
        .unwrap_or_else(|| panic!("{ident} absent"))
}

#[test]
fn overview_and_queries() {
    let s = Session::open(&text_project().join("thermostat.rqtf"), &[]).unwrap();
    let o = s.overview();
    assert_eq!(o.documents.len(), 3);
    assert_eq!(o.requirements, 19);
    assert_eq!((o.covered, o.coverable), (11, 13));
    assert_eq!(o.derived, 2);
    assert!(o.errors >= 1);

    let page = s
        .elements(
            0,
            &ElementQuery {
                status: Some("uncovered".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].ident.as_deref(), Some("THS_REQ5"));

    let all = s.elements(0, &ElementQuery::default()).unwrap();
    assert!(
        all.rows.iter().any(|r| r.class == ElementClass::Section),
        "sections incluses"
    );
    let search = s
        .elements(
            0,
            &ElementQuery {
                text: Some("hystérésis".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(search.rows[0].ident.as_deref(), Some("THS_REQ4"));

    let req1 = find(&s, 0, "THS_REQ1");
    let d = s.details(0, req1).unwrap();
    assert_eq!(d.status, Status::Covered);
    assert_eq!(d.covered_by[0].name, "DES_REQ1 Échantillonnage du capteur");
    assert!(d.text.contains("toutes les 10 secondes"));
    assert!(d.attributes.iter().any(|(n, _)| n == "Priority"));

    // Impact : THS_REQ1 ← DES_REQ1 ← test(s).
    let i = s.impact(0, req1).unwrap();
    assert!(i.upstream.is_empty());
    assert_eq!(i.downstream.len(), 1);
    assert!(!i.downstream[0].children.is_empty());

    let des5 = find(&s, 1, "DES_REQ5");
    assert!(s
        .details(1, des5)
        .unwrap()
        .problems
        .iter()
        .any(|p| p.text.contains("THS_REQ99")));
}

#[test]
fn json_calls() {
    let mut session = None;
    assert!(call(&mut session, "overview", &json!({})).is_err());
    let path = text_project().join("thermostat.rqtf");
    let o = call(&mut session, "open", &json!({ "path": path })).unwrap();
    assert_eq!(o["requirements"], 19);
    let p = call(
        &mut session,
        "elements",
        &json!({ "document": 1, "status": "derived" }),
    )
    .unwrap();
    assert_eq!(p["total"], 2);
    let problems = call(&mut session, "problems", &json!({ "severity": "error" })).unwrap();
    assert!(problems
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["severity"] == "error"));
    assert!(call(&mut session, "nope", &json!({})).is_err());
}

#[test]
fn reload_marks_changes() {
    let dir = std::env::temp_dir().join(format!("rc-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(text_project()).unwrap().flatten() {
        std::fs::copy(f.path(), dir.join(f.file_name())).unwrap();
    }
    let mut s = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();
    assert!(s.changed_files().is_empty());

    // THS_REQ5 devient couverte : le tutoriel attend 100 % sur la spécification.
    let design = dir.join("design.txt");
    let mut text = std::fs::read_to_string(&design).unwrap();
    text.push_str(
        "\nDES_REQ8 : Mode absence\n<<Consigne abaissée de 3 °C.>>\n[Covers: THS_REQ5]\n",
    );
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&design, text).unwrap();
    assert_eq!(s.changed_files().len(), 1);

    s.reload().unwrap();
    assert!(s.changed_files().is_empty());
    let o = s.overview();
    assert_eq!(o.documents[0].covered, Some(6));
    let des8 = find(&s, 1, "DES_REQ8");
    assert_eq!(s.details(1, des8).unwrap().change, Some(Change::New));
    assert_eq!(o.documents[1].changed, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
