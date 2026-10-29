//! Rapports : matrice de traçabilité, résultats d'analyse, description du projet,
//! contrôle des règles. Sorties HTML, Markdown, CSV, XLSX et DOCX.
//!
//! Chaque modèle de rapport produit un [`Report`] neutre (sections, paragraphes,
//! tableaux) ; chaque format sait rendre un `Report`. Les modèles sont écrits en
//! code plutôt qu'en gabarits : le contenu reprend les rapports décrits dans
//! `docs/functional-spec.md` §3.13, sans reprendre de gabarit existant.

mod docx;

use rc_analysis::{Analysis, Target};
use rc_model::Severity;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("XLSX : {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("DOCX : {0}")]
    Docx(String),
    #[error("{0}")]
    Other(String),
}

/// Modèles de rapport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    Matrix,
    Analysis,
    Description,
    Rules,
}

impl Template {
    pub const ALL: [Template; 4] = [
        Template::Matrix,
        Template::Analysis,
        Template::Description,
        Template::Rules,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Template::Matrix => "matrix",
            Template::Analysis => "analysis",
            Template::Description => "description",
            Template::Rules => "rules",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Template::Matrix => "Matrice de traçabilité",
            Template::Analysis => "Résultats d'analyse",
            Template::Description => "Description du projet",
            Template::Rules => "Contrôle des règles",
        }
    }

    pub fn parse(s: &str) -> Option<Template> {
        Template::ALL.into_iter().find(|t| t.id() == s)
    }
}

/// Formats de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Markdown,
    Csv,
    Xlsx,
    Docx,
}

impl Format {
    pub const ALL: [Format; 5] = [
        Format::Html,
        Format::Xlsx,
        Format::Docx,
        Format::Csv,
        Format::Markdown,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Html => "html",
            Format::Markdown => "md",
            Format::Csv => "csv",
            Format::Xlsx => "xlsx",
            Format::Docx => "docx",
        }
    }

    pub fn parse(s: &str) -> Option<Format> {
        let s = s.trim_start_matches('.').to_lowercase();
        match s.as_str() {
            "htm" => Some(Format::Html),
            "markdown" => Some(Format::Markdown),
            _ => Format::ALL.into_iter().find(|f| f.extension() == s),
        }
    }
}

/// Description d'un document du projet, pour les rapports.
#[derive(Debug, Clone, Default)]
pub struct DocumentInfo {
    pub name: String,
    pub type_name: String,
    pub path: String,
    pub category: Option<String>,
}

/// Données d'entrée d'un rapport.
pub struct Input<'a> {
    pub project: &'a str,
    pub path: &'a str,
    /// Une entrée par document de l'analyse, dans le même ordre.
    pub documents: Vec<DocumentInfo>,
    pub analysis: &'a Analysis,
    /// Date de génération, affichée en tête.
    pub generated: &'a str,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub heading: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub title: String,
    pub subtitle: String,
    pub sections: Vec<Section>,
    /// Tableau principal, pour les formats à un seul tableau (CSV).
    pub flat: Table,
}

fn percent(covered: usize, total: usize) -> String {
    if total == 0 {
        "—".into()
    } else {
        format!(
            "{} %",
            (covered as f64 * 100.0 / total as f64).round() as i64
        )
    }
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "Erreur",
        Severity::Warning => "Avertissement",
        Severity::Information => "Information",
    }
}

/// Construit le rapport d'un modèle.
pub fn build(input: &Input, template: Template) -> Report {
    let a = input.analysis;
    let mut report = Report {
        title: template.label().into(),
        subtitle: format!("Projet {} — {}", input.project, input.generated),
        ..Default::default()
    };
    match template {
        Template::Matrix => matrix(input, &mut report),
        Template::Analysis => analysis(input, &mut report),
        Template::Description => description(input, &mut report),
        Template::Rules => {
            let table = Table {
                title: "Messages".into(),
                columns: [
                    "Sévérité",
                    "Règle",
                    "Document",
                    "Élément",
                    "Ligne",
                    "Message",
                ]
                .map(String::from)
                .to_vec(),
                rows: a
                    .messages
                    .iter()
                    .map(|m| {
                        let doc = m
                            .document
                            .as_deref()
                            .and_then(|n| a.documents.iter().position(|d| d.name == n));
                        vec![
                            severity(m.severity).into(),
                            m.rule.clone(),
                            m.document.clone().unwrap_or_else(|| "projet".into()),
                            doc.zip(m.element)
                                .map(|(d, e)| {
                                    a.element_name(Target {
                                        document: d,
                                        element: Some(e),
                                    })
                                })
                                .unwrap_or_default(),
                            m.line.map(|l| l.to_string()).unwrap_or_default(),
                            m.text.clone(),
                        ]
                    })
                    .collect(),
            };
            let mut by_rule: Vec<(String, usize)> = Vec::new();
            for m in &a.messages {
                match by_rule.iter_mut().find(|(r, _)| *r == m.rule) {
                    Some((_, n)) => *n += 1,
                    None => by_rule.push((m.rule.clone(), 1)),
                }
            }
            report.sections.push(Section {
                heading: "Synthèse".into(),
                paragraphs: vec![format!(
                    "{} erreur(s), {} avertissement(s), {} information(s).",
                    a.count(Severity::Error),
                    a.count(Severity::Warning),
                    a.count(Severity::Information)
                )],
                tables: vec![Table {
                    title: "Messages par règle".into(),
                    columns: vec!["Règle".into(), "Nombre".into()],
                    rows: by_rule
                        .into_iter()
                        .map(|(r, n)| vec![r, n.to_string()])
                        .collect(),
                }],
            });
            report.sections.push(Section {
                heading: "Détail".into(),
                paragraphs: Vec::new(),
                tables: vec![table.clone()],
            });
            report.flat = table;
        }
    }
    report
}

fn matrix(input: &Input, report: &mut Report) {
    let a = input.analysis;
    let mut covering: HashMap<Target, Vec<(usize, Target)>> = HashMap::new();
    for l in &a.links {
        covering.entry(l.to).or_default().push((l.channel, l.from));
    }
    let mut flat = Table {
        title: "Matrice".into(),
        columns: [
            "Document couvert",
            "Exigence",
            "État",
            "Document couvrant",
            "Couverte par",
        ]
        .map(String::from)
        .to_vec(),
        rows: Vec::new(),
    };
    for (c, ch) in a.channels.iter().enumerate() {
        let covered_doc = &a.documents[ch.covered];
        let names: Vec<&str> = ch
            .covering
            .iter()
            .map(|&k| a.documents[k].name.as_str())
            .collect();
        let mut table = Table {
            title: format!("{} → {}", names.join(", "), covered_doc.name),
            columns: ["Exigence", "État", "Couverte par"]
                .map(String::from)
                .to_vec(),
            rows: Vec::new(),
        };
        for e in rc_analysis::leaf_requirements(covered_doc) {
            let t = Target {
                document: ch.covered,
                element: Some(e),
            };
            // Une exigence contenue dans une macro-exigence couverte l'est aussi.
            let mut by: Vec<String> = covered_doc
                .ancestors(e)
                .chain(std::iter::once(e))
                .flat_map(|x| {
                    covering
                        .get(&Target {
                            document: ch.covered,
                            element: Some(x),
                        })
                        .into_iter()
                        .flatten()
                        .filter(|(lc, _)| *lc == c)
                        .map(|(_, from)| a.element_name(*from))
                })
                .collect();
            by.dedup();
            let state = if by.is_empty() {
                "Non couverte"
            } else {
                "Couverte"
            };
            table
                .rows
                .push(vec![a.element_name(t), state.into(), by.join(", ")]);
            flat.rows.push(vec![
                covered_doc.name.clone(),
                a.element_name(t),
                state.into(),
                names.join(", "),
                by.join(", "),
            ]);
        }
        report.sections.push(Section {
            heading: table.title.clone(),
            paragraphs: vec![format!(
                "Couverture : {} sur {} ({}).",
                ch.covered_count,
                ch.total,
                percent(ch.covered_count, ch.total)
            )],
            tables: vec![table],
        });
    }
    if a.channels.is_empty() {
        report.sections.push(Section {
            heading: "Aucun canal".into(),
            paragraphs: vec!["Le projet ne déclare aucun canal de couverture.".into()],
            tables: Vec::new(),
        });
    }
    report.flat = flat;
}

fn analysis(input: &Input, report: &mut Report) {
    let a = input.analysis;
    let docs = Table {
        title: "Documents".into(),
        columns: [
            "Document",
            "Type",
            "Exigences",
            "Couvertes",
            "Couverture",
            "Dérivées",
            "Erreurs",
            "Avertissements",
        ]
        .map(String::from)
        .to_vec(),
        rows: a
            .documents
            .iter()
            .zip(&a.summaries)
            .enumerate()
            .map(|(i, (d, s))| {
                vec![
                    d.name.clone(),
                    input
                        .documents
                        .get(i)
                        .map(|x| x.type_name.clone())
                        .unwrap_or_else(|| d.type_name.clone()),
                    s.requirements.to_string(),
                    s.covered.map(|c| c.to_string()).unwrap_or_default(),
                    s.covered
                        .map(|c| percent(c, s.requirements))
                        .unwrap_or_else(|| "—".into()),
                    s.derived.to_string(),
                    s.errors.to_string(),
                    s.warnings.to_string(),
                ]
            })
            .collect(),
    };
    let (covered, total) = a
        .summaries
        .iter()
        .filter_map(|s| s.covered.map(|c| (c, s.requirements)))
        .fold((0, 0), |(x, y), (c, t)| (x + c, y + t));
    let list = |set: &std::collections::BTreeSet<(usize, usize)>, title: &str| Table {
        title: title.into(),
        columns: vec!["Document".into(), "Élément".into()],
        rows: set
            .iter()
            .map(|&(d, e)| {
                vec![
                    a.documents[d].name.clone(),
                    a.element_name(Target {
                        document: d,
                        element: Some(e),
                    }),
                ]
            })
            .collect(),
    };
    report.sections.push(Section {
        heading: "Synthèse".into(),
        paragraphs: vec![format!(
            "{} document(s), couverture globale {} ({} sur {}), {} exigence(s) dérivée(s), {} erreur(s).",
            a.documents.len(),
            percent(covered, total),
            covered,
            total,
            a.derived.len(),
            a.count(Severity::Error)
        )],
        tables: vec![docs.clone()],
    });
    report.sections.push(Section {
        heading: "Exigences non couvertes".into(),
        paragraphs: if a.uncovered.is_empty() {
            vec!["Aucune.".into()]
        } else {
            Vec::new()
        },
        tables: vec![list(&a.uncovered, "Non couvertes")]
            .into_iter()
            .filter(|t| !t.rows.is_empty())
            .collect(),
    });
    report.sections.push(Section {
        heading: "Exigences dérivées".into(),
        paragraphs: if a.derived.is_empty() {
            vec!["Aucune.".into()]
        } else {
            Vec::new()
        },
        tables: vec![list(&a.derived, "Dérivées")]
            .into_iter()
            .filter(|t| !t.rows.is_empty())
            .collect(),
    });
    report.flat = docs;
}

fn description(input: &Input, report: &mut Report) {
    let a = input.analysis;
    let docs = Table {
        title: "Documents".into(),
        columns: ["Document", "Type", "Chemin", "Catégorie", "Exigences"]
            .map(String::from)
            .to_vec(),
        rows: input
            .documents
            .iter()
            .enumerate()
            .map(|(i, d)| {
                vec![
                    d.name.clone(),
                    d.type_name.clone(),
                    d.path.clone(),
                    d.category.clone().unwrap_or_default(),
                    a.summaries
                        .get(i)
                        .map(|s| s.requirements.to_string())
                        .unwrap_or_default(),
                ]
            })
            .collect(),
    };
    let channels = Table {
        title: "Canaux".into(),
        columns: ["Documents couvrants", "Document couvert", "Couverture"]
            .map(String::from)
            .to_vec(),
        rows: a
            .channels
            .iter()
            .map(|c| {
                vec![
                    c.covering
                        .iter()
                        .map(|&k| a.documents[k].name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    a.documents[c.covered].name.clone(),
                    format!(
                        "{} ({}/{})",
                        percent(c.covered_count, c.total),
                        c.covered_count,
                        c.total
                    ),
                ]
            })
            .collect(),
    };
    report.sections.push(Section {
        heading: "Projet".into(),
        paragraphs: vec![
            format!("Fichier : {}", input.path),
            format!(
                "{} document(s), {} canal(aux).",
                input.documents.len(),
                a.channels.len()
            ),
        ],
        tables: vec![docs.clone()],
    });
    report.sections.push(Section {
        heading: "Canaux de couverture".into(),
        paragraphs: if a.channels.is_empty() {
            vec!["Aucun canal.".into()]
        } else {
            Vec::new()
        },
        tables: vec![channels]
            .into_iter()
            .filter(|t| !t.rows.is_empty())
            .collect(),
    });
    report.flat = docs;
}

/// Construit et rend un rapport.
pub fn render(input: &Input, template: Template, format: Format) -> Result<Vec<u8>, ReportError> {
    let report = build(input, template);
    match format {
        Format::Html => Ok(html(&report).into_bytes()),
        Format::Markdown => Ok(markdown(&report).into_bytes()),
        Format::Csv => Ok(csv(&report.flat)),
        Format::Xlsx => xlsx(&report),
        Format::Docx => docx::render(&report).map_err(ReportError::Docx),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Page HTML autonome (styles intégrés, aucune ressource externe).
pub fn html(r: &Report) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"fr\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>\n\
         body {{ font-family: system-ui, 'Segoe UI', sans-serif; margin: 32px auto; max-width: 1100px; padding: 0 16px; color: #1c1c1a; }}\n\
         h1 {{ margin-bottom: 4px; }} .sub {{ color: #6b6b66; margin-top: 0; }}\n\
         table {{ border-collapse: collapse; width: 100%; margin: 8px 0 24px; font-size: 14px; }}\n\
         th, td {{ border: 1px solid #e3e3de; padding: 6px 10px; text-align: left; vertical-align: top; }}\n\
         th {{ background: #f0f0ec; font-weight: 600; }} caption {{ text-align: left; font-weight: 600; padding: 4px 0; }}\n\
         .ko {{ color: #b3261e; }}\n</style>\n</head>\n<body>\n<h1>{}</h1>\n<p class=\"sub\">{}</p>\n",
        esc(&r.title),
        esc(&r.title),
        esc(&r.subtitle)
    );
    for s in &r.sections {
        let _ = writeln!(out, "<h2>{}</h2>", esc(&s.heading));
        for p in &s.paragraphs {
            let _ = writeln!(out, "<p>{}</p>", esc(p));
        }
        for t in &s.tables {
            out.push_str("<table>\n");
            if t.title != s.heading {
                let _ = writeln!(out, "<caption>{}</caption>", esc(&t.title));
            }
            out.push_str("<tr>");
            for c in &t.columns {
                let _ = write!(out, "<th>{}</th>", esc(c));
            }
            out.push_str("</tr>\n");
            for row in &t.rows {
                out.push_str("<tr>");
                for cell in row {
                    let class = if cell == "Non couverte" || cell == "Erreur" {
                        " class=\"ko\""
                    } else {
                        ""
                    };
                    let _ = write!(out, "<td{class}>{}</td>", esc(cell));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</table>\n");
        }
    }
    out.push_str("</body>\n</html>\n");
    out
}

pub fn markdown(r: &Report) -> String {
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut out = format!("# {}\n\n{}\n", r.title, r.subtitle);
    for s in &r.sections {
        let _ = write!(out, "\n## {}\n\n", s.heading);
        for p in &s.paragraphs {
            let _ = write!(out, "{p}\n\n");
        }
        for t in &s.tables {
            let _ = writeln!(
                out,
                "| {} |",
                t.columns
                    .iter()
                    .map(|c| cell(c))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
            let _ = writeln!(out, "|{}", "---|".repeat(t.columns.len()));
            for row in &t.rows {
                let _ = writeln!(
                    out,
                    "| {} |",
                    row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(" | ")
                );
            }
            out.push('\n');
        }
    }
    out
}

/// CSV du tableau principal : séparateur `;` et marque d'ordre des octets, pour
/// une ouverture directe dans un tableur réglé en français.
pub fn csv(t: &Table) -> Vec<u8> {
    let field = |s: &str| {
        if s.contains([';', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_owned()
        }
    };
    let mut out = String::from("\u{feff}");
    let line = |cells: &[String]| cells.iter().map(|c| field(c)).collect::<Vec<_>>().join(";");
    out.push_str(&line(&t.columns));
    out.push_str("\r\n");
    for row in &t.rows {
        out.push_str(&line(row));
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// Classeur : une feuille de synthèse, puis une feuille par tableau.
pub fn xlsx(r: &Report) -> Result<Vec<u8>, ReportError> {
    use rust_xlsxwriter::{Format as Cell, FormatBorder, Workbook};
    let mut wb = Workbook::new();
    let bold = Cell::new().set_bold();
    let header = Cell::new()
        .set_bold()
        .set_background_color(0xF0F0EC)
        .set_border(FormatBorder::Thin);
    let body = Cell::new().set_border(FormatBorder::Thin).set_text_wrap();

    let summary = wb.add_worksheet().set_name("Synthèse")?;
    summary.write_with_format(0, 0, &r.title, &bold)?;
    summary.write(1, 0, &r.subtitle)?;
    let mut row = 3;
    for s in &r.sections {
        summary.write_with_format(row, 0, &s.heading, &bold)?;
        row += 1;
        for p in &s.paragraphs {
            summary.write(row, 0, p)?;
            row += 1;
        }
        row += 1;
    }
    summary.set_column_width(0, 100)?;

    let mut used: Vec<String> = vec!["Synthèse".into()];
    for t in r.sections.iter().flat_map(|s| &s.tables) {
        // Nom de feuille : 31 caractères au plus, sans []:*?/\ et unique.
        let base: String = t
            .title
            .chars()
            .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
            .take(28)
            .collect();
        let mut name = base.clone();
        for k in 2.. {
            if !used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
                break;
            }
            name = format!("{base} {k}");
        }
        used.push(name.clone());
        let ws = wb.add_worksheet().set_name(&name)?;
        for (c, col) in t.columns.iter().enumerate() {
            ws.write_with_format(0, c as u16, col, &header)?;
        }
        let mut widths: Vec<usize> = t.columns.iter().map(|c| c.chars().count()).collect();
        for (i, cells) in t.rows.iter().enumerate() {
            for (c, value) in cells.iter().enumerate() {
                // Les nombres restent des nombres dans le tableur.
                match value.parse::<f64>() {
                    Ok(n) if !value.is_empty() => {
                        ws.write_number_with_format(i as u32 + 1, c as u16, n, &body)?
                    }
                    _ => ws.write_with_format(i as u32 + 1, c as u16, value, &body)?,
                };
                if let Some(w) = widths.get_mut(c) {
                    *w = (*w).max(value.chars().count().min(80));
                }
            }
        }
        for (c, w) in widths.iter().enumerate() {
            ws.set_column_width(c as u16, (*w as f64 + 2.0).min(82.0))?;
        }
        ws.set_freeze_panes(1, 0)?;
        if !t.rows.is_empty() {
            ws.autofilter(0, 0, t.rows.len() as u32, t.columns.len() as u16 - 1)?;
        }
    }
    Ok(wb.save_to_buffer()?)
}
