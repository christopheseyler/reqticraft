//! Réponses JSON des API → XML intermédiaire, sous deux formes :
//!
//! - **attributs** : un objet devient un élément nommé d'après sa clé ; ses
//!   valeurs simples deviennent des attributs, ses objets et tableaux des
//!   éléments enfants (`{"issues":[{"key":"X","fields":{"priority":{"name":"High"}}}]}`
//!   → `<issues key="X"><fields><priority name="High"/></fields></issues>`) ;
//! - **éléments** : chaque membre devient un élément enfant, une valeur simple
//!   son texte (`{"bugs":[{"id":1}]}` → `<bugs><id>1</id></bugs>`).
//!
//! Les clés qui ne sont pas des noms XML valides sont adaptées (`_` à la place
//! des caractères interdits). Les tableaux de valeurs simples répètent l'élément.

use rc_convert::xml_escape;
use serde_json::Value;

/// Nom d'élément ou d'attribut XML dérivé d'une clé JSON.
pub fn name(key: &str) -> String {
    let mut s: String = key
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || !s.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        s.insert(0, '_');
    }
    s
}

pub fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

/// Forme « attributs ».
pub fn attributes(tag: &str, value: &Value, out: &mut String) {
    let tag = name(tag);
    match value {
        Value::Array(items) => {
            for item in items {
                attributes(&tag, item, out);
            }
        }
        Value::Object(map) => {
            out.push('<');
            out.push_str(&tag);
            for (k, v) in map {
                if let Some(s) = scalar(v) {
                    out.push_str(&format!(" {}=\"{}\"", name(k), xml_escape(&s)));
                }
            }
            let complex: Vec<_> = map.iter().filter(|(_, v)| scalar(v).is_none()).collect();
            if complex.is_empty() {
                out.push_str("/>");
                return;
            }
            out.push('>');
            for (k, v) in complex {
                attributes(k, v, out);
            }
            out.push_str(&format!("</{tag}>"));
        }
        v => {
            let s = scalar(v).unwrap_or_default();
            out.push_str(&format!("<{tag}>{}</{tag}>", xml_escape(&s)));
        }
    }
}

/// Forme « éléments ».
pub fn elements(tag: &str, value: &Value, out: &mut String) {
    let tag = name(tag);
    match value {
        Value::Array(items) => {
            for item in items {
                elements(&tag, item, out);
            }
        }
        Value::Object(map) => {
            out.push_str(&format!("<{tag}>"));
            for (k, v) in map {
                elements(k, v, out);
            }
            out.push_str(&format!("</{tag}>"));
        }
        v => {
            let s = scalar(v).unwrap_or_default();
            out.push_str(&format!("<{tag}>{}</{tag}>", xml_escape(&s)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_json_with_attributes() {
        let v = json!({"key": "THS-1", "fields": {"summary": "Démarrer", "priority": {"name": "High"},
            "labels": ["a", "b"], "custom field": null}});
        let mut out = String::new();
        attributes("issues", &v, &mut out);
        assert_eq!(
            out,
            "<issues key=\"THS-1\"><fields summary=\"Démarrer\" custom_field=\"\"><priority name=\"High\"/><labels>a</labels><labels>b</labels></fields></issues>"
        );
    }

    #[test]
    fn maps_json_with_elements() {
        let v = json!({"id": 7, "summary": "x<y", "keywords": ["k1"], "1st": true});
        let mut out = String::new();
        elements("Bug", &v, &mut out);
        assert_eq!(
            out,
            "<Bug><id>7</id><summary>x&lt;y</summary><keywords>k1</keywords><_1st>true</_1st></Bug>"
        );
    }
}
