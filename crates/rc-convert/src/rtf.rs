//! RTF → lignes stylées, comme pour Word : `<style><TAB><numéro><séparateur><texte>`
//! et tableaux `#TABLE` / `#TR|<col> <style><TAB><texte>` (format décrit par le
//! connecteur RTF, `docs/interop-formats.md` §5).
//!
//! Lecteur RTF minimal écrit pour ReqtiCraft : feuille de styles, paragraphes,
//! tableaux, numéros de liste (`\listtext`, `\pntext`), Unicode (`\u`, `\uc`),
//! octets d'une page de codes (`\'hh`, `\ansicpg`). Les images sont ignorées.

use crate::lines::{Lines, Para};
use crate::{ConvertError, ConvertOptions};
use std::collections::HashMap;
use std::path::Path;

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    let bytes = std::fs::read(path).map_err(|source| ConvertError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !bytes.starts_with(b"{\\rtf") {
        return Err(ConvertError::format(path, "en-tête {\\rtf absent"));
    }
    Ok(convert_bytes(&bytes, options.flag("withCharacterStyles")))
}

/// Destinations dont le contenu n'est pas du texte du document.
const SKIPPED: &[&str] = &[
    "fonttbl",
    "colortbl",
    "info",
    "pict",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "fldinst",
    "listtable",
    "listoverridetable",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "xmlnstbl",
    "rsidtbl",
    "generator",
    "pgdsctbl",
    "mmathPr",
    "revtbl",
    "filetbl",
    "bkmkstart",
    "bkmkend",
    "object",
    "shp",
    "shpinst",
    "nonshppict",
    "xe",
    "tc",
    "annotation",
    "atnid",
    "atnauthor",
    "comment",
    "footnote",
    "fldtype",
    "wgrffmtfilter",
    "panose",
    "falt",
    "userprops",
    "docvar",
];

#[derive(Clone)]
struct State {
    skip: bool,
    stylesheet: bool,
    uc: usize,
    char_style: Option<u32>,
}

#[derive(Default)]
struct Entry {
    para: bool,
    id: Option<u32>,
    name: String,
}

struct Reader {
    codepage: &'static encoding_rs::Encoding,
    styles: HashMap<u32, String>,
    char_styles: HashMap<u32, String>,
    with_char_styles: bool,
    // Paragraphe courant.
    style: u32,
    in_table: bool,
    text: String,
    bytes: Vec<u8>,
    runs: Vec<(String, String)>,
    // Tableau courant.
    cell: Vec<Para>,
    row: Vec<Vec<Para>>,
    rows: Vec<Vec<Vec<Para>>>,
    // Feuille de styles en cours de lecture.
    entry: Option<Entry>,
    lines: Lines,
    pending_skip: usize,
}

pub fn convert_bytes(data: &[u8], with_char_styles: bool) -> String {
    let mut r = Reader {
        codepage: encoding_rs::WINDOWS_1252,
        styles: HashMap::new(),
        char_styles: HashMap::new(),
        with_char_styles,
        style: 0,
        in_table: false,
        text: String::new(),
        bytes: Vec::new(),
        runs: Vec::new(),
        cell: Vec::new(),
        row: Vec::new(),
        rows: Vec::new(),
        entry: None,
        lines: Lines::default(),
        pending_skip: 0,
    };
    let mut stack: Vec<State> = Vec::new();
    let mut state = State {
        skip: false,
        stylesheet: false,
        uc: 1,
        char_style: None,
    };
    // Une destination `\*` inconnue est ignorée avec tout son groupe.
    let mut star = false;
    let mut first_in_group = false;
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        match c {
            b'{' => {
                r.flush_bytes(&state);
                stack.push(state.clone());
                first_in_group = true;
                star = false;
                i += 1;
                continue;
            }
            b'}' => {
                r.flush_bytes(&state);
                if state.stylesheet && !stack.last().is_some_and(|s| s.stylesheet) {
                    // Fin de la feuille de styles.
                } else if state.stylesheet {
                    r.end_entry();
                }
                state = stack.pop().unwrap_or(state);
                first_in_group = false;
                i += 1;
                continue;
            }
            b'\\' => {
                let (word, param, next) = control(data, i + 1);
                i = next;
                if word.is_empty() {
                    // Symbole de contrôle.
                    let sym = data.get(i - 1).copied().unwrap_or(b' ');
                    match sym {
                        b'\'' => {
                            if let Some(h) = data.get(i..i + 2) {
                                if let Ok(b) =
                                    u8::from_str_radix(std::str::from_utf8(h).unwrap_or(""), 16)
                                {
                                    if r.pending_skip > 0 {
                                        r.pending_skip -= 1;
                                    } else {
                                        r.bytes.push(b);
                                    }
                                }
                                i += 2;
                            }
                        }
                        b'*' => {
                            star = true;
                            continue;
                        }
                        b'~' => r.push_char(&state, '\u{a0}'),
                        b'-' => {}
                        b'_' => r.push_char(&state, '-'),
                        b'\n' | b'\r' => r.end_paragraph(),
                        b'\\' | b'{' | b'}' => r.push_char(&state, sym as char),
                        _ => {}
                    }
                    first_in_group = false;
                    continue;
                }
                let destination = first_in_group;
                first_in_group = false;
                r.flush_bytes(&state);
                if word == "stylesheet" {
                    state.stylesheet = true;
                    star = false;
                    continue;
                }
                if state.stylesheet {
                    r.style_word(&word, param);
                    star = false;
                    continue;
                }
                if destination && (star || SKIPPED.contains(&word.as_str())) {
                    state.skip = true;
                    star = false;
                    continue;
                }
                star = false;
                if state.skip {
                    continue;
                }
                r.word(&mut state, &word, param);
            }
            b'\r' | b'\n' => {
                i += 1;
                continue;
            }
            _ => {
                first_in_group = false;
                if state.stylesheet {
                    if let Some(e) = &mut r.entry {
                        if c == b';' {
                            r.end_entry();
                        } else {
                            e.name.push(c as char);
                        }
                    }
                } else if !state.skip {
                    if r.pending_skip > 0 {
                        r.pending_skip -= 1;
                    } else {
                        r.bytes.push(c);
                    }
                }
                i += 1;
                continue;
            }
        }
    }
    r.flush_bytes(&state);
    r.end_paragraph();
    r.flush_table();
    r.lines.finish()
}

/// Lit un mot de contrôle : lettres, paramètre numérique signé, espace délimiteur.
fn control(data: &[u8], mut i: usize) -> (String, Option<i32>, usize) {
    let start = i;
    while i < data.len() && data[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == start {
        return (String::new(), None, i + 1);
    }
    let word = String::from_utf8_lossy(&data[start..i]).into_owned();
    let num_start = i;
    if i < data.len() && data[i] == b'-' {
        i += 1;
    }
    while i < data.len() && data[i].is_ascii_digit() {
        i += 1;
    }
    let param = std::str::from_utf8(&data[num_start..i])
        .ok()
        .and_then(|s| s.parse().ok());
    if i < data.len() && data[i] == b' ' {
        i += 1;
    }
    (word, param, i)
}

impl Reader {
    fn word(&mut self, state: &mut State, word: &str, param: Option<i32>) {
        let n = param.unwrap_or(0);
        match word {
            "ansicpg" => {
                if let Some(enc) = codepage(n) {
                    self.codepage = enc;
                }
            }
            "par" | "sect" | "page" => self.end_paragraph(),
            "pard" => {
                self.style = 0;
                self.in_table = false;
            }
            "s" => self.style = n.max(0) as u32,
            "intbl" => self.in_table = true,
            "cell" => {
                self.end_paragraph();
                let cell = std::mem::take(&mut self.cell);
                self.row.push(cell);
            }
            "row" => {
                if !self.cell.is_empty() {
                    let cell = std::mem::take(&mut self.cell);
                    self.row.push(cell);
                }
                let row = std::mem::take(&mut self.row);
                self.rows.push(row);
            }
            "cs" => state.char_style = Some(n.max(0) as u32),
            "plain" => state.char_style = None,
            "uc" => state.uc = n.max(0) as usize,
            "u" => {
                let code = if n < 0 { n + 65536 } else { n } as u32;
                if let Some(ch) = char::from_u32(code) {
                    self.push_char(state, ch);
                }
                self.pending_skip = state.uc;
            }
            "tab" => self.push_char(state, '\t'),
            "line" => self.push_char(state, ' '),
            "emdash" => self.push_char(state, '—'),
            "endash" => self.push_char(state, '–'),
            "bullet" => self.push_char(state, '•'),
            "lquote" => self.push_char(state, '‘'),
            "rquote" => self.push_char(state, '’'),
            "ldblquote" => self.push_char(state, '“'),
            "rdblquote" => self.push_char(state, '”'),
            "emspace" | "enspace" | "qmspace" => self.push_char(state, ' '),
            _ => {}
        }
    }

    fn style_word(&mut self, word: &str, param: Option<i32>) {
        let entry = self.entry.get_or_insert_with(Entry::default);
        match word {
            "s" => {
                entry.para = true;
                entry.id = param.map(|n| n.max(0) as u32);
            }
            "cs" => {
                entry.para = false;
                entry.id = param.map(|n| n.max(0) as u32);
            }
            "ds" | "ts" => {
                entry.para = false;
                entry.id = None;
            }
            _ => {}
        }
    }

    fn end_entry(&mut self) {
        let Some(e) = self.entry.take() else {
            return;
        };
        let name = e.name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match e.id {
            Some(id) if e.para => {
                self.styles.insert(id, name);
            }
            None if e.para || !self.styles.contains_key(&0) && e.name.contains("Normal") => {
                self.styles.insert(0, name);
            }
            Some(id) => {
                self.char_styles.insert(id, name);
            }
            None => {}
        }
    }

    fn push_char(&mut self, state: &State, c: char) {
        self.flush_bytes(state);
        self.push_text(state, &c.to_string());
    }

    fn push_text(&mut self, state: &State, s: &str) {
        self.text.push_str(s);
        if self.with_char_styles {
            if let Some(name) = state.char_style.and_then(|id| self.char_styles.get(&id)) {
                match self.runs.last_mut() {
                    Some((n, t)) if n == name => t.push_str(s),
                    _ => self.runs.push((name.clone(), s.to_owned())),
                }
            }
        }
    }

    fn flush_bytes(&mut self, state: &State) {
        if self.bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.bytes);
        let (text, _, _) = self.codepage.decode(&bytes);
        let text = text.into_owned();
        self.push_text(state, &text);
    }

    fn end_paragraph(&mut self) {
        let text = std::mem::take(&mut self.text);
        let runs = std::mem::take(&mut self.runs);
        let style = self
            .styles
            .get(&self.style)
            .cloned()
            .unwrap_or_else(|| "Normal".into());
        if self.in_table {
            if !text.trim().is_empty() {
                self.cell.push((style, text));
            }
            return;
        }
        self.flush_table();
        self.lines.para(&style, &text);
        for (name, t) in runs {
            self.lines.para(&name, &t);
        }
    }

    fn flush_table(&mut self) {
        if !self.cell.is_empty() {
            let cell = std::mem::take(&mut self.cell);
            self.row.push(cell);
        }
        if !self.row.is_empty() {
            let row = std::mem::take(&mut self.row);
            self.rows.push(row);
        }
        let rows = std::mem::take(&mut self.rows);
        self.lines.table(&rows);
    }
}

fn codepage(n: i32) -> Option<&'static encoding_rs::Encoding> {
    let label = match n {
        932 => "shift_jis".to_owned(),
        936 => "gbk".to_owned(),
        949 => "euc-kr".to_owned(),
        950 => "big5".to_owned(),
        65001 => "utf-8".to_owned(),
        874 | 1250..=1258 => format!("windows-{n}"),
        _ => return None,
    };
    encoding_rs::Encoding::for_label(label.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Arial;}}{\colortbl;\red0\green0\blue0;}
{\stylesheet{\s0 Normal;}{\s1\sbasedon0 heading 1;}{\s2 Requirement_ID;}{\*\cs10 Default Paragraph Font;}{\*\cs11 Covers;}}
{\info{\title secret}}
\pard\s1{\listtext 1\tab}Introduction\par
\pard\s2 REQ1 : Le syst\'e8me d\u233?marre\par
\pard\s0 Texte {\cs11 [Covers: A1]} fin\line suite\par
\trowd\cellx1000\cellx2000\pard\intbl\s0 A1\cell B1\cell\row
\pard\s0 Apr\'e8s{\*\bkmkstart x}{\*\bkmkend x}\par
{\pict\pngblip 89504e47}\par
}";

    #[test]
    fn converts_styles_tables_and_encodings() {
        assert_eq!(
            convert_bytes(SAMPLE.as_bytes(), false),
            "heading 1\t1 Introduction\n\
             Requirement_ID\tREQ1 : Le système démarre\n\
             Normal\tTexte [Covers: A1] fin suite\n\
             #TABLE\n#TR|1 Normal\tA1|2 Normal\tB1\n\
             Normal\tAprès\n"
        );
    }

    #[test]
    fn writes_character_styles_on_demand() {
        let out = convert_bytes(SAMPLE.as_bytes(), true);
        assert!(out.contains("Normal\tTexte [Covers: A1] fin suite\nCovers\t[Covers: A1]\n"));
    }
}
