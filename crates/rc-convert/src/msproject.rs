//! MS Project au format XML (MSPDI, « Enregistrer sous > XML ») → XML
//! intermédiaire de la forme attendue par les types MS Project
//! (`docs/interop-formats.md` §5) : une tâche par élément, ses ressources et
//! ses dépendances.
//!
//! ```xml
//! <Project name="…">
//!   <Task id="…" name="…" notes="…" text1="…" text2="…" percentcomplete="…" hyperlink="…" outlinenumber="…">
//!     <Resource name="…" emailaddress="…"/>
//!     <TaskDependency to="<tâche prédécesseur>"/>
//!   </Task>
//! </Project>
//! ```
//!
//! Le format binaire `.mpp` nécessite MS Project : l'enregistrer en XML.

use crate::{xml_escape, ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

/// Identifiants MSPDI des champs personnalisés Texte1 et Texte2.
const TEXT1: &str = "188743731";
const TEXT2: &str = "188743734";

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mpp"))
    {
        return Err(ConvertError::format(
            path,
            "format MS Project binaire (.mpp) non pris en charge : enregistrer le projet au format XML",
        ));
    }
    let text = crate::read_text(path)?;
    convert_str(&text).map_err(|e| ConvertError::format(path, e))
}

fn field<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.tag_name().name() == name)
        .and_then(|c| c.text())
}

pub fn convert_str(text: &str) -> Result<String, String> {
    let doc = Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err("élément racine Project (MSPDI) attendu".into());
    }
    let section = |name: &str| root.children().find(|c| c.tag_name().name() == name);
    let tasks: Vec<Node> = section("Tasks")
        .map(|t| {
            t.children()
                .filter(|c| c.tag_name().name() == "Task")
                .collect()
        })
        .unwrap_or_default();
    let mut task_names: HashMap<&str, &str> = HashMap::new();
    for t in &tasks {
        if let (Some(uid), Some(name)) = (field(*t, "UID"), field(*t, "Name")) {
            task_names.insert(uid, name);
        }
    }
    let mut resources: HashMap<&str, (&str, &str)> = HashMap::new();
    if let Some(rs) = section("Resources") {
        for r in rs.children().filter(|c| c.tag_name().name() == "Resource") {
            if let Some(uid) = field(r, "UID") {
                resources.insert(
                    uid,
                    (
                        field(r, "Name").unwrap_or(""),
                        field(r, "EmailAddress").unwrap_or(""),
                    ),
                );
            }
        }
    }
    let mut assignments: HashMap<&str, Vec<&str>> = HashMap::new();
    if let Some(a) = section("Assignments") {
        for a in a.children().filter(|c| c.tag_name().name() == "Assignment") {
            if let (Some(task), Some(res)) = (field(a, "TaskUID"), field(a, "ResourceUID")) {
                assignments.entry(task).or_default().push(res);
            }
        }
    }

    let mut out = format!(
        "<Project name=\"{}\">",
        xml_escape(
            field(root, "Title")
                .or_else(|| field(root, "Name"))
                .unwrap_or("")
        )
    );
    for t in tasks {
        let uid = field(t, "UID").unwrap_or("");
        // La tâche 0 est le récapitulatif du projet.
        if uid == "0" || field(t, "IsNull") == Some("1") {
            continue;
        }
        let ext = |id: &str| {
            t.children()
                .filter(|c| c.tag_name().name() == "ExtendedAttribute")
                .find(|c| field(*c, "FieldID") == Some(id))
                .and_then(|c| field(c, "Value"))
                .unwrap_or("")
        };
        out.push_str(&format!(
            "<Task id=\"{}\" name=\"{}\" notes=\"{}\" text1=\"{}\" text2=\"{}\" percentcomplete=\"{}\" hyperlink=\"{}\" outlinenumber=\"{}\">",
            xml_escape(field(t, "ID").unwrap_or(uid)),
            xml_escape(field(t, "Name").unwrap_or("")),
            xml_escape(field(t, "Notes").unwrap_or("")),
            xml_escape(ext(TEXT1)),
            xml_escape(ext(TEXT2)),
            xml_escape(field(t, "PercentComplete").unwrap_or("")),
            xml_escape(field(t, "HyperlinkAddress").or_else(|| field(t, "Hyperlink")).unwrap_or("")),
            xml_escape(field(t, "OutlineNumber").unwrap_or("")),
        ));
        for res in assignments.get(uid).into_iter().flatten() {
            if let Some((name, mail)) = resources.get(res) {
                out.push_str(&format!(
                    "<Resource name=\"{}\" emailaddress=\"{}\"/>",
                    xml_escape(name),
                    xml_escape(mail)
                ));
            }
        }
        for link in t
            .children()
            .filter(|c| c.tag_name().name() == "PredecessorLink")
        {
            if let Some(pred) = field(link, "PredecessorUID").and_then(|p| task_names.get(p)) {
                out.push_str(&format!("<TaskDependency to=\"{}\"/>", xml_escape(pred)));
            }
        }
        out.push_str("</Task>");
    }
    out.push_str("</Project>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tasks_resources_and_dependencies() {
        let xml = r#"<?xml version="1.0"?><Project xmlns="http://schemas.microsoft.com/project"><Title>Plan</Title>
          <Tasks>
            <Task><UID>0</UID><ID>0</ID><Name>Plan</Name></Task>
            <Task><UID>1</UID><ID>1</ID><Name>Valider THS_REQ1</Name><Notes>Covers_THS_REQ1</Notes><PercentComplete>50</PercentComplete>
              <ExtendedAttribute><FieldID>188743731</FieldID><Value>lot A</Value></ExtendedAttribute></Task>
            <Task><UID>2</UID><ID>2</ID><Name>Tester</Name><PredecessorLink><PredecessorUID>1</PredecessorUID></PredecessorLink></Task>
          </Tasks>
          <Resources><Resource><UID>7</UID><Name>Alice</Name><EmailAddress>a@x</EmailAddress></Resource></Resources>
          <Assignments><Assignment><TaskUID>1</TaskUID><ResourceUID>7</ResourceUID></Assignment></Assignments>
        </Project>"#;
        assert_eq!(
            convert_str(xml).unwrap(),
            "<Project name=\"Plan\">\
             <Task id=\"1\" name=\"Valider THS_REQ1\" notes=\"Covers_THS_REQ1\" text1=\"lot A\" text2=\"\" percentcomplete=\"50\" hyperlink=\"\" outlinenumber=\"\"><Resource name=\"Alice\" emailaddress=\"a@x\"/></Task>\
             <Task id=\"2\" name=\"Tester\" notes=\"\" text1=\"\" text2=\"\" percentcomplete=\"\" hyperlink=\"\" outlinenumber=\"\"><TaskDependency to=\"Valider THS_REQ1\"/></Task>\
             </Project>"
        );
    }
}
