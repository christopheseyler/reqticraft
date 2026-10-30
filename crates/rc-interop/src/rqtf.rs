//! Projets `.rqtf` : nouveau format (XML) et ancien format (INI).
//!
//! Correspondance des champs : `docs/interop-formats.md` §2 et §3.

use crate::ini::{split_list, Ini};
use crate::text::sanitize_xml;
use crate::InteropError;
use rc_model::{
    Channel, ChannelKind, DocumentDecl, ModificationDocRef, PathReplacement, Project, ProjectNode,
};
use std::path::PathBuf;

/// Lit un projet ; le format est détecté sur le contenu.
///
/// Les incohérences non bloquantes (canal vers un document absent…) sont
/// renvoyées comme avertissements, comme le fait la règle `unknownCoveringDocument`.
pub fn parse_project(text: &str, name: &str) -> Result<(Project, Vec<String>), InteropError> {
    let mut warnings = Vec::new();
    let mut project = if crate::text::looks_like_xml(text) {
        parse_xml(text, &mut warnings)?
    } else {
        parse_ini(text, &mut warnings)?
    };
    project.name = name.to_owned();
    Ok((project, warnings))
}

fn parse_xml(text: &str, warnings: &mut Vec<String>) -> Result<Project, InteropError> {
    let fixed = sanitize_xml(text);
    let doc = roxmltree::Document::parse(&fixed)?;
    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err(InteropError::Format(format!(
            "élément racine inattendu `{}` (attendu : Project)",
            root.tag_name().name()
        )));
    }
    let mut project = Project::default();
    let mut covers: Vec<(String, Vec<String>)> = Vec::new();

    for node in root.children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "Property" => project.properties.push(property(&node)),
            "Document" | "Folder" | "ProjectReference" => {
                xml_node(&node, &mut project);
            }
            "Cover" => covers.push((
                attr(&node, "outputDocument"),
                node.children()
                    .filter(|c| c.has_tag_name("InputDocument"))
                    .map(|c| attr(&c, "name"))
                    .collect(),
            )),
            "ReplacingPath" => {
                let replacing = components(&node);
                let replaced = node
                    .children()
                    .filter(|c| c.has_tag_name("ReplacedPath"))
                    .map(|c| components(&c))
                    .collect();
                project.path_replacements.push(PathReplacement {
                    replacing,
                    replaced,
                });
            }
            _ => {}
        }
    }

    for (covered, covering) in covers {
        push_channel(&mut project, &covered, &covering, warnings);
    }
    Ok(project)
}

/// Lit un document, un dossier (et son contenu) ou un sous-projet ; renvoie son indice.
fn xml_node(node: &roxmltree::Node, project: &mut Project) -> Option<usize> {
    let index = project.nodes.len();
    match node.tag_name().name() {
        "Document" => {
            let mut doc = DocumentDecl {
                name: attr(node, "name"),
                doc_type: attr(node, "type"),
                path: PathBuf::from(attr(node, "path")),
                absolute_path: node.attribute("absolutePath").map(PathBuf::from),
                category: node.attribute("category").map(str::to_owned),
                position: position(node),
                ..Default::default()
            };
            for child in node.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "Property" => doc.params.push(property(&child)),
                    "ModificationDocument" => doc.modification_docs.push(ModificationDocRef {
                        path: PathBuf::from(attr(&child, "path")),
                        doc_type: attr(&child, "type"),
                    }),
                    "IgnoredType" => doc.ignored_types.push(attr(&child, "name")),
                    _ => {}
                }
            }
            if node.attribute("withRichText") == Some("true") {
                doc.params.push(("withRichText".into(), "1".into()));
            }
            project.nodes.push(ProjectNode::Document(doc));
        }
        "ProjectReference" => project.nodes.push(ProjectNode::SubProject {
            name: attr(node, "name"),
            path: PathBuf::from(attr(node, "path")),
        }),
        // Un dossier contient ses documents ; leurs positions sont celles de la
        // vue intérieure du dossier.
        "Folder" => {
            project.nodes.push(ProjectNode::Folder {
                name: attr(node, "name"),
                default_type: None,
                children: Vec::new(),
                position: position(node),
                size: pair(node, "positionW", "positionH"),
                category: node.attribute("category").map(str::to_owned),
            });
            let children: Vec<usize> = node
                .children()
                .filter(|n| n.is_element())
                .filter_map(|c| xml_node(&c, project))
                .collect();
            if let ProjectNode::Folder { children: c, .. } = &mut project.nodes[index] {
                *c = children;
            }
        }
        _ => return None,
    }
    Some(index)
}

fn parse_ini(text: &str, warnings: &mut Vec<String>) -> Result<Project, InteropError> {
    let ini = Ini::parse(text);
    let files = ini
        .section("Files")
        .ok_or_else(|| InteropError::Format("section [Files] absente".into()))?;
    let mut project = Project {
        properties: variables(files),
        ..Default::default()
    };
    let names = split_list(files.get("Names").unwrap_or_default());
    for name in &names {
        let Some(section) = ini.section(name) else {
            return Err(InteropError::Format(format!(
                "document `{name}` listé dans [Files] mais sans section"
            )));
        };
        let mut doc = DocumentDecl {
            name: name.clone(),
            doc_type: section.get("Type").unwrap_or_default().to_owned(),
            path: PathBuf::from(section.get("Path").unwrap_or_default()),
            absolute_path: section.get_nonempty("AbsolutePath").map(PathBuf::from),
            category: section.get_nonempty("Category").map(str::to_owned),
            params: variables(section),
            position: section.get("GraphicPosition").and_then(parse_at_position),
            ..Default::default()
        };
        for (n, path) in section.numbered("ModificationDocument", "") {
            let doc_type = section
                .get(&format!("ModificationDocument{n}Type"))
                .unwrap_or_default();
            doc.modification_docs.push(ModificationDocRef {
                path: PathBuf::from(path),
                doc_type: doc_type.to_owned(),
            });
        }
        if section.flag("WithRichText") {
            doc.params.push(("withRichText".into(), "1".into()));
        }
        project.nodes.push(ProjectNode::Document(doc));
    }
    // `Cover<n>` est porté par le document couvert et nomme le document couvrant.
    for name in &names {
        let section = ini.section(name).expect("vérifié ci-dessus");
        for (_, covering) in section.numbered("Cover", "") {
            push_channel(&mut project, name, &[covering.to_owned()], warnings);
        }
    }
    Ok(project)
}

fn push_channel(
    project: &mut Project,
    covered: &str,
    covering: &[String],
    warnings: &mut Vec<String>,
) {
    let mut find = |name: &str| {
        let found = project.node_by_name(name);
        if found.is_none() {
            warnings.push(format!("canal ignoré : document inconnu `{name}`"));
        }
        found
    };
    let Some(covered) = find(covered) else { return };
    let covering: Vec<_> = covering.iter().filter_map(|c| find(c)).collect();
    if covering.is_empty() {
        return;
    }
    project.channels.push(Channel {
        kind: ChannelKind::Cover,
        covered,
        covering,
        category: None,
    });
}

fn attr(node: &roxmltree::Node, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_owned()
}

fn property(node: &roxmltree::Node) -> (String, String) {
    (attr(node, "name"), attr(node, "value"))
}

fn position(node: &roxmltree::Node) -> Option<(i64, i64)> {
    pair(node, "positionX", "positionY")
}

fn pair(node: &roxmltree::Node, a: &str, b: &str) -> Option<(i64, i64)> {
    Some((
        node.attribute(a)?.parse().ok()?,
        node.attribute(b)?.parse().ok()?,
    ))
}

fn components(node: &roxmltree::Node) -> Vec<String> {
    node.children()
        .filter(|c| c.has_tag_name("Component"))
        .map(|c| attr(&c, "name"))
        .collect()
}

/// `Variable<n>Name` / `Variable<n>Value` d'une section INI.
fn variables(section: &crate::ini::Section) -> Vec<(String, String)> {
    section
        .numbered("Variable", "Name")
        .into_iter()
        .map(|(n, name)| {
            let value = section
                .get(&format!("Variable{n}Value"))
                .unwrap_or_default();
            (name.to_owned(), value.to_owned())
        })
        .collect()
}

fn parse_at_position(value: &str) -> Option<(i64, i64)> {
    let (x, y) = value.split_once('@')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forme observée dans un projet enregistré par Reqtify (interop-formats.md §2.3),
    // réécrite avec nos noms.
    const FOLDERS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project>
<Folder name="Specs" category="HLRS" positionX="4850" positionY="2345" positionW="1635" positionH="500">
<Document name="SSS" positionX="2420" positionY="2810" type="Word" path="sss.docx">
<Property name="withCharacterStyles" value="1" />
</Document>
<Document name="ICD" positionX="3670" positionY="2800" type="Word" path="icd.docx">
</Document>
</Folder>
<Folder name="Empty" positionX="890" positionY="2410" positionW="1500" positionH="500">
</Folder>
<Document name="Code" positionX="1" positionY="2" type="C" path="src" />
<Cover outputDocument="Specs" positionX="4820" positionY="2920">
<InputDocument name="Code" />
</Cover>
<Cover outputDocument="SSS" positionX="0" positionY="0">
<InputDocument name="SSS" />
</Cover>
</Project>"#;

    #[test]
    fn reads_folders() {
        let (p, warnings) = parse_project(FOLDERS, "f").unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let names: Vec<&str> = p.nodes.iter().map(|n| n.name()).collect();
        assert_eq!(names, ["Specs", "SSS", "ICD", "Empty", "Code"]);
        assert_eq!(
            p.nodes[0],
            ProjectNode::Folder {
                name: "Specs".into(),
                default_type: None,
                children: vec![1, 2],
                position: Some((4850, 2345)),
                size: Some((1635, 500)),
                category: Some("HLRS".into()),
            }
        );
        assert_eq!(p.parent_folder(2), Some(0));
        assert_eq!(p.documents_of(3), Vec::<usize>::new());
        // Canal vers un dossier, et document qui se couvre lui-même.
        assert_eq!(
            (p.channels[0].covered, p.channels[0].covering.clone()),
            (0, vec![4])
        );
        assert_eq!(
            (p.channels[1].covered, p.channels[1].covering.clone()),
            (1, vec![1])
        );
    }

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project>
<Property name="Feature" value="F_WORD" />
<Document name="Spec" positionX="705" positionY="370" type="Word" path="spec.docx" category="Requirements">
<Property name="withImages" value="1" />
<ModificationDocument type="Added Elements" path="modif.xml" />
</Document>
<Document name="Design" positionX="1" positionY="2" type="Word" path="design.docx" />
<Document name="Tests" positionX="1" positionY="2" type="Word" path="tests.docx" />
<Cover outputDocument="Spec" positionX="1007" positionY="710">
<InputDocument name="Design" />
<InputDocument name="Tests" />
</Cover>
</Project>"#;

    #[test]
    fn reads_xml_projects() {
        let (p, _) = parse_project(XML, "demo").unwrap();
        assert_eq!(
            p.properties,
            vec![("Feature".to_owned(), "F_WORD".to_owned())]
        );
        let ProjectNode::Document(spec) = &p.nodes[0] else {
            panic!()
        };
        assert_eq!(spec.doc_type, "Word");
        assert_eq!(spec.position, Some((705, 370)));
        assert_eq!(spec.category.as_deref(), Some("Requirements"));
        assert_eq!(spec.params, vec![("withImages".to_owned(), "1".to_owned())]);
        assert_eq!(spec.modification_docs[0].doc_type, "Added Elements");
        // Un Cover à plusieurs InputDocument : un seul canal, combinaison OU.
        assert_eq!(p.channels.len(), 1);
        assert_eq!(p.channels[0].covered, 0);
        assert_eq!(p.channels[0].covering, vec![1, 2]);
    }

    #[test]
    fn reads_ini_projects_with_the_same_meaning() {
        let ini = "[Files]\r\nNames=Spec,Design\r\nVariable1Name=boostPurgeTypes\r\nVariable1Value=1\r\n\r\n\
                   [Spec]\r\nCover1=Design\r\nCover1Position=1007@710\r\nGraphicPosition=705@370\r\nType=Word\r\nPath=spec.doc\r\n\
                   ModificationDocument1=modif.xml\r\nModificationDocument1Type=Added Elements\r\n\r\n\
                   [Design]\r\nGraphicPosition=1@2\r\nType=Word\r\nPath=design.doc\r\n";
        let (p, _) = parse_project(ini, "demo").unwrap();
        assert_eq!(
            p.properties,
            vec![("boostPurgeTypes".to_owned(), "1".to_owned())]
        );
        let ProjectNode::Document(spec) = &p.nodes[0] else {
            panic!()
        };
        assert_eq!(spec.position, Some((705, 370)));
        assert_eq!(spec.modification_docs[0].path, PathBuf::from("modif.xml"));
        assert_eq!(p.channels.len(), 1);
        assert_eq!(
            (p.channels[0].covered, p.channels[0].covering.clone()),
            (0, vec![1])
        );
    }

    #[test]
    fn reads_composite_projects() {
        let xml = r#"<Project>
<Document name="Specification" positionX="1" positionY="1" type="T" path="s.doc" />
<ProjectReference name="Sub1" positionX="1" positionY="1" type="_ProjectReference" path="Sub1\Sub1.rqtfimage" />
<ReplacingPath><Component name="Specification" />
<ReplacedPath><Component name="Sub1" /><Component name="Specification" /></ReplacedPath>
</ReplacingPath></Project>"#;
        let (p, _) = parse_project(xml, "c").unwrap();
        assert!(matches!(&p.nodes[1], ProjectNode::SubProject { name, .. } if name == "Sub1"));
        assert_eq!(p.path_replacements[0].replacing, vec!["Specification"]);
        assert_eq!(
            p.path_replacements[0].replaced,
            vec![vec!["Sub1", "Specification"]]
        );
    }

    #[test]
    fn warns_about_channels_to_unknown_documents() {
        let ini = "[Files]\nNames=A\n[A]\nCover1=Ghost\nType=Text\nPath=a.txt\n";
        let (p, warnings) = parse_project(ini, "x").unwrap();
        assert!(p.channels.is_empty());
        assert_eq!(warnings.len(), 1);
    }
}
