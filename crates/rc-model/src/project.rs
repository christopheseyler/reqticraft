//! Configuration d'un projet : documents, dossiers, sous-projets et canaux.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Indice d'un nœud dans [`Project::nodes`].
pub type NodeId = usize;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    /// Propriétés du projet ; un même nom peut apparaître plusieurs fois.
    pub properties: Vec<(String, String)>,
    pub nodes: Vec<ProjectNode>,
    pub channels: Vec<Channel>,
    /// Remplacement d'un document de sous-projet par un document du projet.
    pub path_replacements: Vec<PathReplacement>,
}

impl Project {
    /// Cherche un nœud par son nom (les noms de documents sont uniques dans un projet).
    pub fn node_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectNode {
    Document(DocumentDecl),
    Folder {
        name: String,
        default_type: Option<String>,
        children: Vec<NodeId>,
    },
    /// Sous-projet (projet composite), désigné par son fichier de résultats
    /// ou de configuration.
    SubProject {
        name: String,
        path: PathBuf,
    },
}

impl ProjectNode {
    pub fn name(&self) -> &str {
        match self {
            ProjectNode::Document(d) => &d.name,
            ProjectNode::Folder { name, .. } | ProjectNode::SubProject { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentDecl {
    pub name: String,
    /// Nom du type d'analyse appliqué au document.
    pub doc_type: String,
    /// Chemin relatif au répertoire du projet.
    pub path: PathBuf,
    pub absolute_path: Option<PathBuf>,
    pub category: Option<String>,
    /// Valeurs des variables déclarées par le type (images, texte riche…).
    pub params: Vec<(String, String)>,
    pub modification_docs: Vec<ModificationDocRef>,
    /// Types d'éléments à ignorer pour ce document.
    pub ignored_types: Vec<String>,
    /// Position dans le diagramme de l'éditeur de projet.
    pub position: Option<(i64, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModificationDocRef {
    pub path: PathBuf,
    pub doc_type: String,
}

/// Chemin d'un document : sous-projets traversés puis nom du document.
pub type ComponentPath = Vec<String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathReplacement {
    pub replacing: ComponentPath,
    pub replaced: Vec<ComponentPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// Toutes les exigences du document couvert doivent être couvertes.
    Cover,
    /// Les exigences amont doivent être dupliquées en aval.
    Mirror,
    /// Relation informative, sans contrainte.
    Dependency,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub kind: ChannelKind,
    /// Document ou dossier couvert (amont).
    pub covered: NodeId,
    /// Documents couvrants (aval). Plus d'un élément : combinaison « OU ».
    pub covering: Vec<NodeId>,
    pub category: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_nodes_by_name() {
        let project = Project {
            name: "demo".into(),
            nodes: vec![
                ProjectNode::Document(DocumentDecl {
                    name: "Spec".into(),
                    ..Default::default()
                }),
                ProjectNode::SubProject {
                    name: "Fournisseur".into(),
                    path: "sub/sub.rqtf".into(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(project.node_by_name("Fournisseur"), Some(1));
        assert_eq!(project.node_by_name("Absent"), None);
    }

    #[test]
    fn round_trips_through_json() {
        let channel = Channel {
            kind: ChannelKind::Cover,
            covered: 0,
            covering: vec![1, 2],
            category: None,
        };
        let json = serde_json::to_string(&channel).unwrap();
        assert_eq!(serde_json::from_str::<Channel>(&json).unwrap(), channel);
    }
}
