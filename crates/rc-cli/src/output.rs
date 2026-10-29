//! Mise en forme des résultats pour le terminal et en JSON.

use rc_core::rc_analysis::{ratio, Target};
use rc_core::rc_interop::LoadedProject;
use rc_core::rc_model::{CapturedDocument, ElemId, Severity};
use rc_core::{FlatProject, ProjectRun};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;

fn percent(covered: usize, total: usize) -> String {
    format!("{:.0} % ({covered}/{total})", ratio(covered, total) * 100.0)
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "erreur",
        Severity::Warning => "avertissement",
        Severity::Information => "information",
    }
}

pub fn import_report(loaded: &LoadedProject, flat: &FlatProject) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Projet : {} ({})",
        loaded.project.name,
        loaded.path.display()
    );
    let _ = writeln!(out, "\nDocuments ({})", flat.documents.len());
    for entry in &flat.documents {
        if entry.decl.doc_type.is_empty() {
            let _ = writeln!(
                out,
                "  {:<28} remplacé par un document du projet parent",
                entry.name
            );
            continue;
        }
        let path = rc_core::document_path(entry);
        let status = match entry.types.get(&entry.decl.doc_type) {
            None => "type inconnu".to_owned(),
            Some(t) => {
                let source = t
                    .source
                    .as_ref()
                    .map(|s| s.display().to_string())
                    .unwrap_or_default();
                let converter =
                    if rc_core::rc_convert::ConverterId::for_tool(&t.converter).is_some() {
                        format!("convertisseur {}", t.converter)
                    } else {
                        format!("convertisseur {} NON DISPONIBLE", t.converter)
                    };
                format!("{converter}, type lu dans {source}")
            }
        };
        let exists = if path.exists() {
            ""
        } else {
            " — FICHIER ABSENT"
        };
        let _ = writeln!(
            out,
            "  {:<28} {:<20} {}{exists}",
            entry.name, entry.decl.doc_type, status
        );
    }
    let _ = writeln!(out, "\nCanaux ({})", flat.channels.len());
    for ch in &flat.channels {
        let covering: Vec<&str> = ch
            .covering
            .iter()
            .map(|&c| flat.documents[c].name.as_str())
            .collect();
        let _ = writeln!(
            out,
            "  {} ← {}",
            flat.documents[ch.covered].name,
            covering.join(" OU ")
        );
    }
    let _ = writeln!(
        out,
        "\nFiltres : {} ; marques : {} ; propriétés : {}",
        loaded.filters.filters.len(),
        loaded.marks.len(),
        loaded.project.properties.len()
    );
    let notes: Vec<String> = loaded
        .warnings
        .iter()
        .cloned()
        .chain(flat.messages.iter().map(|m| m.text.clone()))
        .collect();
    if !notes.is_empty() {
        let _ = writeln!(out, "\nRemarques");
        for n in notes {
            let _ = writeln!(out, "  - {n}");
        }
    }
    out
}

pub fn analysis_report(run: &ProjectRun, details: bool) -> String {
    let a = &run.analysis;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Projet : {} ({} documents)",
        run.loaded.project.name,
        a.documents.len()
    );
    let _ = writeln!(out, "\nDocuments");
    for (d, doc) in a.documents.iter().enumerate() {
        let s = &a.summaries[d];
        let mut line = format!(
            "  {:<28} {:<10} {:>4} exigences",
            doc.name, doc.type_name, s.requirements
        );
        if let Some(c) = s.covered {
            let _ = write!(line, "   couverture {}", percent(c, s.requirements));
        }
        if s.derived > 0 {
            let _ = write!(line, "   {} dérivée(s)", s.derived);
        }
        let _ = writeln!(out, "{line}");
    }
    if !a.channels.is_empty() {
        let _ = writeln!(out, "\nCanaux");
        for ch in &a.channels {
            // Détail par document couvrant seulement pour une combinaison « OU ».
            let covering: Vec<String> = if ch.per_document.len() == 1 {
                vec![a.documents[ch.per_document[0].0].name.clone()]
            } else {
                ch.per_document
                    .iter()
                    .map(|&(k, n)| format!("{} {}", a.documents[k].name, percent(n, ch.total)))
                    .collect()
            };
            let _ = writeln!(
                out,
                "  {} ← {} : {}",
                a.documents[ch.covered].name,
                covering.join(" OU "),
                percent(ch.covered_count, ch.total)
            );
        }
    }
    let _ = writeln!(
        out,
        "\nRègles : {} erreur(s), {} avertissement(s)",
        a.count(Severity::Error),
        a.count(Severity::Warning)
    );
    if details {
        for m in &a.messages {
            let place = match (&m.document, m.line) {
                (Some(d), Some(l)) => format!("{d}:{l}"),
                (Some(d), None) => d.clone(),
                _ => "projet".into(),
            };
            let _ = writeln!(
                out,
                "  [{}] {:<32} {:<32} {}",
                severity(m.severity),
                m.rule,
                place,
                m.text
            );
        }
    } else {
        let mut by_rule: BTreeMap<(Severity, &str), usize> = BTreeMap::new();
        for m in &a.messages {
            *by_rule.entry((m.severity, m.rule.as_str())).or_default() += 1;
        }
        for ((s, rule), n) in by_rule {
            let _ = writeln!(out, "  [{}] {rule} : {n}", severity(s));
        }
    }
    out
}

pub fn element_tree(doc: &CapturedDocument) -> String {
    fn walk(doc: &CapturedDocument, id: ElemId, depth: usize, out: &mut String) {
        let e = &doc.elements[id];
        let indent = "  ".repeat(depth);
        let _ = writeln!(
            out,
            "{indent}{} [{}] (ligne {})",
            e.display_name(),
            e.type_name,
            e.line
        );
        for a in &e.attributes {
            let _ = writeln!(
                out,
                "{indent}  @{} = {}",
                a.name,
                a.value.as_deref().unwrap_or("(présent)")
            );
        }
        for r in &e.references {
            let _ = writeln!(out, "{indent}  → {} ({})", r.target, r.type_name);
        }
        if !e.text.is_empty() {
            let first = e.text.lines().next().unwrap_or_default();
            let _ = writeln!(
                out,
                "{indent}  « {} »",
                first.chars().take(80).collect::<String>()
            );
        }
        for &c in &e.children {
            walk(doc, c, depth + 1, out);
        }
    }
    let mut out = String::new();
    for r in &doc.references {
        let _ = writeln!(out, "→ {} ({}) [document]", r.target, r.type_name);
    }
    for &root in &doc.roots {
        walk(doc, root, 0, &mut out);
    }
    for m in &doc.messages {
        let _ = writeln!(out, "[{}] {} : {}", severity(m.severity), m.rule, m.text);
    }
    out
}

#[derive(Serialize)]
pub struct JsonReport<'a> {
    project: &'a str,
    documents: Vec<JsonDocument<'a>>,
    channels: Vec<JsonChannel>,
    links: Vec<JsonLink>,
    messages: &'a [rc_core::rc_model::Message],
}

#[derive(Serialize)]
struct JsonDocument<'a> {
    name: &'a str,
    type_name: &'a str,
    requirements: usize,
    covered: Option<usize>,
    derived: Vec<String>,
    uncovered: Vec<String>,
    elements: &'a [rc_core::rc_model::Element],
}

#[derive(Serialize)]
struct JsonChannel {
    covered: String,
    covering: Vec<String>,
    covered_count: usize,
    total: usize,
}

#[derive(Serialize)]
struct JsonLink {
    from_document: String,
    from: String,
    to_document: String,
    to: String,
    reference_type: String,
}

pub fn json_report(run: &ProjectRun) -> JsonReport<'_> {
    let a = &run.analysis;
    let name_of = |t: Target| a.element_name(t);
    JsonReport {
        project: &run.loaded.project.name,
        documents: a
            .documents
            .iter()
            .enumerate()
            .map(|(d, doc)| JsonDocument {
                name: &doc.name,
                type_name: &doc.type_name,
                requirements: a.summaries[d].requirements,
                covered: a.summaries[d].covered,
                derived: a
                    .derived
                    .iter()
                    .filter(|(x, _)| *x == d)
                    .map(|&(_, e)| doc.elements[e].display_name())
                    .collect(),
                uncovered: a
                    .uncovered
                    .iter()
                    .filter(|(x, _)| *x == d)
                    .map(|&(_, e)| doc.elements[e].display_name())
                    .collect(),
                elements: &doc.elements,
            })
            .collect(),
        channels: a
            .channels
            .iter()
            .map(|c| JsonChannel {
                covered: a.documents[c.covered].name.clone(),
                covering: c
                    .covering
                    .iter()
                    .map(|&k| a.documents[k].name.clone())
                    .collect(),
                covered_count: c.covered_count,
                total: c.total,
            })
            .collect(),
        links: a
            .links
            .iter()
            .map(|l| JsonLink {
                from_document: a.documents[l.from.document].name.clone(),
                from: name_of(l.from),
                to_document: a.documents[l.to.document].name.clone(),
                to: name_of(l.to),
                reference_type: l.reference_type.clone(),
            })
            .collect(),
        messages: &a.messages,
    }
}

/// Comparaison de deux états (instantané → instantané ou état courant).
pub fn snapshot_diff(from: &str, to: &str, d: &rc_core::outputs::rc_snapshot::Diff) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Comparaison : {from} → {to}");
    let _ = writeln!(
        out,
        "Couverture : {} → {}",
        percent(d.before.0, d.before.1),
        percent(d.after.0, d.after.1)
    );
    let list = |out: &mut String, title: &str, items: Vec<String>| {
        if items.is_empty() {
            return;
        }
        let _ = writeln!(out, "\n{title} ({})", items.len());
        for i in items {
            let _ = writeln!(out, "  {i}");
        }
    };
    let name =
        |e: &rc_core::outputs::rc_snapshot::ElementState| format!("{} › {}", e.document, e.name());
    list(&mut out, "Ajoutées", d.added.iter().map(name).collect());
    list(&mut out, "Supprimées", d.removed.iter().map(name).collect());
    list(
        &mut out,
        "Modifiées",
        d.modified
            .iter()
            .map(|m| format!("{}  [{}]", name(&m.after), m.fields.join(", ")))
            .collect(),
    );
    list(
        &mut out,
        "Couvertures perdues",
        d.coverage_lost.iter().map(name).collect(),
    );
    list(
        &mut out,
        "Couvertures gagnées",
        d.coverage_gained.iter().map(name).collect(),
    );
    if d.is_empty() {
        let _ = writeln!(out, "\nAucune différence.");
    }
    out
}
