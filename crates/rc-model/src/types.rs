//! Types d'analyse : comment lire un document et y reconnaître les éléments.

use crate::element::ElementClass;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// Intermédiaire texte, capture par expressions régulières.
    Text,
    /// Intermédiaire XML, capture par arbres d'analyse.
    Xml,
    /// Éléments que l'utilisateur peut ajouter depuis l'outil.
    AddedElements,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentType {
    pub name: String,
    pub kind: TypeKind,
    /// Identifiant du convertisseur (natif → intermédiaire).
    pub converter: String,
    pub edit_tool: Option<String>,
    pub file_extensions: Vec<String>,
    pub element_types: Vec<ElementType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementType {
    pub name: String,
    pub class: ElementClass,
    pub rule: CaptureRule,
    /// Types d'éléments autorisés comme parents.
    pub parents: Vec<String>,
    /// Format d'affichage (`$i`, `$l`, `$t`…).
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureRule {
    Regex {
        expression: String,
        sub_expression: Option<String>,
        end_expression: Option<String>,
    },
    XmlTree {
        pattern: String,
    },
    /// Type pour éléments ajoutés : aucune capture.
    None,
}
