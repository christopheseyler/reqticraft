//! Serveur local de l'interface : `POST /api/<méthode>` avec des arguments JSON,
//! et, si un répertoire est donné, les fichiers de l'interface compilée.
//!
//! Il n'écoute que sur 127.0.0.1. Il sert au développement de l'interface dans
//! un navigateur et préfigure l'accès web en lecture.

use anyhow::{Context, Result};
use rc_core::session::{call, Session};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn serve(
    port: u16,
    ui: Option<PathBuf>,
    project: Option<PathBuf>,
    config: &[PathBuf],
) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("écoute sur le port {port}"))?;
    let mut session: Option<Session> = None;
    if let Some(p) = project {
        session = Some(Session::open(&p, config).map_err(anyhow::Error::msg)?);
    }
    let session = Arc::new(Mutex::new(session));
    let ui = Arc::new(ui);
    println!("ReqtiCraft : http://127.0.0.1:{port}/");
    // Un fil par connexion : un navigateur ouvre des connexions à l'avance,
    // qui ne doivent pas bloquer les autres.
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let (session, ui) = (Arc::clone(&session), Arc::clone(&ui));
        std::thread::spawn(move || {
            if let Err(e) = handle(stream, &session, ui.as_deref()) {
                eprintln!("requête : {e:#}");
            }
        });
    }
    Ok(())
}

fn handle(
    mut stream: TcpStream,
    session: &Mutex<Option<Session>>,
    ui: Option<&Path>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let mut parts = request.split_whitespace();
    let (verb, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let path = target.split('?').next().unwrap_or("/");

    if let Some(method) = path.strip_prefix("/api/") {
        if verb != "POST" {
            return respond(
                &mut stream,
                "405 Method Not Allowed",
                "text/plain",
                b"POST attendu",
            );
        }
        let args: Value = if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).unwrap_or(Value::Null)
        };
        let args = if args.is_null() {
            Value::Object(Default::default())
        } else {
            args
        };
        let mut guard = session
            .lock()
            .map_err(|_| anyhow::anyhow!("session indisponible"))?;
        let result = call(&mut guard, method, &args);
        drop(guard);
        return match result {
            Ok(v) => respond(
                &mut stream,
                "200 OK",
                "application/json",
                v.to_string().as_bytes(),
            ),
            Err(e) => respond(
                &mut stream,
                "400 Bad Request",
                "application/json",
                serde_json::json!({ "error": e }).to_string().as_bytes(),
            ),
        };
    }

    let Some(root) = ui else {
        return respond(
            &mut stream,
            "404 Not Found",
            "text/plain",
            b"interface absente (--ui)",
        );
    };
    let relative = path.trim_start_matches('/');
    if relative.split('/').any(|c| c == "..") {
        return respond(
            &mut stream,
            "400 Bad Request",
            "text/plain",
            b"chemin refuse",
        );
    }
    let mut file = root.join(if relative.is_empty() {
        "index.html"
    } else {
        relative
    });
    if !file.is_file() {
        file = root.join("index.html");
    }
    match std::fs::read(&file) {
        Ok(bytes) => respond(&mut stream, "200 OK", mime(&file), &bytes),
        Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"introuvable"),
    }
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}
