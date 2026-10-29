//! Tableurs (xlsx, xlsm, xls, ods, csv) → intermédiaire texte ou XML.
//!
//! Texte : `Worksheet : <nom>`, puis une ligne par rangée non vide, cellules
//! `|<référence> <texte>` séparées par une tabulation, et `End_of_Conversion`.
//! XML : `<Workbook><Worksheet name><Row index><Cell col ref>texte</Cell>`.
//! Format décrit dans la documentation du connecteur Excel (`docs/interop-formats.md` §5).

use crate::{xml_escape, ConvertError};
use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

/// Feuille : nom et rangées (numéro de rangée base 1, cellules (colonne base 1, texte)).
type Sheet = (String, Vec<(usize, Vec<(usize, String)>)>);

pub fn convert_text(path: &Path) -> Result<String, ConvertError> {
    let mut out = String::new();
    for (name, rows) in read(path)? {
        out.push_str(&format!("Worksheet : {name}\n"));
        for (row, cells) in rows {
            let line: Vec<String> = cells
                .iter()
                .map(|(col, text)| format!("|{}{} {}", column_letters(*col), row, text))
                .collect();
            out.push_str(&line.join("\t"));
            out.push('\n');
        }
    }
    out.push_str("End_of_Conversion\n");
    Ok(out)
}

pub fn convert_xml(path: &Path) -> Result<String, ConvertError> {
    let mut out = String::from("<Workbook>");
    for (name, rows) in read(path)? {
        out.push_str(&format!("<Worksheet name=\"{}\">", xml_escape(&name)));
        for (row, cells) in rows {
            out.push_str(&format!("<Row index=\"{row}\">"));
            for (col, text) in cells {
                out.push_str(&format!(
                    "<Cell col=\"{col}\" ref=\"{}{row}\">{}</Cell>",
                    column_letters(col),
                    xml_escape(&text)
                ));
            }
            out.push_str("</Row>");
        }
        out.push_str("</Worksheet>");
    }
    out.push_str("</Workbook>");
    Ok(out)
}

fn read(path: &Path) -> Result<Vec<Sheet>, ConvertError> {
    let is_csv = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        return read_csv(path);
    }
    let mut workbook = open_workbook_auto(path).map_err(|e| ConvertError::format(path, e))?;
    let mut sheets = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|e| ConvertError::format(path, e))?;
        let (row0, col0) = range.start().unwrap_or((0, 0));
        let mut rows = Vec::new();
        for (r, row) in range.rows().enumerate() {
            let cells: Vec<(usize, String)> = row
                .iter()
                .enumerate()
                .filter_map(|(c, cell)| {
                    let text = cell_text(cell);
                    (!text.is_empty()).then(|| (col0 as usize + c + 1, text))
                })
                .collect();
            if !cells.is_empty() {
                rows.push((row0 as usize + r + 1, cells));
            }
        }
        sheets.push((name, rows));
    }
    Ok(sheets)
}

fn read_csv(path: &Path) -> Result<Vec<Sheet>, ConvertError> {
    let text = crate::read_text(path)?;
    // Séparateur : `;` (Excel en français) ou `,`, selon la première ligne.
    let first = text.lines().next().unwrap_or_default();
    let delimiter = if first.matches(';').count() > first.matches(',').count() {
        b';'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for (r, record) in reader.records().enumerate() {
        let record = record.map_err(|e| ConvertError::format(path, e))?;
        let cells: Vec<(usize, String)> = record
            .iter()
            .enumerate()
            .filter_map(|(c, v)| {
                let text = single_line(v);
                (!text.is_empty()).then(|| (c + 1, text))
            })
            .collect();
        if !cells.is_empty() {
            rows.push((r + 1, cells));
        }
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(vec![(name, rows)])
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => single_line(&other.to_string()),
    }
}

/// Les retours à la ligne d'une cellule deviennent des espaces : une rangée
/// tient sur une ligne de l'intermédiaire.
fn single_line(text: &str) -> String {
    text.split(['\r', '\n'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn column_letters(mut col: usize) -> String {
    let mut s = Vec::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        s.push(b'A' + rem as u8);
        col = (col - 1) / 26;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_columns() {
        assert_eq!(column_letters(1), "A");
        assert_eq!(column_letters(26), "Z");
        assert_eq!(column_letters(28), "AB");
    }

    #[test]
    fn converts_csv_like_a_sheet() {
        let path = std::env::temp_dir().join(format!("rc-sheet-{}.csv", std::process::id()));
        std::fs::write(
            &path,
            "ID;Label\r\nREQ1;\"Line one\nline two\"\r\n;\r\nREQ2;Two\r\n",
        )
        .unwrap();
        let out = convert_text(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy();
        assert_eq!(
            out,
            format!("Worksheet : {name}\n|A1 ID\t|B1 Label\n|A2 REQ1\t|B2 Line one line two\n|A4 REQ2\t|B4 Two\nEnd_of_Conversion\n")
        );
        let xml = convert_xml(&path).unwrap();
        assert!(xml.contains(r#"<Row index="2"><Cell col="1" ref="A2">REQ1</Cell>"#));
        std::fs::remove_file(path).unwrap();
    }
}
