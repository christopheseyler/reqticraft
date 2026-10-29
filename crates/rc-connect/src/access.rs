//! Types d'accès : rapatrient une copie locale d'un document distant ou archivé,
//! que le type d'analyse convertit ensuite (`docs/interop-formats.md` §12.2).
//!
//! L'emplacement du document choisit le type d'accès (le paramètre `access`
//! peut le forcer) :
//!
//! | Emplacement | Accès |
//! |---|---|
//! | `https://…`, `http://…`, `ftp://…` | téléchargement |
//! | `archive.zip`, `archive.7z`, `zip:archive.zip!dossier/fichier` | extraction |
//! | `git+https://serveur/depot.git#branche:chemin` | clone Git |
//! | `github:propriétaire/dépôt@branche/chemin` | clone GitHub |
//! | `bitbucket:espace/dépôt@branche/chemin` | clone Bitbucket |
//! | `svn+https://serveur/depot/trunk/chemin` | extraction Subversion |
//! | `cvs:module/chemin` (paramètres `CVSROOT`, `tag`) | extraction CVS |
//! | `p4://depot/chemin` | Perforce |
//! | `gdrive:<identifiant>` | Google Drive (Docs, Sheets, Slides exportés) |
//! | `onedrive:<chemin>` | OneDrive (Microsoft Graph) |
//! | `command:<emplacement>` | commande du profil ou du paramètre `accessCommand` |
//!
//! Comme dans l'outil d'origine, une archive est entièrement extraite pour un
//! type répertoire ; sinon seul le premier fichier ayant une extension attendue
//! (à défaut, le premier fichier) est extrait. Les copies de travail Git,
//! Subversion et CVS sont mises à jour à chaque analyse, en conservant les
//! modifications locales.

use crate::http::{self, Auth};
use crate::{run_tool, ConnectError, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Ce que le type d'analyse attend.
#[derive(Debug, Clone, Copy, Default)]
pub struct Want<'a> {
    pub directory: bool,
    pub extensions: &'a [String],
}

/// Types d'accès reconnus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Local,
    Http,
    Zip,
    SevenZip,
    Git,
    GitHub,
    Bitbucket,
    Svn,
    Cvs,
    Perforce,
    GoogleDrive,
    OneDrive,
    Command,
}

impl Access {
    pub fn parse(name: &str) -> Option<Access> {
        Some(match name.to_ascii_lowercase().as_str() {
            "local" | "file" => Access::Local,
            "http" | "https" | "ftp" => Access::Http,
            "zip" => Access::Zip,
            "7z" | "7zip" | "7-zip" => Access::SevenZip,
            "git" => Access::Git,
            "github" => Access::GitHub,
            "bitbucket" => Access::Bitbucket,
            "svn" | "subversion" => Access::Svn,
            "cvs" => Access::Cvs,
            "p4" | "perforce" => Access::Perforce,
            "gdrive" | "googledrive" | "google" => Access::GoogleDrive,
            "onedrive" => Access::OneDrive,
            "command" | "clearcase" | "synergy" | "pvcs" | "documentum" | "projectwise"
            | "designsync" => Access::Command,
            _ => return None,
        })
    }

    /// Type d'accès déduit de l'emplacement.
    pub fn detect(location: &str, ctx: &Context, want: Want) -> Access {
        if let Some(a) = ctx.param("access").and_then(Access::parse) {
            return a;
        }
        let lower = location.to_ascii_lowercase();
        let schemes = [
            ("git+", Access::Git),
            ("github:", Access::GitHub),
            ("bitbucket:", Access::Bitbucket),
            ("svn+", Access::Svn),
            ("svn:", Access::Svn),
            ("cvs:", Access::Cvs),
            ("p4:", Access::Perforce),
            ("gdrive:", Access::GoogleDrive),
            ("onedrive:", Access::OneDrive),
            ("command:", Access::Command),
            ("zip:", Access::Zip),
            ("http://", Access::Http),
            ("https://", Access::Http),
            ("ftp://", Access::Http),
            ("ftps://", Access::Http),
        ];
        if let Some((_, a)) = schemes.iter().find(|(s, _)| lower.starts_with(s)) {
            return *a;
        }
        let archive = |ext: &str| {
            lower.ends_with(&format!(".{ext}"))
                && !want.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
        };
        if archive("zip") {
            Access::Zip
        } else if archive("7z") {
            Access::SevenZip
        } else {
            Access::Local
        }
    }
}

/// L'emplacement désigne-t-il autre chose qu'un fichier local ?
pub fn is_remote(location: &str, ctx: &Context, want: Want) -> bool {
    Access::detect(location, ctx, want) != Access::Local
}

/// Chemin local du document, après rapatriement si nécessaire.
pub fn resolve(location: &str, ctx: &Context, want: Want) -> Result<PathBuf> {
    let access = Access::detect(location, ctx, want);
    if access != Access::Local {
        std::fs::create_dir_all(&ctx.work_dir).map_err(|e| ConnectError::io(&ctx.work_dir, e))?;
    }
    match access {
        Access::Local => Ok(ctx.base_dir.join(location)),
        Access::Http => {
            let auth = ctx.profile(false)?.map(|p| p.auth()).unwrap_or_default();
            let name = location
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|n| !n.is_empty() && !n.contains(':'))
                .unwrap_or("document");
            let dest = ctx.work_dir.join(name);
            http::download(location, &auth, &dest)?;
            follow_archive(&dest, ctx, want)
        }
        Access::Zip | Access::SevenZip => {
            let spec = location.strip_prefix("zip:").unwrap_or(location);
            let (archive, inner) = match spec.split_once('!') {
                Some((a, i)) => (a, Some(i.trim_start_matches('/'))),
                None => (spec, None),
            };
            let archive = ctx.base_dir.join(archive);
            extract(
                &archive,
                inner,
                &ctx.work_dir.join("archive"),
                want,
                access == Access::SevenZip,
            )
        }
        Access::Git => {
            let spec = location.strip_prefix("git+").unwrap_or(location);
            let (url, rest) = spec.split_once('#').unwrap_or((spec, ""));
            let (branch, sub) = rest.split_once(':').unwrap_or((rest, ""));
            let branch = ctx.param("branch").unwrap_or(branch);
            git(url, branch, sub, ctx, None)
        }
        Access::GitHub | Access::Bitbucket => {
            let spec = location.split_once(':').map(|(_, s)| s).unwrap_or(location);
            let mut parts = spec.splitn(3, '/');
            let owner = parts.next().unwrap_or("");
            let repo_branch = parts.next().unwrap_or("");
            let sub = parts.next().unwrap_or("");
            let (repo, branch) = repo_branch.split_once('@').unwrap_or((repo_branch, ""));
            let profile = ctx.profile(false)?;
            let host = profile
                .map(|p| p.server.replace("://api.github.com", "://github.com"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if access == Access::GitHub {
                        "https://github.com".into()
                    } else {
                        "https://bitbucket.org".into()
                    }
                });
            let url = format!("{host}/{owner}/{repo}.git");
            let branch = ctx.param("branch").unwrap_or(branch);
            let user = if access == Access::GitHub {
                "x-access-token"
            } else {
                "x-token-auth"
            };
            git(&url, branch, sub, ctx, Some(user))
        }
        Access::Svn => {
            let url = location
                .strip_prefix("svn+")
                .or_else(|| location.strip_prefix("svn:"))
                .unwrap_or(location);
            svn(url, ctx, want)
        }
        Access::Cvs => cvs(location.strip_prefix("cvs:").unwrap_or(location), ctx),
        Access::Perforce => {
            let spec = location.strip_prefix("p4:").unwrap_or(location);
            perforce(&format!("//{}", spec.trim_start_matches('/')), ctx, want)
        }
        Access::GoogleDrive => {
            google_drive(location.strip_prefix("gdrive:").unwrap_or(location), ctx)
        }
        Access::OneDrive => one_drive(location.strip_prefix("onedrive:").unwrap_or(location), ctx),
        Access::Command => {
            let template = match ctx.param("accessCommand") {
                Some(c) => c.to_owned(),
                None => ctx
                    .profile(true)?
                    .and_then(|p| p.option("command").map(str::to_owned))
                    .ok_or_else(|| ConnectError::msg("option « command » du profil requise"))?,
            };
            let dest = ctx.work_dir.join("copy");
            let _ = std::fs::remove_dir_all(&dest);
            let _ = std::fs::remove_file(&dest);
            crate::command::execute(
                &template,
                location.strip_prefix("command:").unwrap_or(location),
                ctx,
                &dest,
            )?;
            if !dest.exists() {
                return Err(ConnectError::msg(
                    "la commande d'accès n'a pas produit {output}",
                ));
            }
            Ok(dest)
        }
    }
}

/// Un téléchargement qui est une archive alors que le type ne l'attend pas est extrait.
fn follow_archive(path: &Path, ctx: &Context, want: Want) -> Result<PathBuf> {
    let name = path.to_string_lossy().to_ascii_lowercase();
    if name.ends_with(".zip")
        && !want
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case("zip"))
    {
        return extract(path, None, &ctx.work_dir.join("archive"), want, false);
    }
    Ok(path.to_owned())
}

/// Extrait une archive ZIP (ou 7z par l'outil `7z`) selon la règle documentée.
pub fn extract(
    archive: &Path,
    inner: Option<&str>,
    dest: &Path,
    want: Want,
    seven_zip: bool,
) -> Result<PathBuf> {
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| ConnectError::io(dest, e))?;
    if seven_zip {
        run_tool(
            "7z",
            &[
                "x",
                "-y",
                &format!("-o{}", dest.display()),
                &archive.display().to_string(),
            ],
            None,
            &[],
        )?;
    } else {
        let file = std::fs::File::open(archive).map_err(|e| ConnectError::io(archive, e))?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|e| ConnectError::msg(format!("{} : {e}", archive.display())))?;
        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| ConnectError::msg(e.to_string()))?;
            // Chemins hors de la destination refusés (`../`).
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let out = dest.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| ConnectError::io(&out, e))?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ConnectError::io(parent, e))?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| ConnectError::io(&out, e))?;
            std::io::copy(&mut entry, &mut f).map_err(|e| ConnectError::io(&out, e))?;
        }
    }
    if let Some(inner) = inner {
        return Ok(dest.join(inner));
    }
    if want.directory {
        return Ok(dest.to_owned());
    }
    let mut files = Vec::new();
    list_files(dest, &mut files);
    files.sort();
    let expected = files.iter().find(|f| {
        f.extension().is_some_and(|e| {
            want.extensions
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&e.to_string_lossy()))
        })
    });
    expected
        .or(files.first())
        .cloned()
        .ok_or_else(|| ConnectError::msg(format!("{} : archive vide", archive.display())))
}

fn list_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            list_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Nom de répertoire sûr dérivé d'une URL et d'une branche (`depot.git_main`).
fn checkout_name(url: &str, branch: &str) -> String {
    let last = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("repo");
    let raw = if branch.is_empty() {
        last.to_owned()
    } else {
        format!("{last}_{branch}")
    };
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn git(
    url: &str,
    branch: &str,
    sub: &str,
    ctx: &Context,
    token_user: Option<&str>,
) -> Result<PathBuf> {
    let dir = ctx.work_dir.join(checkout_name(url, branch));
    let profile = ctx.profile(false)?;
    // Jeton transmis par variables d'environnement de configuration Git.
    let header = profile.and_then(|p| {
        p.token().map(|t| {
            let user = p.user.as_deref().or(token_user).unwrap_or("x-access-token");
            format!(
                "Authorization: Basic {}",
                crate::db::base64(format!("{user}:{t}").as_bytes())
            )
        })
    });
    let env: Vec<(&str, &str)> = match &header {
        Some(h) => vec![
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "http.extraHeader"),
            ("GIT_CONFIG_VALUE_0", h.as_str()),
            ("GIT_TERMINAL_PROMPT", "0"),
        ],
        None => vec![("GIT_TERMINAL_PROMPT", "0")],
    };
    let dir_s = dir.display().to_string();
    if dir.join(".git").exists() {
        let mut args = vec!["-C", dir_s.as_str(), "pull", "-q", "--ff-only", "origin"];
        if !branch.is_empty() {
            args.push(branch);
        }
        run_tool("git", &args, None, &env)?;
    } else {
        let mut args = vec!["clone", "-q"];
        if !branch.is_empty() {
            args.push("--branch");
            args.push(branch.trim_start_matches("origin/"));
        }
        args.push(url);
        args.push(&dir_s);
        run_tool("git", &args, None, &env)?;
    }
    Ok(if sub.is_empty() { dir } else { dir.join(sub) })
}

/// Lance un outil en lui passant un secret par l'entrée standard.
fn run_with_stdin(program: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ConnectError::Tool {
            tool: program.into(),
            detail: e.to_string(),
        })?;
    if let Some(mut s) = child.stdin.take() {
        let _ = s.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| ConnectError::Tool {
        tool: program.into(),
        detail: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(ConnectError::Tool {
            tool: program.into(),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(())
}

fn svn(url: &str, ctx: &Context, want: Want) -> Result<PathBuf> {
    let profile = ctx.profile(false)?;
    let mut base: Vec<String> = vec!["--non-interactive".into()];
    if let Some(r) = ctx.param("revision") {
        base.push("-r".into());
        base.push(r.to_owned());
    }
    let password = profile.and_then(|p| p.password().or_else(|| p.token()));
    if let Some(p) = profile {
        if let Some(u) = &p.user {
            base.push("--username".into());
            base.push(u.clone());
        }
    }
    if password.is_some() {
        base.push("--password-from-stdin".into());
    }
    let name = checkout_name(url, "");
    // Un fichier seul est exporté ; un répertoire est extrait en copie de travail.
    let (command, target): (&str, PathBuf) = if want.directory {
        let dir = ctx.work_dir.join(&name);
        if dir.join(".svn").exists() {
            ("update", dir)
        } else {
            ("checkout", dir)
        }
    } else {
        ("export", ctx.work_dir.join(&name))
    };
    let target_s = target.display().to_string();
    let mut args: Vec<&str> = vec![command];
    args.extend(base.iter().map(String::as_str));
    match command {
        "update" => args.push(&target_s),
        "export" => {
            args.push("--force");
            args.push(url);
            args.push(&target_s);
        }
        _ => {
            args.push(url);
            args.push(&target_s);
        }
    }
    run_with_stdin("svn", &args, password.as_deref().unwrap_or(""))?;
    Ok(target)
}

fn cvs(module: &str, ctx: &Context) -> Result<PathBuf> {
    let root = ctx
        .param("CVSROOT")
        .map(str::to_owned)
        .or_else(|| std::env::var("CVSROOT").ok())
        .ok_or_else(|| ConnectError::msg("paramètre CVSROOT requis"))?;
    let dir = ctx.work_dir.join("cvs");
    std::fs::create_dir_all(&dir).map_err(|e| ConnectError::io(&dir, e))?;
    let mut args = vec!["-q", "-d", root.as_str()];
    let tag;
    if dir.join(module).exists() {
        args.extend(["update", "-d"]);
        if let Some(t) = ctx.param("tag") {
            tag = t.to_owned();
            args.extend(["-r", tag.as_str()]);
        }
        args.push(module);
    } else {
        args.push("checkout");
        if let Some(t) = ctx.param("tag") {
            tag = t.to_owned();
            args.extend(["-r", tag.as_str()]);
        }
        args.push(module);
    }
    run_tool("cvs", &args, Some(&dir), &[])?;
    Ok(dir.join(module))
}

fn perforce(depot: &str, ctx: &Context, want: Want) -> Result<PathBuf> {
    let root = ctx.work_dir.join("p4");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| ConnectError::io(&root, e))?;
    if !want.directory {
        let name = depot.rsplit('/').next().unwrap_or("document");
        let dest = root.join(name);
        run_tool(
            "p4",
            &["print", "-q", "-o", &dest.display().to_string(), depot],
            None,
            &[],
        )?;
        return Ok(dest);
    }
    let base = depot.trim_end_matches("/...").trim_end_matches('/');
    let listing = run_tool(
        "p4",
        &["-ztag", "files", "-e", &format!("{base}/...")],
        None,
        &[],
    )?;
    for line in String::from_utf8_lossy(&listing).lines() {
        let Some(file) = line.strip_prefix("... depotFile ") else {
            continue;
        };
        let rel = file
            .strip_prefix(base)
            .unwrap_or(file)
            .trim_start_matches('/');
        let dest = root.join(rel);
        run_tool(
            "p4",
            &["print", "-q", "-o", &dest.display().to_string(), file],
            None,
            &[],
        )?;
    }
    Ok(root)
}

fn google_drive(id: &str, ctx: &Context) -> Result<PathBuf> {
    let profile = ctx.profile(true)?.expect("profil requis");
    let api = if profile.server.is_empty() {
        "https://www.googleapis.com".to_owned()
    } else {
        profile.server.clone()
    };
    let auth = match profile.token() {
        Some(t) => Auth::Bearer(t),
        None => profile.auth(),
    };
    let meta = http::get_json(
        &format!(
            "{api}/drive/v3/files/{}?fields=name,mimeType&supportsAllDrives=true",
            http::encode(id)
        ),
        &auth,
    )?;
    let name = meta
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("document");
    let mime = meta.get("mimeType").and_then(|v| v.as_str()).unwrap_or("");
    // Documents Google : export au format Office équivalent.
    let export = match mime {
        "application/vnd.google-apps.document" => Some((
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        )),
        "application/vnd.google-apps.spreadsheet" => Some((
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        )),
        "application/vnd.google-apps.presentation" => Some((
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
        )),
        _ => None,
    };
    let safe: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let (url, file) = match export {
        Some((m, ext)) => (
            format!(
                "{api}/drive/v3/files/{}/export?mimeType={}",
                http::encode(id),
                http::encode(m)
            ),
            format!("{safe}.{ext}"),
        ),
        None => (
            format!(
                "{api}/drive/v3/files/{}?alt=media&supportsAllDrives=true",
                http::encode(id)
            ),
            safe,
        ),
    };
    let dest = ctx.work_dir.join(file);
    http::download(&url, &auth, &dest)?;
    Ok(dest)
}

fn one_drive(path: &str, ctx: &Context) -> Result<PathBuf> {
    let profile = ctx.profile(true)?.expect("profil requis");
    let api = if profile.server.is_empty() {
        "https://graph.microsoft.com".to_owned()
    } else {
        profile.server.clone()
    };
    let auth = match profile.token() {
        Some(t) => Auth::Bearer(t),
        None => profile.auth(),
    };
    let clean = path.trim_matches('/');
    let encoded: Vec<String> = clean.split('/').map(http::encode).collect();
    let url = format!("{api}/v1.0/me/drive/root:/{}:/content", encoded.join("/"));
    let dest = ctx
        .work_dir
        .join(clean.rsplit('/').next().unwrap_or("document"));
    http::download(&url, &auth, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock;
    use crate::Profiles;

    fn zip_file(path: &Path, files: &[(&str, &str)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for (n, c) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(c.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rc-access-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn extracts_first_expected_file_or_whole_archive() {
        let d = dir("zip");
        zip_file(
            &d.join("a.zip"),
            &[
                ("lisezmoi.txt", "x"),
                ("doc/spec.docx", "y"),
                ("doc/b.c", "z"),
            ],
        );
        let profiles = Profiles::default();
        let ctx = Context {
            base_dir: &d,
            work_dir: d.join("w"),
            profiles: &profiles,
            params: &[],
        };
        let ext = vec!["docx".to_owned()];
        let want = Want {
            directory: false,
            extensions: &ext,
        };
        assert_eq!(Access::detect("a.zip", &ctx, want), Access::Zip);
        let p = resolve("a.zip", &ctx, want).unwrap();
        assert!(p.ends_with("doc/spec.docx"), "{}", p.display());
        let all = resolve(
            "a.zip",
            &ctx,
            Want {
                directory: true,
                extensions: &[],
            },
        )
        .unwrap();
        assert!(all.join("doc").join("b.c").is_file());
        let inner = resolve("zip:a.zip!doc/b.c", &ctx, want).unwrap();
        assert_eq!(std::fs::read_to_string(inner).unwrap(), "z");
        // Une archive attendue par le type reste locale.
        let zip_ext = vec!["zip".to_owned()];
        assert_eq!(
            Access::detect(
                "a.zip",
                &ctx,
                Want {
                    directory: false,
                    extensions: &zip_ext
                }
            ),
            Access::Local
        );
    }

    #[test]
    fn downloads_over_http_and_exports_google_documents() {
        let m = mock::serve(vec![
            ("/files/spec.txt".into(), 200, "text/plain", "REQ1".into()),
            (
                "/drive/v3/files/abc?fields".into(),
                200,
                "application/json",
                r#"{"name":"Spec","mimeType":"application/vnd.google-apps.document"}"#.into(),
            ),
            (
                "/drive/v3/files/abc/export".into(),
                200,
                "application/octet-stream",
                "DOCX".into(),
            ),
        ]);
        let d = dir("http");
        std::env::set_var("RC_TEST_GDRIVE", "oauth");
        let profiles =
            Profiles::parse(&format!("[g]\nserver={}\ntokenEnv=RC_TEST_GDRIVE\n", m.url));
        let params = vec![("profile".to_owned(), "g".to_owned())];
        let ctx = Context {
            base_dir: &d,
            work_dir: d.join("w"),
            profiles: &profiles,
            params: &params,
        };
        let p = resolve(&format!("{}/files/spec.txt", m.url), &ctx, Want::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "REQ1");
        let g = resolve("gdrive:abc", &ctx, Want::default()).unwrap();
        assert!(g.ends_with("Spec.docx"));
        assert_eq!(std::fs::read_to_string(&g).unwrap(), "DOCX");
        let reqs = m.requests.lock().unwrap();
        assert!(reqs.iter().any(|r| r.contains(
            "mimeType=application%2Fvnd.openxmlformats-officedocument.wordprocessingml.document"
        )));
        assert!(reqs
            .iter()
            .filter(|r| r.contains("/drive/"))
            .all(|r| r.contains("Authorization: Bearer oauth")));
    }

    #[test]
    fn clones_and_updates_git_repositories() {
        let d = dir("git");
        let origin = d.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let o = origin.display().to_string();
        let g = |args: &[&str]| {
            let mut full = vec![
                "-C",
                o.as_str(),
                "-c",
                "user.name=A",
                "-c",
                "user.email=a@x",
            ];
            full.extend_from_slice(args);
            run_tool("git", &full, None, &[]).unwrap();
        };
        g(&["init", "-q", "-b", "main"]);
        std::fs::create_dir_all(origin.join("docs")).unwrap();
        std::fs::write(origin.join("docs/spec.md"), "v1").unwrap();
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "v1"]);
        let profiles = Profiles::default();
        let ctx = Context {
            base_dir: &d,
            work_dir: d.join("w"),
            profiles: &profiles,
            params: &[],
        };
        let url = format!(
            "git+file:///{}#main:docs/spec.md",
            o.replace('\\', "/").trim_start_matches('/')
        );
        let p = resolve(&url, &ctx, Want::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");
        std::fs::write(origin.join("docs/spec.md"), "v2").unwrap();
        g(&["commit", "-q", "-am", "v2"]);
        let p = resolve(&url, &ctx, Want::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
    }
}
