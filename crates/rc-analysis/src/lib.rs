//! Analyse : résolution des références, taux de couverture, exigences dérivées et
//! contrôle des règles.
//!
//! Comportements : `docs/functional-spec.md` §3.4 à §3.8.
//! - Couverture d'un canal : exigences feuilles du document couvert référencées par
//!   au moins un document couvrant du canal, sur le total des exigences feuilles.
//! - Plusieurs canaux vers le même document (combinaison « ET ») : une exigence
//!   n'est couverte que si elle l'est dans chaque canal.
//! - Une macro-exigence référencée directement couvre toutes ses exigences ; une
//!   macro-exigence qui référence l'amont fait couvrir ses exigences contenues.
//! - Une exigence d'un document couvrant qui ne couvre rien est dérivée.

use rc_model::{CapturedDocument, ElemId, ElementClass, Message, Severity};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Canal de couverture entre documents (indices dans la liste des documents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInput {
    pub covered: usize,
    pub covering: Vec<usize>,
}

/// Élément désigné par son document et son indice ; `None` = le document lui-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Target {
    pub document: usize,
    pub element: Option<ElemId>,
}

/// Lien de couverture résolu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverLink {
    /// Élément couvrant.
    pub from: Target,
    /// Exigence couverte.
    pub to: Target,
    pub channel: usize,
    pub reference_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChannelCoverage {
    pub covered: usize,
    pub covering: Vec<usize>,
    pub covered_count: usize,
    pub total: usize,
    /// Pour chaque document couvrant : exigences qu'il couvre.
    pub per_document: Vec<(usize, usize)>,
}

impl ChannelCoverage {
    pub fn ratio(&self) -> f64 {
        ratio(self.covered_count, self.total)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DocumentSummary {
    /// Exigences feuilles.
    pub requirements: usize,
    /// Couvertes dans tous les canaux (`None` si le document n'est couvert par aucun canal).
    pub covered: Option<usize>,
    pub derived: usize,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Analysis {
    pub documents: Vec<CapturedDocument>,
    pub links: Vec<CoverLink>,
    pub channels: Vec<ChannelCoverage>,
    pub summaries: Vec<DocumentSummary>,
    pub derived: BTreeSet<(usize, ElemId)>,
    pub uncovered: BTreeSet<(usize, ElemId)>,
    /// Messages de capture et violations de règles.
    pub messages: Vec<Message>,
}

impl Analysis {
    pub fn element_name(&self, target: Target) -> String {
        let doc = &self.documents[target.document];
        match target.element {
            Some(e) => doc.elements[e].display_name(),
            None => doc.name.clone(),
        }
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.messages
            .iter()
            .filter(|m| m.severity == severity)
            .count()
    }
}

pub fn ratio(covered: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        covered as f64 / total as f64
    }
}

/// Exigences feuilles : exigences et macro-exigences sans exigence descendante.
pub fn leaf_requirements(doc: &CapturedDocument) -> Vec<ElemId> {
    (0..doc.elements.len())
        .filter(|&i| {
            doc.elements[i].class.is_requirement()
                && !doc
                    .descendants(i)
                    .iter()
                    .any(|&d| doc.elements[d].class.is_requirement())
        })
        .collect()
}

pub fn analyze(documents: Vec<CapturedDocument>, channels: &[ChannelInput]) -> Analysis {
    let mut a = Analysis {
        documents,
        ..Default::default()
    };
    for doc in &a.documents {
        a.messages.extend(doc.messages.iter().cloned());
    }

    // Index des exigences par identifiant (sans casse), et de tous les éléments
    // structurels pour les références inverses.
    let mut requirements: HashMap<String, Vec<(usize, ElemId)>> = HashMap::new();
    let mut structural: HashMap<String, Vec<(usize, ElemId)>> = HashMap::new();
    for (d, doc) in a.documents.iter().enumerate() {
        let mut seen: HashMap<(String, ElementClass), ElemId> = HashMap::new();
        for (i, e) in doc.elements.iter().enumerate() {
            let Some(ident) = &e.ident else { continue };
            let key = ident.to_lowercase();
            if e.class.is_structural() {
                structural.entry(key.clone()).or_default().push((d, i));
            }
            if e.class.is_requirement() {
                requirements.entry(key.clone()).or_default().push((d, i));
                if seen.insert((key, e.class), i).is_some() {
                    let rule = if e.class == ElementClass::Requirement {
                        "requirementMultipleTimesDefined"
                    } else {
                        "macroRequirementMultipleTimesDefined"
                    };
                    push(
                        &mut a.messages,
                        rule,
                        Severity::Error,
                        format!("{ident} est défini plusieurs fois"),
                        doc,
                        Some(i),
                        e.line,
                    );
                }
            }
        }
    }

    let channel_between = |covered: usize, covering: usize| {
        channels
            .iter()
            .position(|c| c.covered == covered && c.covering.contains(&covering))
    };

    // Résolution des références.
    let mut links = Vec::new();
    let mut new_messages = Vec::new();
    for (d, doc) in a.documents.iter().enumerate() {
        let owners = std::iter::once((None, &doc.references)).chain(
            doc.elements
                .iter()
                .enumerate()
                .map(|(i, e)| (Some(i), &e.references)),
        );
        for (owner, refs) in owners {
            for r in refs {
                let key = r.target.to_lowercase();
                if r.inverse {
                    let Some(o) = owner else {
                        push(
                            &mut new_messages,
                            "inverseReferenceWithoutRequirement",
                            Severity::Error,
                            format!("référence inverse vers {} hors d'une exigence", r.target),
                            doc,
                            None,
                            r.line,
                        );
                        continue;
                    };
                    let candidates = structural.get(&key).map(Vec::as_slice).unwrap_or_default();
                    resolve(
                        &mut links,
                        &mut new_messages,
                        doc,
                        r,
                        candidates,
                        |&(x, t)| {
                            channel_between(d, x).map(|c| {
                                (
                                    Target {
                                        document: x,
                                        element: Some(t),
                                    },
                                    Target {
                                        document: d,
                                        element: Some(o),
                                    },
                                    c,
                                )
                            })
                        },
                        owner,
                    );
                } else {
                    let candidates = requirements
                        .get(&key)
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    if candidates.iter().any(|&(u, t)| u == d && Some(t) == owner) {
                        push(
                            &mut new_messages,
                            "coversItself",
                            Severity::Error,
                            format!("{} se couvre lui-même", r.target),
                            doc,
                            owner,
                            r.line,
                        );
                        continue;
                    }
                    resolve(
                        &mut links,
                        &mut new_messages,
                        doc,
                        r,
                        candidates,
                        |&(u, t)| {
                            channel_between(u, d).map(|c| {
                                (
                                    Target {
                                        document: d,
                                        element: owner,
                                    },
                                    Target {
                                        document: u,
                                        element: Some(t),
                                    },
                                    c,
                                )
                            })
                        },
                        owner,
                    );
                }
            }
            // Liens hors couverture : seule l'existence de la cible est vérifiée.
            if let Some(o) = owner {
                for l in &doc.elements[o].links {
                    if !structural.contains_key(&l.target.to_lowercase()) {
                        push(
                            &mut new_messages,
                            "linkToUnknown",
                            Severity::Error,
                            format!("la cible du lien {} est introuvable", l.target),
                            doc,
                            Some(o),
                            l.line,
                        );
                    }
                }
            }
        }
    }
    a.messages.extend(new_messages);

    // Exigences couvertes, par canal.
    let mut covered_in: Vec<HashSet<ElemId>> = vec![HashSet::new(); channels.len()];
    let mut covered_by_doc: Vec<HashMap<usize, HashSet<ElemId>>> =
        vec![HashMap::new(); channels.len()];
    for l in &links {
        let doc = &a.documents[l.to.document];
        let Some(t) = l.to.element else { continue };
        let mut hit = vec![t];
        if doc.elements[t].class == ElementClass::MacroRequirement {
            hit.extend(doc.descendants(t));
        }
        covered_in[l.channel].extend(hit.iter().copied());
        covered_by_doc[l.channel]
            .entry(l.from.document)
            .or_default()
            .extend(hit);
    }

    let leaves: Vec<Vec<ElemId>> = a.documents.iter().map(leaf_requirements).collect();
    for (c, ch) in channels.iter().enumerate() {
        let total = leaves[ch.covered].len();
        let count = |set: &HashSet<ElemId>| {
            leaves[ch.covered]
                .iter()
                .filter(|e| set.contains(e))
                .count()
        };
        a.channels.push(ChannelCoverage {
            covered: ch.covered,
            covering: ch.covering.clone(),
            covered_count: count(&covered_in[c]),
            total,
            per_document: ch
                .covering
                .iter()
                .map(|&k| (k, covered_by_doc[c].get(&k).map(count).unwrap_or(0)))
                .collect(),
        });
    }

    // Exigences non couvertes (dans au moins un canal) et résumé par document.
    a.summaries = vec![DocumentSummary::default(); a.documents.len()];
    for (d, doc) in a.documents.iter().enumerate() {
        a.summaries[d].requirements = leaves[d].len();
        let incoming: Vec<usize> = (0..channels.len())
            .filter(|&c| channels[c].covered == d)
            .collect();
        if incoming.is_empty() {
            continue;
        }
        let mut covered = 0;
        for &e in &leaves[d] {
            if incoming.iter().all(|&c| covered_in[c].contains(&e)) {
                covered += 1;
            } else {
                a.uncovered.insert((d, e));
                let el = &doc.elements[e];
                push(
                    &mut a.messages,
                    "uncovered",
                    Severity::Error,
                    format!("{} n'est pas couverte", el.display_name()),
                    doc,
                    Some(e),
                    el.line,
                );
            }
        }
        a.summaries[d].covered = Some(covered);
    }

    // Exigences dérivées et entités non couvrantes, dans les documents couvrants.
    let mut covering_elements: HashSet<(usize, ElemId)> = HashSet::new();
    for l in &links {
        if let Some(e) = l.from.element {
            covering_elements.insert((l.from.document, e));
        }
    }
    for (d, doc) in a.documents.iter().enumerate() {
        let covers_something = channels.iter().any(|c| c.covering.contains(&d));
        for (i, e) in doc.elements.iter().enumerate() {
            let covers = |x: ElemId| covering_elements.contains(&(d, x));
            let self_or_macro = covers(i)
                || doc
                    .ancestors(i)
                    .any(|p| doc.elements[p].class == ElementClass::MacroRequirement && covers(p));
            match e.class {
                ElementClass::Requirement | ElementClass::MacroRequirement if covers_something => {
                    if leaves[d].contains(&i) && !self_or_macro {
                        a.derived.insert((d, i));
                        push(
                            &mut a.messages,
                            "derived",
                            Severity::Warning,
                            format!(
                                "{} est dérivée (elle ne couvre aucune exigence)",
                                e.display_name()
                            ),
                            doc,
                            Some(i),
                            e.line,
                        );
                    }
                }
                ElementClass::Entity => {
                    let any_reference = !e.references.is_empty()
                        || doc
                            .descendants(i)
                            .iter()
                            .any(|&x| !doc.elements[x].references.is_empty());
                    if !any_reference {
                        push(
                            &mut a.messages,
                            "coveringNothing",
                            Severity::Warning,
                            format!("l'entité {} ne couvre rien", e.display_name()),
                            doc,
                            Some(i),
                            e.line,
                        );
                    }
                }
                _ => {}
            }
        }
    }
    for &(d, _) in &a.derived {
        a.summaries[d].derived += 1;
    }
    for m in &a.messages {
        let Some(d) = m
            .document
            .as_ref()
            .and_then(|n| a.documents.iter().position(|doc| &doc.name == n))
        else {
            continue;
        };
        match m.severity {
            Severity::Error => a.summaries[d].errors += 1,
            Severity::Warning => a.summaries[d].warnings += 1,
            Severity::Information => {}
        }
    }
    a.links = links;
    a
}

/// Valide une référence contre ses cibles possibles et enregistre les liens.
fn resolve<F>(
    links: &mut Vec<CoverLink>,
    messages: &mut Vec<Message>,
    doc: &CapturedDocument,
    r: &rc_model::Reference,
    candidates: &[(usize, ElemId)],
    mut link_for: F,
    owner: Option<ElemId>,
) where
    F: FnMut(&(usize, ElemId)) -> Option<(Target, Target, usize)>,
{
    if candidates.is_empty() {
        push(
            messages,
            "coverUnknown",
            Severity::Error,
            format!(
                "la cible {} de la référence est introuvable dans le projet",
                r.target
            ),
            doc,
            owner,
            r.line,
        );
        return;
    }
    let mut any = false;
    for c in candidates {
        if let Some((from, to, channel)) = link_for(c) {
            links.push(CoverLink {
                from,
                to,
                channel,
                reference_type: r.type_name.clone(),
            });
            any = true;
        }
    }
    if !any {
        push(
            messages,
            "traceabilityGraphViolation",
            Severity::Error,
            format!(
                "la référence vers {} ne suit aucun canal du projet",
                r.target
            ),
            doc,
            owner,
            r.line,
        );
    }
}

fn push(
    messages: &mut Vec<Message>,
    rule: &str,
    severity: Severity,
    text: String,
    doc: &CapturedDocument,
    element: Option<ElemId>,
    line: usize,
) {
    messages.push(Message {
        rule: rule.to_owned(),
        severity,
        text,
        document: Some(doc.name.clone()),
        element,
        line: (line > 0).then_some(line),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_model::{Element, Reference};

    fn req(id: &str, parent: Option<ElemId>, covers: &[&str]) -> Element {
        let mut e = Element::new("Req", ElementClass::Requirement);
        e.ident = Some(id.into());
        e.parent = parent;
        e.references = covers
            .iter()
            .map(|t| Reference {
                type_name: "Covers".into(),
                target: (*t).into(),
                target_document: None,
                inverse: false,
                attributes: Vec::new(),
                line: 1,
            })
            .collect();
        e
    }

    fn doc(name: &str, elements: Vec<Element>) -> CapturedDocument {
        let mut d = CapturedDocument {
            name: name.into(),
            ..Default::default()
        };
        d.roots = (0..elements.len())
            .filter(|&i| elements[i].parent.is_none())
            .collect();
        d.elements = elements;
        for i in 0..d.elements.len() {
            if let Some(p) = d.elements[i].parent {
                d.elements[p].children.push(i);
            }
        }
        d
    }

    fn rules(a: &Analysis) -> Vec<(&str, String)> {
        a.messages
            .iter()
            .map(|m| {
                (
                    m.rule.as_str(),
                    a.element_name(Target {
                        document: a
                            .documents
                            .iter()
                            .position(|d| Some(&d.name) == m.document.as_ref())
                            .unwrap(),
                        element: m.element,
                    }),
                )
            })
            .collect()
    }

    #[test]
    fn computes_coverage_derived_and_errors() {
        let spec = doc(
            "Spec",
            vec![
                req("R1", None, &[]),
                req("R2", None, &[]),
                req("R3", None, &[]),
            ],
        );
        let design = doc(
            "Design",
            vec![
                req("D1", None, &["R1"]),
                req("D2", None, &["r2", "R9"]),
                req("D3", None, &[]),
            ],
        );
        let a = analyze(
            vec![spec, design],
            &[ChannelInput {
                covered: 0,
                covering: vec![1],
            }],
        );
        assert_eq!((a.channels[0].covered_count, a.channels[0].total), (2, 3));
        assert_eq!(a.summaries[0].covered, Some(2));
        assert_eq!(a.derived.iter().copied().collect::<Vec<_>>(), [(1, 2)]);
        let mut r = rules(&a);
        r.sort();
        assert_eq!(
            r,
            [
                ("coverUnknown", "D2".into()),
                ("derived", "D3".into()),
                ("uncovered", "R3".into())
            ]
        );
    }

    #[test]
    fn requires_a_channel_and_combines_and_or() {
        let spec = doc("Spec", vec![req("R1", None, &[]), req("R2", None, &[])]);
        let design = doc(
            "Design",
            vec![req("D1", None, &["R1"]), req("D2", None, &["R2"])],
        );
        let tests = doc("Tests", vec![req("T1", None, &["R1"])]);
        let stray = doc("Stray", vec![req("S1", None, &["R1"])]);
        // ET : deux canaux indépendants.
        let and = analyze(
            vec![spec.clone(), design.clone(), tests.clone(), stray.clone()],
            &[
                ChannelInput {
                    covered: 0,
                    covering: vec![1],
                },
                ChannelInput {
                    covered: 0,
                    covering: vec![2],
                },
            ],
        );
        assert_eq!(and.summaries[0].covered, Some(1));
        assert!(rules(&and).contains(&("traceabilityGraphViolation", "S1".into())));
        // OU : un canal combiné.
        let or = analyze(
            vec![spec, design, tests, stray],
            &[ChannelInput {
                covered: 0,
                covering: vec![1, 2],
            }],
        );
        assert_eq!(or.summaries[0].covered, Some(2));
        assert_eq!(or.channels[0].per_document, [(1, 2), (2, 1)]);
    }

    #[test]
    fn handles_macro_requirements() {
        let mut m = req("M", None, &[]);
        m.class = ElementClass::MacroRequirement;
        let spec = doc(
            "Spec",
            vec![m, req("R1", Some(0), &[]), req("R2", Some(0), &[])],
        );
        let mut cm = req("CM", None, &["M"]);
        cm.class = ElementClass::MacroRequirement;
        let design = doc("Design", vec![cm, req("D1", Some(0), &[])]);
        let a = analyze(
            vec![spec, design],
            &[ChannelInput {
                covered: 0,
                covering: vec![1],
            }],
        );
        // M référencée directement : R1 et R2 couvertes ; D1 couvre via sa macro.
        assert_eq!((a.channels[0].covered_count, a.channels[0].total), (2, 2));
        assert!(a.derived.is_empty(), "{:?}", a.messages);
    }

    #[test]
    fn reports_duplicates_and_self_coverage() {
        let spec = doc("Spec", vec![req("R1", None, &["R1"]), req("r1", None, &[])]);
        let a = analyze(vec![spec], &[]);
        let r: Vec<_> = rules(&a).into_iter().map(|(r, _)| r).collect();
        assert!(r.contains(&"requirementMultipleTimesDefined"));
        assert!(r.contains(&"coversItself"));
    }
}
