//! Lecture du corpus livré avec Reqtify, en local uniquement (voir CONTRIBUTING.md).
//!
//! `REQTICRAFT_REQTIFY_EXAMPLES` : répertoire `Examples` des documents publics de Reqtify.
//! `REQTICRAFT_REQTIFY_CONFIG` : répertoire `config` de l'installation.
//! Sans ces variables, les tests ne vérifient rien.

use rc_interop::{
    parse_filters, parse_marks, parse_modifications, parse_project, parse_types, read_text,
};
use std::path::{Path, PathBuf};

fn files(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

#[test]
fn every_example_project_and_configuration_file_parses() {
    let Some(examples) = env_dir("REQTICRAFT_REQTIFY_EXAMPLES") else {
        eprintln!("REQTICRAFT_REQTIFY_EXAMPLES absent : test ignoré");
        return;
    };
    let mut failures = Vec::new();
    let projects = files(&examples, "rqtf");
    for path in &projects {
        let text = read_text(path).unwrap();
        match parse_project(&text, "x") {
            Ok((p, _)) => assert!(!p.nodes.is_empty(), "{} : projet vide", path.display()),
            Err(e) => failures.push(format!("{} : {e}", path.display())),
        }
    }
    for path in files(&examples, "types") {
        if let Err(e) = parse_types(&read_text(&path).unwrap()) {
            failures.push(format!("{} : {e}", path.display()));
        }
    }
    for path in files(&examples, "filters") {
        if let Err(e) = parse_filters(&read_text(&path).unwrap()) {
            failures.push(format!("{} : {e}", path.display()));
        }
    }
    for path in files(&examples, "marks") {
        if let Err(e) = parse_marks(&read_text(&path).unwrap()) {
            failures.push(format!("{} : {e}", path.display()));
        }
    }
    for path in files(&examples, "xml") {
        let text = read_text(&path).unwrap();
        if text.contains("<ModificationDocument") {
            if let Err(e) = parse_modifications(&text) {
                failures.push(format!("{} : {e}", path.display()));
            }
        }
    }
    assert!(projects.len() >= 50, "{} projets trouvés", projects.len());
    assert!(failures.is_empty(), "échecs :\n{}", failures.join("\n"));
}

#[test]
fn every_library_type_parses() {
    let Some(config) = env_dir("REQTICRAFT_REQTIFY_CONFIG") else {
        eprintln!("REQTICRAFT_REQTIFY_CONFIG absent : test ignoré");
        return;
    };
    let mut count = 0;
    let mut failures = Vec::new();
    for path in files(&config.join("types"), "types") {
        match parse_types(&read_text(&path).unwrap()) {
            Ok(types) => count += types.len(),
            Err(e) => failures.push(format!("{} : {e}", path.display())),
        }
    }
    assert!(failures.is_empty(), "échecs :\n{}", failures.join("\n"));
    assert!(count > 60, "{count} types lus");
}
