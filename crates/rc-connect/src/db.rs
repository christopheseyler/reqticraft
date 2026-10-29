//! Bases de données → XML de la forme décrite par le connecteur Access
//! (`docs/interop-formats.md` §12.5) :
//!
//! ```xml
//! <table name="Low Level Requirements">
//!   <record><cell name="Requirement ID">LLR1</cell>…</record>
//! </table>
//! ```
//!
//! Selon l'emplacement du document :
//! - fichier `.mdb` / `.accdb` : pilote ODBC Access (Windows), table `table`
//!   (paramètre) ou première table, ou requête `query` ;
//! - `odbc:<chaîne de connexion>` : n'importe quel pilote ODBC (Windows) ;
//! - fichier `.sqlite`, `.sqlite3`, `.db` : outil `sqlite3` ;
//! - sinon, l'emplacement est une requête SQL exécutée sur le profil
//!   (`profile`) : par ODBC si le profil a une option `odbc`, sinon par
//!   `sqlplus` (Oracle, serveur `hôte:port`, option `database`).
//!
//! Sous Windows, l'accès ODBC passe par PowerShell (`System.Data.Odbc`) : la
//! chaîne de connexion et la requête sont transmises par variables d'environnement.

use crate::{run_tool, ConnectError, Context, Result};
use rc_convert::xml_escape;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn query(location: &str, ctx: &Context) -> Result<String> {
    let lower = location.to_ascii_lowercase();
    let local = ctx.base_dir.join(location);
    let table = ctx.param("table").map(str::to_owned);
    let sql = ctx.param("query").map(str::to_owned);
    if lower.ends_with(".mdb") || lower.ends_with(".accdb") {
        let conn = format!(
            "Driver={{Microsoft Access Driver (*.mdb, *.accdb)}};Dbq={};",
            local.display()
        );
        return odbc(&conn, table, sql);
    }
    if let Some(conn) = location.strip_prefix("odbc:") {
        return odbc(conn, table, sql);
    }
    if lower.ends_with(".sqlite") || lower.ends_with(".sqlite3") || lower.ends_with(".db") {
        let (name, sql) = select(table, sql)?;
        let out = run_tool(
            "sqlite3",
            &["-json", &local.display().to_string(), &sql],
            None,
            &[],
        )?;
        let rows: Value = if out.iter().all(u8::is_ascii_whitespace) {
            Value::Array(Vec::new())
        } else {
            serde_json::from_slice(&out).map_err(|e| ConnectError::msg(format!("sqlite3 : {e}")))?
        };
        return Ok(write_table(
            &name,
            rows.as_array().map(Vec::as_slice).unwrap_or(&[]),
        ));
    }
    let profile = ctx.profile(true)?.expect("profil requis");
    if let Some(conn) = profile.option("odbc") {
        return odbc(conn, table, Some(location.to_owned()));
    }
    oracle(profile, location, table.as_deref().unwrap_or("query"))
}

fn select(table: Option<String>, sql: Option<String>) -> Result<(String, String)> {
    match (table, sql) {
        (Some(t), None) => Ok((
            t.clone(),
            format!("SELECT * FROM \"{}\"", t.replace('"', "\"\"")),
        )),
        (t, Some(q)) => Ok((t.unwrap_or_else(|| "query".into()), q)),
        (None, None) => Err(ConnectError::msg("paramètre « table » ou « query » requis")),
    }
}

pub fn write_table(name: &str, rows: &[Value]) -> String {
    let mut out = format!("<table name=\"{}\">", xml_escape(name));
    for row in rows {
        out.push_str("<record>");
        if let Some(map) = row.as_object() {
            for (k, v) in map {
                let v = crate::xml::scalar(v).unwrap_or_else(|| v.to_string());
                out.push_str(&format!(
                    "<cell name=\"{}\">{}</cell>",
                    xml_escape(k),
                    xml_escape(&v)
                ));
            }
        }
        out.push_str("</record>");
    }
    out.push_str("</table>");
    out
}

/// Script PowerShell : exécute `RC_SQL` (ou lit la table `RC_TABLE`, ou la
/// première table) sur `RC_ODBC` et écrit `{"table": …, "rows": […]}` en JSON.
const ODBC_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$c = New-Object System.Data.Odbc.OdbcConnection $env:RC_ODBC
$c.Open()
$table = $env:RC_TABLE
$sql = $env:RC_SQL
if (-not $sql) {
  if (-not $table) {
    $t = $c.GetSchema('Tables') | Where-Object { $_.TABLE_TYPE -eq 'TABLE' } | Select-Object -First 1
    $table = $t.TABLE_NAME
  }
  $sql = 'SELECT * FROM [' + $table + ']'
}
if (-not $table) { $table = 'query' }
$cmd = $c.CreateCommand()
$cmd.CommandText = $sql
$r = $cmd.ExecuteReader()
$rows = New-Object System.Collections.ArrayList
while ($r.Read()) {
  $o = [ordered]@{}
  for ($i = 0; $i -lt $r.FieldCount; $i++) {
    if ($r.IsDBNull($i)) { $o[$r.GetName($i)] = '' } else { $o[$r.GetName($i)] = [string]$r.GetValue($i) }
  }
  [void]$rows.Add($o)
}
$c.Close()
ConvertTo-Json -InputObject ([ordered]@{ table = $table; rows = @($rows) }) -Depth 4 -Compress
"#;

fn odbc(conn: &str, table: Option<String>, sql: Option<String>) -> Result<String> {
    if !cfg!(windows) {
        return Err(ConnectError::msg(
            "accès ODBC disponible sous Windows uniquement (exporter la table en CSV sinon)",
        ));
    }
    let encoded = utf16_base64(ODBC_SCRIPT);
    let out = run_tool(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded],
        None,
        &[
            ("RC_ODBC", conn),
            ("RC_TABLE", table.as_deref().unwrap_or("")),
            ("RC_SQL", sql.as_deref().unwrap_or("")),
        ],
    )?;
    let text = String::from_utf8_lossy(&out);
    let v: Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}'))
        .map_err(|e| ConnectError::msg(format!("ODBC : réponse illisible : {e}")))?;
    let name = v.get("table").and_then(Value::as_str).unwrap_or("query");
    let rows = v
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(write_table(name, &rows))
}

/// Oracle par `sqlplus` : la connexion et la requête passent par l'entrée standard.
fn oracle(profile: &crate::Profile, sql: &str, name: &str) -> Result<String> {
    let user = profile.user.clone().unwrap_or_default();
    let password = profile
        .password()
        .or_else(|| profile.token())
        .unwrap_or_default();
    let database = profile.option("database").unwrap_or("");
    let script = format!(
        "SET MARKUP CSV ON QUOTE ON\nSET FEEDBACK OFF\nSET HEADING ON\nCONNECT {user}/\"{password}\"@//{}/{database}\n{};\nEXIT\n",
        profile.server.trim_start_matches("oracle://"),
        sql.trim().trim_end_matches(';')
    );
    let mut child = Command::new("sqlplus")
        .args(["-S", "-L", "/nolog"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ConnectError::Tool {
            tool: "sqlplus".into(),
            detail: e.to_string(),
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(script.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| ConnectError::Tool {
        tool: "sqlplus".into(),
        detail: e.to_string(),
    })?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("ORA-") || text.contains("SP2-") {
        return Err(ConnectError::Tool {
            tool: "sqlplus".into(),
            detail: text
                .lines()
                .find(|l| l.contains("ORA-") || l.contains("SP2-"))
                .unwrap_or("")
                .to_owned(),
        });
    }
    Ok(write_table(name, &csv_rows(&text)))
}

/// CSV avec en-tête → objets (ordre des colonnes conservé).
fn csv_rows(text: &str) -> Vec<Value> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.trim_start().as_bytes());
    let headers = reader.headers().cloned().unwrap_or_default();
    reader
        .records()
        .flatten()
        .map(|rec| {
            let mut map = serde_json::Map::new();
            for (h, v) in headers.iter().zip(rec.iter()) {
                map.insert(h.to_owned(), Value::String(v.to_owned()));
            }
            Value::Object(map)
        })
        .collect()
}

/// Script PowerShell pour `-EncodedCommand` : UTF-16LE en Base64.
fn utf16_base64(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_access_shaped_tables() {
        let rows = csv_rows(
            "\"Requirement ID\",\"Covered Requirements\"\n\"LLR1\",\"THS_REQ1, THS_REQ2\"\n",
        );
        assert_eq!(
            write_table("LLR", &rows),
            "<table name=\"LLR\"><record><cell name=\"Requirement ID\">LLR1</cell><cell name=\"Covered Requirements\">THS_REQ1, THS_REQ2</cell></record></table>"
        );
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"alice:s3cr\"et"), "YWxpY2U6czNjciJldA==");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    /// Lecture réelle d'un CSV par le pilote texte ODBC, si présent.
    #[cfg(windows)]
    #[test]
    fn reads_through_odbc_when_available() {
        let dir = std::env::temp_dir().join(format!("rc-odbc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("llr.csv"), "ReqId,Covers\nLLR1,THS_REQ1\n").unwrap();
        let conn = format!(
            "Driver={{Microsoft Access Text Driver (*.txt, *.csv)}};Dbq={};",
            dir.display()
        );
        match odbc(&conn, None, Some("SELECT * FROM [llr.csv]".into())) {
            Ok(xml) => assert_eq!(
                xml,
                "<table name=\"query\"><record><cell name=\"ReqId\">LLR1</cell><cell name=\"Covers\">THS_REQ1</cell></record></table>"
            ),
            Err(e) => eprintln!("pilote ODBC texte absent, test ignoré : {e}"),
        }
    }
}
