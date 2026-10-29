//! Plugins de connecteurs : programmes externes (n'importe quel langage)
//! dialoguant en JSON-RPC 2.0 sur leur entrée et sortie standard, un message
//! JSON par ligne. Un plugin fournit des convertisseurs (`convertTool`) que la
//! bibliothèque intégrée ne connaît pas.
//!
//! Déclaration : un répertoire par plugin contenant `plugin.ini` :
//!
//! ```ini
//! [plugin]
//! name=exemple
//! command=python3 plugin.py
//! tools=MonOutil,AutreOutil
//! ```
//!
//! Protocole (voir `docs/plugin-protocol.md`) :
//!
//! - `convert` — paramètres `{tool, location, params, baseDir, workDir}`,
//!   résultat `{format: "text" | "xml", content}` ;
//! - `shutdown` — sans paramètre, envoyé après la conversion.
//!
//! Un plugin est lancé pour chaque document converti, depuis son répertoire.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin {plugin} : {message}")]
    Failed { plugin: String, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    pub name: String,
    pub dir: PathBuf,
    /// Programme et arguments.
    pub command: Vec<String>,
    pub tools: Vec<String>,
}

/// Résultat d'une conversion par plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub xml: bool,
    pub content: String,
}

impl Plugin {
    pub fn read(dir: &Path) -> Option<Plugin> {
        let text = std::fs::read_to_string(dir.join("plugin.ini")).ok()?;
        let ini = rc_interop::ini::Ini::parse(&text);
        let s = ini.section("plugin")?;
        Some(Plugin {
            name: s
                .get_nonempty("name")
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    dir.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                }),
            dir: dir.to_owned(),
            command: split(s.get_nonempty("command")?),
            tools: rc_interop::ini::split_list(s.get("tools").unwrap_or("")),
        })
    }

    pub fn provides(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t.eq_ignore_ascii_case(tool))
    }

    fn fail(&self, message: impl Into<String>) -> PluginError {
        PluginError::Failed {
            plugin: self.name.clone(),
            message: message.into(),
        }
    }

    /// Convertit un document.
    pub fn convert(
        &self,
        tool: &str,
        location: &str,
        params: &[(String, String)],
        base_dir: &Path,
        work_dir: &Path,
    ) -> Result<Output, PluginError> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| self.fail("commande vide"))?;
        // Un programme relatif désigne un fichier du plugin s'il existe.
        let local = self.dir.join(program);
        let program = if local.is_file() {
            local.display().to_string()
        } else {
            program.clone()
        };
        let mut child = Command::new(&program)
            .args(args)
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| self.fail(format!("lancement de {program} : {e}")))?;
        let params: serde_json::Map<String, Value> = params
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let request = json!({
            "jsonrpc": "2.0", "id": 1, "method": "convert",
            "params": {
                "tool": tool, "location": location, "params": params,
                "baseDir": base_dir.display().to_string(),
                "workDir": work_dir.display().to_string(),
            }
        });
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| self.fail("entrée fermée"))?;
        writeln!(stdin, "{request}").map_err(|e| self.fail(e.to_string()))?;
        stdin.flush().map_err(|e| self.fail(e.to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| self.fail("sortie fermée"))?;
        let mut reader = BufReader::new(stdout);
        let mut response: Option<Value> = None;
        let mut line = String::new();
        while reader
            .read_line(&mut line)
            .map_err(|e| self.fail(e.to_string()))?
            > 0
        {
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                if v.get("id") == Some(&json!(1)) {
                    response = Some(v);
                    break;
                }
            }
            line.clear();
        }
        let _ = writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"})
        );
        drop(stdin);
        let out = child
            .wait_with_output()
            .map_err(|e| self.fail(e.to_string()))?;
        let response = response.ok_or_else(|| {
            self.fail(format!(
                "pas de réponse ({})",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        })?;
        if let Some(err) = response.get("error") {
            return Err(self.fail(
                err.get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("erreur")
                    .to_owned(),
            ));
        }
        let result = response
            .get("result")
            .ok_or_else(|| self.fail("réponse sans résultat"))?;
        Ok(Output {
            xml: result.get("format").and_then(Value::as_str) == Some("xml"),
            content: result
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        })
    }
}

/// Plugins des répertoires `plugins/` du projet, de l'utilisateur
/// (`~/.reqticraft/plugins`) et de `REQTICRAFT_PLUGINS` (liste de répertoires).
pub fn discover(project_dir: &Path) -> Vec<Plugin> {
    let mut roots = vec![project_dir.join("plugins")];
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        roots.push(PathBuf::from(home).join(".reqticraft").join("plugins"));
    }
    if let Some(extra) = std::env::var_os("REQTICRAFT_PLUGINS") {
        roots.extend(std::env::split_paths(&extra));
    }
    let mut plugins = Vec::new();
    for root in roots {
        if let Some(p) = Plugin::read(&root) {
            plugins.push(p);
            continue;
        }
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        plugins.extend(dirs.iter().filter_map(|d| Plugin::read(d)));
    }
    plugins
}

fn split(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}
