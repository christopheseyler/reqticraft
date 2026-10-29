//! Interopérabilité avec les fichiers Reqtify : `.rqtf` (XML et INI), `.types`,
//! `.filters`, `.marks` et documents de modification.
//!
//! Spécification : `docs/interop-formats.md`.

pub mod ini;
mod misc;
mod rqtf;
pub mod text;
mod types;

pub use misc::{parse_filters, parse_marks, parse_modifications};
pub use rqtf::parse_project;
pub use types::parse_types;

use rc_model::{DocumentType, FilterSet, Mark, Project};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum InteropError {
    #[error("lecture de {path} : {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("XML invalide : {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("{0}")]
    Format(String),
}

pub fn read_text(path: &Path) -> Result<String, InteropError> {
    std::fs::read(path)
        .map(|bytes| text::decode(&bytes))
        .map_err(|source| InteropError::Io {
            path: path.to_owned(),
            source,
        })
}

/// Ensemble ordonné de types : le premier type d'un nom donné l'emporte.
#[derive(Debug, Clone, Default)]
pub struct TypeLibrary {
    types: Vec<DocumentType>,
}

impl TypeLibrary {
    /// Ajoute des types de priorité inférieure à ceux déjà présents.
    pub fn extend(&mut self, types: impl IntoIterator<Item = DocumentType>) {
        self.types.extend(types);
    }

    pub fn get(&self, name: &str) -> Option<&DocumentType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Types visibles (un seul par nom), dans l'ordre de priorité.
    pub fn visible(&self) -> Vec<&DocumentType> {
        let mut seen = std::collections::HashSet::new();
        self.types
            .iter()
            .filter(|t| seen.insert(t.name.as_str()))
            .collect()
    }
}

/// Lit tous les `.types` d'un répertoire. Avec `recursive`, parcourt aussi les
/// sous-répertoires (organisation de `config\types` en dossiers).
pub fn load_types_dir(dir: &Path, recursive: bool) -> (Vec<DocumentType>, Vec<String>) {
    let mut types = Vec::new();
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    collect_files(dir, "types", recursive, &mut files);
    files.sort();
    for file in files {
        match read_text(&file).and_then(|t| parse_types(&t)) {
            Ok(mut found) => {
                for t in &mut found {
                    t.source = Some(file.clone());
                }
                types.extend(found);
            }
            Err(e) => warnings.push(format!("{} : {e}", file.display())),
        }
    }
    (types, warnings)
}

fn collect_files(dir: &Path, extension: &str, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_files(&path, extension, recursive, out);
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            out.push(path);
        }
    }
}

/// Projet chargé avec sa configuration.
#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub path: PathBuf,
    pub dir: PathBuf,
    pub project: Project,
    pub types: TypeLibrary,
    pub filters: FilterSet,
    pub marks: Vec<Mark>,
    pub warnings: Vec<String>,
}

/// Charge un projet `.rqtf` et sa configuration.
///
/// Ordre de priorité des types : répertoire du projet, puis répertoires de
/// configuration (`config_dirs`, sous-dossier `types` parcouru récursivement,
/// comme l'option `Config=` de l'outil d'origine), puis `builtin`.
pub fn load_project(
    path: &Path,
    config_dirs: &[PathBuf],
    builtin: &[DocumentType],
) -> Result<LoadedProject, InteropError> {
    let text = read_text(path)?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (project, mut warnings) = parse_project(&text, &name)?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();

    let mut types = TypeLibrary::default();
    let (local, w) = load_types_dir(&dir, false);
    types.extend(local);
    warnings.extend(w);
    for config in config_dirs {
        let types_dir = if config.join("types").is_dir() {
            config.join("types")
        } else {
            config.clone()
        };
        let (found, w) = load_types_dir(&types_dir, true);
        types.extend(found);
        warnings.extend(w);
    }
    types.extend(builtin.iter().cloned());

    // Tous les .filters du répertoire sont pris en compte ; seul celui du nom du
    // projet donne le filtre courant.
    let mut filters = FilterSet::default();
    let mut filter_files = Vec::new();
    collect_files(&dir, "filters", false, &mut filter_files);
    filter_files.sort();
    for file in filter_files {
        match read_text(&file).and_then(|t| parse_filters(&t)) {
            Ok(set) => {
                if file
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy() == name)
                {
                    filters.current = set.current;
                }
                filters.filters.extend(set.filters);
            }
            Err(e) => warnings.push(format!("{} : {e}", file.display())),
        }
    }

    let marks_file = dir.join(format!("{name}.marks"));
    let marks = if marks_file.is_file() {
        match read_text(&marks_file).and_then(|t| parse_marks(&t)) {
            Ok(m) => m,
            Err(e) => {
                warnings.push(format!("{} : {e}", marks_file.display()));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok(LoadedProject {
        path: path.to_owned(),
        dir,
        project,
        types,
        filters,
        marks,
        warnings,
    })
}
