//! Convertisseurs : un document natif (texte, docx, xlsx, PDF, code…) devient un
//! fichier intermédiaire texte ou XML, que le moteur de capture analyse ensuite.
//!
//! Formats produits : `docs/interop-formats.md` §5.

mod docx;
mod json;
mod pdf;
mod sheet;

use std::path::{Path, PathBuf};

/// Sortie d'un convertisseur.
#[derive(Debug, Clone, PartialEq)]
pub enum Intermediate {
    Text(String),
    Xml(String),
}

impl Intermediate {
    pub fn as_str(&self) -> &str {
        match self {
            Intermediate::Text(s) | Intermediate::Xml(s) => s,
        }
    }
}

/// Convertisseurs disponibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConverterId {
    /// Fichier texte tel quel (code source compris).
    Text,
    /// Word `.docx` : une ligne `style<TAB>texte` par paragraphe.
    Docx,
    /// Tableur (xlsx, xls, ods, csv) : `Worksheet :` puis `|A1 texte` par cellule.
    SheetText,
    /// Tableur en XML : `<Worksheet><Row><Cell col=…>`.
    SheetXml,
    /// PDF : un paragraphe reconstitué par ligne.
    Pdf,
    /// JSON transformé en XML.
    Json,
    /// Fichier XML tel quel.
    Xml,
}

impl ConverterId {
    /// Convertisseur correspondant à un `convertTool` des types d'analyse.
    pub fn for_tool(tool: &str) -> Option<ConverterId> {
        Some(match tool.to_ascii_lowercase().as_str() {
            "text" | "basicconnector" | "code" | "largecode" | "texteditor" => ConverterId::Text,
            "wordwithstyles" | "word" | "docx" => ConverterId::Docx,
            "excelwithcolnumbers" | "excel" | "csv" | "spreadsheet" => ConverterId::SheetText,
            "excelxml" | "spreadsheetxml" => ConverterId::SheetXml,
            "padafix" | "pdf" => ConverterId::Pdf,
            "json" => ConverterId::Json,
            "xml" => ConverterId::Xml,
            _ => return None,
        })
    }

    pub fn produces_xml(self) -> bool {
        matches!(
            self,
            ConverterId::SheetXml | ConverterId::Json | ConverterId::Xml
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("lecture de {path} : {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} : {message}")]
    Format { path: PathBuf, message: String },
}

impl ConvertError {
    fn format(path: &Path, message: impl std::fmt::Display) -> ConvertError {
        ConvertError::Format {
            path: path.to_owned(),
            message: message.to_string(),
        }
    }
}

/// Paramètres d'une conversion.
#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    /// Variables du document (`withCharacterStyles`, `pagesToExclude`…).
    pub params: Vec<(String, String)>,
    /// Pour un répertoire : motifs de fichiers retenus (`*.c`) ; vide = tous.
    pub matching: Vec<String>,
    /// Extensions retenues quand aucun motif n'est donné.
    pub extensions: Vec<String>,
    pub excluded: Vec<String>,
}

impl ConvertOptions {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn flag(&self, name: &str) -> bool {
        matches!(
            self.param(name).map(str::trim),
            Some("1" | "true" | "TRUE" | "True")
        )
    }
}

/// Convertit un fichier, ou chaque fichier d'un répertoire (document multifichier).
pub fn convert(
    id: ConverterId,
    path: &Path,
    options: &ConvertOptions,
) -> Result<Intermediate, ConvertError> {
    if path.is_dir() {
        return convert_directory(id, path, options);
    }
    let text = convert_file(id, path, options)?;
    Ok(if id.produces_xml() {
        Intermediate::Xml(text)
    } else {
        Intermediate::Text(text)
    })
}

fn convert_file(
    id: ConverterId,
    path: &Path,
    options: &ConvertOptions,
) -> Result<String, ConvertError> {
    match id {
        ConverterId::Text | ConverterId::Xml => read_text(path).map(|t| normalize_newlines(&t)),
        ConverterId::Docx => docx::convert(path, options),
        ConverterId::SheetText => sheet::convert_text(path),
        ConverterId::SheetXml => sheet::convert_xml(path),
        ConverterId::Pdf => pdf::convert(path, options),
        ConverterId::Json => json::convert(path),
    }
}

pub(crate) fn read_text(path: &Path) -> Result<String, ConvertError> {
    std::fs::read(path)
        .map(|b| rc_interop::text::decode(&b))
        .map_err(|source| ConvertError::Io {
            path: path.to_owned(),
            source,
        })
}

pub(crate) fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Document répertoire : chaque fichier retenu est converti puis encadré par
/// `BEGIN_DIRECTORY` / `BEGIN_FILE` (texte) ou `<Dir>` / `<File>` (XML),
/// comme décrit pour les multitypes (`docs/interop-formats.md` §5).
fn convert_directory(
    id: ConverterId,
    root: &Path,
    options: &ConvertOptions,
) -> Result<Intermediate, ConvertError> {
    let mut out = String::new();
    write_directory(id, root, root, options, &mut out)?;
    Ok(if id.produces_xml() {
        Intermediate::Xml(format!("<Directories>{out}</Directories>"))
    } else {
        Intermediate::Text(out)
    })
}

fn write_directory(
    id: ConverterId,
    root: &Path,
    dir: &Path,
    options: &ConvertOptions,
    out: &mut String,
) -> Result<bool, ConvertError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| ConvertError::Io {
            path: dir.to_owned(),
            source,
        })?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();

    let label = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rel = relative(root, dir);
    let mut body = String::new();
    let mut any = false;
    for entry in entries {
        let entry_rel = relative(root, &entry);
        if options
            .excluded
            .iter()
            .any(|p| wildcard_match(p, &entry_rel))
        {
            continue;
        }
        if entry.is_dir() {
            any |= write_directory(id, root, &entry, options, &mut body)?;
        } else if accepts(options, &entry, &entry_rel) {
            let name = entry
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let content = convert_file(id, &entry, options)?;
            if id.produces_xml() {
                body.push_str(&format!(
                    "<File label=\"{}\" path=\"{}\">{}</File>",
                    xml_escape(&name),
                    xml_escape(&entry_rel),
                    strip_xml_declaration(&content)
                ));
            } else {
                body.push_str(&format!("BEGIN_FILE: {name}:{entry_rel}\n"));
                body.push_str(&content);
                if !content.ends_with('\n') {
                    body.push('\n');
                }
                body.push_str(&format!("END_FILE: {entry_rel}\n"));
            }
            any = true;
        }
    }
    if any {
        if id.produces_xml() {
            out.push_str(&format!(
                "<Dir label=\"{}\" path=\"{}\">{body}</Dir>",
                xml_escape(&label),
                xml_escape(&rel)
            ));
        } else {
            out.push_str(&format!(
                "BEGIN_DIRECTORY: {label}:{rel}\n{body}END_DIRECTORY: {rel}\n"
            ));
        }
    }
    Ok(any)
}

fn accepts(options: &ConvertOptions, path: &Path, rel: &str) -> bool {
    if !options.matching.is_empty() {
        return options.matching.iter().any(|p| wildcard_match(p, rel));
    }
    if !options.extensions.is_empty() {
        return path.extension().is_some_and(|e| {
            options
                .extensions
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&e.to_string_lossy()))
        });
    }
    true
}

/// Chemin relatif avec `/` comme séparateur (`.` pour la racine).
fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let s = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    if s.is_empty() {
        ".".to_owned()
    } else {
        s
    }
}

/// Motif `*` / `?` sans tenir compte de la casse. Un motif sans `/` est comparé
/// au nom du fichier ; avec `/`, au chemin relatif. `*` traverse les `/`.
pub fn wildcard_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.replace('\\', "/").to_lowercase();
    let path = path.to_lowercase();
    let subject = if pattern.contains('/') || pattern.starts_with('*') && pattern[1..].contains('*')
    {
        path.as_str()
    } else {
        path.rsplit('/').next().unwrap_or(&path)
    };
    glob(pattern.as_bytes(), subject.as_bytes())
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

pub(crate) fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Caractères interdits en XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn strip_xml_declaration(xml: &str) -> &str {
    let xml = xml.trim_start_matches('\u{feff}').trim_start();
    if xml.starts_with("<?xml") {
        xml.find("?>").map(|i| &xml[i + 2..]).unwrap_or(xml)
    } else {
        xml
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_reqtify_convert_tools() {
        assert_eq!(
            ConverterId::for_tool("BasicConnector"),
            Some(ConverterId::Text)
        );
        assert_eq!(
            ConverterId::for_tool("WordWithStyles"),
            Some(ConverterId::Docx)
        );
        assert_eq!(
            ConverterId::for_tool("ExcelWithColNumbers"),
            Some(ConverterId::SheetText)
        );
        assert_eq!(
            ConverterId::for_tool("ExcelXML"),
            Some(ConverterId::SheetXml)
        );
        assert_eq!(ConverterId::for_tool("PaDaFix"), Some(ConverterId::Pdf));
        assert_eq!(ConverterId::for_tool("DoorsXML"), None);
    }

    #[test]
    fn wildcards_match_names_and_paths() {
        assert!(wildcard_match("*.c", "src/main.c"));
        assert!(wildcard_match("*.C", "src/main.c"));
        assert!(!wildcard_match("*.c", "src/main.cpp"));
        assert!(wildcard_match("*build_*.c", "src/build_x.c"));
        assert!(wildcard_match("*.svn*", "a/.svn/x"));
        assert!(wildcard_match("ma?n.c", "main.c"));
    }

    #[test]
    fn wraps_directories_with_file_markers() {
        let dir = std::env::temp_dir().join(format!("rc-convert-dir-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.c"), "// Implements R1\r\n").unwrap();
        std::fs::write(dir.join("sub").join("b.c"), "x").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignoré").unwrap();
        let options = ConvertOptions {
            matching: vec!["*.c".into()],
            ..Default::default()
        };
        let out = convert(ConverterId::Text, &dir, &options).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(
            out.as_str(),
            format!(
                "BEGIN_DIRECTORY: {name}:.\nBEGIN_FILE: a.c:a.c\n// Implements R1\nEND_FILE: a.c\n\
                 BEGIN_DIRECTORY: sub:sub\nBEGIN_FILE: b.c:sub/b.c\nx\nEND_FILE: sub/b.c\nEND_DIRECTORY: sub\n\
                 END_DIRECTORY: .\n"
            )
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
