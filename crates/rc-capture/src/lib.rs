//! Moteur de capture : applique un type d'analyse à un fichier intermédiaire et
//! produit l'arbre des éléments d'un document.
//!
//! - intermédiaire texte : expressions régulières (`text.rs`) ;
//! - intermédiaire XML : arbres d'analyse (`xml.rs`).
//!
//! Comportements suivis : `docs/functional-spec.md` §3.4 et
//! `docs/interop-formats.md` §4.

mod builder;
mod text;
mod xml;

pub use text::capture_text;
pub use xml::capture_xml;

use rc_model::{CapturedDocument, DocumentType, TypeKind};

/// Capture un document à partir de son intermédiaire, selon le type d'analyse.
pub fn capture(
    doc_type: &DocumentType,
    document: &str,
    intermediate: &str,
    is_xml: bool,
) -> CapturedDocument {
    if is_xml || doc_type.kind == TypeKind::Xml {
        capture_xml(doc_type, document, intermediate)
    } else {
        capture_text(doc_type, document, intermediate)
    }
}

/// Compile une expression avec les options du dialecte des types (PCRE2, UTF-8,
/// `^` et `$` en début et fin de ligne).
pub fn compile(pattern: &str) -> Result<pcre2::bytes::Regex, pcre2::Error> {
    pcre2::bytes::RegexBuilder::new()
        .utf(true)
        .ucp(true)
        .multi_line(true)
        .jit_if_available(true)
        .build(pattern)
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_every_line_with_optional_tail() {
        let re = super::compile(r"^Requirement_ID[ \t]+(\S+)(?:[ \t]*\:[ \t]*(.+)|)$").unwrap();
        let text =
            "Requirement_ID\tREQ1 : A\nX\nRequirement_ID\tREQ2 : B\nRequirement_ID\tREQ3 : C\n";
        let found: Vec<_> = re
            .captures_iter(text.as_bytes())
            .map(|c| String::from_utf8_lossy(c.unwrap().get(1).unwrap().as_bytes()).into_owned())
            .collect();
        assert_eq!(found, ["REQ1", "REQ2", "REQ3"]);
    }
}
