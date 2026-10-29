//! Capture sur un intermédiaire texte, par expressions régulières.
//!
//! Toutes les expressions du type sont appliquées au texte entier ; les
//! correspondances sont ensuite traitées dans l'ordre du texte :
//! - une section se place sous le plus proche élément ouvert de type parent autorisé ;
//! - une exigence ou une entité se termine au prochain élément de même rang, à la
//!   prochaine section ou macro-exigence, ou à son expression de fin ;
//! - attributs, références, liens et textes se rattachent à l'élément ouvert le
//!   plus récent (ou au document).

use crate::builder::{clean, Builder, Fields, Owner};
use crate::compile;
use rc_model::{
    CaptureRole, CaptureRule, CapturedDocument, DocumentType, ElemId, ElementClass, ElementType,
    Severity,
};

#[derive(Debug, Clone, Copy)]
enum Kind {
    Close,
    Open,
    Attach,
}

#[derive(Debug)]
struct Event {
    start: usize,
    kind: Kind,
    et: usize,
    groups: Vec<Option<(usize, usize)>>,
}

impl Event {
    fn rank(&self, types: &[ElementType]) -> u8 {
        match self.kind {
            Kind::Close => 0,
            Kind::Open => match types[self.et].class {
                ElementClass::Section => 1,
                ElementClass::MacroRequirement => 2,
                _ => 3,
            },
            Kind::Attach => 4,
        }
    }
}

pub fn capture_text(
    doc_type: &DocumentType,
    document: &str,
    intermediate: &str,
) -> CapturedDocument {
    let mut b = Builder::new(document, &doc_type.name);
    let text = exclude_text(doc_type, intermediate, &mut b);
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset);
    let bytes = text.as_bytes();

    let mut events = Vec::new();
    for (et_index, et) in doc_type.element_types.iter().enumerate() {
        if et.flags.ignored {
            continue;
        }
        let CaptureRule::Regex {
            expression,
            end_expression,
            ..
        } = &et.rule
        else {
            continue;
        };
        let kind = if et.class.is_structural() {
            Kind::Open
        } else {
            Kind::Attach
        };
        collect(&mut b, et, expression, bytes, kind, et_index, &mut events);
        if let Some(end) = end_expression {
            collect(&mut b, et, end, bytes, Kind::Close, et_index, &mut events);
        }
    }
    events.sort_by_key(|e| (e.start, e.rank(&doc_type.element_types), e.et));

    let types = &doc_type.element_types;
    let mut stack: Vec<ElemId> = Vec::new();
    // Éléments qui attendent leur expression de fin.
    let mut awaiting: Vec<bool> = Vec::new();
    // Niveau capturé des sections (rôle « profondeur »).
    let mut depths: Vec<Option<u32>> = Vec::new();
    let is_awaiting = |awaiting: &Vec<bool>, id: ElemId| awaiting.get(id).copied().unwrap_or(false);

    for ev in &events {
        let et = &types[ev.et];
        let line = line_of(ev.start);
        let fields = fields(et, &ev.groups, &text);
        match ev.kind {
            Kind::Open => {
                let class_of = |b: &Builder, id: ElemId| b.doc.elements[id].class;
                let type_of = |b: &Builder, id: ElemId| b.doc.elements[id].type_name.clone();
                match et.class {
                    ElementClass::Section if fields.depth.is_some() => {
                        // Imbrication par niveau : sous la section ouverte de niveau inférieur.
                        let d = fields.depth.unwrap_or(0);
                        while let Some(&top) = stack.last() {
                            let shallower = class_of(&b, top) == ElementClass::Section
                                && depths.get(top).copied().flatten().is_some_and(|t| t < d);
                            if shallower || is_awaiting(&awaiting, top) {
                                break;
                            }
                            stack.pop();
                        }
                    }
                    ElementClass::Section => {
                        while let Some(&top) = stack.last() {
                            if et.parents.contains(&type_of(&b, top)) || is_awaiting(&awaiting, top)
                            {
                                break;
                            }
                            stack.pop();
                        }
                        let root_allowed =
                            et.parents.is_empty() || et.parents.contains(&doc_type.name);
                        let top_allowed = stack
                            .last()
                            .is_some_and(|&t| et.parents.contains(&type_of(&b, t)));
                        if !root_allowed && !top_allowed {
                            b.message(
                                "badSectionLevel",
                                Severity::Warning,
                                format!(
                                    "section {} hors de la hiérarchie du type",
                                    fields.ident.as_deref().unwrap_or(&et.name)
                                ),
                                None,
                                Some(line),
                            );
                        }
                    }
                    ElementClass::MacroRequirement => {
                        while let Some(&top) = stack.last() {
                            let c = class_of(&b, top);
                            let nest = et.flags.nested && c == ElementClass::MacroRequirement;
                            if c == ElementClass::Section || is_awaiting(&awaiting, top) || nest {
                                break;
                            }
                            stack.pop();
                        }
                    }
                    _ => {
                        while let Some(&top) = stack.last() {
                            let c = class_of(&b, top);
                            let leaf =
                                matches!(c, ElementClass::Requirement | ElementClass::Entity);
                            if !leaf
                                || is_awaiting(&awaiting, top)
                                || et.parents.contains(&type_of(&b, top))
                            {
                                break;
                            }
                            stack.pop();
                        }
                        if et.class == ElementClass::Entity
                            && stack
                                .last()
                                .is_some_and(|&t| class_of(&b, t) == ElementClass::MacroRequirement)
                        {
                            b.message(
                                "entityFoundInMacro",
                                Severity::Error,
                                "entité dans une macro-exigence".into(),
                                None,
                                Some(line),
                            );
                        }
                    }
                }
                let depth = fields.depth;
                let id = b.open(et, fields, stack.last().copied(), line);
                awaiting.resize(id + 1, false);
                depths.resize(id + 1, None);
                depths[id] = depth;
                if matches!(
                    &et.rule,
                    CaptureRule::Regex {
                        end_expression: Some(_),
                        ..
                    }
                ) {
                    awaiting[id] = true;
                }
                stack.push(id);
            }
            Kind::Close => {
                let wanted = fields.ident.as_deref().map(str::to_lowercase);
                let position = stack.iter().rposition(|&id| {
                    let e = &b.doc.elements[id];
                    e.type_name == et.name
                        && wanted.as_ref().is_none_or(|w| {
                            e.ident.as_deref().map(str::to_lowercase).as_ref() == Some(w)
                        })
                });
                match position {
                    Some(p) => {
                        for &id in &stack[p..] {
                            awaiting[id] = false;
                        }
                        stack.truncate(p);
                    }
                    None => b.message(
                        "closingWithoutOpening",
                        Severity::Error,
                        format!("fin de {} sans début", et.name),
                        None,
                        Some(line),
                    ),
                }
            }
            Kind::Attach => {
                let owner = stack.last().map_or(Owner::Document, |&e| Owner::Element(e));
                let targets = targets(et, &ev.groups, &text, &mut b, line);
                b.attach(et, owner, fields, targets, line);
            }
        }
    }

    for (id, waiting) in awaiting.iter().enumerate() {
        if *waiting {
            let name = b.doc.elements[id].display_name();
            let line = b.doc.elements[id].line;
            b.message(
                "openingWithoutClosing",
                Severity::Error,
                format!("{name} n'est jamais fermé"),
                Some(id),
                Some(line),
            );
        }
    }
    delete_elements(doc_type, &text, &mut b);
    b.doc
}

fn collect(
    b: &mut Builder,
    et: &ElementType,
    pattern: &str,
    bytes: &[u8],
    kind: Kind,
    et_index: usize,
    events: &mut Vec<Event>,
) {
    let re = match compile(pattern) {
        Ok(re) => re,
        Err(e) => {
            b.message(
                "analyseError",
                Severity::Error,
                format!("expression invalide pour {} : {e}", et.name),
                None,
                None,
            );
            return;
        }
    };
    for caps in re.captures_iter(bytes) {
        let Ok(caps) = caps else {
            b.message(
                "analyseError",
                Severity::Error,
                format!("échec de l'expression de {}", et.name),
                None,
                None,
            );
            return;
        };
        let whole = caps.get(0).expect("groupe 0 toujours présent");
        if whole.start() == whole.end() {
            continue;
        }
        let groups = (0..caps.len())
            .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
            .collect();
        events.push(Event {
            start: whole.start(),
            kind,
            et: et_index,
            groups,
        });
    }
}

fn group<'t>(groups: &[Option<(usize, usize)>], i: usize, text: &'t str) -> Option<&'t str> {
    groups.get(i).copied().flatten().map(|(s, e)| &text[s..e])
}

/// Affecte les groupes de capture aux champs, selon les rôles du type ou par défaut.
fn fields(et: &ElementType, groups: &[Option<(usize, usize)>], text: &str) -> Fields {
    let mut f = Fields::default();
    if !et.roles.is_empty() {
        for (k, role) in et.roles.iter().enumerate() {
            let Some(v) = group(groups, k + 1, text).and_then(clean) else {
                continue;
            };
            let slot = match role {
                CaptureRole::Ident => &mut f.ident,
                CaptureRole::Label => &mut f.label,
                CaptureRole::Text => &mut f.text,
                CaptureRole::Guid => &mut f.guid,
                CaptureRole::Parent => &mut f.parent,
                CaptureRole::Depth => {
                    if f.depth.is_none() {
                        f.depth = v.parse().ok();
                    }
                    continue;
                }
                CaptureRole::None => continue,
            };
            if slot.is_none() {
                *slot = Some(v);
            }
        }
        return f;
    }
    // `columnIndex` désigne le groupe qui porte le numéro de colonne d'une cellule.
    let column_group: Option<usize> = et
        .properties
        .iter()
        .find(|(k, _)| k == "columnIndex")
        .and_then(|(_, v)| v.trim().parse().ok());
    let participating: Vec<String> = (1..groups.len())
        .filter(|&i| Some(i) != column_group)
        .filter_map(|i| group(groups, i, text).and_then(clean))
        .collect();
    match et.class {
        ElementClass::Text => {
            f.text = participating
                .into_iter()
                .next()
                .or_else(|| group(groups, 0, text).and_then(clean));
        }
        c if c.is_structural() => {
            let mut it = participating.into_iter();
            f.ident = it.next();
            f.label = it.next();
        }
        _ => f.ident = participating.into_iter().next(),
    }
    f
}

/// Cibles d'une référence ou d'un lien : groupe 1 (ou toute la correspondance),
/// découpé par la sous-expression si elle existe.
fn targets(
    et: &ElementType,
    groups: &[Option<(usize, usize)>],
    text: &str,
    b: &mut Builder,
    line: usize,
) -> Vec<String> {
    if !matches!(et.class, ElementClass::Reference | ElementClass::Link) {
        return Vec::new();
    }
    let list = group(groups, 1, text)
        .or_else(|| group(groups, 0, text))
        .unwrap_or_default();
    let CaptureRule::Regex {
        sub_expression: Some(sub),
        ..
    } = &et.rule
    else {
        return clean(list).into_iter().collect();
    };
    let re = match compile(sub) {
        Ok(re) => re,
        Err(e) => {
            b.message(
                "analyseError",
                Severity::Error,
                format!("sous-expression invalide pour {} : {e}", et.name),
                None,
                Some(line),
            );
            return Vec::new();
        }
    };
    re.captures_iter(list.as_bytes())
        .filter_map(Result::ok)
        .filter_map(|c| {
            let m = c.get(1).or_else(|| c.get(0))?;
            clean(&list[m.start()..m.end()])
        })
        .collect()
}

/// Zones exclues : remplacées par des espaces (les retours à la ligne sont
/// conservés pour garder les numéros de ligne).
fn exclude_text(doc_type: &DocumentType, text: &str, b: &mut Builder) -> String {
    let mut out = text.to_owned();
    for pattern in &doc_type.exclude_text {
        let Ok(re) = compile(pattern) else {
            b.message(
                "analyseError",
                Severity::Error,
                format!("expression d'exclusion invalide : {pattern}"),
                None,
                None,
            );
            continue;
        };
        let ranges: Vec<(usize, usize)> = re
            .find_iter(out.as_bytes())
            .filter_map(Result::ok)
            .map(|m| (m.start(), m.end()))
            .collect();
        let mut bytes = out.into_bytes();
        for (s, e) in ranges {
            for byte in &mut bytes[s..e] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        // Les octets remplacés couvrent des caractères entiers : le texte reste UTF-8.
        out = String::from_utf8(bytes).unwrap_or_default();
    }
    out
}

/// Expression de suppression : retire les éléments dont l'identifiant est capturé.
fn delete_elements(doc_type: &DocumentType, text: &str, b: &mut Builder) {
    let Some(pattern) = &doc_type.delete_expression else {
        return;
    };
    let Ok(re) = compile(pattern) else {
        b.message(
            "analyseError",
            Severity::Error,
            format!("expression de suppression invalide : {pattern}"),
            None,
            None,
        );
        return;
    };
    let idents: Vec<String> = re
        .captures_iter(text.as_bytes())
        .filter_map(Result::ok)
        .filter_map(|c| {
            c.get(1)
                .map(|m| text[m.start()..m.end()].trim().to_lowercase())
        })
        .collect();
    let mut removed = Vec::new();
    for ident in idents {
        let found: Vec<ElemId> = b
            .doc
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.class.is_structural()
                    && e.ident.as_deref().map(str::to_lowercase) == Some(ident.clone())
            })
            .map(|(i, _)| i)
            .collect();
        if found.is_empty() {
            b.message(
                "unknownDeletedElement",
                Severity::Error,
                format!("élément à supprimer introuvable : {ident}"),
                None,
                None,
            );
        }
        removed.extend(found);
    }
    b.remove(&removed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_model::{ElementType as Et, TypeKind};

    fn re(name: &str, class: ElementClass, expr: &str) -> Et {
        Et::new(name, class, CaptureRule::regex(expr))
    }

    fn text_type() -> DocumentType {
        let mut t = DocumentType::new("Text", TypeKind::Text, "text");
        t.element_types
            .push(re("H1", ElementClass::Section, r"^(\d+)\.[ \t]+(.*)$"));
        let mut h2 = re("H2", ElementClass::Section, r"^(\d+\.\d+)\.[ \t]+(.*)$");
        h2.parents = vec!["H1".into()];
        t.element_types.push(h2);
        let mut macro_req = Et::new(
            "Macro",
            ElementClass::MacroRequirement,
            CaptureRule::Regex {
                expression: r"\[MacroReq_([^\]]+)".into(),
                sub_expression: None,
                end_expression: Some(r"\[End_of_MacroReq\]".into()),
                row_expression: None,
            },
        );
        macro_req.flags.nested = true;
        t.element_types.push(macro_req);
        t.element_types.push(re(
            "Req",
            ElementClass::Requirement,
            r"^[ \t]*(\S*REQ\d+)(?:[ \t]*:[ \t]*(.+)|)$",
        ));
        t.element_types.push(Et::new(
            "Covers",
            ElementClass::Reference,
            CaptureRule::Regex {
                expression: r"\[Covers[ \t]*:[ \t]*([^\]]*)".into(),
                sub_expression: Some(r"([^,;\s]+)".into()),
                end_expression: None,
                row_expression: None,
            },
        ));
        let mut prio = re(
            "Priority",
            ElementClass::Attribute,
            r"^[ \t]*-?[ \t]*Priority[ \t]*:[ \t]*(\S+)",
        );
        prio.enum_values = vec!["High".into(), "Low".into()];
        t.element_types.push(prio);
        t.element_types
            .push(re("Text", ElementClass::Text, r"<<([\s\S]+?)>>"));
        t
    }

    #[test]
    fn builds_sections_requirements_and_attachments() {
        let input = "Intro text\n1. Intro\n<<Doc text>>\n1.1. Scope\nREQ1 : Capture\n<<Line one\nline two>>\n- Priority : High\n\
                     LL-REQ2\n[Covers: REQ1, REQ3]\n2. Next\nREQ4\n- Priority : Medium\n";
        let doc = capture_text(&text_type(), "Spec", input);
        let names: Vec<_> = doc
            .elements
            .iter()
            .map(|e| (e.type_name.as_str(), e.ident.as_deref(), e.parent))
            .collect();
        assert_eq!(
            names,
            [
                ("H1", Some("1"), None),
                ("H2", Some("1.1"), Some(0)),
                ("Req", Some("REQ1"), Some(1)),
                ("Req", Some("LL-REQ2"), Some(1)),
                ("H1", Some("2"), None),
                ("Req", Some("REQ4"), Some(4)),
            ]
        );
        assert_eq!(doc.elements[0].label.as_deref(), Some("Intro"));
        assert_eq!(doc.elements[0].text, "Doc text");
        assert_eq!(doc.elements[2].label.as_deref(), Some("Capture"));
        assert_eq!(doc.elements[2].text, "Line one\nline two");
        assert_eq!(doc.elements[2].attributes[0].value.as_deref(), Some("High"));
        let targets: Vec<_> = doc.elements[3]
            .references
            .iter()
            .map(|r| r.target.as_str())
            .collect();
        assert_eq!(targets, ["REQ1", "REQ3"]);
        assert_eq!(doc.messages.len(), 1);
        assert_eq!(doc.messages[0].rule, "badAttributeValue");
        assert_eq!(doc.messages[0].line, Some(13));
    }

    #[test]
    fn nests_macro_requirements_until_their_end() {
        let input = "1. S\n[MacroReq_M1]\nREQ1\n[MacroReq_M2]\nREQ2\n[End_of_MacroReq]\nREQ3\n[End_of_MacroReq]\nREQ4\n";
        let doc = capture_text(&text_type(), "Spec", input);
        let tree: Vec<_> = doc
            .elements
            .iter()
            .map(|e| (e.ident.as_deref().unwrap(), e.parent))
            .collect();
        assert_eq!(
            tree,
            [
                ("1", None),
                ("M1", Some(0)),
                ("REQ1", Some(1)),
                ("M2", Some(1)),
                ("REQ2", Some(3)),
                ("REQ3", Some(1)),
                ("REQ4", Some(0))
            ]
        );
        assert!(doc.messages.is_empty(), "{:?}", doc.messages);
    }

    #[test]
    fn reports_unbalanced_end_expressions() {
        let doc = capture_text(&text_type(), "Spec", "[End_of_MacroReq]\n[MacroReq_M]\n");
        let rules: Vec<_> = doc.messages.iter().map(|m| m.rule.as_str()).collect();
        assert_eq!(rules, ["closingWithoutOpening", "openingWithoutClosing"]);
    }

    #[test]
    fn honours_capture_roles() {
        let mut t = DocumentType::new("W", TypeKind::Text, "docx");
        let mut h = re(
            "Heading 1",
            ElementClass::Section,
            r"^[Hh]eading 1\t(?:([\d\.]+)[ \t](.+)|([^\d\n].+))$",
        );
        h.roles = [1, 2, 1, 0]
            .into_iter()
            .map(CaptureRole::from_code)
            .collect();
        t.element_types.push(h);
        let doc = capture_text(&t, "D", "heading 1\t1.\tIntro\nheading 1\tAnnexes\n");
        assert_eq!(doc.elements[0].ident.as_deref(), Some("1."));
        assert_eq!(doc.elements[0].label.as_deref(), Some("Intro"));
        assert_eq!(doc.elements[1].ident.as_deref(), Some("Annexes"));
        assert_eq!(doc.elements[1].label, None);
    }

    #[test]
    fn nests_sections_by_captured_depth() {
        let mut t = DocumentType::new("W", TypeKind::Text, "docx");
        let mut h = re(
            "Heading",
            ElementClass::Section,
            r"^[Hh]eading (\d+)\t(?:([\d\.]+)[ \t](.*)|([^\d\n].+))$",
        );
        h.roles = [6, 1, 2, 1]
            .into_iter()
            .map(CaptureRole::from_code)
            .collect();
        t.element_types.push(h);
        t.element_types.push(re(
            "Req",
            ElementClass::Requirement,
            r"^Requirement_ID\t(\S+)",
        ));
        let input = "heading 1\t1\tA\nheading 2\t1.1\tB\nRequirement_ID\tR1\nheading 3\t1.1.1\tC\nheading 2\t1.2\tD\nheading 1\tAnnexe\n";
        let doc = capture_text(&t, "D", input);
        let tree: Vec<_> = doc
            .elements
            .iter()
            .map(|e| (e.ident.as_deref().unwrap(), e.parent))
            .collect();
        assert_eq!(
            tree,
            [
                ("1", None),
                ("1.1", Some(0)),
                ("R1", Some(1)),
                ("1.1.1", Some(1)),
                ("1.2", Some(0)),
                ("Annexe", None)
            ]
        );
    }

    #[test]
    fn excludes_and_deletes() {
        let mut t = text_type();
        t.exclude_text = vec![r"<<<[\s\S]*?>>>".into()];
        t.delete_expression = Some(r"(\S*REQ\d+)[ \t]*:[ \t]*\(Obsolete\)".into());
        let doc = capture_text(
            &t,
            "S",
            "1. S\n<<<REQ9>>>\nREQ1\n- Priority : High\nREQ2 : (Obsolete)\n<<gone>>\n",
        );
        let idents: Vec<_> = doc
            .elements
            .iter()
            .map(|e| e.ident.as_deref().unwrap())
            .collect();
        assert_eq!(idents, ["1", "REQ1"]);
        assert_eq!(doc.elements[0].children, [1]);
        assert!(doc.messages.is_empty(), "{:?}", doc.messages);
    }

    #[test]
    fn reports_invalid_expressions() {
        let mut t = DocumentType::new("T", TypeKind::Text, "text");
        t.element_types
            .push(re("Bad", ElementClass::Requirement, "(unclosed"));
        let doc = capture_text(&t, "D", "x");
        assert_eq!(doc.messages[0].rule, "analyseError");
    }
}
