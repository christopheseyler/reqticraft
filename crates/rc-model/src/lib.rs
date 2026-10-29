//! Modèle de données de ReqtiCraft.
//!
//! Les concepts suivent `docs/functional-spec.md` §3 et la structure de
//! `docs/architecture-proposal.md` §4. Ce crate ne contient que des données :
//! aucune lecture de fichier, aucune analyse.

pub mod element;
pub mod filters;
pub mod project;
pub mod types;

pub use element::{
    AttributeValue, CapturedDocument, ElemId, Element, ElementClass, ElementKey, Message,
    Reference, Severity,
};
pub use filters::{Filter, FilterCondition, FilterSet, Mark, Modification};
pub use project::{
    Channel, ChannelKind, DocumentDecl, ModificationDocRef, NodeId, PathReplacement, Project,
    ProjectNode,
};
pub use types::{
    CaptureRole, CaptureRule, DocumentType, ElementFlags, ElementType, TypeKind, VariableDecl,
};
