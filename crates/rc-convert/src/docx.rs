//! Word `.docx` → intermédiaire texte.
//!
//! Une ligne par paragraphe : `<nom du style><TAB>[<numéro><séparateur>]<texte>`.
//! Tableaux : `#TABLE`, puis une ligne `#TR` par rangée avec, pour chaque
//! paragraphe de cellule, `|<colonne> <style><TAB><texte>`. Avec le paramètre
//! `withCharacterStyles`, chaque passage en style de caractère ajoute une ligne
//! `<style de caractère><TAB><texte>`.
//! Format décrit dans la documentation du connecteur Word (`docs/interop-formats.md` §5).

use crate::{ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("doc"))
    {
        return Err(ConvertError::format(
            path,
            "format Word 97-2003 (.doc) non pris en charge : enregistrer le document en .docx",
        ));
    }
    let file = std::fs::File::open(path).map_err(|source| ConvertError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ConvertError::format(path, e))?;
    let mut part = |name: &str| -> Result<Option<String>, ConvertError> {
        let Ok(mut entry) = archive.by_name(name) else {
            return Ok(None);
        };
        let mut s = String::new();
        entry
            .read_to_string(&mut s)
            .map_err(|e| ConvertError::format(path, e))?;
        Ok(Some(s))
    };
    let document = part("word/document.xml")?
        .ok_or_else(|| ConvertError::format(path, "word/document.xml absent"))?;
    let styles = part("word/styles.xml")?.unwrap_or_default();
    let numbering = part("word/numbering.xml")?.unwrap_or_default();

    let styles = Styles::parse(&styles);
    let numbering = Numbering::parse(&numbering);
    let doc = Document::parse(&document).map_err(|e| ConvertError::format(path, e))?;
    let body = doc
        .descendants()
        .find(|n| is_w(n, "body"))
        .ok_or_else(|| ConvertError::format(path, "corps du document absent"))?;

    let mut writer = Writer {
        styles: &styles,
        numbering: &numbering,
        counters: HashMap::new(),
        char_styles: options.flag("withCharacterStyles"),
        out: String::new(),
    };
    writer.block(body);
    Ok(writer.out)
}

fn is_w(node: &Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(W)
}

fn w_attr<'a>(node: &Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.attribute((W, name))
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|c| is_w(c, name))
}

#[derive(Default)]
struct Styles {
    names: HashMap<String, String>,
    based_on: HashMap<String, String>,
    /// Numérotation portée par un style de paragraphe (titres numérotés).
    numbering: HashMap<String, (String, usize)>,
    default_paragraph: Option<String>,
}

impl Styles {
    fn parse(xml: &str) -> Styles {
        let mut styles = Styles::default();
        let Ok(doc) = Document::parse(xml) else {
            return styles;
        };
        for s in doc.descendants().filter(|n| is_w(n, "style")) {
            let Some(id) = w_attr(&s, "styleId") else {
                continue;
            };
            if let Some(name) = child(s, "name").and_then(|n| w_attr(&n, "val")) {
                styles.names.insert(id.to_owned(), name.to_owned());
            }
            if let Some(base) = child(s, "basedOn").and_then(|n| w_attr(&n, "val")) {
                styles.based_on.insert(id.to_owned(), base.to_owned());
            }
            if let Some(num) = child(s, "pPr").and_then(|p| child(p, "numPr")) {
                if let Some(num_id) = child(num, "numId").and_then(|n| w_attr(&n, "val")) {
                    let level = child(num, "ilvl")
                        .and_then(|n| w_attr(&n, "val"))
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    styles
                        .numbering
                        .insert(id.to_owned(), (num_id.to_owned(), level));
                }
            }
            if w_attr(&s, "type") == Some("paragraph") && w_attr(&s, "default") == Some("1") {
                styles.default_paragraph = Some(id.to_owned());
            }
        }
        styles
    }

    /// Nom d'origine du style (les alias ne sont pas utilisés).
    fn name(&self, id: &str) -> String {
        self.names.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }

    fn numbering(&self, id: &str) -> Option<(String, usize)> {
        let mut current = Some(id);
        for _ in 0..10 {
            let id = current?;
            if let Some(n) = self.numbering.get(id) {
                return Some(n.clone());
            }
            current = self.based_on.get(id).map(String::as_str);
        }
        None
    }
}

#[derive(Clone)]
struct Level {
    format: String,
    text: String,
    start: u32,
    suffix: &'static str,
}

#[derive(Default)]
struct Numbering {
    /// numId → abstractNumId.
    nums: HashMap<String, String>,
    abstracts: HashMap<String, Vec<Level>>,
}

impl Numbering {
    fn parse(xml: &str) -> Numbering {
        let mut numbering = Numbering::default();
        let Ok(doc) = Document::parse(xml) else {
            return numbering;
        };
        for a in doc.descendants().filter(|n| is_w(n, "abstractNum")) {
            let Some(id) = w_attr(&a, "abstractNumId") else {
                continue;
            };
            let mut levels = vec![
                Level {
                    format: "decimal".into(),
                    text: String::new(),
                    start: 1,
                    suffix: "\t",
                };
                9
            ];
            for lvl in a.children().filter(|n| is_w(n, "lvl")) {
                let Some(i) = w_attr(&lvl, "ilvl").and_then(|v| v.parse::<usize>().ok()) else {
                    continue;
                };
                if i >= 9 {
                    continue;
                }
                let val = |name: &str| child(lvl, name).and_then(|n| w_attr(&n, "val"));
                levels[i] = Level {
                    format: val("numFmt").unwrap_or("decimal").to_owned(),
                    text: val("lvlText").unwrap_or_default().to_owned(),
                    start: val("start").and_then(|v| v.parse().ok()).unwrap_or(1),
                    suffix: match val("suff") {
                        Some("space") => " ",
                        Some("nothing") => "",
                        _ => "\t",
                    },
                };
            }
            numbering.abstracts.insert(id.to_owned(), levels);
        }
        for n in doc.descendants().filter(|n| is_w(n, "num")) {
            if let (Some(id), Some(abs)) = (
                w_attr(&n, "numId"),
                child(n, "abstractNumId").and_then(|a| w_attr(&a, "val")),
            ) {
                numbering.nums.insert(id.to_owned(), abs.to_owned());
            }
        }
        numbering
    }
}

struct Writer<'a> {
    styles: &'a Styles,
    numbering: &'a Numbering,
    /// Compteurs par liste abstraite et par niveau.
    counters: HashMap<String, [u32; 9]>,
    char_styles: bool,
    out: String,
}

impl Writer<'_> {
    fn block(&mut self, node: Node) {
        for c in node.children().filter(|n| n.is_element()) {
            if is_w(&c, "p") {
                let (style, text) = self.paragraph(c);
                self.out.push_str(&style);
                self.out.push('\t');
                self.out.push_str(&text);
                self.out.push('\n');
                self.character_styles(c);
            } else if is_w(&c, "tbl") {
                self.table(c);
            } else if is_w(&c, "sdt") {
                if let Some(content) = child(c, "sdtContent") {
                    self.block(content);
                }
            }
        }
    }

    /// Style et texte (numéro compris) d'un paragraphe.
    fn paragraph(&mut self, p: Node) -> (String, String) {
        let ppr = child(p, "pPr");
        let style_id = ppr
            .and_then(|pp| child(pp, "pStyle"))
            .and_then(|s| w_attr(&s, "val"))
            .map(str::to_owned)
            .or_else(|| self.styles.default_paragraph.clone())
            .unwrap_or_else(|| "Normal".into());
        let style = self.styles.name(&style_id);

        let direct = ppr.and_then(|pp| child(pp, "numPr")).and_then(|num| {
            let id = child(num, "numId").and_then(|n| w_attr(&n, "val"))?;
            let level = child(num, "ilvl")
                .and_then(|n| w_attr(&n, "val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            Some((id.to_owned(), level))
        });
        let number = direct
            .or_else(|| self.styles.numbering(&style_id))
            .and_then(|(id, level)| self.next_number(&id, level));

        let text = run_text(p);
        let text = match number {
            Some((n, sep)) if !n.is_empty() => format!("{n}{sep}{text}"),
            _ => text,
        };
        (style, text)
    }

    fn next_number(&mut self, num_id: &str, level: usize) -> Option<(String, &'static str)> {
        if num_id == "0" || level >= 9 {
            return None;
        }
        let abs = self.numbering.nums.get(num_id)?;
        let levels = self.numbering.abstracts.get(abs)?;
        let counters = self.counters.entry(abs.clone()).or_insert_with(|| {
            let mut c = [0; 9];
            for (i, l) in levels.iter().enumerate() {
                c[i] = l.start.saturating_sub(1);
            }
            c
        });
        counters[level] += 1;
        for (deeper, l) in levels.iter().enumerate().skip(level + 1) {
            counters[deeper] = l.start.saturating_sub(1);
        }
        let lvl = &levels[level];
        if lvl.format == "bullet" || lvl.format == "none" {
            return None;
        }
        let mut text = lvl.text.clone();
        for (i, l) in levels.iter().enumerate().take(level + 1) {
            text = text.replace(
                &format!("%{}", i + 1),
                &format_number(counters[i], &l.format),
            );
        }
        Some((text, lvl.suffix))
    }

    fn table(&mut self, tbl: Node) {
        self.out.push_str("#TABLE\n");
        for row in tbl.children().filter(|n| is_w(n, "tr")) {
            self.out.push_str("#TR");
            for (col, cell) in row.children().filter(|n| is_w(n, "tc")).enumerate() {
                for p in cell.descendants().filter(|n| is_w(n, "p")) {
                    let (style, text) = self.paragraph(p);
                    self.out
                        .push_str(&format!("|{} {}\t{}", col + 1, style, text));
                }
            }
            self.out.push('\n');
        }
    }

    fn character_styles(&mut self, p: Node) {
        if !self.char_styles {
            return;
        }
        let mut current: Option<(String, String)> = None;
        for r in p.descendants().filter(|n| is_w(n, "r")) {
            let style = child(r, "rPr")
                .and_then(|rp| child(rp, "rStyle"))
                .and_then(|s| w_attr(&s, "val"))
                .map(|id| self.styles.name(id));
            let text = single_run_text(r);
            match (&mut current, style) {
                (Some((cur, buf)), Some(s)) if *cur == s => buf.push_str(&text),
                (_, s) => {
                    if let Some((name, buf)) = current.take() {
                        self.out.push_str(&format!("{name}\t{buf}\n"));
                    }
                    current = s.map(|s| (s, text));
                }
            }
        }
        if let Some((name, buf)) = current {
            self.out.push_str(&format!("{name}\t{buf}\n"));
        }
    }
}

fn is_hidden(run: Node) -> bool {
    child(run, "rPr")
        .and_then(|rp| child(rp, "vanish"))
        .is_some_and(|v| !matches!(w_attr(&v, "val"), Some("0" | "false")))
}

fn single_run_text(run: Node) -> String {
    if is_hidden(run) {
        return String::new();
    }
    let mut s = String::new();
    for c in run.children().filter(|n| n.is_element()) {
        if is_w(&c, "t") {
            // Les espaces insécables deviennent des espaces : les types d'analyse
            // attendent `REQ1 : libellé` même quand Word a inséré U+00A0 avant `:`.
            s.extend(c.text().unwrap_or_default().chars().map(|ch| match ch {
                '\u{a0}' | '\u{202f}' => ' ',
                ch => ch,
            }));
        } else if is_w(&c, "tab") {
            s.push('\t');
        } else if is_w(&c, "br") || is_w(&c, "cr") {
            s.push(' ');
        } else if is_w(&c, "noBreakHyphen") {
            s.push('-');
        }
    }
    s
}

/// Texte visible d'un paragraphe (texte supprimé en suivi des modifications exclu).
fn run_text(p: Node) -> String {
    let mut s = String::new();
    for r in p.descendants().filter(|n| is_w(n, "r")) {
        if r.ancestors().any(|a| is_w(&a, "del")) {
            continue;
        }
        s.push_str(&single_run_text(r));
    }
    s
}

fn format_number(n: u32, format: &str) -> String {
    match format {
        "lowerLetter" => letters(n).to_lowercase(),
        "upperLetter" => letters(n),
        "lowerRoman" => roman(n).to_lowercase(),
        "upperRoman" => roman(n),
        _ => n.to_string(),
    }
}

fn letters(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let letter = char::from(b'A' + ((n - 1) % 26) as u8);
    std::iter::repeat_n(letter, ((n - 1) / 26 + 1) as usize).collect()
}

fn roman(mut n: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for &(value, digits) in TABLE {
        while n >= value {
            s.push_str(digits);
            n -= value;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn docx(document_body: &str, styles: &str, numbering: Option<&str>) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rc-docx-{}-{}.docx",
            std::process::id(),
            document_body.len() + styles.len()
        ));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", opts).unwrap();
        write!(
            zip,
            r#"<w:document xmlns:w="{W}"><w:body>{document_body}</w:body></w:document>"#
        )
        .unwrap();
        zip.start_file("word/styles.xml", opts).unwrap();
        write!(zip, r#"<w:styles xmlns:w="{W}">{styles}</w:styles>"#).unwrap();
        if let Some(n) = numbering {
            zip.start_file("word/numbering.xml", opts).unwrap();
            write!(zip, r#"<w:numbering xmlns:w="{W}">{n}</w:numbering>"#).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn p(style: &str, text: &str) -> String {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
        )
    }

    const STYLES: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="ReqID"><w:name w:val="Requirement_ID"/></w:style>
<w:style w:type="character" w:styleId="Req"><w:name w:val="Requirement"/></w:style>"#;

    const NUMBERING: &str = r#"<w:abstractNum w:abstractNumId="7">
<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl>
<w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/><w:suff w:val="space"/></w:lvl>
</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="7"/></w:num>"#;

    #[test]
    fn writes_styles_numbering_and_text() {
        let body = [
            p("Heading1", "Intro"),
            p("Heading2", "Scope"),
            p("Heading2", "Terms"),
            p("Heading1", "Reqs"),
            p("ReqID", "REQ1 : Capture"),
            r#"<w:p><w:r><w:t>Plain </w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>hidden</w:t></w:r><w:r><w:t>text</w:t></w:r></w:p>"#.to_owned(),
        ]
        .concat();
        let path = docx(&body, STYLES, Some(NUMBERING));
        let out = convert(&path, &ConvertOptions::default()).unwrap();
        assert_eq!(
            out,
            "heading 1\t1.\tIntro\nheading 2\t1.1 Scope\nheading 2\t1.2 Terms\nheading 1\t2.\tReqs\n\
             Requirement_ID\tREQ1 : Capture\nNormal\tPlain text\n"
        );
    }

    #[test]
    fn writes_tables_and_character_styles() {
        let body = format!(
            r#"<w:tbl><w:tr><w:tc>{}</w:tc><w:tc>{}{}</w:tc></w:tr></w:tbl><w:p><w:r><w:t>Max </w:t></w:r><w:r><w:rPr><w:rStyle w:val="Req"/></w:rPr><w:t>Response</w:t></w:r></w:p>"#,
            p("Normal", "A1"),
            p("Normal", "B1"),
            p("ReqID", "B1bis")
        );
        let path = docx(&body, STYLES, None);
        let options = ConvertOptions {
            params: vec![("withCharacterStyles".into(), "1".into())],
            ..Default::default()
        };
        let out = convert(&path, &options).unwrap();
        assert_eq!(
            out,
            "#TABLE\n#TR|1 Normal\tA1|2 Normal\tB1|2 Requirement_ID\tB1bis\nNormal\tMax Response\nRequirement\tResponse\n"
        );
    }

    #[test]
    fn refuses_legacy_doc_files() {
        let err = convert(Path::new("spec.doc"), &ConvertOptions::default()).unwrap_err();
        assert!(err.to_string().contains(".doc"));
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(format_number(4, "upperRoman"), "IV");
        assert_eq!(format_number(28, "lowerLetter"), "bb");
    }
}
