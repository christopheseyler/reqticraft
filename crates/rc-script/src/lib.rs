//! Scripts Python : vérifications personnalisées appliquées au modèle analysé.
//!
//! Les scripts sont les fichiers `scripts/*.py` placés à côté du projet. Ils
//! s'exécutent dans un processus Python séparé, qui reçoit le modèle en lecture
//! seule et renvoie des problèmes (JSON-RPC 2.0, un message par ligne, comme les
//! plugins de connecteurs). Voir `docs/python-scripting.md`.
//!
//! Le paquet Python `reqticraft` (API et hôte) est intégré à l'exécutable et
//! écrit dans un répertoire temporaire au premier lancement.

use rc_analysis::Analysis;
use rc_model::{ElementClass, Message, Severity};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PACKAGE: [(&str, &str); 2] = [
    (
        "__init__.py",
        include_str!("../python/reqticraft/__init__.py"),
    ),
    ("_host.py", include_str!("../python/reqticraft/_host.py")),
];

/// Délai par défaut d'une exécution, en secondes.
const TIMEOUT: u64 = 120;

/// Scripts d'un projet : `scripts/*.py` à côté du projet, par ordre alphabétique.
pub fn discover(project_dir: &Path) -> Vec<PathBuf> {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(project_dir.join("scripts"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("py")))
        .collect();
    scripts.sort();
    scripts
}

/// État des scripts d'un projet après une analyse, pour l'interface.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Noms des fichiers de scripts.
    pub scripts: Vec<String>,
    /// Répertoire du projet approuvé.
    pub trusted: bool,
    /// Scripts exécutés lors de la dernière analyse.
    pub ran: bool,
    pub checks: usize,
    pub problems: usize,
    pub errors: usize,
    pub duration_ms: u64,
}

impl Report {
    pub fn new(scripts: &[PathBuf], trusted: bool) -> Report {
        Report {
            scripts: scripts
                .iter()
                .map(|p| {
                    p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect(),
            trusted,
            ..Default::default()
        }
    }
}

/// Résultat d'une exécution : nombre de vérifications et messages produits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub checks: usize,
    pub messages: Vec<Message>,
}

impl Outcome {
    pub fn errors(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.rule == "scriptError")
            .count()
    }
}

// --- Approbation des projets ------------------------------------------------

fn trust_file() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".reqticraft")
            .join("trusted-projects.txt"),
    )
}

fn trust_key(dir: &Path) -> String {
    let d = dir.canonicalize().unwrap_or_else(|_| dir.to_owned());
    let s = d.display().to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Indique si le répertoire du projet a été approuvé pour exécuter ses scripts.
pub fn is_trusted(project_dir: &Path) -> bool {
    let Some(file) = trust_file() else {
        return false;
    };
    let key = trust_key(project_dir);
    std::fs::read_to_string(file)
        .map(|t| t.lines().any(|l| l.trim() == key))
        .unwrap_or(false)
}

/// Approuve le répertoire du projet.
pub fn trust(project_dir: &Path) -> std::io::Result<()> {
    if is_trusted(project_dir) {
        return Ok(());
    }
    let file = trust_file()
        .ok_or_else(|| std::io::Error::other("répertoire de l'utilisateur introuvable"))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    writeln!(f, "{}", trust_key(project_dir))
}

// --- Modèle envoyé aux scripts ---------------------------------------------

fn status(a: &Analysis, leaves: &[HashSet<usize>], d: usize, e: usize) -> &'static str {
    if a.derived.contains(&(d, e)) {
        "derived"
    } else if a.uncovered.contains(&(d, e)) {
        "uncovered"
    } else if leaves[d].contains(&e) && a.summaries[d].covered.is_some() {
        "covered"
    } else {
        "none"
    }
}

fn class_name(c: ElementClass) -> Value {
    serde_json::to_value(c).unwrap_or(Value::Null)
}

fn attributes(list: &[rc_model::AttributeValue]) -> Value {
    Value::Array(list.iter().map(|a| json!([a.name, a.value])).collect())
}

/// Paramètres de la requête `check` : projet, scripts, documents et liens.
pub fn model(name: &str, dir: &Path, analysis: &Analysis, scripts: &[PathBuf]) -> Value {
    let leaves: Vec<HashSet<usize>> = analysis
        .documents
        .iter()
        .map(|d| rc_analysis::leaf_requirements(d).into_iter().collect())
        .collect();
    let documents: Vec<Value> = analysis
        .documents
        .iter()
        .enumerate()
        .map(|(d, doc)| {
            let elements: Vec<Value> = doc
                .elements
                .iter()
                .enumerate()
                .map(|(e, el)| {
                    json!({
                        "type": el.type_name, "kind": class_name(el.class),
                        "id": el.ident, "label": el.label, "text": el.text,
                        "attributes": attributes(&el.attributes),
                        "parent": el.parent, "children": el.children, "line": el.line,
                        "status": status(analysis, &leaves, d, e),
                    })
                })
                .collect();
            json!({
                "name": doc.name, "type": doc.type_name, "text": doc.text,
                "attributes": attributes(&doc.attributes), "roots": doc.roots,
                "elements": elements,
            })
        })
        .collect();
    let links: Vec<Value> = analysis
        .links
        .iter()
        .map(|l| json!([l.from.document, l.from.element, l.to.document, l.to.element]))
        .collect();
    json!({
        "project": {"name": name, "dir": dir.display().to_string()},
        "scripts": scripts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "documents": documents,
        "links": links,
    })
}

// --- Exécution -------------------------------------------------------------

/// Écrit le paquet `reqticraft` dans un répertoire temporaire et renvoie le
/// répertoire à placer dans `PYTHONPATH`.
fn package_dir() -> Result<PathBuf, String> {
    let root =
        std::env::temp_dir().join(format!("reqticraft-python-{}", env!("CARGO_PKG_VERSION")));
    let pkg = root.join("reqticraft");
    std::fs::create_dir_all(&pkg).map_err(|e| format!("{} : {e}", pkg.display()))?;
    for (file, content) in PACKAGE {
        let path = pkg.join(file);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content) {
            std::fs::write(&path, content).map_err(|e| format!("{} : {e}", path.display()))?;
        }
    }
    Ok(root)
}

/// Interpréteurs essayés, dans l'ordre.
fn interpreters() -> Vec<Vec<String>> {
    if let Some(p) = std::env::var_os("REQTICRAFT_PYTHON") {
        return vec![vec![p.to_string_lossy().into_owned()]];
    }
    let list: &[&[&str]] = if cfg!(windows) {
        &[&["python"], &["py", "-3"]]
    } else {
        &[&["python3"], &["python"]]
    };
    list.iter()
        .map(|c| c.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn spawn(dir: &Path, pythonpath: &Path) -> Result<Child, String> {
    let mut paths = vec![pythonpath.to_owned()];
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let pythonpath = std::env::join_paths(paths).map_err(|e| e.to_string())?;
    for candidate in interpreters() {
        let Some((program, pre)) = candidate.split_first() else {
            continue;
        };
        let mut cmd = Command::new(program);
        cmd.args(pre)
            .args(["-X", "utf8", "-m", "reqticraft._host"])
            .env("PYTHONPATH", &pythonpath)
            .env("PYTHONIOENCODING", "utf-8")
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            // Pas de fenêtre de console depuis l'application de bureau.
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        match cmd.spawn() {
            Ok(child) => return Ok(child),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("lancement de {program} : {e}")),
        }
    }
    Err("Python introuvable : installez Python 3.9 ou plus récent, \
         ou indiquez l'interpréteur dans la variable REQTICRAFT_PYTHON"
        .to_owned())
}

fn timeout() -> Duration {
    let secs = std::env::var("REQTICRAFT_SCRIPT_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(TIMEOUT);
    Duration::from_secs(secs)
}

/// Envoie la requête et attend la réponse, dans la limite du délai.
fn exchange(mut child: Child, request: String) -> Result<Value, String> {
    let mut stdin = child.stdin.take().ok_or("entrée fermée")?;
    let stdout = child.stdout.take().ok_or("sortie fermée")?;
    let mut stderr = child.stderr.take().ok_or("sortie d'erreur fermée")?;
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(request.as_bytes());
        let _ = stdin.write_all(b"\n");
    });
    let reader = std::thread::spawn(move || {
        let mut response = None;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                if v.get("id") == Some(&json!(1)) {
                    response = Some(v);
                }
            }
        }
        response
    });
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let deadline = Instant::now() + timeout();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "délai dépassé ({} s) : scripts arrêtés",
                    timeout().as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = writer.join();
    let response = reader.join().ok().flatten();
    let stderr = errors.join().unwrap_or_default();
    let response = response.ok_or_else(|| {
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        format!("pas de réponse de Python ({})", tail.join(" | ").trim())
    })?;
    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("erreur")
            .to_owned());
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "réponse sans résultat".to_owned())
}

fn severity(s: &str) -> Severity {
    match s {
        "error" => Severity::Error,
        "information" => Severity::Information,
        _ => Severity::Warning,
    }
}

fn script_error(text: String) -> Message {
    Message {
        rule: "scriptError".to_owned(),
        severity: Severity::Error,
        text,
        document: None,
        element: None,
        line: None,
    }
}

/// Exécute les scripts sur une analyse. Une erreur d'exécution (Python absent,
/// délai dépassé, réponse invalide) devient un message `scriptError`.
pub fn run(name: &str, dir: &Path, analysis: &Analysis, scripts: &[PathBuf]) -> Outcome {
    if scripts.is_empty() {
        return Outcome::default();
    }
    let result = package_dir().and_then(|pythonpath| {
        let child = spawn(dir, &pythonpath)?;
        let request = json!({
            "jsonrpc": "2.0", "id": 1, "method": "check",
            "params": model(name, dir, analysis, scripts),
        });
        exchange(child, request.to_string())
    });
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            return Outcome {
                checks: 0,
                messages: vec![script_error(format!("scripts : {e}"))],
            }
        }
    };
    // Rattache un problème au document (par son nom) et à l'élément, s'ils existent.
    let locate = |v: &Value| -> (Option<String>, Option<usize>, Option<usize>) {
        let doc = v
            .get("document")
            .and_then(Value::as_u64)
            .and_then(|d| analysis.documents.get(d as usize));
        let Some(doc) = doc else {
            return (None, None, None);
        };
        let element = v
            .get("element")
            .and_then(Value::as_u64)
            .map(|e| e as usize)
            .filter(|&e| e < doc.elements.len());
        let line = element.map(|e| doc.elements[e].line).filter(|&l| l > 0);
        (Some(doc.name.clone()), element, line)
    };
    let text = |v: &Value| {
        v.get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut messages = Vec::new();
    for p in result
        .get("problems")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (document, element, line) = locate(p);
        messages.push(Message {
            rule: p
                .get("rule")
                .and_then(Value::as_str)
                .unwrap_or("script")
                .to_owned(),
            severity: severity(p.get("severity").and_then(Value::as_str).unwrap_or("")),
            text: text(p),
            document,
            element,
            line,
        });
    }
    for e in result
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (document, element, line) = locate(e);
        messages.push(Message {
            document,
            element,
            line,
            ..script_error(text(e))
        });
    }
    Outcome {
        checks: result.get("checks").and_then(Value::as_u64).unwrap_or(0) as usize,
        messages,
    }
}
