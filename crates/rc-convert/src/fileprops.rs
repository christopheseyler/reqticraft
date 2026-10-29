//! Propriétés de fichiers → XML intermédiaire :
//!
//! ```xml
//! <FolderItem name="spec.docx" path="…">
//!   <Property id="0" name="Name" value="spec.docx"/>
//!   <Property id="21" name="Title" value="…"/>
//!   <Property name="Custom.Covers" value="THS_REQ1"/>   <!-- propriété personnalisée -->
//! </FolderItem>
//! ```
//!
//! Un répertoire donne un `<Folder>` contenant un `FolderItem` par fichier.
//! Propriétés lues : système de fichiers (nom, taille, dates), propriétés
//! Office Open XML (`docProps/core.xml`, `app.xml`, `custom.xml`), OpenDocument
//! (`meta.xml`) et PDF (dictionnaire `Info`). Les `id` reprennent la numérotation
//! des colonnes de l'Explorateur Windows pour les propriétés courantes ; leur
//! correspondance avec l'outil d'origine reste à confirmer (question Q11 de
//! `docs/interop-formats.md`).

use crate::archive::Archive;
use crate::{xml_escape, ConvertError, ConvertOptions};
use roxmltree::Document;
use std::path::Path;
use std::time::SystemTime;

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    if path.is_dir() {
        let mut out = format!(
            "<Folder name=\"{}\" path=\"{}\">",
            xml_escape(&file_name(path)),
            xml_escape(&path.display().to_string())
        );
        folder(path, path, options, &mut out)?;
        out.push_str("</Folder>");
        return Ok(out);
    }
    item(path, &path.display().to_string())
}

fn folder(
    root: &Path,
    dir: &Path,
    options: &ConvertOptions,
    out: &mut String,
) -> Result<(), ConvertError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|source| ConvertError::Io {
            path: dir.to_owned(),
            source,
        })?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for entry in entries {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        if options
            .excluded
            .iter()
            .any(|p| crate::wildcard_match(p, &rel))
        {
            continue;
        }
        if entry.is_dir() {
            folder(root, &entry, options, out)?;
        } else if options.matching.is_empty()
            || options
                .matching
                .iter()
                .any(|p| crate::wildcard_match(p, &rel))
        {
            out.push_str(&item(&entry, &rel)?);
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn item(path: &Path, shown: &str) -> Result<String, ConvertError> {
    let meta = std::fs::metadata(path).map_err(|source| ConvertError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut props: Vec<(Option<u32>, String, String)> = vec![
        (Some(0), "Name".into(), file_name(path)),
        (Some(1), "Size".into(), meta.len().to_string()),
    ];
    if let Ok(t) = meta.modified() {
        props.push((Some(3), "DateModified".into(), iso(t)));
    }
    if let Ok(t) = meta.created() {
        props.push((Some(4), "DateCreated".into(), iso(t)));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "docx" | "docm" | "xlsx" | "xlsm" | "pptx" | "pptm" | "vsdx" => {
            if let Ok(mut a) = Archive::open(path) {
                office(&mut a, &mut props);
            }
        }
        "odt" | "ods" | "odp" | "odg" => {
            if let Ok(mut a) = Archive::open(path) {
                opendocument(&mut a, &mut props);
            }
        }
        "pdf" => pdf(path, &mut props),
        _ => {}
    }
    let mut out = format!(
        "<FolderItem name=\"{}\" path=\"{}\">",
        xml_escape(&file_name(path)),
        xml_escape(shown)
    );
    for (id, name, value) in props {
        let id = id.map(|i| format!(" id=\"{i}\"")).unwrap_or_default();
        out.push_str(&format!(
            "<Property{id} name=\"{}\" value=\"{}\"/>",
            xml_escape(&name),
            xml_escape(value.trim())
        ));
    }
    out.push_str("</FolderItem>");
    Ok(out)
}

/// Numéro de colonne de l'Explorateur et nom ReqtiCraft des propriétés courantes.
fn known(local: &str) -> Option<(u32, &'static str)> {
    Some(match local {
        "title" => (21, "Title"),
        "subject" => (22, "Subject"),
        "creator" | "initial-creator" => (20, "Authors"),
        "keywords" | "keyword" => (18, "Tags"),
        "description" => (24, "Comments"),
        "category" => (23, "Categories"),
        "lastModifiedBy" => (10, "LastAuthor"),
        "revision" => (25, "Revision"),
        "contentStatus" => (26, "ContentStatus"),
        "Company" => (27, "Company"),
        "Manager" => (28, "Manager"),
        _ => return None,
    })
}

fn office(a: &mut Archive, props: &mut Vec<(Option<u32>, String, String)>) {
    for part in ["docProps/core.xml", "docProps/app.xml"] {
        let Ok(Some(xml)) = a.text(part) else {
            continue;
        };
        let Ok(doc) = Document::parse(&xml) else {
            continue;
        };
        for n in doc.root_element().children().filter(|n| n.is_element()) {
            if let Some((id, name)) = known(n.tag_name().name()) {
                props.push((Some(id), name.into(), n.text().unwrap_or("").to_owned()));
            }
        }
    }
    if let Ok(Some(xml)) = a.text("docProps/custom.xml") {
        if let Ok(doc) = Document::parse(&xml) {
            for p in doc
                .descendants()
                .filter(|n| n.tag_name().name() == "property")
            {
                let value: String = p
                    .descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect();
                props.push((
                    None,
                    format!("Custom.{}", p.attribute("name").unwrap_or("")),
                    value,
                ));
            }
        }
    }
}

fn opendocument(a: &mut Archive, props: &mut Vec<(Option<u32>, String, String)>) {
    let Ok(Some(xml)) = a.text("meta.xml") else {
        return;
    };
    let Ok(doc) = Document::parse(&xml) else {
        return;
    };
    let Some(meta) = doc.descendants().find(|n| {
        n.tag_name().name() == "meta"
            && n.parent()
                .is_some_and(|p| p.tag_name().name() == "document-meta")
    }) else {
        return;
    };
    for n in meta.children().filter(|n| n.is_element()) {
        let local = n.tag_name().name();
        if local == "user-defined" {
            let name = n
                .attributes()
                .find(|at| at.name() == "name")
                .map(|at| at.value())
                .unwrap_or("");
            props.push((
                None,
                format!("Custom.{name}"),
                n.text().unwrap_or("").to_owned(),
            ));
        } else if let Some((id, name)) = known(local) {
            props.push((Some(id), name.into(), n.text().unwrap_or("").to_owned()));
        }
    }
}

fn pdf(path: &Path, props: &mut Vec<(Option<u32>, String, String)>) {
    let Ok(doc) = lopdf::Document::load(path) else {
        return;
    };
    let Ok(info) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) else {
        return;
    };
    let Ok(dict) = doc.get_dictionary(info) else {
        return;
    };
    for (key, id, name) in [
        (&b"Title"[..], 21, "Title"),
        (b"Author", 20, "Authors"),
        (b"Subject", 22, "Subject"),
        (b"Keywords", 18, "Tags"),
    ] {
        if let Ok(v) = dict.get(key) {
            if let Ok(bytes) = v.as_str() {
                props.push((Some(id), name.into(), pdf_string(bytes)));
            }
        }
    }
}

/// Chaîne PDF : UTF-16BE avec indicateur d'ordre, sinon codage Latin-1.
fn pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    bytes.iter().map(|&b| b as char).collect()
}

/// Date UTC au format ISO 8601 (`2026-10-29T07:15:00Z`).
pub(crate) fn iso(t: SystemTime) -> String {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Algorithme « civil from days » (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    #[test]
    fn formats_iso_dates() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_258_100);
        assert_eq!(iso(t), "2026-10-29T07:15:00Z");
        assert_eq!(iso(SystemTime::UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn reads_office_core_and_custom_properties() {
        let dir = std::env::temp_dir().join(format!("rc-props-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spec.docx");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("docProps/core.xml", opts).unwrap();
        zip.write_all(br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Spec</dc:title><dc:creator>Alice</dc:creator></cp:coreProperties>"#).unwrap();
        zip.start_file("docProps/custom.xml", opts).unwrap();
        zip.write_all(br#"<Properties xmlns:vt="x"><property name="Covers"><vt:lpwstr>THS_REQ1</vt:lpwstr></property></Properties>"#).unwrap();
        zip.finish().unwrap();
        let out = item(&path, "spec.docx").unwrap();
        assert!(out.starts_with("<FolderItem name=\"spec.docx\" path=\"spec.docx\"><Property id=\"0\" name=\"Name\" value=\"spec.docx\"/>"));
        assert!(out.contains("<Property id=\"21\" name=\"Title\" value=\"Spec\"/><Property id=\"20\" name=\"Authors\" value=\"Alice\"/>"));
        assert!(out.contains("<Property name=\"Custom.Covers\" value=\"THS_REQ1\"/>"));
    }
}
