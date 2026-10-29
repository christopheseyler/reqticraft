//! Rapports et instantanés d'un projet analysé : point d'entrée commun à la
//! ligne de commande et à l'interface.

use crate::{document_path, ProjectRun};
use rc_report::{DocumentInfo, Format, Input, Template};
use rc_snapshot::{State, Store};
use std::path::Path;

pub use rc_report;
pub use rc_snapshot;

/// Nom du projet affiché dans les rapports.
pub fn project_name(run: &ProjectRun) -> String {
    run.loaded
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Rend un rapport en mémoire.
pub fn report_bytes(
    run: &ProjectRun,
    template: Template,
    format: Format,
) -> Result<Vec<u8>, String> {
    let documents = run
        .flat
        .documents
        .iter()
        .map(|d| DocumentInfo {
            name: d.name.clone(),
            type_name: d.decl.doc_type.clone(),
            path: document_path(d).display().to_string(),
            category: d.decl.category.clone(),
        })
        .collect();
    let name = project_name(run);
    let path = run.loaded.path.display().to_string();
    let generated = rc_snapshot::now_iso();
    let input = Input {
        project: &name,
        path: &path,
        documents,
        analysis: &run.analysis,
        generated: &generated,
    };
    rc_report::render(&input, template, format).map_err(|e| e.to_string())
}

/// Écrit un rapport ; le format suit l'extension du fichier, à défaut `format`.
pub fn write_report(
    run: &ProjectRun,
    template: Template,
    format: Option<Format>,
    out: &Path,
) -> Result<(), String> {
    let format = format
        .or_else(|| {
            out.extension()
                .and_then(|e| Format::parse(&e.to_string_lossy()))
        })
        .ok_or_else(|| {
            format!(
                "format du rapport inconnu pour {} (html, xlsx, docx, csv ou md)",
                out.display()
            )
        })?;
    let bytes = report_bytes(run, template, format)?;
    std::fs::write(out, bytes).map_err(|e| format!("{} : {e}", out.display()))
}

/// Base d'instantanés du projet (`<projet>.rcsnap`).
pub fn snapshots(run: &ProjectRun) -> Result<Store, String> {
    Store::open(&rc_snapshot::store_path(&run.loaded.path)).map_err(|e| e.to_string())
}

/// État courant, comparable à un instantané.
pub fn current_state(run: &ProjectRun) -> State {
    State::from_analysis(&run.analysis)
}
