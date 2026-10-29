//! Capture sur un intermédiaire XML, par arbres d'analyse.
//!
//! Un arbre d'analyse est un motif XML : balises et attributs à retrouver, dont
//! les valeurs commencent par des marqueurs de rôle suivis d'une expression
//! régulière (`$i^(REQ\d+)$`). Marqueurs pris en charge (sens inférés, voir
//! `docs/interop-formats.md` §4.2) :
//! `$i` identifiant / valeur / cible, `$l` libellé, `$t` texte, `$u` GUID,
//! `$p` parent, `$o` nœud facultatif, `$!` nœud qui doit être absent.
//! `$g`, `$h`, `$c` et `$r` sont acceptés et sans effet.
//!
//! Un élément est produit pour chaque nœud du document qui correspond à la racine
//! du motif. Les éléments structurels s'imbriquent selon l'arbre XML ; les autres
//! se rattachent à l'élément produit sur le même nœud ou sur l'ancêtre le plus proche.

use crate::builder::{clean, Builder, Fields, Owner};
use crate::compile;
use rc_model::{
    CaptureRule, CapturedDocument, DocumentType, ElemId, ElementClass, ElementType, Severity,
};
use std::collections::HashMap;

/// Valeur de motif : marqueurs puis condition.
#[derive(Debug, Default)]
struct Spec {
    roles: Vec<char>,
    regex: Option<pcre2::bytes::Regex>,
}

impl Spec {
    fn parse(value: &str) -> Result<Spec, String> {
        let mut roles = Vec::new();
        let mut rest = value;
        while let Some(r) = rest.strip_prefix('$') {
            let mut chars = r.chars();
            match chars.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '!' => {
                    roles.push(c);
                    rest = chars.as_str();
                }
                _ => break,
            }
        }
        let regex = if rest.is_empty() {
            None
        } else {
            Some(compile(rest).map_err(|e| format!("{rest} : {e}"))?)
        };
        Ok(Spec { roles, regex })
    }

    fn optional(&self) -> bool {
        self.roles.contains(&'o')
    }

    fn negative(&self) -> bool {
        self.roles.contains(&'!')
    }

    /// Vérifie la condition et renvoie la valeur capturée.
    fn eval(&self, value: &str) -> Option<String> {
        match &self.regex {
            None => Some(value.to_owned()),
            Some(re) => {
                let caps = re.captures(value.as_bytes()).ok()??;
                let m = caps.get(1).or_else(|| caps.get(0))?;
                Some(value[m.start()..m.end()].to_owned())
            }
        }
    }
}

#[derive(Debug)]
struct Pattern {
    tag: String,
    attributes: Vec<(String, Spec)>,
    text: Option<Spec>,
    children: Vec<Pattern>,
}

impl Pattern {
    /// Les motifs utilisent des préfixes (`ds:artifact`) sans les déclarer : on les
    /// déclare dans un élément englobant, puisque seuls les noms locaux comptent.
    fn parse(source: &str) -> Result<Pattern, String> {
        let mut prefixes: Vec<&str> = Vec::new();
        let bytes = source.as_bytes();
        for (i, _) in source.match_indices(':') {
            let start = source[..i]
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
                .map_or(0, |p| p + 1);
            let before = start.checked_sub(1).map(|p| bytes[p]);
            if start < i && matches!(before, Some(b'<' | b'/' | b' ')) {
                let prefix = &source[start..i];
                if prefix != "xmlns" && !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
        }
        let declarations: String = prefixes
            .iter()
            .map(|p| format!(" xmlns:{p}=\"urn:reqticraft:{p}\""))
            .collect();
        let wrapped = format!("<pattern{declarations}>{source}</pattern>");
        let doc = roxmltree::Document::parse(&wrapped).map_err(|e| e.to_string())?;
        let root = doc
            .root_element()
            .first_element_child()
            .ok_or_else(|| "motif vide".to_owned())?;
        Pattern::from_node(root)
    }

    fn from_node(node: roxmltree::Node) -> Result<Pattern, String> {
        let attributes = node
            .attributes()
            .map(|a| Ok((a.name().to_owned(), Spec::parse(a.value())?)))
            .collect::<Result<Vec<_>, String>>()?;
        let own_text: String = node
            .children()
            .filter(|c| c.is_text())
            .filter_map(|c| c.text())
            .collect();
        let text = match own_text.trim() {
            "" => None,
            t => Some(Spec::parse(t)?),
        };
        let children = node
            .children()
            .filter(|c| c.is_element())
            .map(Pattern::from_node)
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Pattern {
            tag: local(node.tag_name().name()).to_owned(),
            attributes,
            text,
            children,
        })
    }

    fn optional(&self) -> bool {
        self.text.as_ref().is_some_and(Spec::optional)
    }

    fn negative(&self) -> bool {
        self.text.as_ref().is_some_and(Spec::negative)
    }
}

/// Nom local : `rrm:identifier` → `identifier` (tous les espaces de noms acceptés).
fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

type Bindings = Vec<(char, String)>;

const MAX_BINDINGS: usize = 10_000;

fn matches(p: &Pattern, node: roxmltree::Node) -> Vec<Bindings> {
    if !node.is_element() || node.tag_name().name() != p.tag {
        return Vec::new();
    }
    let mut base: Bindings = Vec::new();
    for (name, spec) in &p.attributes {
        let value = node
            .attributes()
            .find(|a| local(a.name()) == local(name))
            .map(|a| a.value());
        match value.and_then(|v| spec.eval(v)) {
            Some(v) => base.extend(spec.roles.iter().map(|&r| (r, v.clone()))),
            None if spec.optional() => {}
            None => return Vec::new(),
        }
    }
    if let Some(spec) = &p.text {
        let text = node_text(node);
        match spec.eval(text.trim()) {
            Some(v) => base.extend(
                spec.roles
                    .iter()
                    .filter(|r| !matches!(r, 'o' | '!'))
                    .map(|&r| (r, v.clone())),
            ),
            None if spec.optional() => {}
            None => return Vec::new(),
        }
    }
    let mut results = vec![base];
    for child in &p.children {
        let found: Vec<Bindings> = node.children().flat_map(|c| matches(child, c)).collect();
        if child.negative() {
            if !found.is_empty() {
                return Vec::new();
            }
            continue;
        }
        if found.is_empty() {
            if child.optional() {
                continue;
            }
            return Vec::new();
        }
        let mut next = Vec::new();
        'outer: for r in &results {
            for f in &found {
                let mut combined = r.clone();
                combined.extend(f.iter().cloned());
                next.push(combined);
                if next.len() >= MAX_BINDINGS {
                    break 'outer;
                }
            }
        }
        results = next;
    }
    results
}

fn node_text(node: roxmltree::Node) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

fn bound(b: &Bindings, role: char) -> Option<String> {
    b.iter()
        .find(|(r, _)| *r == role)
        .and_then(|(_, v)| clean(v))
}

pub fn capture_xml(
    doc_type: &DocumentType,
    document: &str,
    intermediate: &str,
) -> CapturedDocument {
    let mut b = Builder::new(document, &doc_type.name);
    let doc = match roxmltree::Document::parse(intermediate.trim_start_matches('\u{feff}')) {
        Ok(d) => d,
        Err(e) => {
            b.message(
                "analyseError",
                Severity::Error,
                format!("intermédiaire XML invalide : {e}"),
                None,
                None,
            );
            return b.doc;
        }
    };

    let mut patterns: Vec<(usize, Pattern)> = Vec::new();
    for (i, et) in doc_type.element_types.iter().enumerate() {
        if et.flags.ignored {
            continue;
        }
        if let CaptureRule::XmlTree { pattern, .. } = &et.rule {
            match Pattern::parse(pattern) {
                Ok(p) => patterns.push((i, p)),
                Err(e) => b.message(
                    "analyseError",
                    Severity::Error,
                    format!("arbre d'analyse invalide pour {} : {e}", et.name),
                    None,
                    None,
                ),
            }
        }
    }

    let types = &doc_type.element_types;
    // Nœud XML → élément structurel produit sur ce nœud.
    let mut produced: HashMap<roxmltree::NodeId, ElemId> = HashMap::new();
    let mut deferred_parents: Vec<(ElemId, String)> = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let line = doc.text_pos_at(node.range().start).row as usize;
        // Structurels d'abord, pour que les attributs du même nœud s'y rattachent.
        for pass in [true, false] {
            for (i, p) in &patterns {
                let et: &ElementType = &types[*i];
                if et.class.is_structural() != pass {
                    continue;
                }
                for bindings in matches(p, node) {
                    let fields = Fields {
                        ident: bound(&bindings, 'i'),
                        label: bound(&bindings, 'l'),
                        text: bound(&bindings, 't'),
                        guid: bound(&bindings, 'u'),
                        parent: bound(&bindings, 'p'),
                        depth: None,
                    };
                    if pass {
                        let parent = node
                            .ancestors()
                            .skip(1)
                            .find_map(|a| produced.get(&a.id()).copied());
                        let wanted_parent = fields.parent.clone();
                        let id = b.open(et, fields, parent, line);
                        if let Some(p) = wanted_parent {
                            deferred_parents.push((id, p));
                        }
                        produced.entry(node.id()).or_insert(id);
                    } else {
                        let owner = node
                            .ancestors()
                            .find_map(|a| produced.get(&a.id()).copied())
                            .map_or(Owner::Document, Owner::Element);
                        let targets = match et.class {
                            ElementClass::Reference | ElementClass::Link => {
                                split_targets(&mut b, et, fields.ident.as_deref())
                            }
                            _ => Vec::new(),
                        };
                        b.attach(et, owner, fields, targets, line);
                    }
                }
            }
        }
    }
    reparent(&mut b, deferred_parents);
    b.doc
}

/// Cible capturée, découpée par la sous-expression du type si elle existe.
fn split_targets(b: &mut Builder, et: &ElementType, value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let CaptureRule::XmlTree {
        sub_expression: Some(sub),
        ..
    } = &et.rule
    else {
        return clean(value).into_iter().collect();
    };
    match compile(sub) {
        Ok(re) => re
            .captures_iter(value.as_bytes())
            .filter_map(Result::ok)
            .filter_map(|c| {
                let m = c.get(1).or_else(|| c.get(0))?;
                clean(&value[m.start()..m.end()])
            })
            .collect(),
        Err(e) => {
            b.message(
                "analyseError",
                Severity::Error,
                format!("sous-expression invalide pour {} : {e}", et.name),
                None,
                None,
            );
            Vec::new()
        }
    }
}

/// Applique les parents capturés par `$p` (identifiant ou GUID du parent).
fn reparent(b: &mut Builder, wanted: Vec<(ElemId, String)>) {
    for (id, key) in wanted {
        let found = b.doc.elements.iter().position(|e| {
            e.guid.as_deref() == Some(key.as_str())
                || e.ident
                    .as_deref()
                    .is_some_and(|i| i.eq_ignore_ascii_case(&key))
        });
        let Some(parent) = found.filter(|&p| p != id && !b.doc.ancestors(p).any(|a| a == id))
        else {
            b.message(
                "unknownParentElement",
                Severity::Error,
                format!("parent introuvable : {key}"),
                Some(id),
                None,
            );
            continue;
        };
        match b.doc.elements[id].parent {
            Some(old) => b.doc.elements[old].children.retain(|&c| c != id),
            None => b.doc.roots.retain(|&r| r != id),
        }
        b.doc.elements[id].parent = Some(parent);
        b.doc.elements[parent].children.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_model::{ElementType as Et, TypeKind};

    fn tree(name: &str, class: ElementClass, pattern: &str) -> Et {
        Et::new(
            name,
            class,
            CaptureRule::XmlTree {
                pattern: pattern.into(),
                sub_expression: None,
            },
        )
    }

    #[test]
    fn captures_spreadsheet_rows() {
        let mut t = DocumentType::new("ExcelXML", TypeKind::Xml, "ExcelXML");
        t.element_types.push(tree(
            "Worksheet",
            ElementClass::Section,
            "<Worksheet name='$i'></Worksheet>",
        ));
        t.element_types.push(tree(
            "Requirement",
            ElementClass::Requirement,
            r"<Row><Cell col='^1$'>$i^(REQ\d+)$</Cell><Cell col='^2$'>$l$o</Cell><Cell col='^3$'>$t$o</Cell></Row>",
        ));
        t.element_types.push(tree(
            "Priority",
            ElementClass::Attribute,
            r"<Row><Cell col='^1$'>^REQ\d+$</Cell><Cell col='^4$'>$i</Cell></Row>",
        ));
        let xml = r#"<Workbook><Worksheet name="Reqs">
<Row index="1"><Cell col="1">ID</Cell><Cell col="2">Label</Cell></Row>
<Row index="2"><Cell col="1">REQ1</Cell><Cell col="2">Capture</Cell><Cell col="4">High</Cell></Row>
<Row index="3"><Cell col="1">REQ2</Cell></Row>
</Worksheet></Workbook>"#;
        let doc = capture_xml(&t, "S", xml);
        let tree: Vec<_> = doc
            .elements
            .iter()
            .map(|e| (e.ident.as_deref().unwrap(), e.label.as_deref(), e.parent))
            .collect();
        assert_eq!(
            tree,
            [
                ("Reqs", None, None),
                ("REQ1", Some("Capture"), Some(0)),
                ("REQ2", None, Some(0))
            ]
        );
        assert_eq!(doc.elements[1].attributes[0].value.as_deref(), Some("High"));
        assert!(doc.elements[2].attributes.is_empty());
        assert_eq!(doc.elements[1].line, 3);
    }

    #[test]
    fn captures_json_arrays_as_several_references() {
        let mut t = DocumentType::new("JSON", TypeKind::Xml, "json");
        t.element_types.push(tree(
            "Requirement",
            ElementClass::Requirement,
            "<object><string name='^id$'>$i</string></object>",
        ));
        t.element_types.push(tree(
            "Covers",
            ElementClass::Reference,
            "<array name='^covers$'><string>$i</string></array>",
        ));
        let xml = r#"<array><object><string name="id">D1</string><array name="covers"><string>R1</string><string>R2</string></array></object></array>"#;
        let doc = capture_xml(&t, "D", xml);
        let targets: Vec<_> = doc.elements[0]
            .references
            .iter()
            .map(|r| r.target.as_str())
            .collect();
        assert_eq!(targets, ["R1", "R2"]);
    }

    #[test]
    fn handles_negative_nodes_namespaces_and_parents() {
        let mut t = DocumentType::new("X", TypeKind::Xml, "xml");
        t.element_types.push(tree(
            "Req",
            ElementClass::Requirement,
            "<ds:req id='$i' parent='$p$o'><draft>$!</draft></ds:req>",
        ));
        let xml = r#"<root xmlns:ds="urn:x"><ds:req id="A"/><ds:req id="B" parent="A"/><ds:req id="C"><draft/></ds:req></root>"#;
        let doc = capture_xml(&t, "D", xml);
        let tree: Vec<_> = doc
            .elements
            .iter()
            .map(|e| (e.ident.as_deref().unwrap(), e.parent))
            .collect();
        assert_eq!(tree, [("A", None), ("B", Some(0))]);
    }

    #[test]
    fn parses_markers() {
        let s = Spec::parse(r"$i^(REQ\d+)$").unwrap();
        assert_eq!(s.roles, ['i']);
        assert_eq!(s.eval("REQ12").as_deref(), Some("REQ12"));
        assert_eq!(s.eval("X"), None);
        assert_eq!(Spec::parse("$l$o").unwrap().roles, ['l', 'o']);
    }
}
