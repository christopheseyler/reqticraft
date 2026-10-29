//! Markdown (CommonMark et tableaux GFM) → lignes stylées, avec les mêmes
//! styles que le HTML rendu : `H1`…`H6`, `P`, `LI`, `PRE`, tableaux `#TABLE`
//! / `#TR|<col> TH|TD`, images `<Picture<source>`. Les marques d'emphase, de
//! code et de lien sont retirées du texte ; `[Covers: X]` est conservé.

use crate::lines::Lines;
use crate::{ConvertError, ConvertOptions};
use std::path::Path;

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    let text = crate::read_text(path)?;
    Ok(convert_str(&text))
}

pub fn convert_str(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Lines::default();
    let mut para: Vec<String> = Vec::new();
    let mut item: Option<String> = None;
    let mut i = 0;

    fn flush(out: &mut Lines, para: &mut Vec<String>, item: &mut Option<String>) {
        if let Some(li) = item.take() {
            emit(out, "LI", &li);
        }
        if !para.is_empty() {
            emit(out, "P", &para.join(" "));
            para.clear();
        }
    }

    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Bloc de code délimité.
        if let Some(fence) = fence_of(trimmed) {
            flush(&mut out, &mut para, &mut item);
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(fence) {
                out.para("PRE", lines[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut out, &mut para, &mut item);
            i += 1;
            continue;
        }
        // Titre ATX.
        if let Some((level, title)) = atx(trimmed) {
            flush(&mut out, &mut para, &mut item);
            emit(&mut out, &format!("H{level}"), title);
            i += 1;
            continue;
        }
        // Titre Setext : le paragraphe en cours devient un titre.
        if !para.is_empty() && item.is_none() && is_underline(trimmed) {
            let level = if trimmed.starts_with('=') { 1 } else { 2 };
            let title = para.join(" ");
            para.clear();
            emit(&mut out, &format!("H{level}"), &title);
            i += 1;
            continue;
        }
        if is_rule(trimmed) {
            flush(&mut out, &mut para, &mut item);
            i += 1;
            continue;
        }
        // Tableau : ligne d'en-tête suivie d'une ligne de séparation.
        if trimmed.contains('|') && i + 1 < lines.len() && is_delimiter(lines[i + 1].trim()) {
            flush(&mut out, &mut para, &mut item);
            let mut rows = vec![cells(trimmed, "TH")];
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                rows.push(cells(lines[i].trim(), "TD"));
                i += 1;
            }
            let rows: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    r.into_iter()
                        .map(|(style, text)| {
                            let text = inline(&text, &mut Vec::new());
                            vec![(style, text)]
                        })
                        .collect()
                })
                .collect();
            out.table(&rows);
            continue;
        }
        // Élément de liste.
        if let Some(rest) = list_item(trimmed) {
            flush(&mut out, &mut para, &mut item);
            item = Some(rest.to_owned());
            i += 1;
            continue;
        }
        // Code indenté (hors liste et paragraphe).
        if indent >= 4 && para.is_empty() && item.is_none() {
            out.para("PRE", &line[4.min(line.len())..]);
            i += 1;
            continue;
        }
        // Citation : le contenu est traité comme du texte courant.
        let content = trimmed.trim_start_matches('>').trim_start();
        match &mut item {
            Some(li) if indent > 0 => {
                li.push(' ');
                li.push_str(content);
            }
            Some(_) => {
                flush(&mut out, &mut para, &mut item);
                para.push(content.to_owned());
            }
            None => para.push(content.to_owned()),
        }
        i += 1;
    }
    flush(&mut out, &mut para, &mut item);
    out.finish()
}

fn emit(out: &mut Lines, style: &str, text: &str) {
    let mut pictures = Vec::new();
    let text = inline(text, &mut pictures);
    out.para(style, &text);
    for p in pictures {
        out.raw(&format!("<Picture<{p}>"));
    }
}

fn fence_of(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn atx(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let title = rest.trim();
    let title = title.trim_end_matches('#').trim_end();
    Some((level, title))
}

fn is_underline(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && (line.chars().all(|c| c == '=') || line.chars().all(|c| c == '-'))
}

fn is_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && ["-", "*", "_"]
            .iter()
            .any(|m| compact.chars().all(|c| c.to_string() == *m))
}

fn is_delimiter(line: &str) -> bool {
    let line = line.trim().trim_matches('|');
    !line.is_empty()
        && line.split('|').all(|cell| {
            let c = cell.trim().trim_matches(':');
            !c.is_empty() && c.chars().all(|ch| ch == '-')
        })
}

fn cells(line: &str, style: &str) -> Vec<(String, String)> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    line.split('|')
        .map(|c| (style.to_owned(), c.trim().to_owned()))
        .collect()
}

fn list_item(line: &str) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        // Case à cocher GFM.
        let rest = rest
            .strip_prefix("[ ] ")
            .or_else(|| rest.strip_prefix("[x] "))
            .unwrap_or(rest);
        return Some(rest);
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 9 {
        let rest = &line[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(r);
        }
    }
    None
}

/// Texte d'une ligne Markdown : images mises de côté, liens réduits à leur
/// texte, code et emphase sans leurs marques, échappements retirés.
fn inline(text: &str, pictures: &mut Vec<String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '`' => {
                let ticks = chars[i..].iter().take_while(|&&c| c == '`').count();
                let close = (i + ticks..chars.len())
                    .find(|&j| chars[j..].iter().take_while(|&&c| c == '`').count() == ticks);
                if let Some(j) = close {
                    out.extend(&chars[i + ticks..j]);
                    i = j + ticks;
                } else {
                    out.extend(&chars[i..i + ticks]);
                    i += ticks;
                }
                continue;
            }
            '!' | '[' => {
                let image = c == '!';
                let open = if image { i + 1 } else { i };
                if chars.get(open) == Some(&'[') {
                    if let Some((label, url, end)) = link(&chars, open) {
                        if image {
                            pictures.push(url);
                        } else {
                            out.push_str(&inline(&label, pictures));
                        }
                        i = end;
                        continue;
                    }
                }
            }
            _ => {}
        }
        out.push(c);
        i += 1;
    }
    strip_emphasis(&out)
}

/// `[texte](url)` à partir de `[` ; renvoie le texte, l'URL et la position suivante.
fn link(chars: &[char], open: usize) -> Option<(String, String, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (j, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = (close + 2..chars.len()).find(|&j| chars[j] == ')')?;
    let label: String = chars[open + 1..close].iter().collect();
    let target: String = chars[close + 2..end].iter().collect();
    let url = target.split_whitespace().next().unwrap_or("").to_owned();
    Some((label, url, end + 1))
}

/// Retire `**`, `__`, `~~` et les `*`/`_` d'emphase ; un `_` à l'intérieur
/// d'un mot (`REQ_1`) est conservé.
fn strip_emphasis(text: &str) -> String {
    let text = text.replace("**", "").replace("~~", "");
    let chars: Vec<char> = text.chars().collect();
    let mut remove = vec![false; chars.len()];
    for marker in ['*', '_'] {
        let mut i = 0;
        while i < chars.len() {
            let opens = chars[i] == marker
                && !remove[i]
                && chars.get(i + 1).is_some_and(|c| !c.is_whitespace())
                && (i == 0 || !chars[i - 1].is_alphanumeric());
            if opens {
                let close = (i + 2..chars.len()).find(|&j| {
                    chars[j] == marker
                        && !chars[j - 1].is_whitespace()
                        && chars.get(j + 1).is_none_or(|c| !c.is_alphanumeric())
                });
                if let Some(j) = close {
                    remove[i] = true;
                    remove[j] = true;
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
    chars
        .iter()
        .zip(remove)
        .filter(|(_, r)| !r)
        .map(|(c, _)| c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_markdown_blocks() {
        let md = "# Spécification\n\nREQ_1 : Le système **doit** démarrer.\n[Covers: A1]\n\n\
                  Sous-titre\n----------\n\n- premier\n  suite\n- *second*\n\n\
                  | Id | Texte |\n|---|:---:|\n| R1 | `a` |\n\n```\n# pas un titre\n```\n\n\
                  ![schéma](img/x.png) voir [la norme](http://x)\n";
        assert_eq!(
            convert_str(md),
            "H1\tSpécification\n\
             P\tREQ_1 : Le système doit démarrer. [Covers: A1]\n\
             H2\tSous-titre\n\
             LI\tpremier suite\n\
             LI\tsecond\n\
             #TABLE\n#TR|1 TH\tId|2 TH\tTexte\n#TR|1 TD\tR1|2 TD\ta\n\
             PRE\t# pas un titre\n\
             P\tvoir la norme\n\
             <Picture<img/x.png>\n"
        );
    }

    #[test]
    fn keeps_underscores_inside_words() {
        assert_eq!(strip_emphasis("REQ_1 et _accent_"), "REQ_1 et accent");
    }
}
