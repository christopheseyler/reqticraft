//! Intermédiaire « lignes stylées », commun aux traitements de texte :
//! `<style><TAB><texte>` par paragraphe, `#TABLE` puis une ligne `#TR` par rangée
//! avec `|<colonne> <style><TAB><texte>` par paragraphe de cellule
//! (`docs/interop-formats.md` §5).

/// Paragraphe d'une cellule ou du corps : style et texte.
pub type Para = (String, String);

#[derive(Default)]
pub struct Lines {
    out: String,
}

impl Lines {
    /// Ajoute un paragraphe ; les paragraphes vides sont omis.
    pub fn para(&mut self, style: &str, text: &str) {
        let text = clean(text);
        if text.trim().is_empty() {
            return;
        }
        self.out.push_str(&clean(style));
        self.out.push('\t');
        self.out.push_str(text.trim());
        self.out.push('\n');
    }

    /// Ligne brute (marqueurs propres à un format : `SLIDE 1`, `<Picture<…>`…).
    pub fn raw(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }

    /// Tableau : rangées, cellules, paragraphes de chaque cellule.
    pub fn table(&mut self, rows: &[Vec<Vec<Para>>]) {
        if rows.is_empty() {
            return;
        }
        self.out.push_str("#TABLE\n");
        for row in rows {
            self.out.push_str("#TR");
            for (col, cell) in row.iter().enumerate() {
                for (style, text) in cell {
                    let text = clean(text);
                    if text.trim().is_empty() {
                        continue;
                    }
                    self.out
                        .push_str(&format!("|{} {}\t{}", col + 1, clean(style), text.trim()));
                }
            }
            self.out.push('\n');
        }
    }

    pub fn finish(self) -> String {
        self.out
    }
}

/// Une ligne par paragraphe : sauts de ligne et tabulations deviennent des
/// espaces, comme les espaces insécables (voir le convertisseur Word).
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' | '\u{a0}' | '\u{202f}' | '\u{2028}' | '\u{2029}' => ' ',
            c => c,
        })
        .filter(|c| !c.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_paragraphs_and_tables() {
        let mut l = Lines::default();
        l.para("Heading 1", "1\u{a0}Intro\n");
        l.para("Normal", "  ");
        l.table(&[vec![
            vec![("P".into(), "a\tb".into())],
            vec![("P".into(), "c".into()), ("P".into(), "d".into())],
        ]]);
        assert_eq!(
            l.finish(),
            "Heading 1\t1 Intro\n#TABLE\n#TR|1 P\ta b|2 P\tc|2 P\td\n"
        );
    }
}
