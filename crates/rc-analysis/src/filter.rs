//! Filtres : conditions sur les éléments, combinées en ET ou en OU.
//!
//! Comportement décrit dans `docs/functional-spec.md` §3.11. Les identifiants
//! d'opérateurs `equal`, `isPresent` et `isAbsent` sont observés dans les fichiers
//! `.filters` (`docs/interop-formats.md` §6) ; les autres sont propres à
//! ReqtiCraft, faute d'identifiants observés.
//!
//! Une condition porte sur un élément de la **classe d'application**
//! (`AbstractRequirement` par défaut : exigences et macro-exigences). Le
//! **receveur** est ce que la condition examine : un attribut nommé, l'identifiant,
//! le libellé, le texte, l'état de couverture, le type ou le document.

use crate::Analysis;
use rc_model::{CapturedDocument, ElemId, Element, ElementClass, Filter, FilterCondition};
use std::collections::{BTreeSet, HashMap};

/// Opérateurs reconnus, avec leur libellé et le besoin d'une valeur.
pub const OPERATORS: &[(&str, &str, bool)] = &[
    ("equal", "égal à", true),
    ("notEqual", "différent de", true),
    ("contains", "contient", true),
    ("notContains", "ne contient pas", true),
    ("matches", "correspond à l'expression", true),
    ("isPresent", "est présent", false),
    ("isAbsent", "est absent", false),
    ("greater", "supérieur à", true),
    ("less", "inférieur à", true),
    ("isCovered", "est couverte", false),
    ("isNotCovered", "n'est pas couverte", false),
    ("isDerived", "est dérivée", false),
    ("hasProblems", "a des problèmes", false),
    ("hasReferences", "a des références sortantes", false),
];

/// Receveurs reconnus (`receiverClass`).
pub const RECEIVERS: &[(&str, &str)] = &[
    ("Attribute", "Attribut"),
    ("Ident", "Identifiant"),
    ("Label", "Libellé"),
    ("Text", "Texte"),
    ("Type", "Type d'élément"),
    ("Document", "Nom du document"),
    ("Element", "Élément (couverture, problèmes)"),
];

/// Classes d'application reconnues (`applicationClass`).
pub const APPLICATION_CLASSES: &[(&str, &str)] = &[
    ("AbstractRequirement", "Exigence ou macro-exigence"),
    ("Requirement", "Exigence"),
    ("MacroRequirement", "Macro-exigence"),
    ("Section", "Section"),
    ("Entity", "Entité"),
    ("Any", "Exigence, entité ou section"),
];

/// Classe à laquelle un filtre s'applique, d'après sa première condition.
pub fn applies_to(filter: &Filter, class: ElementClass) -> bool {
    let name = filter
        .conditions
        .first()
        .and_then(|c| c.application_class.as_deref())
        .unwrap_or("AbstractRequirement");
    class_matches(name, class)
}

fn class_matches(name: &str, class: ElementClass) -> bool {
    match name {
        "Requirement" => class == ElementClass::Requirement,
        "MacroRequirement" => class == ElementClass::MacroRequirement,
        "Section" => class == ElementClass::Section,
        "Entity" | "EntityStrict" => class == ElementClass::Entity,
        "Any" => class.is_structural(),
        _ => class.is_requirement(),
    }
}

/// Contexte d'évaluation : analyse (pour les états) et paramètres `$nom`.
pub struct Context<'a> {
    pub analysis: Option<&'a Analysis>,
    pub params: &'a HashMap<String, String>,
}

impl Context<'_> {
    /// Remplace `$nom` par la valeur du paramètre, ou à défaut de la variable
    /// d'environnement du même nom.
    fn expand(&self, value: &str) -> String {
        let mut out = String::new();
        let mut rest = value;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let tail = &rest[i + 1..];
            let len = tail
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(tail.len());
            let name = &tail[..len];
            if name.is_empty() {
                out.push('$');
            } else if let Some(v) = self.params.get(name) {
                out.push_str(v);
            } else if let Ok(v) = std::env::var(name) {
                out.push_str(&v);
            } else {
                out.push('$');
                out.push_str(name);
            }
            rest = &tail[len..];
        }
        out.push_str(rest);
        out
    }
}

/// Paramètres `$nom` utilisés par un filtre.
pub fn parameters(filter: &Filter) -> Vec<String> {
    let mut out = BTreeSet::new();
    for c in &filter.conditions {
        let Some(v) = &c.value else { continue };
        let mut rest = v.as_str();
        while let Some(i) = rest.find('$') {
            let tail = &rest[i + 1..];
            let len = tail
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(tail.len());
            if len > 0 {
                out.insert(tail[..len].to_owned());
            }
            rest = &tail[len..];
        }
    }
    out.into_iter().collect()
}

/// Un élément passe-t-il le filtre ? Les éléments d'une autre classe que la
/// classe d'application passent toujours.
pub fn passes(
    filter: &Filter,
    doc_index: usize,
    doc: &CapturedDocument,
    e: ElemId,
    ctx: &Context,
) -> bool {
    let el = &doc.elements[e];
    if !applies_to(filter, el.class) || filter.conditions.is_empty() {
        return true;
    }
    let mut results = filter
        .conditions
        .iter()
        .map(|c| condition(c, doc_index, doc, el, e, ctx));
    if filter.all {
        results.all(|r| r)
    } else {
        results.any(|r| r)
    }
}

fn condition(
    c: &FilterCondition,
    d: usize,
    doc: &CapturedDocument,
    el: &Element,
    e: ElemId,
    ctx: &Context,
) -> bool {
    if let Some(t) = &c.receiver_document_type {
        if !t.is_empty() && t != "*" && *t != doc.type_name {
            // Condition sur un autre type de document : sans objet ici.
            return c.operator == "isAbsent";
        }
    }
    let expected = c
        .value
        .as_deref()
        .map(|v| ctx.expand(v))
        .unwrap_or_default();
    let status =
        |test: fn(&Analysis, usize, ElemId) -> bool| ctx.analysis.is_some_and(|a| test(a, d, e));
    match c.operator.as_str() {
        "isCovered" => {
            return status(|a, d, e| {
                a.summaries[d].covered.is_some()
                    && !a.uncovered.contains(&(d, e))
                    && !a.derived.contains(&(d, e))
            })
        }
        "isNotCovered" => return status(|a, d, e| a.uncovered.contains(&(d, e))),
        "isDerived" => return status(|a, d, e| a.derived.contains(&(d, e))),
        "hasProblems" => {
            return ctx.analysis.is_some_and(|a| {
                a.messages
                    .iter()
                    .any(|m| m.document.as_deref() == Some(&doc.name) && m.element == Some(e))
            })
        }
        "hasReferences" => return !el.references.is_empty(),
        _ => {}
    }
    let values: Vec<String> = match c.receiver_class.as_str() {
        "Ident" => el.ident.clone().into_iter().collect(),
        "Label" => el.label.clone().into_iter().collect(),
        "Text" => {
            let mut t = vec![el.text.clone()];
            t.extend(
                el.children
                    .iter()
                    .map(|&k| &doc.elements[k])
                    .filter(|k| !k.class.is_structural())
                    .map(|k| k.text.clone()),
            );
            vec![t.join("\n")]
        }
        "Type" => vec![el.type_name.clone()],
        "Document" => vec![doc.name.clone()],
        "Element" => vec![el.display_name()],
        // `Attribute` et receveurs inconnus : attribut nommé par le type d'élément.
        _ => {
            let name = c.receiver_element_type.as_deref().unwrap_or("");
            el.attributes
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case(name))
                .map(|a| a.value.clone().unwrap_or_default())
                .chain(
                    el.children
                        .iter()
                        .map(|&k| &doc.elements[k])
                        .filter(|k| {
                            k.class == ElementClass::Attribute
                                && k.type_name.eq_ignore_ascii_case(name)
                        })
                        .map(|k| {
                            k.ident
                                .clone()
                                .or(k.label.clone())
                                .unwrap_or_else(|| k.text.clone())
                        }),
                )
                .collect()
        }
    };
    let lower = expected.to_lowercase();
    let number = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
    match c.operator.as_str() {
        "isPresent" => !values.is_empty(),
        "isAbsent" => values.is_empty(),
        "equal" => values
            .iter()
            .any(|v| v.trim().eq_ignore_ascii_case(expected.trim())),
        "notEqual" => !values
            .iter()
            .any(|v| v.trim().eq_ignore_ascii_case(expected.trim())),
        "contains" => values.iter().any(|v| v.to_lowercase().contains(&lower)),
        "notContains" => !values.iter().any(|v| v.to_lowercase().contains(&lower)),
        "matches" => match regex_lite(&expected) {
            Some(re) => values
                .iter()
                .any(|v| re.is_match(v.as_bytes()).unwrap_or(false)),
            None => false,
        },
        "greater" => values
            .iter()
            .any(|v| matches!((number(v), number(&expected)), (Some(a), Some(b)) if a > b)),
        "less" => values
            .iter()
            .any(|v| matches!((number(v), number(&expected)), (Some(a), Some(b)) if a < b)),
        _ => false,
    }
}

fn regex_lite(pattern: &str) -> Option<pcre2::bytes::Regex> {
    pcre2::bytes::RegexBuilder::new()
        .utf(true)
        .caseless(true)
        .build(pattern)
        .ok()
}

/// Filtre d'analyse : les exigences filtrées sont neutralisées (sans
/// identifiant ni référence) avant l'analyse, qui recalcule les taux.
pub fn neutralize(documents: &mut [CapturedDocument], filter: &Filter, ctx: &Context) {
    for (d, doc) in documents.iter_mut().enumerate() {
        let failing: Vec<ElemId> = (0..doc.elements.len())
            .filter(|&e| doc.elements[e].class.is_structural() && !passes(filter, d, doc, e, ctx))
            .collect();
        for e in failing {
            let el = &mut doc.elements[e];
            el.class = ElementClass::Text;
            el.ident = None;
            el.references.clear();
            el.links.clear();
        }
    }
}
