//! Lecture des fichiers INI de l'ancien format (`.rqtf`, `.types`, `.filters`).
//!
//! Sections dans l'ordre du fichier ; clés dans l'ordre, doublons conservés.
//! Commentaires : lignes commençant par `;`. Valeur : tout ce qui suit le premier `=`.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ini {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        for raw in text.lines() {
            let line = raw.trim_start_matches('\u{feff}').trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                ini.sections.push(Section {
                    name: trimmed[1..trimmed.len() - 1].to_owned(),
                    entries: Vec::new(),
                });
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if ini.sections.is_empty() {
                    ini.sections.push(Section::default());
                }
                let section = ini.sections.last_mut().expect("section créée ci-dessus");
                section
                    .entries
                    .push((key.trim().to_owned(), value.to_owned()));
            }
        }
        ini
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Valeur non vide.
    pub fn get_nonempty(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|v| !v.trim().is_empty())
    }

    pub fn flag(&self, key: &str) -> bool {
        matches!(
            self.get(key).map(str::trim),
            Some("1" | "TRUE" | "true" | "True")
        )
    }

    /// Entrées numérotées `<prefix><n><suffix>` dans l'ordre croissant de `n`.
    pub fn numbered<'a>(&'a self, prefix: &str, suffix: &str) -> Vec<(u32, &'a str)> {
        let mut out: Vec<(u32, &str)> = self
            .entries
            .iter()
            .filter_map(|(k, v)| {
                let middle = k.strip_prefix(prefix)?.strip_suffix(suffix)?;
                let n = middle.parse::<u32>().ok()?;
                Some((n, v.as_str()))
            })
            .collect();
        out.sort_by_key(|(n, _)| *n);
        out
    }
}

/// Découpe une liste séparée par des virgules.
pub fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_keys_and_numbered_entries() {
        let ini = Ini::parse(
            "[Files]\r\nNames=A,B\r\n; commentaire\r\n\r\n[A]\r\nCover2=C\r\nCover1=B\r\nPath=a=b.txt\r\n",
        );
        assert_eq!(ini.sections.len(), 2);
        let a = ini.section("A").unwrap();
        assert_eq!(a.get("Path"), Some("a=b.txt"));
        assert_eq!(a.numbered("Cover", ""), vec![(1, "B"), (2, "C")]);
        assert_eq!(
            split_list(ini.section("Files").unwrap().get("Names").unwrap()),
            ["A", "B"]
        );
    }

    #[test]
    fn numbered_ignores_other_suffixes() {
        let ini = Ini::parse("[A]\nCover1=B\nCover1Position=1@2\n");
        assert_eq!(
            ini.section("A").unwrap().numbered("Cover", ""),
            vec![(1, "B")]
        );
    }
}
