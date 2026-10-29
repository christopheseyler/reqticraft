//! ReqIF 1.x (`.reqif`, et `.reqifz` : archive ZIP de fichiers ReqIF) → XML
//! intermédiaire propre à ReqtiCraft, une fois les références résolues :
//!
//! ```xml
//! <ReqIF title="…">
//!   <Specification id="…" name="…" type="…">
//!     <Object id="…" ident="…" kind="heading|requirement" type="…" label="…" text="…">
//!       <Attribute name="…" value="…"/>
//!       <Link type="…" target="…"/>     <!-- relation dont l'objet est la source -->
//!       <Object …>…</Object>            <!-- hiérarchie de la spécification -->
//!     </Object>
//!   </Specification>
//! </ReqIF>
//! ```
//!
//! `ident` vaut `ReqIF.ForeignID` s'il existe, sinon l'identifiant ReqIF ;
//! `label` vaut `ReqIF.ChapterName` ou `ReqIF.Name` ; `text` vaut `ReqIF.Text`.
//! Un objet est un titre (`kind="heading"`) s'il a un `ReqIF.ChapterName` et
//! pas de `ReqIF.Text`. Les valeurs XHTML sont réduites à leur texte.
//! Format d'échange : spécification OMG ReqIF 1.2 (formal/16-07-01).

use crate::archive::Archive;
use crate::{xml_escape, ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    let is_archive = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("reqifz"));
    if !is_archive {
        let text = crate::read_text(path)?;
        return convert_str(&text).map_err(|e| ConvertError::format(path, e));
    }
    let mut archive = Archive::open(path)?;
    let mut names: Vec<String> = archive
        .names()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".reqif"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err(ConvertError::format(
            path,
            "aucun fichier .reqif dans l'archive",
        ));
    }
    let mut out = String::from("<ReqIFArchive>");
    for name in names {
        let text = archive.require(&name)?;
        let xml =
            convert_str(&text).map_err(|e| ConvertError::format(path, format!("{name} : {e}")))?;
        out.push_str(&xml.replacen(
            "<ReqIF ",
            &format!("<ReqIF file=\"{}\" ", xml_escape(&name)),
            1,
        ));
    }
    out.push_str("</ReqIFArchive>");
    Ok(out)
}

/// Objet de spécification lu.
#[derive(Default)]
struct Object {
    type_name: String,
    long_name: Option<String>,
    attributes: Vec<(String, String)>,
}

impl Object {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.trim().is_empty())
    }
}

fn local<'a>(node: &Node<'a, 'a>) -> &'a str {
    node.tag_name().name()
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|c| c.is_element() && local(c) == name)
}

fn children<'a>(node: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> + 'a {
    node.children()
        .filter(move |c| c.is_element() && local(c) == name)
}

/// Texte de l'unique référence contenue (`<TYPE><SPEC-OBJECT-TYPE-REF>id</…></TYPE>`).
fn reference(node: Node, container: &str) -> Option<String> {
    let c = child(node, container)?;
    c.descendants()
        .find(|n| n.is_element() && local(n).ends_with("-REF"))
        .and_then(|n| n.text())
        .map(|t| t.trim().to_owned())
}

pub fn convert_str(text: &str) -> Result<String, String> {
    let doc = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if local(&root) != "REQ-IF" {
        return Err("élément racine REQ-IF attendu".into());
    }
    // Noms des types, des définitions d'attributs et des valeurs énumérées.
    let mut names: HashMap<String, String> = HashMap::new();
    for n in root.descendants().filter(|n| n.is_element()) {
        if let (Some(id), Some(name)) = (n.attribute("IDENTIFIER"), n.attribute("LONG-NAME")) {
            names.insert(id.to_owned(), name.to_owned());
        }
    }
    let name_of = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());

    let mut objects: HashMap<String, Object> = HashMap::new();
    for so in root
        .descendants()
        .filter(|n| n.is_element() && local(n) == "SPEC-OBJECT")
    {
        let Some(id) = so.attribute("IDENTIFIER") else {
            continue;
        };
        let mut object = Object {
            type_name: reference(so, "TYPE")
                .map(|t| name_of(&t))
                .unwrap_or_default(),
            long_name: so.attribute("LONG-NAME").map(str::to_owned),
            attributes: Vec::new(),
        };
        if let Some(values) = child(so, "VALUES") {
            for v in values.children().filter(|n| n.is_element()) {
                let Some(def) = reference(v, "DEFINITION") else {
                    continue;
                };
                let value = match local(&v) {
                    "ATTRIBUTE-VALUE-XHTML" => {
                        child(v, "THE-VALUE").map(xhtml_text).unwrap_or_default()
                    }
                    "ATTRIBUTE-VALUE-ENUMERATION" => child(v, "VALUES")
                        .map(|vals| {
                            vals.descendants()
                                .filter(|n| n.is_element() && local(n) == "ENUM-VALUE-REF")
                                .filter_map(|n| n.text())
                                .map(|t| name_of(t.trim()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default(),
                    _ => v.attribute("THE-VALUE").unwrap_or("").to_owned(),
                };
                object.attributes.push((name_of(&def), value));
            }
        }
        objects.insert(id.to_owned(), object);
    }
    let ident = |id: &str| -> String {
        objects
            .get(id)
            .and_then(|o| o.attr("ReqIF.ForeignID"))
            .map(str::to_owned)
            .unwrap_or_else(|| id.to_owned())
    };

    // Relations : rangées par source.
    let mut links: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for rel in root
        .descendants()
        .filter(|n| n.is_element() && local(n) == "SPEC-RELATION")
    {
        let (Some(source), Some(target)) = (reference(rel, "SOURCE"), reference(rel, "TARGET"))
        else {
            continue;
        };
        let kind = reference(rel, "TYPE")
            .map(|t| name_of(&t))
            .unwrap_or_default();
        links
            .entry(source)
            .or_default()
            .push((kind, ident(&target)));
    }

    let title = root
        .descendants()
        .find(|n| n.is_element() && local(n) == "TITLE")
        .and_then(|n| n.text())
        .unwrap_or("");
    let mut out = format!("<ReqIF title=\"{}\">", xml_escape(title));
    for spec in root
        .descendants()
        .filter(|n| n.is_element() && local(n) == "SPECIFICATION")
    {
        out.push_str(&format!(
            "<Specification id=\"{}\" name=\"{}\" type=\"{}\">",
            xml_escape(spec.attribute("IDENTIFIER").unwrap_or("")),
            xml_escape(spec.attribute("LONG-NAME").unwrap_or("")),
            xml_escape(
                &reference(spec, "TYPE")
                    .map(|t| name_of(&t))
                    .unwrap_or_default()
            ),
        ));
        if let Some(c) = child(spec, "CHILDREN") {
            hierarchy(c, &objects, &links, &ident, &mut out);
        }
        out.push_str("</Specification>");
    }
    out.push_str("</ReqIF>");
    Ok(out)
}

fn hierarchy(
    children_node: Node,
    objects: &HashMap<String, Object>,
    links: &HashMap<String, Vec<(String, String)>>,
    ident: &dyn Fn(&str) -> String,
    out: &mut String,
) {
    for h in children(children_node, "SPEC-HIERARCHY") {
        let Some(id) = reference(h, "OBJECT") else {
            continue;
        };
        let empty = Object::default();
        let o = objects.get(&id).unwrap_or(&empty);
        let chapter = o.attr("ReqIF.ChapterName");
        let text = o.attr("ReqIF.Text");
        let kind = if chapter.is_some() && text.is_none() {
            "heading"
        } else {
            "requirement"
        };
        let label = chapter
            .or_else(|| o.attr("ReqIF.Name"))
            .or(o.long_name.as_deref())
            .unwrap_or("");
        out.push_str(&format!(
            "<Object id=\"{}\" ident=\"{}\" kind=\"{kind}\" type=\"{}\" label=\"{}\" text=\"{}\">",
            xml_escape(&id),
            xml_escape(&ident(&id)),
            xml_escape(&o.type_name),
            xml_escape(label),
            xml_escape(text.unwrap_or("")),
        ));
        for (name, value) in &o.attributes {
            out.push_str(&format!(
                "<Attribute name=\"{}\" value=\"{}\"/>",
                xml_escape(name),
                xml_escape(value)
            ));
        }
        for (kind, target) in links.get(&id).into_iter().flatten() {
            out.push_str(&format!(
                "<Link type=\"{}\" target=\"{}\"/>",
                xml_escape(kind),
                xml_escape(target)
            ));
        }
        if let Some(c) = child(h, "CHILDREN") {
            hierarchy(c, objects, links, ident, out);
        }
        out.push_str("</Object>");
    }
}

/// Texte d'une valeur XHTML : blocs séparés par une espace.
fn xhtml_text(node: Node) -> String {
    let mut parts = Vec::new();
    collect(node, &mut parts);
    parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect(node: Node, out: &mut Vec<String>) {
    for c in node.children() {
        if c.is_text() {
            if let Some(t) = c.text() {
                match out.last_mut() {
                    Some(last) => last.push_str(t),
                    None => out.push(t.to_owned()),
                }
            }
        } else if c.is_element() {
            let block = matches!(
                local(&c),
                "p" | "div" | "li" | "br" | "tr" | "td" | "h1" | "h2" | "h3"
            );
            if block {
                out.push(String::new());
            }
            collect(c, out);
            if block {
                out.push(String::new());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd" xmlns:xhtml="http://www.w3.org/1999/xhtml">
 <THE-HEADER><REQ-IF-HEADER IDENTIFIER="h"><TITLE>Thermostat</TITLE></REQ-IF-HEADER></THE-HEADER>
 <CORE-CONTENT><REQ-IF-CONTENT>
  <DATATYPES>
   <DATATYPE-DEFINITION-STRING IDENTIFIER="dt-s" LONG-NAME="T_String"/>
   <DATATYPE-DEFINITION-ENUMERATION IDENTIFIER="dt-p" LONG-NAME="T_Prio"><SPECIFIED-VALUES>
    <ENUM-VALUE IDENTIFIER="ev-h" LONG-NAME="High"/></SPECIFIED-VALUES></DATATYPE-DEFINITION-ENUMERATION>
  </DATATYPES>
  <SPEC-TYPES>
   <SPEC-OBJECT-TYPE IDENTIFIER="t-req" LONG-NAME="Requirement"><SPEC-ATTRIBUTES>
    <ATTRIBUTE-DEFINITION-STRING IDENTIFIER="a-id" LONG-NAME="ReqIF.ForeignID"/>
    <ATTRIBUTE-DEFINITION-STRING IDENTIFIER="a-ch" LONG-NAME="ReqIF.ChapterName"/>
    <ATTRIBUTE-DEFINITION-XHTML IDENTIFIER="a-tx" LONG-NAME="ReqIF.Text"/>
    <ATTRIBUTE-DEFINITION-ENUMERATION IDENTIFIER="a-pr" LONG-NAME="Priority"/>
   </SPEC-ATTRIBUTES></SPEC-OBJECT-TYPE>
   <SPEC-RELATION-TYPE IDENTIFIER="t-sat" LONG-NAME="Satisfies"/>
   <SPECIFICATION-TYPE IDENTIFIER="t-spec" LONG-NAME="Spec"/>
  </SPEC-TYPES>
  <SPEC-OBJECTS>
   <SPEC-OBJECT IDENTIFIER="o1"><TYPE><SPEC-OBJECT-TYPE-REF>t-req</SPEC-OBJECT-TYPE-REF></TYPE><VALUES>
    <ATTRIBUTE-VALUE-STRING THE-VALUE="Exigences"><DEFINITION><ATTRIBUTE-DEFINITION-STRING-REF>a-ch</ATTRIBUTE-DEFINITION-STRING-REF></DEFINITION></ATTRIBUTE-VALUE-STRING>
   </VALUES></SPEC-OBJECT>
   <SPEC-OBJECT IDENTIFIER="o2"><TYPE><SPEC-OBJECT-TYPE-REF>t-req</SPEC-OBJECT-TYPE-REF></TYPE><VALUES>
    <ATTRIBUTE-VALUE-STRING THE-VALUE="REQ-1"><DEFINITION><ATTRIBUTE-DEFINITION-STRING-REF>a-id</ATTRIBUTE-DEFINITION-STRING-REF></DEFINITION></ATTRIBUTE-VALUE-STRING>
    <ATTRIBUTE-VALUE-XHTML><DEFINITION><ATTRIBUTE-DEFINITION-XHTML-REF>a-tx</ATTRIBUTE-DEFINITION-XHTML-REF></DEFINITION>
     <THE-VALUE><xhtml:div><xhtml:p>Le système <xhtml:b>doit</xhtml:b></xhtml:p><xhtml:p>démarrer.</xhtml:p></xhtml:div></THE-VALUE></ATTRIBUTE-VALUE-XHTML>
    <ATTRIBUTE-VALUE-ENUMERATION><DEFINITION><ATTRIBUTE-DEFINITION-ENUMERATION-REF>a-pr</ATTRIBUTE-DEFINITION-ENUMERATION-REF></DEFINITION><VALUES><ENUM-VALUE-REF>ev-h</ENUM-VALUE-REF></VALUES></ATTRIBUTE-VALUE-ENUMERATION>
   </VALUES></SPEC-OBJECT>
   <SPEC-OBJECT IDENTIFIER="o3"><TYPE><SPEC-OBJECT-TYPE-REF>t-req</SPEC-OBJECT-TYPE-REF></TYPE><VALUES>
    <ATTRIBUTE-VALUE-STRING THE-VALUE="SYS-9"><DEFINITION><ATTRIBUTE-DEFINITION-STRING-REF>a-id</ATTRIBUTE-DEFINITION-STRING-REF></DEFINITION></ATTRIBUTE-VALUE-STRING>
   </VALUES></SPEC-OBJECT>
  </SPEC-OBJECTS>
  <SPEC-RELATIONS>
   <SPEC-RELATION IDENTIFIER="r1"><TYPE><SPEC-RELATION-TYPE-REF>t-sat</SPEC-RELATION-TYPE-REF></TYPE>
    <SOURCE><SPEC-OBJECT-REF>o2</SPEC-OBJECT-REF></SOURCE><TARGET><SPEC-OBJECT-REF>o3</SPEC-OBJECT-REF></TARGET></SPEC-RELATION>
  </SPEC-RELATIONS>
  <SPECIFICATIONS>
   <SPECIFICATION IDENTIFIER="s1" LONG-NAME="Spécification"><TYPE><SPECIFICATION-TYPE-REF>t-spec</SPECIFICATION-TYPE-REF></TYPE><CHILDREN>
    <SPEC-HIERARCHY IDENTIFIER="h1"><OBJECT><SPEC-OBJECT-REF>o1</SPEC-OBJECT-REF></OBJECT><CHILDREN>
     <SPEC-HIERARCHY IDENTIFIER="h2"><OBJECT><SPEC-OBJECT-REF>o2</SPEC-OBJECT-REF></OBJECT></SPEC-HIERARCHY>
    </CHILDREN></SPEC-HIERARCHY>
   </CHILDREN></SPECIFICATION>
  </SPECIFICATIONS>
 </REQ-IF-CONTENT></CORE-CONTENT>
</REQ-IF>"#;

    #[test]
    fn resolves_objects_hierarchy_and_relations() {
        assert_eq!(
            convert_str(SAMPLE).unwrap(),
            "<ReqIF title=\"Thermostat\"><Specification id=\"s1\" name=\"Spécification\" type=\"Spec\">\
             <Object id=\"o1\" ident=\"o1\" kind=\"heading\" type=\"Requirement\" label=\"Exigences\" text=\"\">\
             <Attribute name=\"ReqIF.ChapterName\" value=\"Exigences\"/>\
             <Object id=\"o2\" ident=\"REQ-1\" kind=\"requirement\" type=\"Requirement\" label=\"\" text=\"Le système doit démarrer.\">\
             <Attribute name=\"ReqIF.ForeignID\" value=\"REQ-1\"/>\
             <Attribute name=\"ReqIF.Text\" value=\"Le système doit démarrer.\"/>\
             <Attribute name=\"Priority\" value=\"High\"/>\
             <Link type=\"Satisfies\" target=\"SYS-9\"/></Object></Object></Specification></ReqIF>"
        );
    }
}
