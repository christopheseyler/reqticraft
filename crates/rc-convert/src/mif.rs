//! FrameMaker MIF → lignes stylées : `<balise de paragraphe><TAB><numéro><texte>`
//! (format décrit par le connecteur FrameMaker, `docs/interop-formats.md` §5),
//! tableaux `#TABLE` / `#TR` placés à l'ancre du tableau.
//!
//! Les fichiers `.fm` et `.book` binaires nécessitent FrameMaker : les
//! enregistrer au format MIF.

use crate::lines::{Lines, Para};
use crate::{ConvertError, ConvertOptions};
use std::collections::HashMap;
use std::path::Path;

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    let text = crate::read_text(path)?;
    if !text.trim_start().starts_with("<MIFFile") {
        return Err(ConvertError::format(
            path,
            "fichier MIF attendu (<MIFFile …>) : enregistrer le document FrameMaker au format MIF",
        ));
    }
    Ok(convert_str(&text))
}

#[derive(Debug, Clone)]
enum Item {
    Str(String),
    Word(String),
    Node(Statement),
}

#[derive(Debug, Clone)]
struct Statement {
    name: String,
    items: Vec<Item>,
}

impl Statement {
    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Statement> + 'a {
        self.items.iter().filter_map(move |i| match i {
            Item::Node(s) if s.name == name => Some(s),
            _ => None,
        })
    }

    fn child(&self, name: &str) -> Option<&Statement> {
        self.items.iter().find_map(|i| match i {
            Item::Node(s) if s.name == name => Some(s),
            _ => None,
        })
    }

    /// Première valeur (chaîne ou mot) de l'instruction.
    fn value(&self) -> Option<&str> {
        self.items.iter().find_map(|i| match i {
            Item::Str(s) | Item::Word(s) => Some(s.as_str()),
            Item::Node(_) => None,
        })
    }

    fn descendants<'a>(&'a self, name: &'a str, out: &mut Vec<&'a Statement>) {
        for i in &self.items {
            if let Item::Node(s) = i {
                if s.name == name {
                    out.push(s);
                } else {
                    s.descendants(name, out);
                }
            }
        }
    }
}

fn parse(text: &str) -> Vec<Item> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut stack: Vec<Statement> = vec![Statement {
        name: String::new(),
        items: Vec::new(),
    }];
    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '<' => {
                i += 1;
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                stack.push(Statement {
                    name: chars[start..i].iter().collect(),
                    items: Vec::new(),
                });
                continue;
            }
            '>' => {
                if stack.len() > 1 {
                    let s = stack.pop().expect("pile non vide");
                    stack.last_mut().expect("racine").items.push(Item::Node(s));
                }
            }
            '`' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        match chars[i] {
                            't' => s.push('\t'),
                            '>' => s.push('>'),
                            'q' => s.push('\''),
                            'Q' => s.push('`'),
                            '\\' => s.push('\\'),
                            'x' => {
                                let hex: String = chars[i + 1..].iter().take(2).collect::<String>();
                                if let Some(ch) =
                                    u32::from_str_radix(&hex, 16).ok().and_then(frame_char)
                                {
                                    s.push(ch);
                                }
                                i += 2;
                                // Un espace termine la séquence.
                                if i + 1 < chars.len() && chars[i + 1] == ' ' {
                                    i += 1;
                                }
                            }
                            other => s.push(other),
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                stack.last_mut().expect("racine").items.push(Item::Str(s));
            }
            c if c.is_whitespace() => {}
            _ => {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '<' | '>')
                {
                    i += 1;
                }
                stack
                    .last_mut()
                    .expect("racine")
                    .items
                    .push(Item::Word(chars[start..i].iter().collect()));
                continue;
            }
        }
        i += 1;
    }
    while stack.len() > 1 {
        let s = stack.pop().expect("pile non vide");
        stack.last_mut().expect("racine").items.push(Item::Node(s));
    }
    stack.pop().map(|s| s.items).unwrap_or_default()
}

/// Caractères `\xNN` du jeu FrameMaker les plus courants (Latin-1 sinon).
fn frame_char(code: u32) -> Option<char> {
    Some(match code {
        0x04 => '\t',
        0x09 => '\t',
        0x10 | 0x11 => ' ',
        0x15 => '-',
        0xd0 => '–',
        0xd1 => '—',
        0xd2 => '“',
        0xd3 => '”',
        0xd4 => '‘',
        0xd5 => '’',
        0xa5 => '•',
        0x87 => 'á',
        0x88 => 'à',
        0x89 => 'â',
        0x8e => 'é',
        0x8f => 'è',
        0x90 => 'ê',
        0x91 => 'ë',
        0x94 => 'î',
        0x95 => 'ï',
        0x99 => 'ô',
        0x9d => 'ù',
        0x9e => 'û',
        0x8d => 'ç',
        0x83 => 'É',
        _ => char::from_u32(code)?,
    })
}

pub fn convert_str(text: &str) -> String {
    let root = Statement {
        name: String::new(),
        items: parse(text),
    };
    let mut tables: HashMap<String, &Statement> = HashMap::new();
    let mut found = Vec::new();
    root.descendants("Tbl", &mut found);
    for tbl in found {
        if let Some(id) = tbl.child("TblID").and_then(Statement::value) {
            tables.insert(id.to_owned(), tbl);
        }
    }
    let flows: Vec<&Statement> = root.children("TextFlow").collect();
    let tagged: Vec<&Statement> = flows
        .iter()
        .copied()
        .filter(|f| {
            f.child("TFTag")
                .and_then(Statement::value)
                .is_some_and(|t| !t.is_empty())
        })
        .collect();
    let flows = if tagged.is_empty() { flows } else { tagged };

    let mut lines = Lines::default();
    for flow in flows {
        for para in flow.children("Para") {
            let (style, text, anchors) = paragraph(para);
            lines.para(&style, &text);
            for id in anchors {
                if let Some(tbl) = tables.get(&id) {
                    lines.table(&table(tbl));
                }
            }
        }
    }
    lines.finish()
}

/// Style, numéro et texte d'un paragraphe ; tableaux ancrés.
fn paragraph(para: &Statement) -> (String, String, Vec<String>) {
    let pgf = para.child("Pgf");
    let style = para
        .child("PgfTag")
        .or_else(|| pgf.and_then(|p| p.child("PgfTag")))
        .and_then(Statement::value)
        .unwrap_or("Body")
        .to_owned();
    let number = para
        .child("PgfNumString")
        .or_else(|| pgf.and_then(|p| p.child("PgfNumString")))
        .and_then(Statement::value)
        .unwrap_or("");
    let mut text = number.to_owned();
    let mut anchors = Vec::new();
    for line in para.children("ParaLine") {
        for item in &line.items {
            let Item::Node(s) = item else { continue };
            match s.name.as_str() {
                "String" => text.push_str(s.value().unwrap_or("")),
                "Char" => match s.value().unwrap_or("") {
                    "Tab" => text.push('\t'),
                    "HardSpace" | "ThinSpace" | "EnSpace" | "EmSpace" | "NumberSpace" => {
                        text.push(' ')
                    }
                    "EmDash" => text.push('—'),
                    "EnDash" => text.push('–'),
                    "HardHyphen" => text.push('-'),
                    _ => {}
                },
                "ATbl" => {
                    if let Some(id) = s.value() {
                        anchors.push(id.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    (style, text, anchors)
}

fn table(tbl: &Statement) -> Vec<Vec<Vec<Para>>> {
    let mut rows = Vec::new();
    for part in ["TblH", "TblBody", "TblF"] {
        for section in tbl.children(part) {
            for row in section.children("Row") {
                let mut cells = Vec::new();
                for cell in row.children("Cell") {
                    let mut paras = Vec::new();
                    if let Some(content) = cell.child("CellContent") {
                        for para in content.children("Para") {
                            let (style, text, _) = paragraph(para);
                            paras.push((style, text));
                        }
                    }
                    cells.push(paras);
                }
                rows.push(cells);
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_paragraphs_numbers_and_tables() {
        let mif = r"<MIFFile 2015> # commentaire
<Tbls <Tbl <TblID 7> <TblBody <Row <Cell <CellContent <Para <PgfTag `CellBody'> <ParaLine <String `A1'>>>>> <Cell <CellContent <Para <PgfTag `CellBody'> <ParaLine <String `B1'>>>>>>>>>
<TextFlow <TFTag `A'>
 <Para <Pgf <PgfTag `Heading1'> <PgfNumString `1.\t'>> <ParaLine <String `Intro'>>>
 <Para <PgfTag `Requirement_ID'> <ParaLine <String `REQ1 : D\xd5 abord'> <Char Tab> <String `fin'>> <ParaLine <ATbl 7>>>
>
<TextFlow <Para <PgfTag `Footer'> <ParaLine <String `page'>>>>
";
        assert_eq!(
            convert_str(mif),
            "Heading1\t1. Intro\nRequirement_ID\tREQ1 : D’abord fin\n#TABLE\n#TR|1 CellBody\tA1|2 CellBody\tB1\n"
        );
    }
}
