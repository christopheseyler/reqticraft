//! Écriture des projets, types et filtres au format XML lu par [`crate::parse_project`],
//! [`crate::parse_types`] et [`crate::parse_filters`].
//!
//! Les attributs écrits sont ceux que les lecteurs reconnaissent
//! (`docs/interop-formats.md` §2, §4 et §6) ; un aller-retour lecture → écriture →
//! lecture redonne le même modèle.

use rc_model::{
    CaptureRole, CaptureRule, DocumentType, ElementClass, ElementType, FilterSet, Project,
    ProjectNode, TypeKind,
};
use std::fmt::Write;

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c => out.push(c),
        }
    }
    out
}

fn attr(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"{}\"", escape(value));
}

fn opt(out: &mut String, name: &str, value: Option<&str>) {
    if let Some(v) = value {
        attr(out, name, v);
    }
}

/// Projet `.rqtf` (format XML).
pub fn project_xml(project: &Project) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Project>\n");
    for (k, v) in &project.properties {
        out.push_str("<Property");
        attr(&mut out, "name", k);
        attr(&mut out, "value", v);
        out.push_str(" />\n");
    }
    for node in &project.nodes {
        match node {
            ProjectNode::Document(d) => {
                out.push_str("<Document");
                attr(&mut out, "name", &d.name);
                let (x, y) = d.position.unwrap_or((0, 0));
                attr(&mut out, "positionX", &x.to_string());
                attr(&mut out, "positionY", &y.to_string());
                attr(&mut out, "type", &d.doc_type);
                attr(&mut out, "path", &d.path.to_string_lossy());
                opt(
                    &mut out,
                    "absolutePath",
                    d.absolute_path.as_deref().map(|p| p.to_str().unwrap_or("")),
                );
                opt(&mut out, "category", d.category.as_deref());
                out.push_str(">\n");
                for (k, v) in &d.params {
                    out.push_str("<Property");
                    attr(&mut out, "name", k);
                    attr(&mut out, "value", v);
                    out.push_str(" />\n");
                }
                for m in &d.modification_docs {
                    out.push_str("<ModificationDocument");
                    attr(&mut out, "type", &m.doc_type);
                    attr(&mut out, "path", &m.path.to_string_lossy());
                    out.push_str(" />\n");
                }
                for t in &d.ignored_types {
                    out.push_str("<IgnoredType");
                    attr(&mut out, "name", t);
                    out.push_str(" />\n");
                }
                out.push_str("</Document>\n");
            }
            ProjectNode::SubProject { name, path } => {
                out.push_str("<ProjectReference");
                attr(&mut out, "name", name);
                attr(&mut out, "type", "_ProjectReference");
                attr(&mut out, "path", &path.to_string_lossy());
                out.push_str(" />\n");
            }
            // Les dossiers n'ont pas de forme XML observée : leurs documents
            // sont écrits à plat.
            ProjectNode::Folder { .. } => {}
        }
    }
    for c in &project.channels {
        out.push_str("<Cover");
        attr(&mut out, "outputDocument", project.nodes[c.covered].name());
        out.push_str(">\n");
        for &k in &c.covering {
            out.push_str("<InputDocument");
            attr(&mut out, "name", project.nodes[k].name());
            out.push_str(" />\n");
        }
        out.push_str("</Cover>\n");
    }
    for r in &project.path_replacements {
        out.push_str("<ReplacingPath>");
        for c in &r.replacing {
            out.push_str("<Component");
            attr(&mut out, "name", c);
            out.push_str(" />");
        }
        for replaced in &r.replaced {
            out.push_str("<ReplacedPath>");
            for c in replaced {
                out.push_str("<Component");
                attr(&mut out, "name", c);
                out.push_str(" />");
            }
            out.push_str("</ReplacedPath>");
        }
        out.push_str("</ReplacingPath>\n");
    }
    out.push_str("</Project>\n");
    out
}

fn class_name(class: ElementClass) -> &'static str {
    match class {
        ElementClass::Section => "Section",
        ElementClass::MacroRequirement => "MacroRequirement",
        ElementClass::Requirement => "Requirement",
        ElementClass::Entity => "Entity",
        ElementClass::Reference => "CoverLink",
        ElementClass::Attribute => "Attribute",
        ElementClass::ReferenceAttribute => "LinkAttribute",
        ElementClass::Link => "Link",
        ElementClass::Value => "Value",
        ElementClass::Text => "Text",
        ElementClass::Picture => "Picture",
        ElementClass::Table => "Table",
    }
}

fn role_code(role: CaptureRole) -> u32 {
    match role {
        CaptureRole::None => 0,
        CaptureRole::Ident => 1,
        CaptureRole::Label => 2,
        CaptureRole::Text => 3,
        CaptureRole::Guid => 4,
        CaptureRole::Parent => 5,
        CaptureRole::Depth => 6,
    }
}

fn element_type(out: &mut String, e: &ElementType) {
    let terminal = matches!(
        e.class,
        ElementClass::Text | ElementClass::Picture | ElementClass::Table
    );
    out.push_str(if terminal {
        "<TerminalType"
    } else {
        "<ElementType"
    });
    attr(out, "name", &e.name);
    attr(out, "elementClass", class_name(e.class));
    match &e.rule {
        CaptureRule::Regex {
            expression,
            sub_expression,
            end_expression,
            row_expression,
        } => {
            attr(out, "regularExpression", expression);
            opt(out, "subExpression", sub_expression.as_deref());
            opt(out, "endExpression", end_expression.as_deref());
            opt(out, "reRow", row_expression.as_deref());
        }
        CaptureRule::XmlTree {
            pattern,
            sub_expression,
        } => {
            attr(out, "analysisTree", pattern);
            opt(out, "subExpression", sub_expression.as_deref());
        }
        CaptureRule::None => {}
    }
    opt(out, "printExpression", e.display.as_deref());
    let f = &e.flags;
    if f.hidden {
        attr(out, "isVisible", "false");
    }
    for (on, name) in [
        (f.ignored, "isIgnored"),
        (f.multiple, "isMultiple"),
        (f.nested, "isNested"),
        (f.inverse, "isInverse"),
        (f.boolean, "isBoolean"),
        (f.identify_children, "identifyChildren"),
    ] {
        if on {
            attr(out, name, "true");
        }
    }
    for (k, role) in e.roles.iter().take(4).enumerate() {
        attr(
            out,
            &format!("index{}", k + 1),
            &role_code(*role).to_string(),
        );
    }
    let column = e
        .properties
        .iter()
        .find(|(k, _)| k == "columnIndex")
        .map(|(_, v)| v.as_str());
    opt(out, "columnIndex", column);
    out.push_str(">\n");
    for p in &e.parents {
        out.push_str("<ParentType");
        attr(out, "name", p);
        out.push_str(" />\n");
    }
    for v in &e.enum_values {
        out.push_str("<EnumValue");
        attr(out, "value", v);
        out.push_str(" />\n");
    }
    for (k, v) in e.properties.iter().filter(|(k, _)| k != "columnIndex") {
        out.push_str("<Property");
        attr(out, "name", k);
        attr(out, "value", v);
        out.push_str(" />\n");
    }
    out.push_str(if terminal {
        "</TerminalType>\n"
    } else {
        "</ElementType>\n"
    });
}

/// Fichier `.types` (format XML).
pub fn types_xml(types: &[DocumentType]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types>\n");
    for t in types {
        out.push_str("<DocumentType");
        attr(&mut out, "name", &t.name);
        match t.kind {
            TypeKind::Xml => attr(&mut out, "kind", "xml"),
            TypeKind::AddedElements => attr(&mut out, "kind", "automatic"),
            TypeKind::Text => {}
        }
        attr(&mut out, "convertTool", &t.converter);
        opt(&mut out, "editTool", t.edit_tool.as_deref());
        opt(&mut out, "reDelete", t.delete_expression.as_deref());
        out.push_str(">\n");
        for (tag, values) in [
            ("FileExtension", &t.file_extensions),
            ("MatchingFileName", &t.matching_file_names),
            ("ExcludedFileName", &t.excluded_file_names),
            ("ExcludingAT", &t.exclude_text),
        ] {
            for v in values {
                let _ = writeln!(out, "<{tag} value=\"{}\" />", escape(v));
            }
        }
        for (k, v) in &t.properties {
            out.push_str("<Property");
            attr(&mut out, "name", k);
            attr(&mut out, "value", v);
            out.push_str(" />\n");
        }
        for v in &t.variables {
            out.push_str("<Variable");
            attr(&mut out, "name", &v.name);
            opt(&mut out, "label", v.label.as_deref());
            opt(&mut out, "type", v.var_type.as_deref());
            out.push_str(" />\n");
        }
        for e in &t.element_types {
            element_type(&mut out, e);
        }
        out.push_str("</DocumentType>\n");
    }
    out.push_str("</Types>\n");
    out
}

/// Fichier `.filters` (format XML). L'attribut `isAnd` n'est pas observé dans
/// les fichiers d'exemple : il est écrit seulement pour une combinaison ET.
pub fn filters_xml(set: &FilterSet) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Filters>\n");
    for f in &set.filters {
        out.push_str("<Filter");
        attr(&mut out, "name", &f.name);
        if f.all {
            attr(&mut out, "isAnd", "true");
        }
        out.push_str(">\n");
        for c in &f.conditions {
            out.push_str("<Condition");
            opt(&mut out, "applicationClass", c.application_class.as_deref());
            attr(&mut out, "operator", &c.operator);
            opt(&mut out, "value", c.value.as_deref());
            attr(&mut out, "receiverClass", &c.receiver_class);
            opt(
                &mut out,
                "receiverDocumentType",
                c.receiver_document_type.as_deref(),
            );
            opt(
                &mut out,
                "receiverElementType",
                c.receiver_element_type.as_deref(),
            );
            out.push_str(" />\n");
        }
        out.push_str("</Filter>\n");
    }
    out.push_str("</Filters>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_filters, parse_project, parse_types};
    use rc_model::{
        Channel, ChannelKind, DocumentDecl, Filter, FilterCondition, ModificationDocRef,
    };
    use std::path::PathBuf;

    #[test]
    fn projects_round_trip() {
        let project = Project {
            name: "demo".into(),
            properties: vec![("Feature".into(), "a<b".into())],
            nodes: vec![
                ProjectNode::Document(DocumentDecl {
                    name: "Spec \"A\"".into(),
                    doc_type: "Word".into(),
                    path: PathBuf::from("docs/spec & co.docx"),
                    category: Some("Requirements".into()),
                    params: vec![("withImages".into(), "1".into())],
                    modification_docs: vec![ModificationDocRef {
                        path: PathBuf::from("m.xml"),
                        doc_type: "Added".into(),
                    }],
                    ignored_types: vec!["Note".into()],
                    position: Some((10, -20)),
                    ..Default::default()
                }),
                ProjectNode::Document(DocumentDecl {
                    name: "Design".into(),
                    doc_type: "Text".into(),
                    path: PathBuf::from("design.txt"),
                    position: Some((1, 2)),
                    ..Default::default()
                }),
            ],
            channels: vec![Channel {
                kind: ChannelKind::Cover,
                covered: 0,
                covering: vec![1],
                category: None,
            }],
            path_replacements: Vec::new(),
        };
        let (back, warnings) = parse_project(&project_xml(&project), "demo").unwrap();
        assert!(warnings.is_empty());
        assert_eq!(back, project);
    }

    #[test]
    fn types_round_trip() {
        let mut t = DocumentType::new("Mine", TypeKind::Text, "text");
        t.file_extensions = vec!["txt".into()];
        t.exclude_text = vec!["/\\*[\\s\\S]*?\\*/".into()];
        let mut req = ElementType::new(
            "Req",
            ElementClass::Requirement,
            CaptureRule::regex("^(LL-REQ_\\d+)\\s*:\\s*(.*)$"),
        );
        req.roles = vec![CaptureRole::Ident, CaptureRole::Label];
        req.parents = vec!["Mine".into()];
        let mut cov = ElementType::new(
            "Covers",
            ElementClass::Reference,
            CaptureRule::Regex {
                expression: "\\[Covers:([^\\]]*)\\]".into(),
                sub_expression: Some("([^,\\s]+)".into()),
                end_expression: None,
                row_expression: None,
            },
        );
        cov.flags.multiple = true;
        let text = ElementType::new(
            "Text",
            ElementClass::Text,
            CaptureRule::regex("<<([\\s\\S]+?)>>"),
        );
        t.element_types = vec![req, cov, text];
        let back = parse_types(&types_xml(std::slice::from_ref(&t))).unwrap();
        assert_eq!(back, vec![t]);
    }

    #[test]
    fn filters_round_trip() {
        let set = FilterSet {
            filters: vec![Filter {
                name: "High".into(),
                all: true,
                conditions: vec![FilterCondition {
                    application_class: Some("AbstractRequirement".into()),
                    receiver_class: "Attribute".into(),
                    receiver_document_type: Some("Text".into()),
                    receiver_element_type: Some("Priority".into()),
                    operator: "equal".into(),
                    value: Some("$level".into()),
                }],
                raw_flags: Vec::new(),
            }],
            current: None,
        };
        assert_eq!(parse_filters(&filters_xml(&set)).unwrap(), set);
    }
}
