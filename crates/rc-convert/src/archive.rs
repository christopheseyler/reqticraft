//! Lecture des parties d'un conteneur ZIP (OOXML, OpenDocument, ReqIFz…).

use crate::ConvertError;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Archive {
    path: PathBuf,
    zip: zip::ZipArchive<std::fs::File>,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Archive, ConvertError> {
        let file = std::fs::File::open(path).map_err(|source| ConvertError::Io {
            path: path.to_owned(),
            source,
        })?;
        let zip = zip::ZipArchive::new(file).map_err(|e| ConvertError::format(path, e))?;
        Ok(Archive {
            path: path.to_owned(),
            zip,
        })
    }

    /// Partie texte (UTF-8), ou `None` si elle n'existe pas.
    pub fn text(&mut self, name: &str) -> Result<Option<String>, ConvertError> {
        let Some(bytes) = self.bytes(name)? else {
            return Ok(None);
        };
        Ok(Some(rc_interop::text::decode(&bytes)))
    }

    pub fn bytes(&mut self, name: &str) -> Result<Option<Vec<u8>>, ConvertError> {
        let Ok(mut entry) = self.zip.by_name(name) else {
            return Ok(None);
        };
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ConvertError::format(&self.path, e))?;
        Ok(Some(buf))
    }

    /// Partie obligatoire.
    pub fn require(&mut self, name: &str) -> Result<String, ConvertError> {
        self.text(name)?
            .ok_or_else(|| ConvertError::format(&self.path, format!("{name} absent")))
    }

    pub fn names(&self) -> Vec<String> {
        self.zip.file_names().map(str::to_owned).collect()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Résout une cible de relation OOXML (`../media/x.png`) depuis une partie.
pub fn resolve(base_part: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_owned();
    }
    let mut parts: Vec<&str> = base_part.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Relations d'une partie OOXML : identifiant → (type, cible résolue).
pub fn relationships(
    archive: &mut Archive,
    part: &str,
) -> Result<Vec<(String, String, String)>, ConvertError> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let rels_name = if dir.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{dir}/_rels/{file}.rels")
    };
    let Some(xml) = archive.text(&rels_name)? else {
        return Ok(Vec::new());
    };
    let doc =
        roxmltree::Document::parse(&xml).map_err(|e| ConvertError::format(archive.path(), e))?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("Relationship"))
        .filter_map(|n| {
            let id = n.attribute("Id")?;
            let kind = n.attribute("Type").unwrap_or("");
            let target = n.attribute("Target")?;
            let resolved = if n.attribute("TargetMode") == Some("External") {
                target.to_owned()
            } else {
                resolve(part, target)
            };
            Some((
                id.to_owned(),
                kind.rsplit('/').next().unwrap_or("").to_owned(),
                resolved,
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_targets() {
        assert_eq!(
            resolve("ppt/slides/slide1.xml", "../notesSlides/notesSlide1.xml"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert_eq!(
            resolve("ppt/presentation.xml", "slides/slide2.xml"),
            "ppt/slides/slide2.xml"
        );
        assert_eq!(resolve("x/y.xml", "/ppt/a.xml"), "ppt/a.xml");
    }
}
