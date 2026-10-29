//! Types d'analyse `.types` : format XML et format INI.
//!
//! Vocabulaire : `docs/interop-formats.md` §4.

use crate::ini::{split_list, Ini, Section};
use crate::text::sanitize_xml;
use crate::InteropError;
use rc_model::{
    CaptureRole, CaptureRule, DocumentType, ElementClass, ElementFlags, ElementType, TypeKind,
    VariableDecl,
};

/// Lit un fichier `.types` ; le format est détecté sur le contenu.
pub fn parse_types(text: &str) -> Result<Vec<DocumentType>, InteropError> {
    if crate::text::looks_like_xml(text) {
        parse_xml(text)
    } else {
        Ok(parse_ini(text))
    }
}

// --- XML --------------------------------------------------------------------

fn parse_xml(text: &str) -> Result<Vec<DocumentType>, InteropError> {
    let fixed = sanitize_xml(text);
    let doc = roxmltree::Document::parse(&fixed)?;
    let root = doc.root_element();
    if root.tag_name().name() != "Types" {
        return Err(InteropError::Format(format!(
            "élément racine inattendu `{}` (attendu : Types)",
            root.tag_name().name()
        )));
    }
    Ok(root
        .children()
        .filter(|n| n.has_tag_name("DocumentType"))
        .map(|n| xml_document_type(&n))
        .collect())
}

fn xml_document_type(node: &roxmltree::Node) -> DocumentType {
    let kind = match node.attribute("kind") {
        Some("xml") => TypeKind::Xml,
        Some("automatic") => TypeKind::AddedElements,
        _ => TypeKind::Text,
    };
    let mut t = DocumentType::new(
        node.attribute("name").unwrap_or_default(),
        kind,
        node.attribute("convertTool").unwrap_or_default(),
    );
    t.edit_tool = node.attribute("editTool").map(str::to_owned);
    t.delete_expression = node.attribute("reDelete").map(str::to_owned);
    for child in node.children().filter(|n| n.is_element()) {
        let value = || child.attribute("value").unwrap_or_default().to_owned();
        match child.tag_name().name() {
            "FileExtension" => t.file_extensions.push(value()),
            "MatchingFileName" => t.matching_file_names.push(value()),
            "ExcludedFileName" => t.excluded_file_names.push(value()),
            "ExcludingAT" => t.exclude_text.push(value()),
            "Property" => t.properties.push((
                child.attribute("name").unwrap_or_default().to_owned(),
                value(),
            )),
            "Variable" => t.variables.push(VariableDecl {
                name: child.attribute("name").unwrap_or_default().to_owned(),
                label: child.attribute("label").map(str::to_owned),
                var_type: child.attribute("type").map(str::to_owned),
            }),
            "ElementType" | "TerminalType" => {
                if let Some(e) = xml_element_type(&child) {
                    t.element_types.push(e);
                }
            }
            _ => {}
        }
    }
    t
}

fn xml_element_type(node: &roxmltree::Node) -> Option<ElementType> {
    let class = element_class(node.attribute("elementClass")?)?;
    let rule = if let Some(tree) = node.attribute("analysisTree") {
        CaptureRule::XmlTree {
            pattern: tree.to_owned(),
            sub_expression: node.attribute("subExpression").map(str::to_owned),
        }
    } else if let Some(re) = node.attribute("regularExpression") {
        CaptureRule::Regex {
            expression: re.to_owned(),
            sub_expression: node.attribute("subExpression").map(str::to_owned),
            end_expression: node.attribute("endExpression").map(str::to_owned),
            row_expression: node.attribute("reRow").map(str::to_owned),
        }
    } else {
        // Types pour éléments ajoutés, ou élément sans règle de capture.
        CaptureRule::None
    };
    let mut e = ElementType::new(node.attribute("name").unwrap_or_default(), class, rule);
    e.display = node.attribute("printExpression").map(str::to_owned);
    let is = |name: &str| node.attribute(name) == Some("true");
    e.flags = ElementFlags {
        hidden: node.attribute("isVisible") == Some("false"),
        ignored: is("isIgnored"),
        multiple: is("isMultiple"),
        nested: is("isNested"),
        inverse: is("isInverse"),
        boolean: is("isBoolean"),
        identify_children: is("identifyChildren"),
    };
    let indexes: Vec<u32> = (1..=4)
        .map_while(|k| node.attribute(format!("index{k}").as_str()))
        .filter_map(|v| v.parse().ok())
        .collect();
    e.roles = indexes.into_iter().map(CaptureRole::from_code).collect();
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "ParentType" => e
                .parents
                .push(child.attribute("name").unwrap_or_default().to_owned()),
            "EnumValue" => e
                .enum_values
                .push(child.attribute("value").unwrap_or_default().to_owned()),
            "Property" => e.properties.push((
                child.attribute("name").unwrap_or_default().to_owned(),
                child.attribute("value").unwrap_or_default().to_owned(),
            )),
            _ => {}
        }
    }
    if let Some(col) = node.attribute("columnIndex") {
        e.properties.push(("columnIndex".into(), col.to_owned()));
    }
    Some(e)
}

/// `elementClass` du format XML.
fn element_class(name: &str) -> Option<ElementClass> {
    Some(match name {
        "Section" => ElementClass::Section,
        "MacroRequirement" => ElementClass::MacroRequirement,
        "Requirement" => ElementClass::Requirement,
        "Entity" | "EntityStrict" => ElementClass::Entity,
        "CoverLink" | "Reference" => ElementClass::Reference,
        "Attribute" => ElementClass::Attribute,
        "LinkAttribute" => ElementClass::ReferenceAttribute,
        "Link" => ElementClass::Link,
        "Value" => ElementClass::Value,
        "Text" => ElementClass::Text,
        "Picture" => ElementClass::Picture,
        "Table" => ElementClass::Table,
        _ => return None,
    })
}

// --- INI --------------------------------------------------------------------

/// Préfixes de clés INI et classe correspondante.
const INI_CLASSES: &[(&str, ElementClass)] = &[
    ("Section", ElementClass::Section),
    ("MacroRequirement", ElementClass::MacroRequirement),
    ("Requirement", ElementClass::Requirement),
    ("Entity", ElementClass::Entity),
    ("Reference", ElementClass::Reference),
    ("LinkAttribute", ElementClass::ReferenceAttribute),
    ("Attribute", ElementClass::Attribute),
    ("Link", ElementClass::Link),
    ("Value", ElementClass::Value),
    ("Text", ElementClass::Text),
    ("Picture", ElementClass::Picture),
    ("Table", ElementClass::Table),
];

fn parse_ini(text: &str) -> Vec<DocumentType> {
    let ini = Ini::parse(text);
    let Some(header) = ini.section("Types") else {
        return Vec::new();
    };
    split_list(header.get("Names").unwrap_or_default())
        .iter()
        .filter_map(|name| ini.section(name).map(|s| ini_document_type(name, s)))
        .collect()
}

fn ini_document_type(name: &str, s: &Section) -> DocumentType {
    let kind = match s.get("Type").map(str::trim) {
        Some("xml") => TypeKind::Xml,
        Some("automatic") => TypeKind::AddedElements,
        _ => TypeKind::Text,
    };
    let mut t = DocumentType::new(name, kind, s.get("Tool").unwrap_or_default().trim());
    t.edit_tool = s.get_nonempty("ToolEdit").map(str::to_owned);
    t.file_extensions = s.get("FileExtension").map(split_list).unwrap_or_default();
    t.matching_file_names = s
        .get("MatchingFileNames")
        .map(|v| {
            v.split([',', ';'])
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    t.exclude_text = s
        .numbered("ExcludingText", "")
        .into_iter()
        .map(|(_, v)| v.to_owned())
        .collect();
    t.variables = s
        .numbered("Variable", "Name")
        .into_iter()
        .map(|(n, var)| VariableDecl {
            name: var.to_owned(),
            label: s.get(&format!("Variable{n}Label")).map(str::to_owned),
            var_type: s.get(&format!("Variable{n}Type")).map(str::to_owned),
        })
        .collect();

    for &(prefix, class) in INI_CLASSES {
        for (n, element_name) in s.numbered(prefix, "Name") {
            // `Attribute1Name` ne doit pas être pris pour `LinkAttribute`, et inversement :
            // `numbered` exige un nombre juste après le préfixe, ce qui suffit.
            let key = |suffix: &str| format!("{prefix}{n}{suffix}");
            let regex_key = if matches!(
                class,
                ElementClass::Text | ElementClass::Picture | ElementClass::Table
            ) {
                key("RegularExpression")
            } else {
                key("")
            };
            let rule = if let Some(tree) = s.get_nonempty(&key("Tree")) {
                CaptureRule::XmlTree {
                    pattern: tree.to_owned(),
                    sub_expression: s.get_nonempty(&key("Sub")).map(str::to_owned),
                }
            } else if let Some(re) = s.get_nonempty(&regex_key) {
                CaptureRule::Regex {
                    expression: re.to_owned(),
                    sub_expression: s.get_nonempty(&key("Sub")).map(str::to_owned),
                    end_expression: s.get_nonempty(&key("End")).map(str::to_owned),
                    row_expression: s.get_nonempty(&key("Row")).map(str::to_owned),
                }
            } else {
                CaptureRule::None
            };
            let mut e = ElementType::new(element_name, class, rule);
            e.display = s.get_nonempty(&key("Print")).map(str::to_owned);
            e.parents = s.get(&key("Parents")).map(split_list).unwrap_or_default();
            e.roles = s
                .get(&key("Indexes"))
                .map(|v| {
                    v.split(',')
                        .filter_map(|c| c.trim().parse::<u32>().ok())
                        .map(CaptureRole::from_code)
                        .collect()
                })
                .unwrap_or_default();
            e.flags = ElementFlags {
                hidden: s.get(&key("IsVisible")).map(str::trim) == Some("0"),
                ignored: s.flag(&key("IsIgnored")),
                multiple: s.flag(&key("IsMultiple")),
                nested: s.flag(&key("IsNested")),
                inverse: s.flag(&key("IsInverse")),
                boolean: s.flag(&key("IsBoolean")),
                identify_children: s.flag(&key("IdentifyChildren")),
            };
            // `High|ko,Medium|nt` : valeur puis icône.
            e.enum_values = s
                .get(&key("EnumValues"))
                .map(|v| {
                    split_list(v)
                        .into_iter()
                        .map(|x| x.split('|').next().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            for (k, prop) in s.numbered(&format!("{prefix}{n}Value"), "Key") {
                let value = s
                    .get(&format!("{prefix}{n}Value{k}Value"))
                    .unwrap_or_default();
                e.properties.push((prop.to_owned(), value.to_owned()));
            }
            if let Some(col) = s.get_nonempty(&key("ColumnIndex")) {
                e.properties.push(("columnIndex".into(), col.to_owned()));
            }
            t.element_types.push(e);
        }
    }
    // Ordre d'origine : par classe puis numéro. Le moteur trie ensuite par position.
    if kind == TypeKind::Text
        && t.element_types
            .iter()
            .any(|e| matches!(e.rule, CaptureRule::XmlTree { .. }))
        && !t
            .element_types
            .iter()
            .any(|e| matches!(e.rule, CaptureRule::Regex { .. }))
    {
        t.kind = TypeKind::Xml;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_xml_types() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types coupling="Text">
<DocumentType name="Text" convertTool="BasicConnector" editTool="TextEditor">
<FileExtension value="txt" />
<ElementType name="Heading1" elementClass="Section" regularExpression="^(\d+)\.[ \t]+(.*)$" />
<ElementType name="Heading2" elementClass="Section" regularExpression="^(\d+\.\d+)\.[ \t]+(.*)$">
<ParentType name="Heading1" />
</ElementType>
<ElementType name="Covers" elementClass="CoverLink" regularExpression="\[Covers:([^\]]*)" subExpression="([^,]+)" />
<ElementType name="Priority" elementClass="Attribute" regularExpression="Priority:(\S+)" index1="1" index2="0">
<EnumValue value="High" /><EnumValue value="Low" />
</ElementType>
<TerminalType name="Text" elementClass="Text" regularExpression="\&lt;\&lt;([\s\S]+?)\>\>" />
</DocumentType>
<DocumentType name="X" kind="xml" convertTool="ExcelXML">
<ElementType name="Sheet" elementClass="Section" analysisTree="&lt;Worksheet name='$i'>&lt;/Worksheet>" />
</DocumentType>
</Types>"#;
        let types = parse_types(xml).unwrap();
        assert_eq!(types.len(), 2);
        let text = &types[0];
        assert_eq!(text.converter, "BasicConnector");
        assert_eq!(text.file_extensions, ["txt"]);
        assert_eq!(text.element_types.len(), 5);
        assert_eq!(text.element_types[1].parents, ["Heading1"]);
        let covers = &text.element_types[2];
        assert_eq!(covers.class, ElementClass::Reference);
        assert!(
            matches!(&covers.rule, CaptureRule::Regex { sub_expression: Some(s), .. } if s == "([^,]+)")
        );
        assert_eq!(text.element_types[3].enum_values, ["High", "Low"]);
        assert_eq!(
            text.element_types[3].roles,
            [CaptureRole::Ident, CaptureRole::None]
        );
        assert!(
            matches!(&text.element_types[4].rule, CaptureRule::Regex { expression, .. } if expression == r"\<\<([\s\S]+?)\>\>")
        );
        assert_eq!(types[1].kind, TypeKind::Xml);
        assert!(
            matches!(&types[1].element_types[0].rule, CaptureRule::XmlTree { pattern, .. } if pattern == "<Worksheet name='$i'></Worksheet>")
        );
    }

    #[test]
    fn reads_ini_types() {
        let ini = "[Types]\nNames=Added,MyWord\n\n\
                   [Added]\nTool=text\nRequirement1Name=SRS Req\nAttribute1Name=Critical\nAttribute1IsBoolean=1\n\
                   Attribute2Name=Priority\nAttribute2EnumValues=High|ko,Low|ok\nLinkAttribute1Name=Partial\nType=automatic\n\n\
                   [MyWord]\n;commentaire\nTool=WordWithStyles\nSection1Name=Heading 1\nSection1=^Heading 1\\t(.+)$\nSection1Indexes=1,2,1,0\n\
                   Section2Name=Heading 2\nSection2Parents=Heading 1\nSection2=^Heading 2\\t(.+)$\n\
                   Reference1Name=Coverage\nReference1=\\[Covers:([^\\]]*)\nReference1Sub=([^,;]+)\n\
                   Text1Name=Text\nText1RegularExpression=^Requirement_Text\\t(.+)$\n";
        let types = parse_types(ini).unwrap();
        assert_eq!(types.len(), 2);
        assert_eq!(types[0].kind, TypeKind::AddedElements);
        let critical = types[0].element_type("Critical").unwrap();
        assert!(critical.flags.boolean);
        assert_eq!(
            types[0].element_type("Priority").unwrap().enum_values,
            ["High", "Low"]
        );
        assert_eq!(
            types[0].element_type("Partial").unwrap().class,
            ElementClass::ReferenceAttribute
        );
        let word = &types[1];
        assert_eq!(word.converter, "WordWithStyles");
        let h1 = word.element_type("Heading 1").unwrap();
        assert_eq!(
            h1.roles,
            [
                CaptureRole::Ident,
                CaptureRole::Label,
                CaptureRole::Ident,
                CaptureRole::None
            ]
        );
        assert_eq!(
            word.element_type("Heading 2").unwrap().parents,
            ["Heading 1"]
        );
        let cov = word.element_type("Coverage").unwrap();
        assert!(
            matches!(&cov.rule, CaptureRule::Regex { sub_expression: Some(s), .. } if s == "([^,;]+)")
        );
        assert_eq!(word.element_type("Text").unwrap().class, ElementClass::Text);
    }
}
