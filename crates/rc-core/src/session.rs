//! Session de travail de l'interface : projet ouvert, résultats d'analyse et
//! requêtes paginées.
//!
//! L'interface ne reçoit jamais tout le modèle : chaque requête renvoie une vue
//! sérialisable (vue d'ensemble, page d'éléments, détails, impact, problèmes).
//! [`call`] expose les mêmes requêtes sous forme `méthode + arguments JSON`, pour
//! l'application de bureau comme pour le serveur local.

use crate::{
    analyze_captured, document_path, load_project, run_loaded, AnalysisFilter, ProjectRun,
};
use rc_analysis::Target;
use rc_interop::{LoadedProject, TypeLibrary};
use rc_model::DocumentType;
use rc_model::{CapturedDocument, ElemId, Element, ElementClass, Severity};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// État d'un élément par rapport à la couverture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Covered,
    Uncovered,
    Derived,
    /// Hors couverture : section, entité, document non couvert.
    None,
}

/// Évolution d'un élément depuis le chargement précédent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    New,
    Modified,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub index: usize,
    pub name: String,
    pub type_name: String,
    pub path: String,
    pub requirements: usize,
    pub covered: Option<usize>,
    pub ratio: Option<f64>,
    pub derived: usize,
    pub errors: usize,
    pub warnings: usize,
    pub changed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub covered: usize,
    pub covering: Vec<usize>,
    pub covered_count: usize,
    pub total: usize,
    pub ratio: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub name: String,
    pub path: String,
    pub documents: Vec<DocumentInfo>,
    pub channels: Vec<ChannelInfo>,
    pub requirements: usize,
    pub covered: usize,
    pub coverable: usize,
    pub ratio: Option<f64>,
    pub derived: usize,
    pub errors: usize,
    pub warnings: usize,
    pub informations: usize,
    /// Modifications non enregistrées.
    pub dirty: bool,
    pub display_filter: Option<String>,
    pub analysis_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementRef {
    pub document: usize,
    pub element: Option<ElemId>,
    pub name: String,
    pub document_name: String,
    pub status: Status,
    /// Type et texte de l'élément lié, donnés seulement dans les liens du panneau Détails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: ElemId,
    pub name: String,
    pub ident: Option<String>,
    pub label: Option<String>,
    pub type_name: String,
    pub class: ElementClass,
    pub depth: usize,
    pub status: Status,
    pub change: Option<Change>,
    pub problems: usize,
}

#[derive(Debug, Serialize)]
pub struct Page {
    pub total: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub rule: String,
    pub severity: Severity,
    pub text: String,
    pub document: Option<usize>,
    pub document_name: Option<String>,
    pub element: Option<ElemId>,
    pub element_name: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub document: usize,
    pub document_name: String,
    pub id: ElemId,
    pub name: String,
    pub ident: Option<String>,
    pub label: Option<String>,
    pub type_name: String,
    pub class: ElementClass,
    pub text: String,
    pub line: usize,
    pub status: Status,
    pub change: Option<Change>,
    pub added: bool,
    pub path: Vec<ElementRef>,
    pub attributes: Vec<(String, Option<String>)>,
    pub references: Vec<String>,
    pub covers: Vec<ElementRef>,
    pub covered_by: Vec<ElementRef>,
    pub problems: Vec<Problem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactNode {
    #[serde(flatten)]
    pub target: ElementRef,
    /// Élément déjà présent plus haut dans la branche : la branche s'arrête.
    pub cycle: bool,
    pub children: Vec<ImpactNode>,
}

#[derive(Debug, Serialize)]
pub struct Impact {
    pub root: ElementRef,
    /// Exigences couvertes par l'élément, de proche en proche.
    pub upstream: Vec<ImpactNode>,
    /// Éléments qui couvrent l'élément, de proche en proche.
    pub downstream: Vec<ImpactNode>,
}

/// Empreinte d'un élément, pour repérer les modifications entre deux chargements.
type Fingerprints = HashMap<(String, String, String), String>;

/// État d'édition, conservé d'une analyse à l'autre.
#[derive(Clone, Default)]
pub struct Edit {
    /// Types du projet modifiés ou créés dans l'éditeur de types, prioritaires.
    pub local_types: Vec<DocumentType>,
    /// Types chargés avec le projet (projet, configuration, bibliothèque).
    pub base_types: TypeLibrary,
    pub display_filter: Option<AnalysisFilter>,
    pub analysis_filter: Option<AnalysisFilter>,
    /// Modifications non enregistrées.
    pub dirty: bool,
}

pub struct Session {
    pub path: PathBuf,
    pub config_dirs: Vec<PathBuf>,
    pub run: ProjectRun,
    pub edit: Edit,
    stamps: Vec<(PathBuf, Option<SystemTime>)>,
    changes: Vec<HashMap<ElemId, Change>>,
    up: HashMap<Target, Vec<Target>>,
    down: HashMap<Target, Vec<Target>>,
    leaves: Vec<HashSet<ElemId>>,
    /// Éléments masqués par le filtre d'affichage.
    hidden: HashSet<(usize, ElemId)>,
    /// Scripts Python du projet et résultat de leur dernière exécution.
    pub scripts: rc_script::Report,
}

impl Session {
    pub fn open(path: &Path, config_dirs: &[PathBuf]) -> Result<Session, String> {
        let loaded = load_project(path, config_dirs).map_err(|e| e.to_string())?;
        let edit = Edit {
            base_types: loaded.types.clone(),
            ..Default::default()
        };
        let run = run_loaded(loaded, config_dirs, None);
        Ok(Session::from_run(path, config_dirs, run, edit, None))
    }

    fn from_run(
        path: &Path,
        config_dirs: &[PathBuf],
        mut run: ProjectRun,
        edit: Edit,
        previous: Option<&Fingerprints>,
    ) -> Session {
        let scripts = crate::apply_scripts(&mut run, false);
        let mut stamps = vec![(path.to_owned(), stamp(path))];
        for entry in &run.flat.documents {
            let p = document_path(entry);
            if p.exists() {
                let s = stamp(&p);
                stamps.push((p, s));
            }
        }
        let changes = run
            .analysis
            .documents
            .iter()
            .map(|doc| match previous {
                None => HashMap::new(),
                Some(prev) => doc
                    .elements
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.class.is_structural())
                    .filter_map(|(i, e)| {
                        let key = key(doc, e)?;
                        match prev.get(&key) {
                            None => Some((i, Change::New)),
                            Some(f) if *f != fingerprint(doc, e) => Some((i, Change::Modified)),
                            Some(_) => None,
                        }
                    })
                    .collect(),
            })
            .collect();
        let mut up: HashMap<Target, Vec<Target>> = HashMap::new();
        let mut down: HashMap<Target, Vec<Target>> = HashMap::new();
        for l in &run.analysis.links {
            push_unique(up.entry(l.from).or_default(), l.to);
            push_unique(down.entry(l.to).or_default(), l.from);
        }
        let leaves = run
            .analysis
            .documents
            .iter()
            .map(|d| rc_analysis::leaf_requirements(d).into_iter().collect())
            .collect();
        let mut session = Session {
            path: path.to_owned(),
            config_dirs: config_dirs.to_vec(),
            run,
            edit,
            stamps,
            changes,
            up,
            down,
            leaves,
            hidden: HashSet::new(),
            scripts,
        };
        session.compute_hidden();
        session
    }

    /// Relance l'analyse ; les éléments nouveaux ou modifiés sont signalés.
    ///
    /// Sans modification en cours, le projet est relu sur le disque ; sinon
    /// seuls les documents sont recapturés.
    pub fn reload(&mut self) -> Result<(), String> {
        let previous = self.fingerprints();
        let mut edit = self.edit.clone();
        let loaded = if edit.dirty {
            self.run.loaded.clone()
        } else {
            let l = load_project(&self.path, &self.config_dirs).map_err(|e| e.to_string())?;
            edit.base_types = l.types.clone();
            l
        };
        self.rerun(loaded, edit, Some(&previous));
        Ok(())
    }

    /// Analyse de nouveau un projet (modifié ou non) avec l'état d'édition donné.
    fn rerun(&mut self, mut loaded: LoadedProject, edit: Edit, previous: Option<&Fingerprints>) {
        let mut types = TypeLibrary::default();
        types.extend(edit.local_types.iter().cloned());
        types.extend(edit.base_types.visible().into_iter().cloned());
        loaded.types = types;
        let run = run_loaded(loaded, &self.config_dirs, edit.analysis_filter.as_ref());
        let path = self.path.clone();
        *self = Session::from_run(&path, &self.config_dirs, run, edit, previous);
    }

    pub(crate) fn rerun_with(&mut self, loaded: LoadedProject, edit: Edit) {
        self.rerun(loaded, edit, None);
    }

    /// Date de modification du fichier de projet, après un enregistrement.
    pub(crate) fn refresh_stamp(&mut self) {
        let p = self.path.clone();
        let s = stamp(&p);
        self.stamps[0] = (p, s);
    }

    /// Analyse de nouveau sans recapturer (changement de filtre).
    pub(crate) fn refilter(&mut self) {
        let analysis = analyze_captured(
            &self.run.loaded,
            &self.run.flat,
            &self.run.captured,
            self.edit.analysis_filter.as_ref(),
        );
        let run = ProjectRun {
            loaded: self.run.loaded.clone(),
            flat: self.run.flat.clone(),
            captured: std::mem::take(&mut self.run.captured),
            analysis,
        };
        let changes = std::mem::take(&mut self.changes);
        let path = self.path.clone();
        *self = Session::from_run(&path, &self.config_dirs, run, self.edit.clone(), None);
        self.changes = changes;
    }

    fn compute_hidden(&mut self) {
        self.hidden.clear();
        let Some((filter, params)) = &self.edit.display_filter else {
            return;
        };
        let a = &self.run.analysis;
        let ctx = rc_analysis::filter::Context {
            analysis: Some(a),
            params,
        };
        for (d, doc) in a.documents.iter().enumerate() {
            for e in 0..doc.elements.len() {
                if doc.elements[e].class.is_structural()
                    && !rc_analysis::filter::passes(filter, d, doc, e, &ctx)
                {
                    self.hidden.insert((d, e));
                }
            }
        }
        // Un élément hors de la classe filtrée (section…) est masqué quand tous
        // ses descendants structurels le sont.
        for (d, doc) in a.documents.iter().enumerate() {
            for e in (0..doc.elements.len()).rev() {
                let el = &doc.elements[e];
                if !el.class.is_structural()
                    || self.hidden.contains(&(d, e))
                    || rc_analysis::filter::applies_to(filter, el.class)
                {
                    continue;
                }
                let structural: Vec<ElemId> = doc
                    .descendants(e)
                    .into_iter()
                    .filter(|&k| doc.elements[k].class.is_structural())
                    .collect();
                if !structural.is_empty()
                    && structural.iter().all(|k| self.hidden.contains(&(d, *k)))
                {
                    self.hidden.insert((d, e));
                }
            }
        }
    }

    fn fingerprints(&self) -> Fingerprints {
        let mut out = HashMap::new();
        for doc in &self.run.analysis.documents {
            for e in &doc.elements {
                if let Some(k) = key(doc, e) {
                    out.insert(k, fingerprint(doc, e));
                }
            }
        }
        out
    }

    /// Fichiers modifiés sur disque depuis le dernier chargement.
    pub fn changed_files(&self) -> Vec<String> {
        self.stamps
            .iter()
            .filter(|(p, s)| stamp(p) != *s)
            .map(|(p, _)| p.display().to_string())
            .collect()
    }

    fn doc(&self, d: usize) -> Result<&CapturedDocument, String> {
        self.run
            .analysis
            .documents
            .get(d)
            .ok_or_else(|| format!("document {d} inconnu"))
    }

    pub fn status(&self, d: usize, e: ElemId) -> Status {
        let a = &self.run.analysis;
        if a.derived.contains(&(d, e)) {
            Status::Derived
        } else if a.uncovered.contains(&(d, e)) {
            Status::Uncovered
        } else if self.leaves[d].contains(&e) && a.summaries[d].covered.is_some() {
            Status::Covered
        } else {
            Status::None
        }
    }

    fn reference(&self, t: Target) -> ElementRef {
        let doc = &self.run.analysis.documents[t.document];
        ElementRef {
            document: t.document,
            element: t.element,
            name: self.run.analysis.element_name(t),
            document_name: doc.name.clone(),
            status: t
                .element
                .map_or(Status::None, |e| self.status(t.document, e)),
            type_name: None,
            text: None,
        }
    }

    /// Référence enrichie du type et du texte, pour naviguer d'un lien à l'autre.
    fn link(&self, t: Target) -> ElementRef {
        let mut r = self.reference(t);
        if let Some(e) = t.element {
            let doc = &self.run.analysis.documents[t.document];
            if let Some(el) = doc.elements.get(e) {
                r.type_name = Some(el.type_name.clone());
                r.text = Some(element_text(doc, e));
            }
        }
        r
    }

    fn document_index(&self, name: &str) -> Option<usize> {
        self.run
            .analysis
            .documents
            .iter()
            .position(|d| d.name == name)
    }

    pub fn overview(&self) -> Overview {
        let a = &self.run.analysis;
        let documents: Vec<DocumentInfo> = a
            .documents
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                let s = &a.summaries[i];
                DocumentInfo {
                    index: i,
                    name: doc.name.clone(),
                    type_name: doc.type_name.clone(),
                    path: document_path(&self.run.flat.documents[i])
                        .display()
                        .to_string(),
                    requirements: s.requirements,
                    covered: s.covered,
                    ratio: s.covered.map(|c| rc_analysis::ratio(c, s.requirements)),
                    derived: s.derived,
                    errors: s.errors,
                    warnings: s.warnings,
                    changed: self.changes[i].len(),
                }
            })
            .collect();
        let (covered, coverable) = documents
            .iter()
            .filter_map(|d| d.covered.map(|c| (c, d.requirements)))
            .fold((0, 0), |(a, b), (c, t)| (a + c, b + t));
        Overview {
            name: self.run.loaded.path.file_stem().map_or_else(
                || self.path.display().to_string(),
                |s| s.to_string_lossy().into_owned(),
            ),
            path: self.path.display().to_string(),
            requirements: documents.iter().map(|d| d.requirements).sum(),
            covered,
            coverable,
            ratio: (coverable > 0).then(|| rc_analysis::ratio(covered, coverable)),
            derived: a.derived.len(),
            errors: a.count(Severity::Error),
            warnings: a.count(Severity::Warning),
            informations: a.count(Severity::Information),
            dirty: self.edit.dirty,
            display_filter: self
                .edit
                .display_filter
                .as_ref()
                .map(|(f, _)| f.name.clone()),
            analysis_filter: self
                .edit
                .analysis_filter
                .as_ref()
                .map(|(f, _)| f.name.clone()),
            channels: a
                .channels
                .iter()
                .map(|c| ChannelInfo {
                    covered: c.covered,
                    covering: c.covering.clone(),
                    covered_count: c.covered_count,
                    total: c.total,
                    ratio: c.ratio(),
                })
                .collect(),
            documents,
        }
    }

    /// Éléments structurels d'un document, dans l'ordre du document.
    pub fn elements(&self, d: usize, query: &ElementQuery) -> Result<Page, String> {
        let doc = self.doc(d)?;
        let needle = query.text.as_deref().unwrap_or("").trim().to_lowercase();
        let problems = self.problem_counts(d);
        let mut rows = Vec::new();
        let mut stack: Vec<(ElemId, usize)> = doc.roots.iter().rev().map(|&r| (r, 0)).collect();
        while let Some((e, depth)) = stack.pop() {
            let el = &doc.elements[e];
            stack.extend(el.children.iter().rev().map(|&c| (c, depth + 1)));
            if !el.class.is_structural() || self.hidden.contains(&(d, e)) {
                continue;
            }
            let status = self.status(d, e);
            let change = self.changes[d].get(&e).copied();
            let keep = match query.status.as_deref() {
                None | Some("all") => true,
                Some("requirements") => el.class.is_requirement(),
                Some("changed") => change.is_some(),
                Some("problems") => problems.contains_key(&e),
                Some(s) => {
                    serde_json::to_value(status)
                        .ok()
                        .and_then(|v| v.as_str().map(|v| v == s))
                        == Some(true)
                }
            } && (needle.is_empty() || matches(el, &needle));
            if keep {
                rows.push(Row {
                    id: e,
                    name: el.display_name(),
                    ident: el.ident.clone(),
                    label: el.label.clone(),
                    type_name: el.type_name.clone(),
                    class: el.class,
                    depth,
                    status,
                    change,
                    problems: problems.get(&e).copied().unwrap_or(0),
                });
            }
        }
        let total = rows.len();
        let rows = rows
            .into_iter()
            .skip(query.offset)
            .take(query.limit.unwrap_or(200))
            .collect();
        Ok(Page { total, rows })
    }

    fn problem_counts(&self, d: usize) -> HashMap<ElemId, usize> {
        let name = &self.run.analysis.documents[d].name;
        let mut out = HashMap::new();
        for m in &self.run.analysis.messages {
            if m.document.as_ref() == Some(name) {
                if let Some(e) = m.element {
                    *out.entry(e).or_insert(0) += 1;
                }
            }
        }
        out
    }

    pub fn details(&self, d: usize, e: ElemId) -> Result<Details, String> {
        let doc = self.doc(d)?;
        let el = doc
            .elements
            .get(e)
            .ok_or_else(|| format!("élément {e} inconnu"))?;
        let t = Target {
            document: d,
            element: Some(e),
        };
        let refs = |m: &HashMap<Target, Vec<Target>>| {
            m.get(&t)
                .map(|v| v.iter().map(|&x| self.link(x)).collect())
                .unwrap_or_default()
        };
        let mut path: Vec<ElementRef> = doc
            .ancestors(e)
            .map(|a| {
                self.reference(Target {
                    document: d,
                    element: Some(a),
                })
            })
            .collect();
        path.reverse();
        Ok(Details {
            document: d,
            document_name: doc.name.clone(),
            id: e,
            name: el.display_name(),
            ident: el.ident.clone(),
            label: el.label.clone(),
            type_name: el.type_name.clone(),
            class: el.class,
            text: element_text(doc, e),
            line: el.line,
            status: self.status(d, e),
            change: self.changes[d].get(&e).copied(),
            added: el.added,
            path,
            attributes: el
                .attributes
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect(),
            references: el
                .references
                .iter()
                .map(|r| {
                    if r.inverse {
                        format!("couvert par {}", r.target)
                    } else {
                        r.target.clone()
                    }
                })
                .collect(),
            covers: refs(&self.up),
            covered_by: refs(&self.down),
            problems: self
                .problems(None)
                .into_iter()
                .filter(|p| p.document == Some(d) && p.element == Some(e))
                .collect(),
        })
    }

    pub fn impact(&self, d: usize, e: ElemId) -> Result<Impact, String> {
        self.doc(d)?;
        let root = Target {
            document: d,
            element: Some(e),
        };
        let mut budget = 5000;
        let mut branch = HashSet::from([root]);
        let upstream = self.tree(&self.up, root, &mut branch, &mut budget);
        let downstream = self.tree(&self.down, root, &mut branch, &mut budget);
        Ok(Impact {
            root: self.reference(root),
            upstream,
            downstream,
        })
    }

    fn tree(
        &self,
        edges: &HashMap<Target, Vec<Target>>,
        from: Target,
        branch: &mut HashSet<Target>,
        budget: &mut usize,
    ) -> Vec<ImpactNode> {
        let mut out = Vec::new();
        for &t in edges.get(&from).map(Vec::as_slice).unwrap_or_default() {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let cycle = !branch.insert(t);
            let children = if cycle {
                Vec::new()
            } else {
                let c = self.tree(edges, t, branch, budget);
                branch.remove(&t);
                c
            };
            out.push(ImpactNode {
                target: self.reference(t),
                cycle,
                children,
            });
        }
        out
    }

    pub fn problems(&self, severity: Option<Severity>) -> Vec<Problem> {
        let a = &self.run.analysis;
        a.messages
            .iter()
            .filter(|m| severity.is_none_or(|s| m.severity == s))
            .map(|m| {
                let document = m.document.as_deref().and_then(|n| self.document_index(n));
                Problem {
                    rule: m.rule.clone(),
                    severity: m.severity,
                    text: m.text.clone(),
                    document,
                    document_name: m.document.clone(),
                    element: m.element,
                    element_name: document.zip(m.element).and_then(|(d, e)| {
                        a.documents[d].elements.get(e).map(Element::display_name)
                    }),
                    line: m.line,
                }
            })
            .collect()
    }

    /// Fichier ou répertoire source d'un document.
    pub fn source(&self, d: usize) -> Result<PathBuf, String> {
        self.doc(d)?;
        Ok(document_path(&self.run.flat.documents[d]))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ElementQuery {
    pub text: Option<String>,
    /// `all`, `requirements`, `covered`, `uncovered`, `derived`, `changed`, `problems`.
    pub status: Option<String>,
    pub offset: usize,
    pub limit: Option<usize>,
}

fn push_unique(v: &mut Vec<Target>, t: Target) {
    if !v.contains(&t) {
        v.push(t);
    }
}

fn stamp(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_dir() {
        return meta.modified().ok();
    }
    // Répertoire (code source) : date la plus récente de son contenu.
    let mut latest = meta.modified().ok();
    let mut stack = vec![path.to_owned()];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            seen += 1;
            if seen > 10_000 {
                return latest;
            }
            let Ok(m) = entry.metadata() else { continue };
            if m.is_dir() {
                stack.push(entry.path());
            }
            latest = latest.max(m.modified().ok());
        }
    }
    latest
}

fn key(doc: &CapturedDocument, e: &Element) -> Option<(String, String, String)> {
    let id = e
        .guid
        .clone()
        .or_else(|| e.ident.as_ref().map(|i| i.to_lowercase()))?;
    Some((doc.name.clone(), e.type_name.clone(), id))
}

fn fingerprint(doc: &CapturedDocument, e: &Element) -> String {
    let mut f = format!("{:?}\u{1}{}", e.label, e.text);
    for a in &e.attributes {
        f.push_str(&format!("\u{1}{}={:?}", a.name, a.value));
    }
    for r in &e.references {
        f.push_str(&format!("\u{1}>{}{}", r.inverse, r.target));
    }
    // Le texte rattaché (paragraphes enfants) fait partie de l'élément.
    for &c in &e.children {
        let child = &doc.elements[c];
        if !child.class.is_structural() {
            f.push_str(&format!("\u{1}{:?}:{}", child.class, child.text));
        }
    }
    f
}

/// Texte d'un élément et texte rattaché (paragraphes, tableaux).
fn element_text(doc: &CapturedDocument, e: ElemId) -> String {
    let el = &doc.elements[e];
    let mut parts = vec![el.text.trim().to_owned()];
    for &c in &el.children {
        let child = &doc.elements[c];
        if matches!(
            child.class,
            ElementClass::Text | ElementClass::Table | ElementClass::Picture
        ) {
            parts.push(child.text.trim().to_owned());
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.dedup();
    parts.join("\n")
}

fn matches(e: &Element, needle: &str) -> bool {
    [
        e.ident.as_deref(),
        e.label.as_deref(),
        Some(e.text.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|s| s.to_lowercase().contains(needle))
}

/// Ouvre un fichier avec l'application associée du système.
pub fn open_with_system(path: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("explorer");
        c.arg(path);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("ouverture de {} : {e}", path.display()))
}

fn arg<T: for<'de> Deserialize<'de>>(args: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("argument « {name} » : {e}"))
}

fn to_json<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Exécute une requête de l'interface sur la session courante.
pub fn call(session: &mut Option<Session>, method: &str, args: &Value) -> Result<Value, String> {
    if method == "open" {
        let path: PathBuf = arg(args, "path")?;
        let config: Vec<PathBuf> = arg::<Option<Vec<PathBuf>>>(args, "config")?.unwrap_or_default();
        *session = Some(Session::open(&path, &config)?);
        return to_json(session.as_ref().map(Session::overview));
    }
    if method == "version" {
        return Ok(json!({ "version": env!("CARGO_PKG_VERSION") }));
    }
    if method == "newProject" {
        let path: PathBuf = arg(args, "path")?;
        crate::session_edit::create_project(&path)?;
        *session = Some(Session::open(&path, &[])?);
        return to_json(session.as_ref().map(Session::overview));
    }
    if method == "close" {
        *session = None;
        return Ok(Value::Null);
    }
    let s = session.as_mut().ok_or("aucun projet ouvert")?;
    match method {
        "overview" => to_json(s.overview()),
        "elements" => {
            let d: usize = arg(args, "document")?;
            let q: ElementQuery =
                serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
            to_json(s.elements(d, &q)?)
        }
        "element" => to_json(s.details(arg(args, "document")?, arg(args, "element")?)?),
        "impact" => to_json(s.impact(arg(args, "document")?, arg(args, "element")?)?),
        "problems" => to_json(s.problems(arg(args, "severity")?)),
        "changes" => to_json(s.changed_files()),
        "scripts" => to_json(&s.scripts),
        "trustScripts" => {
            rc_script::trust(&s.run.loaded.dir).map_err(|e| e.to_string())?;
            s.refilter();
            to_json(s.overview())
        }
        "reload" => {
            s.reload()?;
            to_json(s.overview())
        }
        "openSource" => {
            let p = s.source(arg(args, "document")?)?;
            open_with_system(&p)?;
            Ok(Value::Null)
        }
        "project" => to_json(s.project_info()),
        "updateProject" => {
            s.update_project(arg(args, "project")?)?;
            to_json(s.overview())
        }
        "types" => to_json(s.type_infos()),
        "type" => to_json(s.document_type(&arg::<String>(args, "name")?)?),
        "saveType" => {
            let previous: Option<String> = arg(args, "previous")?;
            s.save_type(arg(args, "type")?, previous.as_deref())?;
            to_json(s.overview())
        }
        "deleteType" => {
            s.delete_type(&arg::<String>(args, "name")?)?;
            to_json(s.overview())
        }
        "testType" => {
            let params: Option<Vec<(String, String)>> = arg(args, "params")?;
            let file: PathBuf = arg(args, "file")?;
            to_json(s.test_type(&arg(args, "type")?, &file, params.unwrap_or_default())?)
        }
        "filters" => to_json(s.filters_info()),
        "saveFilters" => {
            s.save_filters(arg(args, "filters")?)?;
            to_json(s.overview())
        }
        "applyFilter" => {
            let name: Option<String> = arg(args, "name")?;
            let params: Option<HashMap<String, String>> = arg(args, "params")?;
            s.apply_filter(
                &arg::<String>(args, "mode")?,
                name.as_deref(),
                params.unwrap_or_default(),
            )?;
            to_json(s.overview())
        }
        "save" => {
            let path: Option<PathBuf> = arg(args, "path")?;
            let saved = s.save(path)?;
            Ok(json!({ "path": saved }))
        }
        "snapshots" => to_json(s.snapshots()?),
        "createSnapshot" => to_json(s.create_snapshot(&arg::<String>(args, "name")?)?),
        "deleteSnapshot" => {
            s.delete_snapshot(arg(args, "id")?)?;
            Ok(Value::Null)
        }
        "compare" => to_json(s.compare(arg(args, "from")?, arg(args, "to")?)?),
        "history" => to_json(s.history(arg(args, "document")?, arg(args, "element")?)?),
        "reportTemplates" => Ok(json!({
            "templates": rc_report::Template::ALL
                .iter()
                .map(|t| json!({ "id": t.id(), "label": t.label() }))
                .collect::<Vec<_>>(),
            "formats": rc_report::Format::ALL
                .iter()
                .map(|f| f.extension())
                .collect::<Vec<_>>(),
        })),
        "report" => {
            let path: PathBuf = arg(args, "path")?;
            let written = s.report(
                &arg::<String>(args, "template")?,
                &arg::<String>(args, "format")?,
                &path,
            )?;
            if arg::<Option<bool>>(args, "open")?.unwrap_or(false) {
                open_with_system(Path::new(&written))?;
            }
            Ok(json!({ "path": written }))
        }
        _ => Err(format!("méthode inconnue : {method}")),
    }
}
