//! Modèle de données de ReqtiCraft.
//!
//! Les concepts suivent `docs/functional-spec.md` §3 et la structure de
//! `docs/architecture-proposal.md` §4. Ce crate ne contient que des données :
//! aucune lecture de fichier, aucune analyse.

pub mod element;
pub mod project;
pub mod types;

pub use element::{ElementClass, ElementKey};
pub use project::{Channel, ChannelKind, DocumentDecl, Project, ProjectNode};
pub use types::{CaptureRule, DocumentType, ElementType, TypeKind};
