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
}
