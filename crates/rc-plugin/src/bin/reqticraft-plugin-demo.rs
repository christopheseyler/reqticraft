//! Plugin de démonstration du protocole : l'outil `Lines` transforme chaque
//! ligne `ID : texte` du fichier désigné en exigence XML
//! `<req id="ID">texte</req>`. Sert aux tests et d'exemple d'implémentation.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        match req.get("method").and_then(Value::as_str) {
            Some("convert") => {
                let p = &req["params"];
                let base = p["baseDir"].as_str().unwrap_or(".");
                let path = std::path::Path::new(base).join(p["location"].as_str().unwrap_or(""));
                let reply = match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        let mut xml = String::from("<reqs>");
                        for l in text.lines() {
                            if let Some((id, t)) = l.split_once(':') {
                                xml.push_str(&format!(
                                    "<req id=\"{}\">{}</req>",
                                    id.trim(),
                                    t.trim()
                                ));
                            }
                        }
                        xml.push_str("</reqs>");
                        json!({"jsonrpc": "2.0", "id": id, "result": {"format": "xml", "content": xml}})
                    }
                    Err(e) => json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": -32000, "message": format!("{}: {e}", path.display())}}),
                };
                let _ = writeln!(stdout, "{reply}");
                let _ = stdout.flush();
            }
            Some("shutdown") => break,
            _ => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "méthode inconnue"}})
                );
            }
        }
    }
}
