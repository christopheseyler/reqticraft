//! Types d'analyse : comment lire un document et y reconnaître les éléments.

use crate::element::ElementClass;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// Intermédiaire texte, capture par expressions régulières.
    Text,
    /// Intermédiaire XML, capture par arbres d'analyse.
    Xml,
    /// Éléments que l'utilisateur peut ajouter depuis l'outil (aucune capture).
    AddedElements,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentType {
    pub name: String,
    pub kind: TypeKind,
    /// Identifiant du convertisseur (natif → intermédiaire), tel qu'écrit dans le fichier.
    pub converter: String,
    pub edit_tool: Option<String>,
    /// Extensions sans le point (`docx`, `xlsx`…).
    pub file_extensions: Vec<String>,
    /// Motifs de noms pour les documents « répertoire » (`*.c`, `*.h`…).
    pub matching_file_names: Vec<String>,
    pub excluded_file_names: Vec<String>,
    pub variables: Vec<VariableDecl>,
    pub properties: Vec<(String, String)>,
    /// Zones de texte ignorées avant la capture.
    pub exclude_text: Vec<String>,
    /// Expression qui capture l'identifiant des éléments à supprimer.
    pub delete_expression: Option<String>,
    pub element_types: Vec<ElementType>,
    /// Fichier d'où vient la définition (diagnostic).
    pub source: Option<PathBuf>,
}

impl DocumentType {
    pub fn new(name: impl Into<String>, kind: TypeKind, converter: impl Into<String>) -> Self {
        DocumentType {
            name: name.into(),
            kind,
            converter: converter.into(),
            edit_tool: None,
            file_extensions: Vec::new(),
            matching_file_names: Vec::new(),
            excluded_file_names: Vec::new(),
            variables: Vec::new(),
            properties: Vec::new(),
            exclude_text: Vec::new(),
            delete_expression: None,
            element_types: Vec::new(),
            source: None,
        }
    }

    pub fn element_type(&self, name: &str) -> Option<&ElementType> {
        self.element_types.iter().find(|e| e.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDecl {
    pub name: String,
    pub label: Option<String>,
    pub var_type: Option<String>,
}

/// Rôle d'un groupe de capture d'une expression régulière.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureRole {
    None,
    Ident,
    Label,
    Text,
    Guid,
    Parent,
    Depth,
}

impl CaptureRole {
    /// Code numérique des fichiers Reqtify (`Indexes=1,2,1,0`, `index1="1"`).
    ///
    /// Inférence documentée dans `docs/interop-formats.md` §4.1 : le k-ième nombre
    /// donne le rôle du k-ième groupe (0 aucun, 1 identifiant, 2 libellé, 3 texte).
    /// Les codes 4 à 6 sont supposés suivre l'ordre du menu de l'éditeur.
    pub fn from_code(code: u32) -> CaptureRole {
        match code {
            1 => CaptureRole::Ident,
            2 => CaptureRole::Label,
            3 => CaptureRole::Text,
            4 => CaptureRole::Guid,
            5 => CaptureRole::Parent,
            6 => CaptureRole::Depth,
            _ => CaptureRole::None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementFlags {
    pub hidden: bool,
    pub ignored: bool,
    pub multiple: bool,
    pub nested: bool,
    /// Référence « est couvert par » (portée par l'exigence amont).
    pub inverse: bool,
    pub boolean: bool,
    pub identify_children: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementType {
    pub name: String,
    pub class: ElementClass,
    pub rule: CaptureRule,
    /// Types d'éléments autorisés comme parents (le nom du type de document = racine).
    pub parents: Vec<String>,
    /// Format d'affichage (`$i`, `$l`, `$t`…).
    pub display: Option<String>,
    /// Rôle de chaque groupe de capture ; vide = rôles par défaut.
    pub roles: Vec<CaptureRole>,
    pub flags: ElementFlags,
    pub enum_values: Vec<String>,
    pub properties: Vec<(String, String)>,
}

impl ElementType {
    pub fn new(name: impl Into<String>, class: ElementClass, rule: CaptureRule) -> Self {
        ElementType {
            name: name.into(),
            class,
            rule,
            parents: Vec::new(),
            display: None,
            roles: Vec::new(),
            flags: ElementFlags::default(),
            enum_values: Vec::new(),
            properties: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CaptureRule {
    Regex {
        expression: String,
        sub_expression: Option<String>,
        end_expression: Option<String>,
        /// Lignes d'un tableau (`reRow`).
        row_expression: Option<String>,
    },
    XmlTree {
        pattern: String,
        /// Découpe la valeur capturée en plusieurs cibles (références, liens).
        sub_expression: Option<String>,
    },
    /// Aucune capture (types pour éléments ajoutés).
    None,
}

impl CaptureRule {
    pub fn regex(expression: impl Into<String>) -> Self {
        CaptureRule::Regex {
            expression: expression.into(),
            sub_expression: None,
            end_expression: None,
            row_expression: None,
        }
    }
}
