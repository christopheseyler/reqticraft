//! OpenDocument Texte (`.odt`, OpenOffice et LibreOffice Writer) → lignes
//! stylées, comme pour Word : `<style><TAB><numéro><séparateur><texte>` et
//! tableaux `#TABLE` / `#TR` (format décrit par le connecteur Writer,
//! `docs/interop-formats.md` §5). Le style est le nom affiché (`Heading 1`).

use crate::archive::Archive;
use crate::lines::{Lines, Para};
use crate::{ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    let mut archive = Archive::open(path)?;
    let content = archive.require("content.xml")?;
    let styles_xml = archive.text("styles.xml")?.unwrap_or_default();
    let content = Document::parse(&content).map_err(|e| ConvertError::format(path, e))?;
    let mut styles = Styles::default();
    if let Ok(doc) = Document::parse(&styles_xml) {
        styles.read(&doc);
    }
    styles.read(&content);
    let body = content
        .descendants()
        .find(|n| n.has_tag_name((OFFICE, "text")))
        .ok_or_else(|| ConvertError::format(path, "corps du document absent"))?;
    let mut w = Writer {
        styles: &styles,
        counters: [0; 10],
        char_styles: options.flag("withCharacterStyles"),
        lines: Lines::default(),
    };
    w.block(body);
    Ok(w.lines.finish())
}

const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";

#[derive(Default)]
struct Styles {
    /// Nom interne → (nom affiché, parent).
    styles: HashMap<String, (Option<String>, Option<String>)>,
    /// Format de numérotation des titres par niveau (`1`, `i`, `A`…), vide si aucun.
    outline: HashMap<u32, (String, String)>,
}

impl Styles {
    fn read(&mut self, doc: &Document) {
        for s in doc
            .descendants()
            .filter(|n| n.has_tag_name((STYLE, "style")))
        {
            let Some(name) = s.attribute((STYLE, "name")) else {
                continue;
            };
            self.styles.insert(
                name.to_owned(),
                (
                    s.attribute((STYLE, "display-name")).map(str::to_owned),
                    s.attribute((STYLE, "parent-style-name")).map(str::to_owned),
                ),
            );
        }
        for l in doc
            .descendants()
            .filter(|n| n.has_tag_name((TEXT, "outline-level-style")))
        {
            let level = l
                .attribute((TEXT, "level"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let format = l.attribute((STYLE, "num-format")).unwrap_or("").to_owned();
            let suffix = l.attribute((STYLE, "num-suffix")).unwrap_or("").to_owned();
            self.outline.insert(level, (format, suffix));
        }
    }

    /// Nom affiché d'un style ; les styles automatiques (`P1`) renvoient à leur parent.
    fn display(&self, name: &str) -> String {
        let mut current = name.to_owned();
        for _ in 0..10 {
            match self.styles.get(&current) {
                Some((display, Some(parent))) if display.is_none() && is_automatic(&current) => {
                    current = parent.clone();
                }
                Some((Some(display), _)) => return display.clone(),
                _ => break,
            }
        }
        decode_name(&current)
    }
}

/// Styles automatiques : `P1`, `T3`, `Table1.A1`…
fn is_automatic(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.as_str().chars().all(|c| c.is_ascii_digit())
        && name.len() > 1
}

/// `Heading_20_1` → `Heading 1` (codage `_xx_` des noms de styles).
fn decode_name(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(i) = rest.find('_') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find('_') {
            Some(j) if j > 0 && j <= 4 && tail[..j].chars().all(|c| c.is_ascii_hexdigit()) => {
                if let Some(c) = u32::from_str_radix(&tail[..j], 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    out.push(c);
                }
                rest = &tail[j + 1..];
            }
            _ => {
                out.push('_');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Formats de numérotation OpenDocument → noms utilisés par le convertisseur Word.
fn odf_format(format: &str) -> &'static str {
    match format {
        "i" => "lowerRoman",
        "I" => "upperRoman",
        "a" => "lowerLetter",
        "A" => "upperLetter",
        _ => "decimal",
    }
}

struct Writer<'a> {
    styles: &'a Styles,
    counters: [u32; 10],
    char_styles: bool,
    lines: Lines,
}

impl Writer<'_> {
    fn block(&mut self, node: Node) {
        for c in node.children().filter(|n| n.is_element()) {
            let tag = c.tag_name();
            match (tag.namespace(), tag.name()) {
                (Some(TEXT), "p") => {
                    let (style, text) = self.paragraph(c, None);
                    self.lines.para(&style, &text);
                    self.character_styles(c);
                }
                (Some(TEXT), "h") => {
                    let level = c
                        .attribute((TEXT, "outline-level"))
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1u32);
                    let (style, text) = self.paragraph(c, Some(level));
                    self.lines.para(&style, &text);
                    self.character_styles(c);
                }
                (Some(TABLE), "table") => {
                    let rows = self.table(c);
                    self.lines.table(&rows);
                }
                (Some(TEXT), "tracked-changes") | (Some(TEXT), "sequence-decls") => {}
                _ => self.block(c),
            }
        }
    }

    fn paragraph(&mut self, p: Node, level: Option<u32>) -> (String, String) {
        let style = self
            .styles
            .display(p.attribute((TEXT, "style-name")).unwrap_or("Standard"));
        let mut text = String::new();
        let mut explicit_number = None;
        inline(p, &mut text, &mut explicit_number);
        let number = match (explicit_number, level) {
            (Some(n), _) => Some(n),
            (None, Some(level)) => self.next_number(level),
            _ => None,
        };
        let text = match number {
            Some(n) if !n.trim().is_empty() => format!("{}\t{text}", n.trim_end()),
            _ => text,
        };
        (style, text)
    }

    fn next_number(&mut self, level: u32) -> Option<String> {
        let l = level.clamp(1, 10) as usize;
        self.counters[l - 1] += 1;
        for c in self.counters.iter_mut().skip(l) {
            *c = 0;
        }
        let (format, suffix) = self.styles.outline.get(&level)?;
        if format.is_empty() {
            return None;
        }
        let parts: Vec<String> = (1..=l)
            .filter(|i| {
                self.styles
                    .outline
                    .get(&(*i as u32))
                    .is_some_and(|(f, _)| !f.is_empty())
            })
            .map(|i| {
                crate::docx::format_number(
                    self.counters[i - 1].max(1),
                    odf_format(&self.styles.outline[&(i as u32)].0),
                )
            })
            .collect();
        Some(format!("{}{suffix}", parts.join(".")))
    }

    fn character_styles(&mut self, p: Node) {
        if !self.char_styles {
            return;
        }
        for span in p.descendants().filter(|n| n.has_tag_name((TEXT, "span"))) {
            let Some(name) = span.attribute((TEXT, "style-name")) else {
                continue;
            };
            if is_automatic(name) {
                continue;
            }
            let mut text = String::new();
            inline(span, &mut text, &mut None);
            self.lines.para(&self.styles.display(name), &text);
        }
    }

    fn table(&mut self, table: Node) -> Vec<Vec<Vec<Para>>> {
        let mut rows = Vec::new();
        for row in table
            .descendants()
            .filter(|n| n.has_tag_name((TABLE, "table-row")))
            .filter(|r| {
                r.ancestors()
                    .find(|a| a.has_tag_name((TABLE, "table")))
                    .is_some_and(|t| t.id() == table.id())
            })
        {
            let mut cells = Vec::new();
            for cell in row.children().filter(|n| {
                n.has_tag_name((TABLE, "table-cell"))
                    || n.has_tag_name((TABLE, "covered-table-cell"))
            }) {
                let mut sub = Writer {
                    styles: self.styles,
                    counters: self.counters,
                    char_styles: false,
                    lines: Lines::default(),
                };
                sub.block(cell);
                let paras = sub
                    .lines
                    .finish()
                    .lines()
                    .filter_map(|l| {
                        l.split_once('\t')
                            .map(|(s, t)| (s.to_owned(), t.to_owned()))
                    })
                    .collect();
                cells.push(paras);
            }
            rows.push(cells);
        }
        rows
    }
}

/// Texte d'un paragraphe : espaces `text:s`, tabulations, sauts de ligne ;
/// notes de bas de page et annotations exclues.
fn inline(node: Node, out: &mut String, number: &mut Option<String>) {
    for c in node.children() {
        if c.is_text() {
            out.push_str(c.text().unwrap_or(""));
            continue;
        }
        if !c.is_element() {
            continue;
        }
        let tag = c.tag_name();
        if tag.namespace() != Some(TEXT) {
            // Cadres de texte : leur contenu est conservé.
            if tag.name() == "frame" || tag.name() == "text-box" {
                inline(c, out, number);
            }
            continue;
        }
        match tag.name() {
            "s" => {
                let n = c
                    .attribute((TEXT, "c"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1usize);
                out.push_str(&" ".repeat(n));
            }
            "tab" => out.push('\t'),
            "line-break" => out.push(' '),
            "number" => *number = Some(c.text().unwrap_or("").to_owned()),
            "note" | "annotation" | "bookmark" | "bookmark-start" | "bookmark-end"
            | "soft-page-break" | "change-start" | "change-end" | "change" => {}
            _ => inline(c, out, number),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn odt(content_body: &str, styles: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rc-odt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("t{}.odt", content_body.len()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("mimetype", opts).unwrap();
        zip.write_all(b"application/vnd.oasis.opendocument.text")
            .unwrap();
        zip.start_file("content.xml", opts).unwrap();
        write!(
            zip,
            r#"<office:document-content xmlns:office="{OFFICE}" xmlns:text="{TEXT}" xmlns:style="{STYLE}" xmlns:table="{TABLE}"><office:automatic-styles><style:style style:name="P1" style:parent-style-name="Requirement_5f_ID"/></office:automatic-styles><office:body><office:text>{content_body}</office:text></office:body></office:document-content>"#
        )
        .unwrap();
        zip.start_file("styles.xml", opts).unwrap();
        write!(
            zip,
            r#"<office:document-styles xmlns:office="{OFFICE}" xmlns:text="{TEXT}" xmlns:style="{STYLE}"><office:styles>{styles}</office:styles></office:document-styles>"#
        )
        .unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn converts_headings_styles_and_tables() {
        let styles = r#"<style:style style:name="Heading_20_1" style:display-name="Heading 1"/>
            <style:style style:name="Requirement_5f_ID" style:display-name="Requirement_ID"/>
            <text:outline-style><text:outline-level-style text:level="1" style:num-format="1"/><text:outline-level-style text:level="2" style:num-format="1"/></text:outline-style>"#;
        let body = r#"<text:h text:style-name="Heading_20_1" text:outline-level="1">Intro</text:h>
            <text:p text:style-name="P1">REQ1<text:s text:c="2"/>: Démarrer<text:note><text:note-body><text:p>note</text:p></text:note-body></text:note></text:p>
            <text:h text:style-name="Heading_20_2" text:outline-level="2">Détail</text:h>
            <table:table><table:table-row><table:table-cell><text:p text:style-name="Standard">A1</text:p></table:table-cell><table:table-cell><text:p>B1</text:p></table:table-cell></table:table-row></table:table>
            <text:list><text:list-item><text:p text:style-name="Standard">puce</text:p></text:list-item></text:list>"#;
        let path = odt(body, styles);
        assert_eq!(
            convert(&path, &ConvertOptions::default()).unwrap(),
            "Heading 1\t1 Intro\n\
             Requirement_ID\tREQ1  : Démarrer\n\
             Heading 2\t1.1 Détail\n\
             #TABLE\n#TR|1 Standard\tA1|2 Standard\tB1\n\
             Standard\tpuce\n"
        );
    }

    #[test]
    fn decodes_style_names() {
        assert_eq!(decode_name("Heading_20_1"), "Heading 1");
        assert_eq!(decode_name("Requirement_5f_ID"), "Requirement_ID");
        assert_eq!(decode_name("my_style"), "my_style");
    }
}
