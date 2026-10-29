//! Filtres (`.filters`), marques (`.marks`) et documents de modification.
//!
//! Formats : `docs/interop-formats.md` §6 et §7.

use crate::ini::{split_list, Ini};
use crate::text::sanitize_xml;
use crate::InteropError;
use rc_model::{Filter, FilterCondition, FilterSet, Mark, Modification};

pub fn parse_filters(text: &str) -> Result<FilterSet, InteropError> {
    if crate::text::looks_like_xml(text) {
        let fixed = sanitize_xml(text);
        let doc = roxmltree::Document::parse(&fixed)?;
        let filters = doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("Filter"))
            .map(|f| Filter {
                name: f.attribute("name").unwrap_or_default().to_owned(),
                // L'attribut de combinaison n'est pas observé : OU par défaut, comme en INI.
                all: matches!(f.attribute("isAnd"), Some("true" | "1")),
                conditions: f
                    .children()
                    .filter(|c| c.has_tag_name("Condition"))
                    .map(|c| FilterCondition {
                        application_class: c.attribute("applicationClass").map(str::to_owned),
                        receiver_class: c.attribute("receiverClass").unwrap_or_default().to_owned(),
                        receiver_document_type: c
                            .attribute("receiverDocumentType")
                            .map(str::to_owned),
                        receiver_element_type: c
                            .attribute("receiverElementType")
                            .map(str::to_owned),
                        operator: c.attribute("operator").unwrap_or_default().to_owned(),
                        value: c.attribute("value").map(str::to_owned),
                    })
                    .collect(),
                raw_flags: Vec::new(),
            })
            .collect();
        return Ok(FilterSet {
            filters,
            current: None,
        });
    }

    let ini = Ini::parse(text);
    let header = ini
        .section("Filters")
        .ok_or_else(|| InteropError::Format("section [Filters] absente".into()))?;
    let current = header
        .get_nonempty("Current")
        .filter(|c| *c != "(no filter)")
        .map(str::to_owned);
    let filters = split_list(header.get("Names").unwrap_or_default())
        .into_iter()
        .filter_map(|name| {
            let s = ini.section(&name)?;
            Some(Filter {
                all: s.flag("IsAnd"),
                raw_flags: s
                    .get("Booleans")
                    .map(|b| b.split(',').map(|x| x.trim() == "1").collect())
                    .unwrap_or_default(),
                conditions: s
                    .numbered("Cond", "")
                    .into_iter()
                    .map(|(_, v)| ini_condition(v))
                    .collect(),
                name,
            })
        })
        .collect();
    Ok(FilterSet { filters, current })
}

/// `Attribute.<type de document>.<type d'élément>,<opérateur>,<valeur>`.
fn ini_condition(value: &str) -> FilterCondition {
    let mut parts = value.splitn(3, ',');
    let receiver = parts.next().unwrap_or_default();
    let operator = parts.next().unwrap_or_default().trim().to_owned();
    let value = parts.next().map(str::to_owned).filter(|v| v != "a value");
    let mut path = receiver.splitn(3, '.');
    FilterCondition {
        application_class: None,
        receiver_class: path.next().unwrap_or_default().to_owned(),
        receiver_document_type: path.next().map(str::to_owned),
        receiver_element_type: path.next().map(str::to_owned),
        operator,
        value,
    }
}

pub fn parse_marks(text: &str) -> Result<Vec<Mark>, InteropError> {
    let fixed = sanitize_xml(text);
    let doc = roxmltree::Document::parse(&fixed)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("Mark"))
        .map(|m| {
            // Entier décimal ; lu comme un COLORREF Windows 0x00BBGGRR (inférence).
            let c: u32 = m
                .attribute("color")
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            Mark {
                name: m.attribute("name").unwrap_or_default().to_owned(),
                label: m.attribute("text").unwrap_or_default().to_owned(),
                color: (
                    (c & 0xFF) as u8,
                    ((c >> 8) & 0xFF) as u8,
                    ((c >> 16) & 0xFF) as u8,
                ),
            }
        })
        .collect())
}

/// Document de modification : racine `<ModificationDocument>` ou élément unique.
pub fn parse_modifications(text: &str) -> Result<Vec<Modification>, InteropError> {
    let fixed = sanitize_xml(text);
    let doc = roxmltree::Document::parse(&fixed)?;
    let root = doc.root_element();
    if root.has_tag_name("ModificationDocument") {
        Ok(root
            .children()
            .filter(|n| n.is_element() && !n.tag_name().name().starts_with('_'))
            .map(|n| modification(&n))
            .collect())
    } else {
        Ok(vec![modification(&root)])
    }
}

fn modification(node: &roxmltree::Node) -> Modification {
    Modification {
        tag: node.tag_name().name().to_owned(),
        type_name: node.attribute("type").unwrap_or_default().to_owned(),
        ident: node.attribute("ident").map(str::to_owned),
        label: node.attribute("label").map(str::to_owned),
        children: node
            .children()
            .filter(|n| n.is_element())
            .map(|n| modification(&n))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_xml_and_ini_filters() {
        let xml = r#"<Filters><Filter name="Passed">
<Condition applicationClass="AbstractRequirement" operator="equal" value="OK" receiverClass="Attribute" receiverDocumentType="Tests" receiverElementType="Result" />
</Filter></Filters>"#;
        let set = parse_filters(xml).unwrap();
        let c = &set.filters[0].conditions[0];
        assert_eq!(
            (c.operator.as_str(), c.value.as_deref()),
            ("equal", Some("OK"))
        );
        assert_eq!(c.receiver_element_type.as_deref(), Some("Result"));

        let ini = "[Filters]\nNames=Untested\nCurrent=(no filter)\n\n[Untested]\nBooleans=1,0\nIsAnd=1\n\
                   Cond1=Attribute.Tests Log.OK,isAbsent,a value\nCond2=Attribute.Tests Log.KO,equal,x\n";
        let set = parse_filters(ini).unwrap();
        assert_eq!(set.current, None);
        let f = &set.filters[0];
        assert!(f.all);
        assert_eq!(f.raw_flags, [true, false]);
        assert_eq!(
            f.conditions[0].receiver_document_type.as_deref(),
            Some("Tests Log")
        );
        assert_eq!(f.conditions[0].value, None);
        assert_eq!(f.conditions[1].value.as_deref(), Some("x"));
    }

    #[test]
    fn reads_marks_colors_as_bgr() {
        let marks = parse_marks(r#"<Marks version="3"><Mark name="approved" color="4509764" text="Approved"></Mark></Marks>"#).unwrap();
        assert_eq!(marks[0].color, (0x44, 0xD0, 0x44));
        assert_eq!(marks[0].label, "Approved");
    }

    #[test]
    fn reads_modification_documents() {
        let xml = r#"<ModificationDocument><_Value name="textFormat">4</_Value>
<Section type="Part" ident="P1"><Reference type="Implements" ident="REQ6" label="Hole"></Reference></Section>
<Requirement type="R" ident="REQ1"><Attribute type="Status" ident="Approved"></Attribute></Requirement>
</ModificationDocument>"#;
        let mods = parse_modifications(xml).unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].children[0].ident.as_deref(), Some("REQ6"));
        assert_eq!(mods[1].children[0].type_name, "Status");

        let single = parse_modifications(
            r#"<Requirement type="R" ident="X"><Attribute type="Critical" /></Requirement>"#,
        )
        .unwrap();
        assert_eq!(single[0].children[0].ident, None);
    }
}
