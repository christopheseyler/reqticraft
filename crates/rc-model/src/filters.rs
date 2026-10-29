//! Filtres, marques et documents de modification.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FilterSet {
    pub filters: Vec<Filter>,
    /// Filtre actif à l'ouverture du projet.
    pub current: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub name: String,
    /// `true` : toutes les conditions (ET) ; `false` : au moins une (OU).
    pub all: bool,
    pub conditions: Vec<FilterCondition>,
    /// Drapeaux bruts de l'ancien format (`Booleans=`), sens inconnu.
    pub raw_flags: Vec<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FilterCondition {
    pub application_class: Option<String>,
    pub receiver_class: String,
    pub receiver_document_type: Option<String>,
    pub receiver_element_type: Option<String>,
    pub operator: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub name: String,
    pub label: String,
    /// Couleur RGB.
    pub color: (u8, u8, u8),
}

/// Élément d'un document de modification : ajout ou complément à un élément capturé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modification {
    /// Balise d'origine : `Requirement`, `Section`, `Reference`, `Attribute`, `Link`…
    pub tag: String,
    pub type_name: String,
    pub ident: Option<String>,
    pub label: Option<String>,
    pub children: Vec<Modification>,
}
