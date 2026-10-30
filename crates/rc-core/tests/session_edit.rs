//! Édition depuis l'interface : parcours P1 (projet créé pas à pas), P2 (type
//! dupliqué puis modifié), filtres et enregistrement dans les deux formats.

use rc_core::rc_model::{
    CaptureRule, Channel, ChannelKind, DocumentDecl, Filter, FilterCondition, FilterSet,
    ProjectNode,
};
use rc_core::session::{call, ElementQuery, Session};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rc-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/thermostat/text");
    for f in ["specification.txt", "design.txt", "tests.txt"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    dir
}

fn doc(name: &str, path: &str, x: i64) -> ProjectNode {
    ProjectNode::Document(DocumentDecl {
        name: name.into(),
        doc_type: "Text".into(),
        path: PathBuf::from(path),
        position: Some((x, 100)),
        ..Default::default()
    })
}

#[test]
fn p1_project_built_step_by_step() {
    let dir = workspace("p1");
    let path = dir.join("tutoriel.rqtf");
    let mut session = None;
    call(&mut session, "newProject", &json!({ "path": path })).unwrap();
    let s = session.as_mut().unwrap();
    assert_eq!(s.overview().documents.len(), 0);

    // Deux documents et un canal Conception → Spécification.
    let mut project = s.project_info().project;
    project.nodes = vec![
        doc("Spécification", "specification.txt", 0),
        doc("Conception", "design.txt", 300),
    ];
    project.channels = vec![Channel {
        kind: ChannelKind::Cover,
        covered: 0,
        covering: vec![1],
        category: None,
        position: None,
    }];
    s.update_project(project.clone()).unwrap();
    let o = s.overview();
    assert_eq!((o.documents.len(), o.requirements), (2, 13));
    assert_eq!(o.documents[0].covered, Some(5));
    assert!(o.dirty);

    // Un canal vers un document absent est refusé.
    let mut bad = project.clone();
    bad.channels[0].covering = vec![7];
    assert!(s.update_project(bad).is_err());

    // Troisième document et canal Tests → Conception, puis enregistrement.
    project.nodes.push(doc("Tests", "tests.txt", 600));
    project.channels.push(Channel {
        kind: ChannelKind::Cover,
        covered: 1,
        covering: vec![2],
        category: None,
        position: None,
    });
    s.update_project(project).unwrap();
    s.save(None).unwrap();
    assert!(!s.overview().dirty);

    let reopened = Session::open(&path, &[]).unwrap().overview();
    assert_eq!(reopened.documents.len(), 3);
    assert_eq!((reopened.covered, reopened.coverable), (11, 13));

    // Format natif : même résultat à la relecture.
    let native = dir.join("tutoriel.rcproj");
    s.save(Some(native.clone())).unwrap();
    let n = Session::open(&native, &[]).unwrap().overview();
    assert_eq!((n.covered, n.coverable, n.requirements), (11, 13, 19));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folders_group_documents() {
    let dir = workspace("folders");
    let path = dir.join("groupes.rcproj");
    let mut session = None;
    call(&mut session, "newProject", &json!({ "path": path })).unwrap();
    let s = session.as_mut().unwrap();

    // « Réalisation » regroupe la conception et les tests ; les canaux restent
    // entre documents, à travers le groupe.
    let mut project = s.project_info().project;
    project.nodes = vec![
        doc("Spécification", "specification.txt", 0),
        doc("Conception", "design.txt", 300),
        doc("Tests", "tests.txt", 600),
        ProjectNode::Folder {
            name: "Réalisation".into(),
            default_type: None,
            children: vec![1, 2],
            position: Some((300, 300)),
            size: None,
            category: None,
        },
    ];
    project.channels = vec![
        Channel {
            kind: ChannelKind::Cover,
            covered: 0,
            covering: vec![1],
            category: None,
            position: None,
        },
        Channel {
            kind: ChannelKind::Cover,
            covered: 1,
            covering: vec![2],
            category: None,
            position: None,
        },
    ];
    s.update_project(project.clone()).unwrap();
    let o = s.overview();
    assert_eq!(o.documents.len(), 3);
    assert_eq!((o.covered, o.coverable), (11, 13));

    // Un canal depuis le dossier : Spécification ← Conception OU Tests.
    let mut via = project.clone();
    via.channels[0].covering = vec![3];
    s.update_project(via).unwrap();
    assert_eq!(s.overview().documents[0].covered, Some(5));

    // Refusés : un document dans deux dossiers, un cycle.
    let mut bad = project.clone();
    bad.nodes.push(ProjectNode::Folder {
        name: "Autre".into(),
        default_type: None,
        children: vec![2, 3],
        position: None,
        size: None,
        category: None,
    });
    assert!(s.update_project(bad.clone()).is_err());
    if let ProjectNode::Folder { children, .. } = &mut bad.nodes[3] {
        children.retain(|&c| c != 2);
        children.push(4);
    }
    if let ProjectNode::Folder { children, .. } = &mut bad.nodes[4] {
        *children = vec![3];
    }
    assert!(s.update_project(bad).is_err());

    // Les deux formats gardent le dossier.
    s.update_project(project).unwrap();
    let rqtf = dir.join("groupes.rqtf");
    s.save(Some(rqtf.clone())).unwrap();
    let r = Session::open(&rqtf, &[]).unwrap();
    let rp = r.project_info().project;
    let tests = rp.node_by_name("Tests").unwrap();
    assert_eq!(
        rp.nodes[rp.parent_folder(tests).unwrap()].name(),
        "Réalisation"
    );
    assert_eq!(r.overview().covered, 11);
    s.save(Some(path.clone())).unwrap();
    let reopened = Session::open(&path, &[]).unwrap();
    let p = reopened.project_info().project;
    assert_eq!(p.parent_folder(2), Some(3));
    assert!(matches!(
        &p.nodes[3],
        ProjectNode::Folder {
            position: Some((300, 300)),
            ..
        }
    ));
    assert_eq!(reopened.overview().covered, 11);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p2_type_duplicated_and_tested() {
    let dir = workspace("p2");
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/thermostat/text/thermostat.rqtf");
    std::fs::copy(src, dir.join("thermostat.rqtf")).unwrap();
    let mut s = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();

    // Dupliquer « Text » en un type qui ne reconnaît que les exigences de test.
    let mut t = s.document_type("Text").unwrap();
    t.name = "Text tests".into();
    let req = t
        .element_types
        .iter_mut()
        .find(|e| e.class == rc_core::rc_model::ElementClass::Requirement)
        .expect("type d'exigence");
    req.rule = CaptureRule::regex(r"^(TST_REQ\d+)\s*:\s*(.*)$");

    let result = s.test_type(&t, Path::new("tests.txt"), Vec::new()).unwrap();
    let reqs = result
        .rows
        .iter()
        .filter(|r| r.class == rc_core::rc_model::ElementClass::Requirement)
        .count();
    let other = s
        .test_type(&t, Path::new("design.txt"), Vec::new())
        .unwrap()
        .rows
        .iter()
        .filter(|r| r.class == rc_core::rc_model::ElementClass::Requirement)
        .count();
    assert!(reqs > 0 && other == 0, "{reqs} / {other}");
    assert!(result.intermediate_lines > 0);

    // Expression invalide : refusée avec le nom de l'élément.
    let mut broken = t.clone();
    broken.element_types[0].rule = CaptureRule::regex("(oups");
    let err = s.save_type(broken, None).unwrap_err();
    assert!(err.contains(&t.element_types[0].name), "{err}");

    // Enregistré dans le projet, le type est écrit dans thermostat.types.
    s.save_type(t, None).unwrap();
    assert!(s
        .type_infos()
        .iter()
        .any(|i| i.name == "Text tests" && i.origin == "modified"));
    s.save(None).unwrap();
    let again = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();
    assert!(again
        .type_infos()
        .iter()
        .any(|i| i.name == "Text tests" && i.origin == "project"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Exigences de la spécification de priorité donnée ; les autres documents
/// sont gardés en entier (combinaison OU).
fn priority_filter(value: &str) -> Filter {
    Filter {
        name: "Priorité".into(),
        all: false,
        conditions: vec![
            FilterCondition {
                application_class: Some("AbstractRequirement".into()),
                receiver_class: "Attribute".into(),
                receiver_document_type: None,
                receiver_element_type: Some("Priority".into()),
                operator: "equal".into(),
                value: Some(value.into()),
            },
            FilterCondition {
                application_class: Some("AbstractRequirement".into()),
                receiver_class: "Document".into(),
                receiver_document_type: None,
                receiver_element_type: None,
                operator: "notEqual".into(),
                value: Some("Spécification".into()),
            },
        ],
        raw_flags: Vec::new(),
    }
}

#[test]
fn display_and_analysis_filters() {
    let dir = workspace("filters");
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/thermostat/text/thermostat.rqtf");
    std::fs::copy(src, dir.join("thermostat.rqtf")).unwrap();
    let mut s = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();
    let requirements = |s: &Session| {
        s.elements(
            0,
            &ElementQuery {
                status: Some("requirements".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .total
    };
    assert_eq!(requirements(&s), 6);

    s.save_filters(FilterSet {
        filters: vec![priority_filter("$niveau")],
        current: None,
    })
    .unwrap();
    assert_eq!(s.filters_info().parameters["Priorité"], ["niveau"]);
    let params = HashMap::from([("niveau".to_owned(), "Low".to_owned())]);

    // Affichage : THS_REQ5 et THS_REQ6 (Low) seulement ; les taux ne changent pas.
    s.apply_filter("display", Some("Priorité"), params.clone())
        .unwrap();
    assert_eq!(requirements(&s), 2);
    assert_eq!(s.overview().documents[0].covered, Some(5));

    // Analyse : les exigences filtrées sont ignorées, la couverture est recalculée.
    s.apply_filter("display", None, HashMap::new()).unwrap();
    s.apply_filter("analysis", Some("Priorité"), params)
        .unwrap();
    let o = s.overview();
    assert_eq!(o.analysis_filter.as_deref(), Some("Priorité"));
    assert_eq!(
        (o.documents[0].requirements, o.documents[0].covered),
        (2, Some(1))
    );

    // Les filtres sont enregistrés avec le projet.
    s.save(None).unwrap();
    let again = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();
    assert_eq!(again.filters_info().filters.filters.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p4_snapshot_compare_and_report() {
    let dir = workspace("p4");
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/thermostat/text/thermostat.rqtf");
    std::fs::copy(src, dir.join("thermostat.rqtf")).unwrap();
    let mut s = Session::open(&dir.join("thermostat.rqtf"), &[]).unwrap();
    let v1 = s.create_snapshot("V1").unwrap();
    assert!(s.create_snapshot("V1").is_err());

    let spec = dir.join("specification.txt");
    let text = std::fs::read_to_string(&spec)
        .unwrap()
        .replace("toutes les 10 secondes", "toutes les 5 secondes");
    std::fs::write(&spec, text).unwrap();
    s.reload().unwrap();

    let d = s.compare(v1.id, None).unwrap();
    assert_eq!(d.modified.len(), 1);
    assert_eq!(d.modified[0].after.ident.as_deref(), Some("THS_REQ1"));
    assert!(d.added.is_empty() && d.removed.is_empty());

    let req1 = s.run.analysis.documents[0]
        .elements
        .iter()
        .position(|e| e.ident.as_deref() == Some("THS_REQ1"))
        .unwrap();
    let h = s.history(0, req1).unwrap();
    assert_eq!(h.len(), 2);
    assert!(h[0].state.as_ref().unwrap().text.contains("10 secondes"));
    assert!(h[1].snapshot.is_none() && h[1].state.as_ref().unwrap().text.contains("5 secondes"));

    // Matrice exportée en XLSX, relative au répertoire du projet.
    let out = s
        .report("matrix", "xlsx", Path::new("matrice.xlsx"))
        .unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
    let html = s
        .report("description", "html", Path::new("projet.html"))
        .unwrap();
    assert!(std::fs::read_to_string(html)
        .unwrap()
        .contains("Spécification"));
    assert!(s.report("inconnu", "html", Path::new("x.html")).is_err());

    s.delete_snapshot(v1.id).unwrap();
    assert!(s.snapshots().unwrap().is_empty());
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
