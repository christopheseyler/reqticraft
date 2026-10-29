//! Client HTTP(S)/FTP minimal fondé sur l'outil `curl` (livré avec Windows 10
//! et suivants, macOS et les distributions Linux) : proxys, certificats du
//! système et protocoles sont ceux de `curl`. Le programme peut être remplacé
//! par la variable `REQTICRAFT_CURL`.
//!
//! Les identifiants sont transmis par l'entrée standard (`curl -K -`) : ils
//! n'apparaissent ni dans la ligne de commande ni dans des fichiers.

use crate::{ConnectError, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Auth {
    #[default]
    None,
    Basic {
        user: String,
        password: String,
    },
    Bearer(String),
    /// En-tête personnalisé (`Private-Token`…) et sa valeur.
    Header(String, String),
}

#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub url: &'a str,
    /// `GET` par défaut.
    pub method: Option<&'a str>,
    pub headers: Vec<(String, String)>,
    pub body: Option<&'a [u8]>,
    pub auth: Auth,
    /// Fichier de cookies conservé entre deux requêtes (authentification par formulaire).
    pub cookie_jar: Option<&'a Path>,
    /// Écrit le corps de la réponse dans ce fichier plutôt qu'en mémoire.
    pub output: Option<&'a Path>,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        rc_interop::text::decode(&self.body)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().rev().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }
}

fn curl() -> String {
    std::env::var("REQTICRAFT_CURL").unwrap_or_else(|_| "curl".to_owned())
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_file(suffix: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("reqticraft-{}-{n}{suffix}", std::process::id()))
}

/// Chaîne entre guillemets pour un fichier de configuration curl.
fn quoted(value: &str) -> String {
    let mut s = String::from("\"");
    for c in value.chars() {
        match c {
            '"' | '\\' => {
                s.push('\\');
                s.push(c);
            }
            '\n' | '\r' => {}
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

pub fn send(req: &Request) -> Result<Response> {
    let headers_file = temp_file(".hdr");
    let body_file = req
        .output
        .map(Path::to_owned)
        .unwrap_or_else(|| temp_file(".body"));
    let mut config = String::new();
    config.push_str(&format!("url = {}\n", quoted(req.url)));
    for (k, v) in &req.headers {
        config.push_str(&format!("header = {}\n", quoted(&format!("{k}: {v}"))));
    }
    match &req.auth {
        Auth::None => {}
        Auth::Basic { user, password } => {
            config.push_str(&format!(
                "user = {}\n",
                quoted(&format!("{user}:{password}"))
            ));
        }
        Auth::Bearer(token) => {
            config.push_str(&format!(
                "header = {}\n",
                quoted(&format!("Authorization: Bearer {token}"))
            ));
        }
        Auth::Header(name, value) => {
            config.push_str(&format!(
                "header = {}\n",
                quoted(&format!("{name}: {value}"))
            ));
        }
    }
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "-L".into(),
        "--max-time".into(),
        "300".into(),
        "-K".into(),
        "-".into(),
        "-o".into(),
        body_file.display().to_string(),
        "-D".into(),
        headers_file.display().to_string(),
        "-w".into(),
        "%{http_code}".into(),
    ];
    if let Some(m) = req.method {
        args.push("-X".into());
        args.push(m.to_owned());
    }
    let data_file = req.body.map(|b| {
        let f = temp_file(".data");
        (f, b)
    });
    if let Some((f, b)) = &data_file {
        std::fs::write(f, b).map_err(|e| ConnectError::io(f, e))?;
        args.push("--data-binary".into());
        args.push(format!("@{}", f.display()));
    }
    if let Some(jar) = req.cookie_jar {
        args.push("-b".into());
        args.push(jar.display().to_string());
        args.push("-c".into());
        args.push(jar.display().to_string());
    }
    let program = curl();
    let mut child = Command::new(&program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ConnectError::Tool {
            tool: program.clone(),
            detail: e.to_string(),
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| ConnectError::Tool {
        tool: program.clone(),
        detail: e.to_string(),
    })?;
    if let Some((f, _)) = &data_file {
        let _ = std::fs::remove_file(f);
    }
    let headers = std::fs::read(&headers_file)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let _ = std::fs::remove_file(&headers_file);
    let body = if req.output.is_some() {
        Vec::new()
    } else {
        let b = std::fs::read(&body_file).unwrap_or_default();
        let _ = std::fs::remove_file(&body_file);
        b
    };
    let status: u16 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    if status == 0 {
        return Err(ConnectError::Tool {
            tool: program,
            detail: format!(
                "{} : {}",
                req.url,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(Response {
        status,
        headers,
        body,
    })
}

/// Requête qui doit réussir (code 2xx).
pub fn fetch(req: &Request) -> Result<Response> {
    let resp = send(req)?;
    // Le protocole FTP renvoie 226 ; `file://` renvoie 0, traité plus haut.
    if !(200..300).contains(&resp.status) {
        let mut body = resp.text();
        body.truncate(500);
        return Err(ConnectError::Http {
            status: resp.status,
            url: req.url.to_owned(),
            body,
        });
    }
    Ok(resp)
}

pub fn get_json(url: &str, auth: &Auth) -> Result<serde_json::Value> {
    let resp = fetch(&Request {
        url,
        headers: vec![("Accept".into(), "application/json".into())],
        auth: auth.clone(),
        ..Default::default()
    })?;
    serde_json::from_slice(&resp.body)
        .map_err(|e| ConnectError::msg(format!("{url} : réponse JSON invalide : {e}")))
}

pub fn download(url: &str, auth: &Auth, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConnectError::io(parent, e))?;
    }
    fetch(&Request {
        url,
        auth: auth.clone(),
        output: Some(dest),
        ..Default::default()
    })?;
    Ok(())
}

/// Encode une valeur de paramètre d'URL.
pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod mock {
    //! Serveur HTTP de test : répond selon le chemin et garde les requêtes reçues.
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    pub struct Mock {
        pub url: String,
        pub requests: Arc<Mutex<Vec<String>>>,
    }

    /// `routes` : (préfixe de chemin et de requête, statut, type, corps).
    pub fn serve(routes: Vec<(String, u16, &'static str, String)>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                head.push_str(&String::from_utf8_lossy(&body));
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                log.lock().unwrap().push(head);
                let (status, kind, body) = routes
                    .iter()
                    .filter(|(p, ..)| target.starts_with(p.as_str()))
                    .max_by_key(|(p, ..)| p.len())
                    .map(|(_, s, k, b)| (*s, *k, b.clone()))
                    .unwrap_or((404, "text/plain", "absent".into()));
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Mock { url, requests }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_credentials_through_stdin_and_reads_status() {
        let m = mock::serve(vec![(
            "/ok".into(),
            200,
            "application/json",
            "{\"a\":1}".into(),
        )]);
        let json = get_json(
            &format!("{}/ok?x=1", m.url),
            &Auth::Basic {
                user: "alice".into(),
                password: "s3cr\"et".into(),
            },
        )
        .unwrap();
        assert_eq!(json["a"], 1);
        let req = m.requests.lock().unwrap()[0].clone();
        assert!(req.starts_with("GET /ok?x=1 "));
        // alice:s3cr"et en Base64.
        assert!(
            req.contains("Authorization: Basic YWxpY2U6czNjciJldA=="),
            "{req}"
        );
        let err = fetch(&Request {
            url: &format!("{}/absent", m.url),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConnectError::Http { status: 404, .. }));
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(
            encode("project = THS AND x"),
            "project%20%3D%20THS%20AND%20x"
        );
    }
}
