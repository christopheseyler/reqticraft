//! Décodage des fichiers texte et XML tolérant.

use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};

/// Décode des octets : BOM UTF-8/UTF-16 si présent, sinon UTF-8 valide,
/// sinon Windows-1252 (fichiers Reqtify anciens ou édités en ANSI).
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return UTF_16LE.decode_with_bom_removal(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return UTF_16BE.decode_with_bom_removal(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Indique si un contenu est du XML (premier caractère significatif `<`).
pub fn looks_like_xml(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
}

/// Prépare un texte XML pour `roxmltree` : retire la déclaration d'encodage
/// (le texte est déjà décodé) et échappe les `<` laissés bruts dans des valeurs
/// d'attribut, que l'outil d'origine tolère (observé dans des `.types` d'exemple).
pub fn sanitize_xml(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}');
    let mut out = String::with_capacity(text.len());
    let mut quote: Option<char> = None;
    let mut in_tag = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                    out.push(c);
                } else if c == '<' {
                    out.push_str("&lt;");
                } else {
                    out.push(c);
                }
            }
            None => {
                if c == '<' {
                    in_tag = true;
                } else if c == '>' {
                    in_tag = false;
                } else if in_tag && (c == '"' || c == '\'') {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
        // Les commentaires et sections CDATA peuvent contenir des guillemets isolés.
        if in_tag && quote.is_none() && out.ends_with("<!--") {
            for c in chars.by_ref() {
                out.push(c);
                if out.ends_with("-->") {
                    break;
                }
            }
            in_tag = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_boms_and_ansi() {
        assert_eq!(decode(b"\xEF\xBB\xBFabc"), "abc");
        assert_eq!(decode(b"\xFF\xFEa\x00b\x00"), "ab");
        assert_eq!(decode(b"caf\xE9"), "café");
        assert_eq!(decode("café".as_bytes()), "café");
    }

    #[test]
    fn escapes_raw_lt_in_attributes_only() {
        let fixed = sanitize_xml(r#"<a tree="<row>$i</row>">x &lt; y</a>"#);
        assert_eq!(fixed, r#"<a tree="&lt;row>$i&lt;/row>">x &lt; y</a>"#);
        assert!(roxmltree::Document::parse(&fixed).is_ok());
    }

    #[test]
    fn leaves_comments_alone() {
        let xml = "<!-- l'outil --><a b='1'/>";
        assert_eq!(sanitize_xml(xml), xml);
    }
}
