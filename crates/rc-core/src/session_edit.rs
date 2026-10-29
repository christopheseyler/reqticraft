//! Édition depuis l'interface : projet, types d'analyse, filtres, enregistrement.
//!
//! Les modifications sont appliquées en mémoire et l'analyse est relancée ;
//! [`Session::save`] les écrit au format `.rqtf` (avec `<projet>.types` et
//! `<projet>.filters`) ou au format natif `.rcproj`.

use crate::session::Session;
use crate::{capture_document, intermediate, DocumentEntry};
use rc_analysis::filter;
use rc_interop::{write, NativeProject, TypeLibrary};
use rc_model::{CaptureRule, DocumentType, ElementClass, FilterSet, Project};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    pub name: String,
    pub kind: rc_model::TypeKind,
    pub converter: String,
    /// `project`, `modified`, `configuration` ou `library`.
    pub origin: &'static str,
    pub elements: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub project: Project,
    pub path: String,
    /// `rqtf` ou `rcproj`.
    pub format: &'static str,
    pub dirty: bool,
    pub types: Vec<TypeInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRow {
    pub depth: usize,
    pub class: ElementClass,
    pub type_name: String,
    pub name: String,
    pub text: String,
    pub references: Vec<String>,
    pub attributes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub rows: Vec<TestRow>,
    /// Nombre d'éléments par type d'élément.
    pub counts: Vec<(String, usize)>,
    pub messages: Vec<String>,
    /// Début de l'intermédiaire (texte ou XML) produit par le convertisseur.
    pub intermediate: String,
    pub intermediate_lines: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiltersInfo {
    pub filters: FilterSet,
    /// Paramètres `$nom` de chaque filtre.
    pub parameters: HashMap<String, Vec<String>>,
    pub display: Option<String>,
    pub analysis: Option<String>,
    pub operators: Vec<(&'static str, &'static str, bool)>,
    pub receivers: Vec<(&'static str, &'static str)>,
    pub application_classes: Vec<(&'static str, &'static str)>,
}

/// Vérifie les expressions d'un type : chaque erreur nomme l'élément fautif.
pub fn check_type(t: &DocumentType) -> Result<(), String> {
    if t.name.trim().is_empty() {
        return Err("le type doit avoir un nom".into());
    }
    let mut errors = Vec::new();
    let mut check = |what: &str, re: &Option<String>| {
        if let Some(re) = re.as_deref().filter(|r| !r.is_empty()) {
            if let Err(e) = rc_capture::compile(re) {
                errors.push(format!("{what} : {e}"));
            }
        }
    };
    for e in &t.element_types {
        if let CaptureRule::Regex {
            expression,
            sub_expression,
            end_expression,
            row_expression,
        } = &e.rule
        {
            check(&e.name, &Some(expression.clone()));
            check(&format!("{} (sous-expression)", e.name), sub_expression);
            check(&format!("{} (fin)", e.name), end_expression);
            check(&format!("{} (lignes)", e.name), row_expression);
        }
    }
    for x in &t.exclude_text {
        check("exclusion de texte", &Some(x.clone()));
    }
    check("suppression", &t.delete_expression);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn check_project(p: &Project) -> Result<(), String> {
    let mut names = HashSet::new();
    for n in &p.nodes {
        if n.name().trim().is_empty() {
            return Err("chaque document doit avoir un nom".into());
        }
        if !names.insert(n.name().to_lowercase()) {
            return Err(format!("le nom « {} » est utilisé deux fois", n.name()));
        }
    }
    for c in &p.channels {
        if c.covered >= p.nodes.len() || c.covering.iter().any(|&k| k >= p.nodes.len()) {
            return Err("un canal désigne un document absent".into());
        }
        if c.covering.contains(&c.covered) {
            return Err(format!(
                "le document « {} » ne peut pas se couvrir lui-même",
                p.nodes[c.covered].name()
            ));
        }
    }
    Ok(())
}

/// Nouveau projet vide, écrit sur le disque.
pub fn create_project(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} existe déjà", path.display()));
    }
    let project = Project::default();
    let text = if rc_interop::is_native(path) {
        NativeProject::new(project, Vec::new(), FilterSet::default()).to_json()
    } else {
        write::project_xml(&project)
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{} : {e}", path.display()))
}

impl Session {
    fn type_origin(&self, t: &DocumentType) -> &'static str {
        match &t.source {
            None => "library",
            Some(s) if s.starts_with("<reqticraft>") => "library",
            Some(s) if s.parent() == Some(self.run.loaded.dir.as_path()) => "project",
            Some(_) => "configuration",
        }
    }

    pub fn type_infos(&self) -> Vec<TypeInfo> {
        let local: HashSet<&str> = self
            .edit
            .local_types
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        let mut out: Vec<TypeInfo> = self
            .edit
            .local_types
            .iter()
            .map(|t| (t, "modified"))
            .chain(
                self.edit
                    .base_types
                    .visible()
                    .into_iter()
                    .filter(|t| !local.contains(t.name.as_str()))
                    .map(|t| (t, self.type_origin(t))),
            )
            .map(|(t, origin)| TypeInfo {
                name: t.name.clone(),
                kind: t.kind,
                converter: t.converter.clone(),
                origin,
                elements: t.element_types.len(),
            })
            .collect();
        out.sort_by_key(|t| t.name.to_lowercase());
        out
    }

    pub fn project_info(&self) -> ProjectInfo {
        ProjectInfo {
            project: self.run.loaded.project.clone(),
            path: self.path.display().to_string(),
            format: if rc_interop::is_native(&self.path) {
                "rcproj"
            } else {
                "rqtf"
            },
            dirty: self.edit.dirty,
            types: self.type_infos(),
        }
    }

    /// Remplace la configuration du projet (documents, canaux) et relance l'analyse.
    pub fn update_project(&mut self, project: Project) -> Result<(), String> {
        check_project(&project)?;
        let mut loaded = self.run.loaded.clone();
        loaded.project = Project {
            name: loaded.project.name.clone(),
            ..project
        };
        let mut edit = self.edit.clone();
        edit.dirty = true;
        self.rerun_with(loaded, edit);
        Ok(())
    }

    pub fn document_type(&self, name: &str) -> Result<DocumentType, String> {
        self.edit
            .local_types
            .iter()
            .find(|t| t.name == name)
            .or_else(|| self.edit.base_types.get(name))
            .cloned()
            .ok_or_else(|| format!("type « {name} » inconnu"))
    }

    /// Enregistre un type dans le projet (création, modification ou renommage).
    pub fn save_type(&mut self, t: DocumentType, previous: Option<&str>) -> Result<(), String> {
        check_type(&t)?;
        let mut edit = self.edit.clone();
        edit.local_types
            .retain(|x| x.name != t.name && Some(x.name.as_str()) != previous);
        edit.local_types.insert(0, t);
        edit.dirty = true;
        let loaded = self.run.loaded.clone();
        self.rerun_with(loaded, edit);
        Ok(())
    }

    pub fn delete_type(&mut self, name: &str) -> Result<(), String> {
        let mut edit = self.edit.clone();
        let before = edit.local_types.len();
        edit.local_types.retain(|t| t.name != name);
        if edit.local_types.len() == before {
            return Err(format!("« {name} » n'est pas un type modifié du projet"));
        }
        edit.dirty = true;
        let loaded = self.run.loaded.clone();
        self.rerun_with(loaded, edit);
        Ok(())
    }

    /// Applique un type, enregistré ou non, à un fichier : éléments capturés,
    /// messages et début de l'intermédiaire.
    pub fn test_type(
        &self,
        t: &DocumentType,
        file: &Path,
        params: Vec<(String, String)>,
    ) -> Result<TestResult, String> {
        check_type(t)?;
        let mut types = TypeLibrary::default();
        types.extend([t.clone()]);
        let entry = DocumentEntry {
            name: file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "document".into()),
            decl: rc_model::DocumentDecl {
                name: String::new(),
                doc_type: t.name.clone(),
                path: file.to_owned(),
                params,
                ..Default::default()
            },
            base_dir: self.run.loaded.dir.clone(),
            types,
        };
        let inter = intermediate(&entry, t).map_err(|(_, e)| e)?;
        let text = inter.as_str();
        let doc = capture_document(&entry);
        let mut rows = Vec::new();
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut stack: Vec<(usize, usize)> = doc.roots.iter().rev().map(|&r| (r, 0)).collect();
        while let Some((e, depth)) = stack.pop() {
            let el = &doc.elements[e];
            stack.extend(el.children.iter().rev().map(|&c| (c, depth + 1)));
            match counts.iter_mut().find(|(n, _)| *n == el.type_name) {
                Some((_, c)) => *c += 1,
                None => counts.push((el.type_name.clone(), 1)),
            }
            if rows.len() >= 2000 {
                continue;
            }
            let mut short: String = el.text.trim().chars().take(200).collect();
            if el.text.trim().chars().count() > 200 {
                short.push('…');
            }
            rows.push(TestRow {
                depth,
                class: el.class,
                type_name: el.type_name.clone(),
                name: el.display_name(),
                text: short,
                references: el.references.iter().map(|r| r.target.clone()).collect(),
                attributes: el
                    .attributes
                    .iter()
                    .map(|a| match &a.value {
                        Some(v) => format!("{} = {v}", a.name),
                        None => a.name.clone(),
                    })
                    .collect(),
            });
        }
        Ok(TestResult {
            rows,
            counts,
            messages: doc.messages.iter().map(|m| m.text.clone()).collect(),
            intermediate: text.lines().take(400).collect::<Vec<_>>().join("\n"),
            intermediate_lines: text.lines().count(),
        })
    }

    pub fn filters_info(&self) -> FiltersInfo {
        let filters = self.run.loaded.filters.clone();
        FiltersInfo {
            parameters: filters
                .filters
                .iter()
                .map(|f| (f.name.clone(), filter::parameters(f)))
                .collect(),
            filters,
            display: self
                .edit
                .display_filter
                .as_ref()
                .map(|(f, _)| f.name.clone()),
            analysis: self
                .edit
                .analysis_filter
                .as_ref()
                .map(|(f, _)| f.name.clone()),
            operators: filter::OPERATORS.to_vec(),
            receivers: filter::RECEIVERS.to_vec(),
            application_classes: filter::APPLICATION_CLASSES.to_vec(),
        }
    }

    pub fn save_filters(&mut self, filters: FilterSet) -> Result<(), String> {
        let mut names = HashSet::new();
        for f in &filters.filters {
            if f.name.trim().is_empty() || !names.insert(f.name.to_lowercase()) {
                return Err(format!("nom de filtre vide ou en double : « {} »", f.name));
            }
        }
        self.run.loaded.filters = filters;
        self.edit.dirty = true;
        // Les filtres appliqués sont relus d'après leur nouvelle définition.
        let refresh = |slot: &Option<crate::AnalysisFilter>, set: &FilterSet| {
            slot.as_ref().and_then(|(f, p)| {
                set.filters
                    .iter()
                    .find(|x| x.name == f.name)
                    .map(|x| (x.clone(), p.clone()))
            })
        };
        self.edit.display_filter = refresh(&self.edit.display_filter, &self.run.loaded.filters);
        self.edit.analysis_filter = refresh(&self.edit.analysis_filter, &self.run.loaded.filters);
        self.refilter();
        Ok(())
    }

    /// Applique (ou retire, sans nom) un filtre à l'affichage ou à l'analyse.
    pub fn apply_filter(
        &mut self,
        mode: &str,
        name: Option<&str>,
        params: HashMap<String, String>,
    ) -> Result<(), String> {
        let chosen = match name {
            None => None,
            Some(n) => Some((
                self.run
                    .loaded
                    .filters
                    .filters
                    .iter()
                    .find(|f| f.name == n)
                    .cloned()
                    .ok_or_else(|| format!("filtre « {n} » inconnu"))?,
                params,
            )),
        };
        match mode {
            "display" => self.edit.display_filter = chosen,
            "analysis" => self.edit.analysis_filter = chosen,
            _ => return Err(format!("mode « {mode} » inconnu (display ou analysis)")),
        }
        self.refilter();
        Ok(())
    }

    /// Enregistre le projet. Sans chemin, au même endroit ; le format suit
    /// l'extension (`.rcproj` : natif, sinon `.rqtf` XML).
    pub fn save(&mut self, path: Option<PathBuf>) -> Result<String, String> {
        let path = path.unwrap_or_else(|| self.path.clone());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "projet".into());
        let write_file = |p: &Path, text: String| {
            std::fs::write(p, text).map_err(|e| format!("{} : {e}", p.display()))
        };
        let mut project = self.run.loaded.project.clone();
        let native = rc_interop::is_native(&path);
        // Types du projet : ceux modifiés, puis ceux lus dans le répertoire du projet.
        let local: HashSet<&str> = self
            .edit
            .local_types
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        let own_file = self.run.loaded.dir.join(format!("{stem}.types"));
        let mut project_types: Vec<DocumentType> = self.edit.local_types.clone();
        project_types.extend(
            self.edit
                .base_types
                .visible()
                .into_iter()
                .filter(|t| !local.contains(t.name.as_str()))
                .filter(|t| t.source.as_deref() == Some(own_file.as_path()))
                .cloned(),
        );
        for t in &mut project_types {
            t.source = None;
        }
        if native {
            project.name = String::new();
            let n = NativeProject::new(project, project_types, self.run.loaded.filters.clone());
            write_file(&path, n.to_json())?;
        } else {
            write_file(&path, write::project_xml(&project))?;
            if !project_types.is_empty() {
                write_file(
                    &dir.join(format!("{stem}.types")),
                    write::types_xml(&project_types),
                )?;
            }
            if !self.run.loaded.filters.filters.is_empty() {
                write_file(
                    &dir.join(format!("{stem}.filters")),
                    write::filters_xml(&self.run.loaded.filters),
                )?;
            }
        }
        self.path = path.clone();
        self.edit.dirty = false;
        self.refresh_stamp();
        Ok(path.display().to_string())
    }
}

/// État d'un élément dans un instantané, pour son historique.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub snapshot: Option<rc_snapshot::SnapshotInfo>,
    pub state: Option<rc_snapshot::ElementState>,
}

impl Session {
    fn store(&self) -> Result<rc_snapshot::Store, String> {
        crate::outputs::snapshots(&self.run)
    }

    pub fn snapshots(&self) -> Result<Vec<rc_snapshot::SnapshotInfo>, String> {
        self.store()?.list().map_err(|e| e.to_string())
    }

    pub fn create_snapshot(&self, name: &str) -> Result<rc_snapshot::SnapshotInfo, String> {
        self.store()?
            .create(name, &crate::outputs::current_state(&self.run))
            .map_err(|e| e.to_string())
    }

    pub fn delete_snapshot(&self, id: i64) -> Result<(), String> {
        self.store()?.delete(id).map_err(|e| e.to_string())
    }

    /// Compare un instantané à un autre, ou à l'état courant (`to` absent).
    pub fn compare(&self, from: i64, to: Option<i64>) -> Result<rc_snapshot::Diff, String> {
        let store = self.store()?;
        let old = store.load(from).map_err(|e| e.to_string())?;
        let new = match to {
            Some(t) => store.load(t).map_err(|e| e.to_string())?,
            None => crate::outputs::current_state(&self.run),
        };
        Ok(rc_snapshot::diff(&old, &new))
    }

    /// Historique d'un élément : chaque instantané, puis l'état courant.
    pub fn history(&self, d: usize, e: usize) -> Result<Vec<HistoryEntry>, String> {
        let doc = self
            .run
            .analysis
            .documents
            .get(d)
            .ok_or("document inconnu")?;
        let el = doc.elements.get(e).ok_or("élément inconnu")?;
        let current = crate::outputs::current_state(&self.run)
            .elements
            .into_iter()
            .find(|s| {
                s.document == doc.name
                    && s.type_name == el.type_name
                    && s.ident == el.ident
                    && s.guid == el.guid
            });
        let Some((document, _, key)) = current.as_ref().and_then(|c| c.key()) else {
            return Ok(Vec::new());
        };
        let mut out: Vec<HistoryEntry> = self
            .store()?
            .history(&document, &key)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(s, state)| HistoryEntry {
                snapshot: Some(s),
                state,
            })
            .collect();
        out.push(HistoryEntry {
            snapshot: None,
            state: current,
        });
        Ok(out)
    }

    /// Écrit un rapport et renvoie son chemin.
    pub fn report(&self, template: &str, format: &str, path: &Path) -> Result<String, String> {
        let template = rc_report::Template::parse(template)
            .ok_or_else(|| format!("modèle « {template} » inconnu"))?;
        let format = rc_report::Format::parse(format)
            .ok_or_else(|| format!("format « {format} » inconnu"))?;
        let path = if path.is_relative() {
            self.run.loaded.dir.join(path)
        } else {
            path.to_owned()
        };
        crate::outputs::write_report(&self.run, template, Some(format), &path)?;
        Ok(path.display().to_string())
    }
}
