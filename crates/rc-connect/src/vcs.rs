//! Historiques de gestion de versions → XML de la forme `svn log --xml`
//! (types « SVN Log », `docs/interop-formats.md` §12.4) :
//!
//! ```xml
//! <log>
//!   <logentry revision="…">
//!     <author>…</author><date>…</date><msg>…</msg>
//!     <paths><path action="M">fichier</path></paths>
//!   </logentry>
//! </log>
//! ```
//!
//! Git et Perforce sont présentés sous la même forme, pour que les mêmes types
//! d'analyse s'appliquent. Paramètres : `revision` (plage ou révision),
//! `branch` (Git), `maxCount`.

use crate::{run_tool, ConnectError, Context, Result};
use rc_convert::xml_escape;
use std::path::{Path, PathBuf};

fn local_path(location: &str, ctx: &Context) -> Result<PathBuf> {
    let p = ctx.base_dir.join(location);
    if !p.exists() {
        return Err(ConnectError::msg(format!(
            "copie de travail introuvable : {}",
            p.display()
        )));
    }
    Ok(p)
}

struct Entry {
    revision: String,
    author: String,
    date: String,
    msg: String,
    paths: Vec<(String, String)>,
}

fn write_log(entries: &[Entry]) -> String {
    let mut out = String::from("<log>");
    for e in entries {
        out.push_str(&format!(
            "<logentry revision=\"{}\"><author>{}</author><date>{}</date><msg>{}</msg>",
            xml_escape(&e.revision),
            xml_escape(&e.author),
            xml_escape(&e.date),
            xml_escape(e.msg.trim_end())
        ));
        if !e.paths.is_empty() {
            out.push_str("<paths>");
            for (action, path) in &e.paths {
                out.push_str(&format!(
                    "<path action=\"{}\">{}</path>",
                    xml_escape(action),
                    xml_escape(path)
                ));
            }
            out.push_str("</paths>");
        }
        out.push_str("</logentry>");
    }
    out.push_str("</log>");
    out
}

/// Journal Git d'une copie de travail (ou d'un fichier ou répertoire qu'elle contient).
pub fn git_log(location: &str, ctx: &Context) -> Result<String> {
    let path = local_path(location, ctx)?;
    let (dir, filter) = if path.is_dir() {
        (path.clone(), None)
    } else {
        (
            path.parent().unwrap_or(Path::new(".")).to_owned(),
            path.file_name().map(|f| f.to_string_lossy().into_owned()),
        )
    };
    let mut args: Vec<String> = vec![
        "log".into(),
        "--date=iso-strict".into(),
        "--name-status".into(),
        "--no-color".into(),
        "--format=%x1e%H%x1f%an%x1f%ad%x1f%B%x1f".into(),
    ];
    if let Some(n) = ctx.param("maxCount") {
        args.push(format!("--max-count={n}"));
    }
    if let Some(rev) = ctx.param("revision").or_else(|| ctx.param("branch")) {
        args.push(rev.to_owned());
    }
    args.push("--".into());
    args.push(filter.unwrap_or_else(|| ".".into()));
    let dir_s = dir.display().to_string();
    let mut full = vec!["-C".to_owned(), dir_s];
    full.extend(args);
    let refs: Vec<&str> = full.iter().map(String::as_str).collect();
    let out = run_tool("git", &refs, None, &[])?;
    Ok(write_log(&parse_git(&String::from_utf8_lossy(&out))))
}

fn parse_git(text: &str) -> Vec<Entry> {
    text.split('\u{1e}')
        .filter(|r| !r.trim().is_empty())
        .filter_map(|record| {
            let mut f = record.splitn(5, '\u{1f}');
            let revision = f.next()?.trim().to_owned();
            let author = f.next()?.to_owned();
            let date = f.next()?.to_owned();
            let msg = f.next()?.to_owned();
            let paths = f
                .next()
                .unwrap_or("")
                .lines()
                .filter_map(|l| {
                    let mut parts = l.split('\t');
                    let action = parts.next()?.trim();
                    let file = parts.next_back()?;
                    (!action.is_empty()).then(|| (action[..1].to_owned(), file.to_owned()))
                })
                .collect();
            Some(Entry {
                revision,
                author,
                date,
                msg,
                paths,
            })
        })
        .collect()
}

/// Journal Subversion (`svn log --xml -v`), repris tel quel.
pub fn svn_log(location: &str, ctx: &Context) -> Result<String> {
    let path = if location.contains("://") {
        location.to_owned()
    } else {
        local_path(location, ctx)?.display().to_string()
    };
    let mut args = vec!["log", "--xml", "-v", "--non-interactive"];
    let rev;
    if let Some(r) = ctx.param("revision") {
        rev = r.to_owned();
        args.push("-r");
        args.push(&rev);
    }
    args.push(&path);
    let out = run_tool("svn", &args, None, &[])?;
    let text = rc_interop::text::decode(&out);
    let t = text.trim_start();
    Ok(if t.starts_with("<?xml") {
        t.find("?>").map(|i| t[i + 2..].to_owned()).unwrap_or(text)
    } else {
        text
    })
}

/// Journal Perforce (`p4 -ztag changes -l -t <chemin>/...`).
pub fn p4_log(location: &str, ctx: &Context) -> Result<String> {
    let spec = if location.starts_with("//") {
        location.to_owned()
    } else {
        let p = local_path(location, ctx)?;
        if p.is_dir() {
            format!("{}/...", p.display())
        } else {
            p.display().to_string()
        }
    };
    let max;
    let mut args = vec!["-ztag", "changes", "-l", "-t"];
    if let Some(n) = ctx.param("maxCount") {
        max = n.to_owned();
        args.push("-m");
        args.push(&max);
    }
    args.push(&spec);
    let out = run_tool("p4", &args, None, &[])?;
    Ok(write_log(&parse_p4(&String::from_utf8_lossy(&out))))
}

fn parse_p4(text: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;
    let mut in_desc = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("... ") {
            let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
            in_desc = false;
            match key {
                "change" => {
                    if let Some(e) = current.take() {
                        entries.push(e);
                    }
                    current = Some(Entry {
                        revision: value.to_owned(),
                        author: String::new(),
                        date: String::new(),
                        msg: String::new(),
                        paths: Vec::new(),
                    });
                }
                "user" => {
                    if let Some(e) = &mut current {
                        e.author = value.to_owned();
                    }
                }
                "time" => {
                    if let (Some(e), Ok(secs)) = (&mut current, value.parse::<u64>()) {
                        e.date = rc_convert_iso(secs);
                    }
                }
                "desc" => {
                    if let Some(e) = &mut current {
                        e.msg = value.to_owned();
                        in_desc = true;
                    }
                }
                _ => {}
            }
        } else if in_desc {
            if let Some(e) = &mut current {
                e.msg.push('\n');
                e.msg.push_str(line);
            }
        }
    }
    entries.extend(current);
    entries
}

fn rc_convert_iso(secs: u64) -> String {
    rc_convert::iso_date(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profiles;

    #[test]
    fn reads_git_history_as_svn_log() {
        let dir = std::env::temp_dir().join(format!("rc-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let mut full = vec![
                "-C",
                dir.to_str().unwrap(),
                "-c",
                "user.name=Alice",
                "-c",
                "user.email=a@x",
            ];
            full.extend_from_slice(args);
            run_tool("git", &full, None, &[]).unwrap();
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.c"), "x").unwrap();
        git(&["add", "a.c"]);
        git(&[
            "commit",
            "-q",
            "-m",
            "Mesure <capteur>\n\nImplements THS_REQ1",
        ]);
        let profiles = Profiles::default();
        let ctx = Context {
            base_dir: &dir,
            work_dir: dir.join(".w"),
            profiles: &profiles,
            params: &[],
        };
        let out = git_log(".", &ctx).unwrap();
        assert!(out.starts_with("<log><logentry revision=\""), "{out}");
        assert!(out.contains("<author>Alice</author>"));
        assert!(out.contains("<msg>Mesure &lt;capteur&gt;\n\nImplements THS_REQ1</msg><paths><path action=\"A\">a.c</path></paths></logentry></log>"), "{out}");
    }

    #[test]
    fn parses_perforce_tagged_changes() {
        let text = "... change 12\n... time 0\n... user bob\n... desc Fix THS_REQ2\nsecond line\n\n... change 11\n... user ann\n... desc Init\n";
        let e = parse_p4(text);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].revision, "12");
        assert_eq!(e[0].date, "1970-01-01T00:00:00Z");
        assert_eq!(e[0].msg, "Fix THS_REQ2\nsecond line\n");
        assert_eq!(e[1].author, "ann");
    }
}
