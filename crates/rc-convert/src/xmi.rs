//! Modèles UML/SysML échangés en XMI (`.xmi`, `.uml`, et `.mdzip` : archive
//! d'un projet Magic Cyber Systems Engineer / Cameo) → XML intermédiaire.
//!
//! Le XMI d'origine est conservé tel quel, pour les types d'analyse qui le lisent
//! directement (`uml:Model`, `packagedElement xmi:type="uml:Class"`…), et il est
//! suivi d'un résumé où les stéréotypes et les relations sont déjà résolus :
//!
//! ```xml
//! <XmiDocument>
//!   …XMI d'origine…
//!   <Summary>
//!     <Element id="…" type="Class" name="…" stereotype="Requirement" ident="REQ1" text="…" owner="…">
//!       <Relation kind="Satisfy" target="<ident de la cible>"/>
//!     </Element>
//!   </Summary>
//! </XmiDocument>
//! ```
//!
//! `ident` vaut l'attribut `Id` du stéréotype SysML `Requirement`, sinon le nom.
//! Une relation (abstraction, dépendance, réalisation) est rangée sous son
//! client et désigne son fournisseur ; `kind` est son stéréotype (`Satisfy`,
//! `Verify`, `DeriveReqt`, `Refine`, `Trace`…) ou, à défaut, son type UML.
//! Sources : spécifications OMG XMI 2.5.1, UML 2.5.1 et SysML 1.6.

use crate::archive::Archive;
use crate::{xml_escape, ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let text = if ext == "mdzip" || ext == "zip" {
        let mut archive = Archive::open(path)?;
        let mut best: Option<String> = None;
        for name in archive.names() {
            let Some(content) = archive.text(&name)? else {
                continue;
            };
            if content.contains("XMI")
                && content.contains("uml:Model")
                && best.as_ref().is_none_or(|b| content.len() > b.len())
            {
                best = Some(content);
            }
        }
        best.ok_or_else(|| ConvertError::format(path, "aucun modèle XMI dans l'archive"))?
    } else {
        crate::read_text(path)?
    };
    convert_str(&text).map_err(|e| ConvertError::format(path, e))
}

/// Valeur d'un attribut XMI (`xmi:id`, `xmi:type`, `xmi:idref`) quel que soit
/// l'espace de noms XMI utilisé.
fn xmi_attr<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| {
            a.name() == name
                && a.namespace()
                    .is_some_and(|ns| ns.to_ascii_lowercase().contains("xmi"))
        })
        .map(|a| a.value())
}

/// Valeur d'une propriété portée en attribut ou en élément enfant.
fn property<'a>(node: Node<'a, 'a>, name: &str) -> Option<String> {
    node.attributes()
        .find(|a| a.name() == name && a.namespace().is_none())
        .map(|a| a.value().to_owned())
        .or_else(|| {
            node.children()
                .find(|c| c.is_element() && c.tag_name().name() == name)
                .map(|c| {
                    c.descendants()
                        .filter(|n| n.is_text())
                        .filter_map(|n| n.text())
                        .collect::<String>()
                })
        })
}

/// Références multiples : attribut `client="id1 id2"` ou éléments `<client xmi:idref>`.
fn references(node: Node, name: &str) -> Vec<String> {
    let mut out: Vec<String> = node
        .attribute(name)
        .map(|v| v.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    for c in node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == name)
    {
        if let Some(r) = xmi_attr(c, "idref").or_else(|| {
            c.attribute("href")
                .map(|h| h.rsplit('#').next().unwrap_or(h))
        }) {
            out.push(r.to_owned());
        }
    }
    out
}

#[derive(Default)]
struct Element {
    kind: String,
    name: String,
    owner: Option<String>,
    stereotype: Option<String>,
    ident: Option<String>,
    text: Option<String>,
    relations: Vec<(String, String)>,
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

    let mut order: Vec<String> = Vec::new();
    let mut elements: HashMap<String, Element> = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let Some(id) = xmi_attr(n, "id") else {
            continue;
        };
        // Les applications de stéréotypes sont traitées plus bas.
        if n.attributes().any(|a| a.name().starts_with("base_")) {
            continue;
        }
        let kind = xmi_attr(n, "type")
            .map(|t| t.rsplit(':').next().unwrap_or(t).to_owned())
            .unwrap_or_else(|| n.tag_name().name().to_owned());
        let owner = n
            .ancestors()
            .skip(1)
            .find_map(|a| a.is_element().then(|| xmi_attr(a, "id")).flatten())
            .map(str::to_owned);
        order.push(id.to_owned());
        elements.insert(
            id.to_owned(),
            Element {
                kind,
                name: n.attribute("name").unwrap_or("").to_owned(),
                owner,
                ..Default::default()
            },
        );
    }
    // Stéréotypes : `<sysml:Requirement base_Class="id" Id="R1" Text="…"/>`.
    for n in doc.descendants().filter(|n| n.is_element()) {
        let Some(base) = n
            .attributes()
            .find(|a| a.name().starts_with("base_"))
            .map(|a| a.value().to_owned())
        else {
            continue;
        };
        let Some(e) = elements.get_mut(&base) else {
            continue;
        };
        let stereotype = n.tag_name().name().to_owned();
        if stereotype.eq_ignore_ascii_case("Requirement") || e.stereotype.is_none() {
            if let Some(id) = property(n, "Id").or_else(|| property(n, "id")) {
                e.ident = Some(id);
            }
            if let Some(t) = property(n, "Text").or_else(|| property(n, "text")) {
                e.text = Some(t);
            }
            e.stereotype = Some(stereotype);
        }
    }
    // Relations : du client vers le fournisseur.
    let mut relations: Vec<(String, String, String)> = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let Some(id) = xmi_attr(n, "id") else {
            continue;
        };
        let Some(e) = elements.get(id) else { continue };
        if !matches!(
            e.kind.as_str(),
            "Abstraction" | "Dependency" | "Realization" | "Usage" | "Trace"
        ) {
            continue;
        }
        let kind = e.stereotype.clone().unwrap_or_else(|| e.kind.clone());
        for client in references(n, "client") {
            for supplier in references(n, "supplier") {
                relations.push((client.clone(), kind.clone(), supplier));
            }
        }
    }
    let ident_of = |id: &str| -> String {
        elements
            .get(id)
            .map(|e| e.ident.clone().unwrap_or_else(|| e.name.clone()))
            .unwrap_or_else(|| id.to_owned())
    };
    let resolved: Vec<(String, String, String)> = relations
        .into_iter()
        .map(|(c, k, s)| {
            let target = ident_of(&s);
            (c, k, target)
        })
        .collect();
    for (client, kind, target) in resolved {
        if let Some(e) = elements.get_mut(&client) {
            e.relations.push((kind, target));
        }
    }

    let body = text.trim_start_matches('\u{feff}').trim_start();
    let body = if body.starts_with("<?xml") {
        body.find("?>").map(|i| &body[i + 2..]).unwrap_or(body)
    } else {
        body
    };
    let body = strip_doctype(body);
    let mut out = format!("<XmiDocument>{body}<Summary>");
    for id in &order {
        let e = &elements[id];
        out.push_str(&format!(
            "<Element id=\"{}\" type=\"{}\" name=\"{}\" stereotype=\"{}\" ident=\"{}\" text=\"{}\" owner=\"{}\">",
            xml_escape(id),
            xml_escape(&e.kind),
            xml_escape(&e.name),
            xml_escape(e.stereotype.as_deref().unwrap_or("")),
            xml_escape(e.ident.as_deref().unwrap_or(&e.name)),
            xml_escape(e.text.as_deref().unwrap_or("")),
            xml_escape(e.owner.as_deref().unwrap_or("")),
        ));
        for (kind, target) in &e.relations {
            out.push_str(&format!(
                "<Relation kind=\"{}\" target=\"{}\"/>",
                xml_escape(kind),
                xml_escape(target)
            ));
        }
        out.push_str("</Element>");
    }
    out.push_str("</Summary></XmiDocument>");
    Ok(out)
}

fn strip_doctype(xml: &str) -> &str {
    let t = xml.trim_start();
    if t.starts_with("<!DOCTYPE") {
        if let Some(i) = t.find('>') {
            return &t[i + 1..];
        }
    }
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_requirements_and_relations() {
        let xmi = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmi:XMI xmlns:xmi="http://www.omg.org/spec/XMI/20131001" xmlns:uml="http://www.omg.org/spec/UML/20161101" xmlns:sysml="http://www.omg.org/spec/SysML/20181001/SysML">
 <uml:Model xmi:id="m" name="Thermostat">
  <packagedElement xmi:type="uml:Class" xmi:id="r1" name="Température"/>
  <packagedElement xmi:type="uml:Class" xmi:id="b1" name="Régulateur"/>
  <packagedElement xmi:type="uml:Abstraction" xmi:id="s1" client="b1" supplier="r1"/>
 </uml:Model>
 <sysml:Requirement xmi:id="st1" base_Class="r1" Id="THS_REQ1" Text="Mesurer la température"/>
 <sysml:Block xmi:id="st2" base_Class="b1"/>
 <sysml:Satisfy xmi:id="st3" base_Abstraction="s1"/>
</xmi:XMI>"#;
        let out = convert_str(xmi).unwrap();
        assert!(out.starts_with("<XmiDocument>\n<xmi:XMI"));
        let summary = &out[out.find("<Summary>").unwrap()..];
        assert_eq!(
            summary,
            "<Summary>\
             <Element id=\"m\" type=\"Model\" name=\"Thermostat\" stereotype=\"\" ident=\"Thermostat\" text=\"\" owner=\"\"></Element>\
             <Element id=\"r1\" type=\"Class\" name=\"Température\" stereotype=\"Requirement\" ident=\"THS_REQ1\" text=\"Mesurer la température\" owner=\"m\"></Element>\
             <Element id=\"b1\" type=\"Class\" name=\"Régulateur\" stereotype=\"Block\" ident=\"Régulateur\" text=\"\" owner=\"m\"><Relation kind=\"Satisfy\" target=\"THS_REQ1\"/></Element>\
             <Element id=\"s1\" type=\"Abstraction\" name=\"\" stereotype=\"Satisfy\" ident=\"\" text=\"\" owner=\"m\"></Element>\
             </Summary></XmiDocument>"
        );
    }
}
