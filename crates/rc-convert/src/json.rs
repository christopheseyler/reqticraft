//! JSON → XML, selon la correspondance décrite par la documentation du connecteur
//! JSON : `<object>`, `<array>`, `<string>`, `<integer>`, `<real>`, `<true/>`,
//! `<false/>`, `<null/>` ; un membre d'objet porte son nom dans `name`.

use crate::{xml_escape, ConvertError};
use serde_json::Value;
use std::path::Path;

pub fn convert(path: &Path) -> Result<String, ConvertError> {
    let text = crate::read_text(path)?;
    let value: Value = serde_json::from_str(&text).map_err(|e| ConvertError::format(path, e))?;
    let mut out = String::new();
    write(&value, None, &mut out);
    Ok(out)
}

fn write(value: &Value, name: Option<&str>, out: &mut String) {
    let name_attr = name
        .map(|n| format!(" name=\"{}\"", xml_escape(n)))
        .unwrap_or_default();
    match value {
        Value::Object(map) => {
            out.push_str(&format!("<object{name_attr}>"));
            for (k, v) in map {
                write(v, Some(k), out);
            }
            out.push_str("</object>");
        }
        Value::Array(items) => {
            out.push_str(&format!("<array{name_attr}>"));
            for v in items {
                write(v, None, out);
            }
            out.push_str("</array>");
        }
        Value::String(s) => out.push_str(&format!("<string{name_attr}>{}</string>", xml_escape(s))),
        Value::Number(n) if n.is_f64() => out.push_str(&format!("<real{name_attr}>{n}</real>")),
        Value::Number(n) => out.push_str(&format!("<integer{name_attr}>{n}</integer>")),
        Value::Bool(true) => out.push_str(&format!("<true{name_attr}/>")),
        Value::Bool(false) => out.push_str(&format!("<false{name_attr}/>")),
        Value::Null => out.push_str(&format!("<null{name_attr}/>")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_json_values_to_xml() {
        let value: Value =
            serde_json::from_str(r#"{"id":"R<1>","n":2,"x":1.5,"ok":true,"no":null,"l":["a"]}"#)
                .unwrap();
        let mut out = String::new();
        write(&value, None, &mut out);
        assert_eq!(
            out,
            r#"<object><string name="id">R&lt;1&gt;</string><integer name="n">2</integer><real name="x">1.5</real><true name="ok"/><null name="no"/><array name="l"><string>a</string></array></object>"#
        );
    }
}
