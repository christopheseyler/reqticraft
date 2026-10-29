//! Façade de ReqtiCraft : charge un projet, convertit et capture chaque document,
//! applique les documents de modification, puis analyse le tout.
//!
//! C'est le point d'entrée commun à la ligne de commande et à l'application.

use rc_analysis::{Analysis, ChannelInput};
use rc_connect::access::{self, Want};
use rc_connect::{Connector, Context, Profiles};
use rc_convert::{ConvertOptions, ConverterId, Intermediate};
use rc_interop::{LoadedProject, TypeLibrary};
use rc_model::{
    AttributeValue, CapturedDocument, DocumentDecl, DocumentType, ElementFlags, Message,
    Modification, ProjectNode, Reference, Severity, TypeKind,
};
use std::path::{Path, PathBuf};

pub mod session;
pub mod session_edit;

pub use rc_analysis;
pub use rc_connect;
pub use rc_convert;
pub use rc_interop;
pub use rc_model;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Interop(#[from] rc_interop::InteropError),
    #[error(transparent)]
    Convert(#[from] rc_convert::ConvertError),
    #[error("{0}")]
    Other(String),
}

/// Fichiers de la bibliothèque de types ReqtiCraft (écrits à partir des
/// conventions documentées, voir `docs/functional-spec.md` §6).
const LIBRARY: &[(&str, &str)] = &[
    ("text.types", include_str!("../library/text.types")),
    ("word.types", include_str!("../library/word.types")),
    ("excel.types", include_str!("../library/excel.types")),
    ("pdf.types", include_str!("../library/pdf.types")),
    ("code.types", include_str!("../library/code.types")),
    ("json.types", include_str!("../library/json.types")),
    ("document.types", include_str!("../library/document.types")),
    ("markup.types", include_str!("../library/markup.types")),
    (
        "structured.types",
        include_str!("../library/structured.types"),
    ),
    ("alm.types", include_str!("../library/alm.types")),
    ("history.types", include_str!("../library/history.types")),
];

/// Types d'analyse livrés avec ReqtiCraft.
pub fn builtin_types() -> Vec<DocumentType> {
    LIBRARY
        .iter()
        .flat_map(|(name, text)| {
            let mut types = rc_interop::parse_types(text)
                .unwrap_or_else(|e| panic!("bibliothèque {name} invalide : {e}"));
            for t in &mut types {
                t.source = Some(PathBuf::from(format!("<reqticraft>/{name}")));
            }
            types
        })
        .collect()
}

/// Charge un projet avec la bibliothèque intégrée en dernier recours.
pub fn load_project(path: &Path, config_dirs: &[PathBuf]) -> Result<LoadedProject, CoreError> {
    Ok(rc_interop::load_project(
        path,
        config_dirs,
        &builtin_types(),
    )?)
}

/// Document à analyser, après aplatissement des sous-projets.
#[derive(Debug, Clone)]
pub struct DocumentEntry {
    /// Nom unique : `Document`, ou `SousProjet/Document`.
    pub name: String,
    pub decl: DocumentDecl,
    /// Répertoire de référence des chemins relatifs.
    pub base_dir: PathBuf,
    /// Types visibles pour ce document (ceux de son projet).
    pub types: TypeLibrary,
}

/// Projet aplati : documents et canaux en indices de documents.
#[derive(Debug, Clone, Default)]
pub struct FlatProject {
    pub documents: Vec<DocumentEntry>,
    pub channels: Vec<ChannelInput>,
    pub messages: Vec<Message>,
}

/// Aplatit un projet et ses sous-projets. Un sous-projet est lu depuis son `.rqtf`
/// (le fichier de résultats `.rqtfimage` n'est pas lu, voir `docs/interop-formats.md` §8).
pub fn flatten(loaded: &LoadedProject, config_dirs: &[PathBuf]) -> FlatProject {
    let mut flat = FlatProject::default();
    flatten_into(loaded, config_dirs, "", &mut flat, 0);
    flat
}

fn flatten_into(
    loaded: &LoadedProject,
    config_dirs: &[PathBuf],
    prefix: &str,
    flat: &mut FlatProject,
    depth: usize,
) {
    let project = &loaded.project;
    // Nœud du projet → document aplati (les sous-projets n'ont pas d'indice propre).
    let mut node_doc: Vec<Option<usize>> = vec![None; project.nodes.len()];
    let mut sub_docs: Vec<(String, Vec<(String, usize)>)> = Vec::new();

    for (n, node) in project.nodes.iter().enumerate() {
        match node {
            ProjectNode::Document(decl) => {
                node_doc[n] = Some(flat.documents.len());
                flat.documents.push(DocumentEntry {
                    name: format!("{prefix}{}", decl.name),
                    decl: decl.clone(),
                    base_dir: loaded.dir.clone(),
                    types: loaded.types.clone(),
                });
            }
            ProjectNode::SubProject { name, path } => {
                let mut rqtf = loaded.dir.join(native(path));
                if rqtf
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("rqtfimage"))
                {
                    rqtf.set_extension("rqtf");
                }
                if depth > 8 {
                    flat.messages.push(project_message(
                        "cyclicGraph",
                        format!("sous-projets trop imbriqués : {name}"),
                    ));
                    continue;
                }
                match load_project(&rqtf, config_dirs) {
                    Ok(sub) => {
                        let start = flat.documents.len();
                        flatten_into(
                            &sub,
                            config_dirs,
                            &format!("{prefix}{name}/"),
                            flat,
                            depth + 1,
                        );
                        let docs = (start..flat.documents.len())
                            .map(|d| {
                                (
                                    flat.documents[d].name[prefix.len() + name.len() + 1..]
                                        .to_owned(),
                                    d,
                                )
                            })
                            .collect();
                        sub_docs.push((name.clone(), docs));
                    }
                    Err(e) => flat.messages.push(project_message(
                        "unreadableDocument",
                        format!("sous-projet {name} illisible ({}) : {e}", rqtf.display()),
                    )),
                }
            }
            ProjectNode::Folder { .. } => {}
        }
    }

    for ch in &project.channels {
        let covered = node_doc[ch.covered];
        let covering: Vec<usize> = ch.covering.iter().filter_map(|&c| node_doc[c]).collect();
        match covered {
            Some(c) if !covering.is_empty() => flat.channels.push(ChannelInput {
                covered: c,
                covering,
            }),
            _ => flat.messages.push(project_message(
                "unknownCoveringDocument",
                "canal vers un sous-projet entier : non pris en charge, ignoré".into(),
            )),
        }
    }

    // Remplacements : le document remplaçant prend la place du document remplacé
    // dans les canaux du sous-projet, et ce dernier n'est pas analysé.
    for rep in &project.path_replacements {
        let Some(replacing) = find_path(&rep.replacing, project, &node_doc, &sub_docs) else {
            flat.messages.push(project_message(
                "unknownReplacingDocument",
                format!(
                    "document remplaçant introuvable : {}",
                    rep.replacing.join("/")
                ),
            ));
            continue;
        };
        for replaced in &rep.replaced {
            let Some(old) = find_path(replaced, project, &node_doc, &sub_docs) else {
                flat.messages.push(project_message(
                    "unknownReplacingDocument",
                    format!("document remplacé introuvable : {}", replaced.join("/")),
                ));
                continue;
            };
            for ch in &mut flat.channels {
                if ch.covered == old {
                    ch.covered = replacing;
                }
                for c in &mut ch.covering {
                    if *c == old {
                        *c = replacing;
                    }
                }
            }
            flat.documents[old].decl.doc_type = String::new();
        }
    }
}

fn find_path(
    path: &[String],
    project: &rc_model::Project,
    node_doc: &[Option<usize>],
    subs: &[(String, Vec<(String, usize)>)],
) -> Option<usize> {
    match path {
        [doc] => project.node_by_name(doc).and_then(|n| node_doc[n]),
        [sub, rest @ ..] => {
            let (_, docs) = subs.iter().find(|(name, _)| name == sub)?;
            let wanted = rest.join("/");
            docs.iter()
                .find(|(name, _)| *name == wanted)
                .map(|&(_, d)| d)
        }
        [] => None,
    }
}

fn project_message(rule: &str, text: String) -> Message {
    Message {
        rule: rule.to_owned(),
        severity: Severity::Error,
        text,
        document: None,
        element: None,
        line: None,
    }
}

/// Chemin d'un projet Reqtify adapté au système : les `\` des projets écrits
/// sous Windows deviennent des `/` ailleurs.
pub fn native(path: &Path) -> PathBuf {
    if cfg!(windows) {
        path.to_owned()
    } else {
        PathBuf::from(path.to_string_lossy().replace('\\', "/"))
    }
}

/// Chemin du fichier ou répertoire d'un document.
pub fn document_path(entry: &DocumentEntry) -> PathBuf {
    let relative = entry.base_dir.join(native(&entry.decl.path));
    if relative.exists() {
        return relative;
    }
    match &entry.decl.absolute_path {
        Some(abs) if abs.exists() => abs.clone(),
        _ => relative,
    }
}

/// Options de conversion d'un document : paramètres et sélection de fichiers.
pub fn convert_options(doc_type: &DocumentType, decl: &DocumentDecl) -> ConvertOptions {
    ConvertOptions {
        params: decl.params.clone(),
        matching: doc_type.matching_file_names.clone(),
        extensions: doc_type.file_extensions.clone(),
        excluded: doc_type.excluded_file_names.clone(),
    }
}

/// Convertit un document selon son type.
pub fn convert_document(
    doc_type: &DocumentType,
    path: &Path,
    options: &ConvertOptions,
) -> Result<Intermediate, String> {
    let id = ConverterId::for_tool(&doc_type.converter)
        .ok_or_else(|| format!("convertisseur « {} » non disponible", doc_type.converter))?;
    rc_convert::convert(id, path, options).map_err(|e| e.to_string())
}

/// Répertoire de travail d'un document (copies locales, caches des connecteurs).
pub fn work_dir(entry: &DocumentEntry) -> PathBuf {
    let safe: String = entry
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    entry.base_dir.join(".reqticraft").join("work").join(safe)
}

/// Emplacement du document tel qu'écrit dans le projet (chemin, URL ou requête).
pub fn location(entry: &DocumentEntry) -> String {
    entry.decl.path.to_string_lossy().into_owned()
}

/// Produit l'intermédiaire d'un document : convertisseur intégré (après
/// rapatriement par un type d'accès si l'emplacement est distant ou archivé),
/// connecteur (outil ALM, journal de versions, base de données, commande) ou
/// plugin. En cas d'échec : identifiant de règle et message.
pub fn intermediate(
    entry: &DocumentEntry,
    doc_type: &DocumentType,
) -> Result<Intermediate, (&'static str, String)> {
    let profiles = Profiles::load(&entry.base_dir);
    let location = location(entry);
    let ctx = Context {
        base_dir: &entry.base_dir,
        work_dir: work_dir(entry),
        profiles: &profiles,
        params: &entry.decl.params,
    };
    if let Some(id) = ConverterId::for_tool(&doc_type.converter) {
        let want = Want {
            directory: !doc_type.matching_file_names.is_empty()
                || doc_type
                    .properties
                    .iter()
                    .any(|(k, v)| k.eq_ignore_ascii_case("ForceProcessingAsDirectory") && v == "1"),
            extensions: &doc_type.file_extensions,
        };
        let path = if access::is_remote(&location, &ctx, want) {
            access::resolve(&location, &ctx, want)
                .map_err(|e| ("unreadableDocument", e.to_string()))?
        } else {
            document_path(entry)
        };
        if !path.exists() {
            return Err((
                "unreadableDocument",
                format!("fichier introuvable : {}", path.display()),
            ));
        }
        return rc_convert::convert(id, &path, &convert_options(doc_type, &entry.decl))
            .map_err(|e| ("conversionProblem", e.to_string()));
    }
    if let Some(connector) = Connector::for_tool(&doc_type.converter) {
        return connector
            .run(&location, &ctx)
            .map_err(|e| ("conversionProblem", e.to_string()));
    }
    if let Some(plugin) = rc_plugin::discover(&entry.base_dir)
        .into_iter()
        .find(|p| p.provides(&doc_type.converter))
    {
        let out = plugin
            .convert(
                &doc_type.converter,
                &location,
                &entry.decl.params,
                &entry.base_dir,
                &ctx.work_dir,
            )
            .map_err(|e| ("conversionProblem", e.to_string()))?;
        return Ok(if out.xml {
            Intermediate::Xml(out.content)
        } else {
            Intermediate::Text(out.content)
        });
    }
    Err((
        "unknownConvertMethod",
        format!("convertisseur « {} » non disponible", doc_type.converter),
    ))
}

/// Résultat complet : projet chargé, documents aplatis et analyse.
pub struct ProjectRun {
    pub loaded: LoadedProject,
    pub flat: FlatProject,
    /// Documents capturés, avant tout filtre d'analyse.
    pub captured: Vec<CapturedDocument>,
    pub analysis: Analysis,
}

/// Filtre d'analyse et valeurs de ses paramètres.
pub type AnalysisFilter = (rc_model::Filter, std::collections::HashMap<String, String>);

/// Charge et analyse un projet.
pub fn analyze_project(path: &Path, config_dirs: &[PathBuf]) -> Result<ProjectRun, CoreError> {
    let loaded = load_project(path, config_dirs)?;
    Ok(run_loaded(loaded, config_dirs, None))
}

/// Analyse un projet déjà chargé (éventuellement modifié en mémoire).
pub fn run_loaded(
    loaded: LoadedProject,
    config_dirs: &[PathBuf],
    filter: Option<&AnalysisFilter>,
) -> ProjectRun {
    let flat = flatten(&loaded, config_dirs);
    let captured: Vec<CapturedDocument> = flat.documents.iter().map(capture_document).collect();
    let analysis = analyze_captured(&loaded, &flat, &captured, filter);
    ProjectRun {
        loaded,
        flat,
        captured,
        analysis,
    }
}

/// Analyse des documents déjà capturés. Avec un filtre d'analyse, les éléments
/// filtrés sont ignorés et les taux recalculés ; les conditions sur l'état de
/// couverture sont évaluées sur l'analyse sans filtre.
pub fn analyze_captured(
    loaded: &LoadedProject,
    flat: &FlatProject,
    captured: &[CapturedDocument],
    filter: Option<&AnalysisFilter>,
) -> Analysis {
    let mut analysis = rc_analysis::analyze(captured.to_vec(), &flat.channels);
    if let Some((f, params)) = filter {
        let mut documents = captured.to_vec();
        let ctx = rc_analysis::filter::Context {
            analysis: Some(&analysis),
            params,
        };
        rc_analysis::filter::neutralize(&mut documents, f, &ctx);
        analysis = rc_analysis::analyze(documents, &flat.channels);
    }
    analysis.messages.extend(flat.messages.iter().cloned());
    analysis
        .messages
        .extend(loaded.warnings.iter().map(|w| Message {
            severity: Severity::Warning,
            ..project_message("projectFile", w.clone())
        }));
    analysis
}

/// Convertit, capture et complète un document. Les problèmes deviennent des
/// messages du document (règles `unknownDocumentType`, `conversionProblem`…).
pub fn capture_document(entry: &DocumentEntry) -> CapturedDocument {
    let mut doc = CapturedDocument {
        name: entry.name.clone(),
        type_name: entry.decl.doc_type.clone(),
        ..Default::default()
    };
    // Document remplacé par un document du projet parent : non analysé.
    if entry.decl.doc_type.is_empty() {
        return doc;
    }
    let fail = |doc: &mut CapturedDocument, rule: &str, text: String| {
        doc.messages.push(Message {
            rule: rule.to_owned(),
            severity: Severity::Error,
            text,
            document: Some(entry.name.clone()),
            element: None,
            line: None,
        });
    };
    let Some(doc_type) = entry.types.get(&entry.decl.doc_type) else {
        fail(
            &mut doc,
            "unknownDocumentType",
            format!("type d'analyse inconnu : {}", entry.decl.doc_type),
        );
        return doc;
    };
    if doc_type.kind == TypeKind::AddedElements {
        return doc;
    }
    let intermediate = match intermediate(entry, doc_type) {
        Ok(i) => i,
        Err((rule, text)) => {
            fail(&mut doc, rule, text);
            return doc;
        }
    };
    let mut effective = doc_type.clone();
    for et in &mut effective.element_types {
        if entry.decl.ignored_types.contains(&et.name) {
            et.flags = ElementFlags {
                ignored: true,
                ..et.flags.clone()
            };
        }
    }
    let is_xml = matches!(intermediate, Intermediate::Xml(_));
    let mut captured = rc_capture::capture(&effective, &entry.name, intermediate.as_str(), is_xml);
    captured.type_name = doc_type.name.clone();

    for m in &entry.decl.modification_docs {
        let path = entry.base_dir.join(native(&m.path));
        match rc_interop::read_text(&path).and_then(|t| rc_interop::parse_modifications(&t)) {
            Ok(mods) => apply_modifications(&mut captured, &mods),
            Err(e) => captured.messages.push(Message {
                rule: "unreadableDocument".into(),
                severity: Severity::Warning,
                text: format!(
                    "document de modification {} illisible : {e}",
                    path.display()
                ),
                document: Some(entry.name.clone()),
                element: None,
                line: None,
            }),
        }
    }
    captured
}

/// Applique un document de modification : attributs, références et liens ajoutés
/// aux éléments désignés par leur identifiant.
pub fn apply_modifications(doc: &mut CapturedDocument, mods: &[Modification]) {
    for m in mods {
        let target = m.ident.as_ref().and_then(|ident| {
            doc.elements.iter().position(|e| {
                e.class.is_structural()
                    && e.ident
                        .as_deref()
                        .is_some_and(|i| i.eq_ignore_ascii_case(ident))
            })
        });
        match m.tag.as_str() {
            "Attribute" => doc.attributes.push(attribute(m)),
            "Reference" => doc.references.push(reference(m)),
            _ => match target {
                Some(e) => {
                    for c in &m.children {
                        match c.tag.as_str() {
                            "Attribute" => doc.elements[e].attributes.push(attribute(c)),
                            "Reference" => doc.elements[e].references.push(reference(c)),
                            "Link" => doc.elements[e].links.push(reference(c)),
                            _ => apply_modifications(doc, std::slice::from_ref(c)),
                        }
                    }
                }
                None => doc.messages.push(Message {
                    rule: "elementModifiedNotFound".into(),
                    severity: Severity::Warning,
                    text: format!(
                        "élément modifié introuvable dans le document d'origine : {}",
                        m.ident.as_deref().unwrap_or(&m.type_name)
                    ),
                    document: Some(doc.name.clone()),
                    element: None,
                    line: None,
                }),
            },
        }
    }
}

fn attribute(m: &Modification) -> AttributeValue {
    AttributeValue {
        name: m.type_name.clone(),
        value: m.ident.clone(),
    }
}

fn reference(m: &Modification) -> Reference {
    Reference {
        type_name: m.type_name.clone(),
        target: m.ident.clone().unwrap_or_default(),
        target_document: None,
        inverse: false,
        attributes: Vec::new(),
        line: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_model::{Element, ElementClass};

    #[test]
    fn builtin_library_parses() {
        let types = builtin_types();
        let names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Text",
                "Word",
                "Excel",
                "ExcelXML",
                "PDF",
                "Code",
                "JSON",
                "XML",
                "Writer",
                "RTF",
                "FrameMaker MIF",
                "HTML",
                "Markdown",
                "AsciiDoc",
                "PowerPoint",
                "ReqIF",
                "Visio",
                "MS Project",
                "SysML",
                "Media",
                "File Properties",
                "Jira",
                "Redmine",
                "Bugzilla",
                "codebeamer",
                "Jama",
                "Polarion",
                "DOORS Next",
                "Change Management",
                "Git Log",
                "SVN Log",
                "Perforce Log",
                "Database"
            ]
        );
        for t in &types {
            assert!(
                ConverterId::for_tool(&t.converter).is_some()
                    || Connector::for_tool(&t.converter).is_some(),
                "{}",
                t.name
            );
            // Les motifs XML se compilent sans erreur.
            if t.kind == TypeKind::Xml {
                let doc = rc_capture::capture(t, "x", "<root/>", true);
                assert!(doc.messages.is_empty(), "{} : {:?}", t.name, doc.messages);
            }
            for et in &t.element_types {
                match &et.rule {
                    rc_model::CaptureRule::Regex { expression, .. } => {
                        rc_capture::compile(expression)
                            .unwrap_or_else(|e| panic!("{}/{} : {e}", t.name, et.name));
                    }
                    rc_model::CaptureRule::XmlTree { .. } => {}
                    rc_model::CaptureRule::None => panic!("{}/{} sans règle", t.name, et.name),
                }
            }
        }
    }

    #[test]
    fn applies_modification_documents() {
        let mut doc = CapturedDocument::default();
        let mut e = Element::new("Req", ElementClass::Requirement);
        e.ident = Some("REQ1".into());
        doc.elements.push(e);
        let mods = rc_interop::parse_modifications(
            r#"<ModificationDocument><Requirement type="R" ident="req1"><Attribute type="Status" ident="OK"/><Reference type="Implements" ident="HL1"/></Requirement><Requirement type="R" ident="GHOST"/></ModificationDocument>"#,
        )
        .unwrap();
        apply_modifications(&mut doc, &mods);
        assert_eq!(doc.elements[0].attributes[0].value.as_deref(), Some("OK"));
        assert_eq!(doc.elements[0].references[0].target, "HL1");
        assert_eq!(doc.messages[0].rule, "elementModifiedNotFound");
    }
}
