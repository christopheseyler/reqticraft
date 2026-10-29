//! PDF → intermédiaire texte : un paragraphe reconstitué par ligne.
//!
//! Les caractères sont regroupés en lignes (même ligne de base), puis les lignes
//! en paragraphes. Une ligne termine son paragraphe quand :
//! - la ligne suivante change de taille de police ;
//! - l'interligne dépasse 1,6 fois la taille de police ;
//! - elle finit par `.`, `:`, `;`, `!`, `?` ou `]` ;
//! - elle est nettement plus courte que la ligne la plus large de la page
//!   (règle des « phrases entières »).
//!
//! Une césure en fin de ligne (`-` suivi d'une minuscule) est supprimée.
//!
//! Paramètres : `pagesToExclude` (`1,2,4-8`), `password`, `withStyles` (les
//! paragraphes égaux à un signet sont préfixés par `Heading <niveau><TAB>`).
//! Règles inspirées de la documentation du connecteur PDF ; seuils maison.

use crate::{ConvertError, ConvertOptions};
use pdf_extract::{Document, MediaBox, OutputDev, OutputError, Transform};
use std::collections::BTreeSet;
use std::path::Path;

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    let bytes = std::fs::read(path).map_err(|source| ConvertError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut doc = Document::load_mem(&bytes).map_err(|e| ConvertError::format(path, e))?;
    if doc.is_encrypted() {
        let password = options.param("password").unwrap_or_default();
        doc.decrypt(password)
            .map_err(|e| ConvertError::format(path, format!("PDF protégé : {e}")))?;
    }
    let excluded = options
        .param("pagesToExclude")
        .map(parse_pages)
        .unwrap_or_default();
    let titles = if options.flag("withStyles") {
        bookmarks(&doc)
    } else {
        Vec::new()
    };

    let mut collector = Collector::default();
    for &page in doc.get_pages().keys() {
        if excluded.contains(&page) {
            continue;
        }
        pdf_extract::output_doc_page(&doc, &mut collector, page)
            .map_err(|e| ConvertError::format(path, e))?;
    }

    let mut out = String::new();
    for page in &collector.pages {
        for paragraph in paragraphs(page) {
            let style = titles
                .iter()
                .find(|(title, _)| normalize(title) == normalize(&paragraph))
                .map(|(_, level)| format!("Heading {level}\t"));
            out.push_str(style.as_deref().unwrap_or_default());
            out.push_str(&paragraph);
            out.push('\n');
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct Glyph {
    x: f64,
    y: f64,
    size: f64,
    advance: f64,
    text: String,
}

#[derive(Default)]
struct Collector {
    pages: Vec<Vec<Glyph>>,
}

impl OutputDev for Collector {
    fn begin_page(
        &mut self,
        _: u32,
        _: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.pages.push(Vec::new());
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        spacing: f64,
        font_size: f64,
        ch: &str,
    ) -> Result<(), OutputError> {
        // Taille effective : côté du carré de même aire que le vecteur (taille, taille) transformé.
        let vx = font_size * (trm.m11 + trm.m21);
        let vy = font_size * (trm.m12 + trm.m22);
        let size = (vx * vy).abs().sqrt();
        let scale = (trm.m11 * trm.m11 + trm.m12 * trm.m12).sqrt();
        if let Some(page) = self.pages.last_mut() {
            page.push(Glyph {
                x: trm.m31,
                y: trm.m32,
                size,
                advance: (width * font_size + spacing) * scale,
                text: ch.to_owned(),
            });
        }
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

#[derive(Debug)]
struct Line {
    y: f64,
    size: f64,
    left: f64,
    right: f64,
    text: String,
}

/// Regroupe les caractères d'une page en lignes, de haut en bas.
fn lines(glyphs: &[Glyph]) -> Vec<Line> {
    let mut sorted: Vec<&Glyph> = glyphs.iter().filter(|g| g.size > 0.0).collect();
    sorted.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut rows: Vec<Vec<&Glyph>> = Vec::new();
    for g in sorted {
        match rows.last_mut() {
            Some(row) if (row[0].y - g.y).abs() <= row[0].size.max(g.size) * 0.4 => row.push(g),
            _ => rows.push(vec![g]),
        }
    }
    rows.into_iter()
        .map(|mut row| {
            row.sort_by(|a, b| a.x.total_cmp(&b.x));
            let mut text = String::new();
            let mut end: Option<f64> = None;
            for g in &row {
                if let Some(e) = end {
                    // Un blanc supérieur à un quart de la taille de police sépare deux mots.
                    if g.x - e > g.size * 0.25 && !text.ends_with(' ') && g.text != " " {
                        text.push(' ');
                    }
                }
                text.push_str(&g.text);
                end = Some(g.x + g.advance);
            }
            let size = row.iter().map(|g| g.size).fold(0.0, f64::max);
            Line {
                y: row[0].y,
                size,
                left: row[0].x,
                right: end.unwrap_or(row[0].x),
                text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            }
        })
        .filter(|l| !l.text.is_empty())
        .collect()
}

fn paragraphs(glyphs: &[Glyph]) -> Vec<String> {
    let lines = lines(glyphs);
    let max_width = lines.iter().map(|l| l.right - l.left).fold(0.0, f64::max);
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, line) in lines.iter().enumerate() {
        if current.is_empty() {
            current = line.text.clone();
        } else if current.ends_with('-') && line.text.chars().next().is_some_and(char::is_lowercase)
        {
            current.pop();
            current.push_str(&line.text);
        } else {
            current.push(' ');
            current.push_str(&line.text);
        }
        let ends = match lines.get(i + 1) {
            None => true,
            Some(next) => {
                (next.size - line.size).abs() > line.size * 0.1
                    || line.y - next.y > line.size.max(next.size) * 1.6
                    || line.text.ends_with(['.', ':', ';', '!', '?', ']'])
                    || line.right - line.left < max_width * 0.75
            }
        };
        if ends {
            out.push(std::mem::take(&mut current));
        }
    }
    out
}

/// Signets à plat : (titre, niveau base 1).
fn bookmarks(doc: &Document) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    let Ok(outlines) = catalog.get(b"Outlines").and_then(|o| o.as_reference()) else {
        return out;
    };
    let Ok(root) = doc.get_dictionary(outlines) else {
        return out;
    };
    if let Ok(first) = root.get(b"First").and_then(|f| f.as_reference()) {
        walk_outline(doc, first, 1, &mut out, &mut BTreeSet::new());
    }
    out
}

fn walk_outline(
    doc: &Document,
    mut id: lopdf::ObjectId,
    level: usize,
    out: &mut Vec<(String, usize)>,
    seen: &mut BTreeSet<lopdf::ObjectId>,
) {
    while seen.insert(id) {
        let Ok(item) = doc.get_dictionary(id) else {
            return;
        };
        if let Ok(title) = item.get(b"Title").and_then(|t| t.as_str()) {
            out.push((
                lopdf::decode_text_string(&lopdf::Object::String(
                    title.to_vec(),
                    lopdf::StringFormat::Literal,
                ))
                .unwrap_or_default(),
                level,
            ));
        }
        if let Ok(child) = item.get(b"First").and_then(|f| f.as_reference()) {
            walk_outline(doc, child, level + 1, out, seen);
        }
        match item.get(b"Next").and_then(|n| n.as_reference()) {
            Ok(next) => id = next,
            Err(_) => return,
        }
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `1,2,4-8` (`;` accepté) → pages exclues.
fn parse_pages(spec: &str) -> BTreeSet<u32> {
    let mut pages = BTreeSet::new();
    for part in spec
        .split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                    pages.extend(a..=b);
                }
            }
            None => {
                if let Ok(p) = part.parse() {
                    pages.insert(p);
                }
            }
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs(lines: &[(f64, f64, &str)]) -> Vec<Glyph> {
        // (y, taille, texte) ; un glyphe par caractère, avance = 0,5 × taille.
        let mut out = Vec::new();
        for &(y, size, text) in lines {
            for (i, ch) in text.chars().enumerate() {
                out.push(Glyph {
                    x: 50.0 + i as f64 * size * 0.5,
                    y,
                    size,
                    advance: size * 0.5,
                    text: ch.to_string(),
                });
            }
        }
        out
    }

    #[test]
    fn rebuilds_paragraphs() {
        let page = glyphs(&[
            (800.0, 16.0, "1. Titre"),
            (770.0, 10.0, "REQ1 : Capture"),
            (
                756.0,
                10.0,
                "Une longue phrase qui occupe toute la largeur de la page et qui conti-",
            ),
            (
                742.0,
                10.0,
                "nue sur la ligne suivante avant de se terminer ici.",
            ),
            (728.0, 10.0, "[Covers: A1]"),
        ]);
        assert_eq!(
            paragraphs(&page),
            [
                "1. Titre",
                "REQ1 : Capture",
                "Une longue phrase qui occupe toute la largeur de la page et qui continue sur la ligne suivante avant de se terminer ici.",
                "[Covers: A1]",
            ]
        );
    }

    #[test]
    fn parses_page_ranges() {
        assert_eq!(
            parse_pages("1, 3-5;9").into_iter().collect::<Vec<_>>(),
            [1, 3, 4, 5, 9]
        );
    }
}
