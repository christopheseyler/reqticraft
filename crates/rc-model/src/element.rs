//! Éléments de traçabilité capturés dans les documents.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementClass {
    Section,
    MacroRequirement,
    Requirement,
    Entity,
    /// Référence de couverture.
    Reference,
    Attribute,
    ReferenceAttribute,
    /// Lien hors couverture.
    Link,
    Value,
    Text,
    Picture,
    Table,
}

impl ElementClass {
    /// Éléments qui forment l'arbre d'un document.
    pub fn is_structural(self) -> bool {
        matches!(
            self,
            ElementClass::Section
                | ElementClass::MacroRequirement
                | ElementClass::Requirement
                | ElementClass::Entity
        )
    }

    /// Éléments qu'une référence de couverture peut viser.
    pub fn is_requirement(self) -> bool {
        matches!(
            self,
            ElementClass::Requirement | ElementClass::MacroRequirement
        )
    }
}

/// Indice d'un élément dans [`CapturedDocument::elements`].
pub type ElemId = usize;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub type_name: String,
    pub class: ElementClass,
    pub ident: Option<String>,
    pub label: Option<String>,
    pub guid: Option<String>,
    pub text: String,
    /// `None` : directement sous la racine du document.
    pub parent: Option<ElemId>,
    pub children: Vec<ElemId>,
    pub attributes: Vec<AttributeValue>,
    pub references: Vec<Reference>,
    pub links: Vec<Reference>,
    /// Ligne (base 1) dans le fichier intermédiaire, pour la navigation.
    pub line: usize,
    /// Élément ajouté par un document de modification.
    pub added: bool,
}

impl Element {
    pub fn new(type_name: impl Into<String>, class: ElementClass) -> Self {
        Element {
            type_name: type_name.into(),
            class,
            ident: None,
            label: None,
            guid: None,
            text: String::new(),
            parent: None,
            children: Vec::new(),
            attributes: Vec::new(),
            references: Vec::new(),
            links: Vec::new(),
            line: 0,
            added: false,
        }
    }

    /// Nom affiché : identifiant et libellé quand ils diffèrent.
    pub fn display_name(&self) -> String {
        match (&self.ident, &self.label) {
            (Some(i), Some(l)) if i != l => format!("{i} {l}"),
            (Some(i), _) => i.clone(),
            (None, Some(l)) => l.clone(),
            (None, None) => self.type_name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeValue {
    pub name: String,
    /// `None` pour un attribut booléen (présence seule).
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub type_name: String,
    /// Identifiant visé, tel que capturé.
    pub target: String,
    /// Document visé, s'il est précisé.
    pub target_document: Option<String>,
    /// « Est couvert par » : la cible couvre l'élément porteur.
    pub inverse: bool,
    pub attributes: Vec<AttributeValue>,
    pub line: usize,
}

/// Résultat de la capture d'un document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapturedDocument {
    pub name: String,
    pub type_name: String,
    pub elements: Vec<Element>,
    /// Éléments de premier niveau.
    pub roots: Vec<ElemId>,
    /// Texte, attributs et références rattachés au document lui-même.
    pub text: String,
    pub attributes: Vec<AttributeValue>,
    pub references: Vec<Reference>,
    pub messages: Vec<Message>,
}

impl CapturedDocument {
    /// Ancêtres d'un élément, du parent direct à la racine.
    pub fn ancestors(&self, id: ElemId) -> impl Iterator<Item = ElemId> + '_ {
        std::iter::successors(self.elements[id].parent, move |&p| self.elements[p].parent)
    }

    /// Descendants d'un élément (parcours en profondeur).
    pub fn descendants(&self, id: ElemId) -> Vec<ElemId> {
        let mut out = Vec::new();
        let mut stack: Vec<ElemId> = self.elements[id].children.iter().rev().copied().collect();
        while let Some(e) = stack.pop() {
            out.push(e);
            stack.extend(self.elements[e].children.iter().rev().copied());
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Information,
}

/// Message de capture ou violation de règle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Identifiant public de la règle (`uncovered`, `coverUnknown`…).
    pub rule: String,
    pub severity: Severity,
    pub text: String,
    pub document: Option<String>,
    pub element: Option<ElemId>,
    pub line: Option<usize>,
}

/// Clé stable d'un élément d'une analyse à l'autre.
///
/// Elle sert à rattacher marques, revues et ajouts. L'identifiant est comparé
/// sans tenir compte de la casse, le GUID en en tenant compte.
///
/// Ce n'est pas une relation d'équivalence (le GUID prime quand les deux clés
/// en ont un), d'où une méthode [`ElementKey::matches`] plutôt que `PartialEq`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ElementKey {
    pub document: String,
    pub element_type: String,
    pub ident: Option<String>,
    pub guid: Option<String>,
}

impl ElementKey {
    /// Indique si deux clés désignent le même élément.
    pub fn matches(&self, other: &Self) -> bool {
        if self.document != other.document || self.element_type != other.element_type {
            return false;
        }
        match (&self.guid, &other.guid) {
            (Some(a), Some(b)) => a == b,
            _ => match (&self.ident, &other.ident) {
                (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(ident: Option<&str>, guid: Option<&str>) -> ElementKey {
        ElementKey {
            document: "Spec".into(),
            element_type: "Requirement".into(),
            ident: ident.map(Into::into),
            guid: guid.map(Into::into),
        }
    }

    #[test]
    fn idents_compare_case_insensitively() {
        assert!(key(Some("REQ1"), None).matches(&key(Some("req1"), None)));
    }

    #[test]
    fn guids_compare_case_sensitively_and_win_over_idents() {
        assert!(!key(Some("REQ1"), Some("abc")).matches(&key(Some("REQ1"), Some("ABC"))));
        assert!(key(Some("REQ1"), Some("abc")).matches(&key(Some("REQ2"), Some("abc"))));
    }

    #[test]
    fn walks_ancestors_and_descendants() {
        let mut doc = CapturedDocument::default();
        for _ in 0..3 {
            doc.elements.push(Element::new("S", ElementClass::Section));
        }
        doc.elements[1].parent = Some(0);
        doc.elements[2].parent = Some(1);
        doc.elements[0].children = vec![1];
        doc.elements[1].children = vec![2];
        assert_eq!(doc.ancestors(2).collect::<Vec<_>>(), vec![1, 0]);
        assert_eq!(doc.descendants(0), vec![1, 2]);
    }
}
