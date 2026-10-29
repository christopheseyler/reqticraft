//! HTML → lignes stylées.
//!
//! Correspondances documentées par le connecteur HTML (`docs/interop-formats.md` §5) :
//! `H1`…`H6`, `P`, `LI`, `PRE` deviennent le style de la ligne, suivi de
//! `.<classe>` quand l'élément porte un attribut `class` ; `TABLE` → `#TABLE`,
//! `TR` → `#TR`, `TD`/`TH` → `|<colonne> <balise>` ; `IMG` → `<Picture<source>` ;
//! `BR` coupe la ligne. Le découpage en jetons (entités, attributs, texte brut des
//! scripts) est confié à `html5ever` ; l'arbre est reconstruit avec les règles
//! de fermeture implicite usuelles (`p`, `li`, `td`, `tr`…).

use crate::lines::{Lines, Para};
use crate::{ConvertError, ConvertOptions};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use std::cell::RefCell;
use std::path::Path;

pub fn convert(path: &Path, _options: &ConvertOptions) -> Result<String, ConvertError> {
    let text = crate::read_text(path)?;
    Ok(convert_str(&text))
}

// ---------------------------------------------------------------- arbre

#[derive(Debug, Default)]
pub(crate) struct Elem {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Debug)]
pub(crate) enum Child {
    Text(String),
    Elem(Elem),
}

impl Elem {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Elem> + 'a {
        self.children.iter().filter_map(move |c| match c {
            Child::Elem(e) if e.name == name => Some(e),
            _ => None,
        })
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Balises qui ferment un `p` ouvert.
const CLOSES_P: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "dl",
    "table",
    "pre",
    "blockquote",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "aside",
    "hr",
    "figure",
    "form",
    "details",
    "address",
    "main",
];

struct Builder {
    stack: Vec<Elem>,
}

impl Builder {
    fn open(&self, name: &str) -> Option<usize> {
        self.stack.iter().rposition(|e| e.name == name)
    }

    /// Ferme `name` s'il est ouvert sans franchir une des balises `barriers`.
    fn close_within(&mut self, name: &str, barriers: &[&str]) {
        for i in (1..self.stack.len()).rev() {
            let n = self.stack[i].name.as_str();
            if n == name {
                self.pop_to(i);
                return;
            }
            if barriers.contains(&n) {
                return;
            }
        }
    }

    fn pop_to(&mut self, index: usize) {
        while self.stack.len() > index.max(1) {
            let e = self.stack.pop().expect("pile non vide");
            self.stack
                .last_mut()
                .expect("racine")
                .children
                .push(Child::Elem(e));
        }
    }

    fn text(&mut self, t: &str) {
        let top = self.stack.last_mut().expect("racine");
        match top.children.last_mut() {
            Some(Child::Text(s)) => s.push_str(t),
            _ => top.children.push(Child::Text(t.to_owned())),
        }
    }

    fn start(&mut self, name: &str, attrs: Vec<(String, String)>, self_closing: bool) {
        const CELL_SCOPE: &[&str] = &["table"];
        if CLOSES_P.contains(&name) {
            self.close_within("p", &["table", "td", "th", "li", "div", "blockquote"]);
        }
        match name {
            "li" => self.close_within("li", &["ul", "ol", "table"]),
            "dt" | "dd" => {
                self.close_within("dt", &["dl", "table"]);
                self.close_within("dd", &["dl", "table"]);
            }
            "tr" => {
                self.close_within("td", CELL_SCOPE);
                self.close_within("th", CELL_SCOPE);
                self.close_within("tr", CELL_SCOPE);
            }
            "td" | "th" => {
                self.close_within("td", &["tr", "table"]);
                self.close_within("th", &["tr", "table"]);
            }
            "thead" | "tbody" | "tfoot" => {
                self.close_within("tr", CELL_SCOPE);
                for s in ["thead", "tbody", "tfoot"] {
                    self.close_within(s, CELL_SCOPE);
                }
            }
            _ => {}
        }
        let elem = Elem {
            name: name.to_owned(),
            attrs,
            children: Vec::new(),
        };
        if VOID.contains(&name) || self_closing {
            self.stack
                .last_mut()
                .expect("racine")
                .children
                .push(Child::Elem(elem));
        } else {
            self.stack.push(elem);
        }
    }

    fn end(&mut self, name: &str) {
        if let Some(i) = self.open(name) {
            if i > 0 {
                self.pop_to(i);
            }
        }
    }
}

struct Sink(RefCell<Builder>);

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut b = self.0.borrow_mut();
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                match tag.kind {
                    TagKind::StartTag => {
                        let attrs = tag
                            .attrs
                            .iter()
                            .map(|a| (a.name.local.to_string(), a.value.to_string()))
                            .collect();
                        b.start(&name, attrs, tag.self_closing);
                        match name.as_str() {
                            "script" => return TokenSinkResult::RawData(RawKind::ScriptData),
                            "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                                return TokenSinkResult::RawData(RawKind::Rawtext)
                            }
                            "textarea" | "title" => {
                                return TokenSinkResult::RawData(RawKind::Rcdata)
                            }
                            _ => {}
                        }
                    }
                    TagKind::EndTag => b.end(&name),
                }
            }
            Token::CharacterTokens(t) => b.text(&t),
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

/// Arbre simplifié d'un document HTML ; la racine porte le nom `#document`.
pub(crate) fn parse(html: &str) -> Elem {
    let sink = Sink(RefCell::new(Builder {
        stack: vec![Elem {
            name: "#document".into(),
            ..Default::default()
        }],
    }));
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    let tok = Tokenizer::new(sink, TokenizerOpts::default());
    let _ = tok.feed(&input);
    tok.end();
    let mut b = tok.sink.0.into_inner();
    b.pop_to(1);
    b.stack.pop().unwrap_or_default()
}

// ---------------------------------------------------------------- conversion

pub fn convert_str(html: &str) -> String {
    let doc = parse(html);
    let body = find(&doc, "body").unwrap_or(&doc);
    let mut w = Walker::default();
    w.block(body, "BODY");
    w.flush("BODY");
    w.lines.finish()
}

fn find<'a>(e: &'a Elem, name: &str) -> Option<&'a Elem> {
    for c in &e.children {
        if let Child::Elem(c) = c {
            if c.name == name {
                return Some(c);
            }
            if let Some(f) = find(c, name) {
                return Some(f);
            }
        }
    }
    None
}

/// Balises dont le contenu est ignoré.
const SKIPPED: &[&str] = &[
    "head", "script", "style", "noscript", "template", "svg", "math", "iframe", "object", "title",
];
/// Paragraphes : leur texte forme une ligne.
const PARAGRAPHS: &[&str] = &[
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "li",
    "pre",
    "dt",
    "dd",
    "caption",
    "figcaption",
    "summary",
    "address",
];
/// Conteneurs de blocs.
const BLOCKS: &[&str] = &[
    "#document",
    "html",
    "body",
    "div",
    "section",
    "article",
    "main",
    "header",
    "footer",
    "nav",
    "aside",
    "ul",
    "ol",
    "dl",
    "blockquote",
    "figure",
    "details",
    "form",
    "fieldset",
    "center",
    "hr",
];

#[derive(Default)]
struct Walker {
    lines: Lines,
    /// Texte en ligne hors paragraphe (directement dans un `div`…).
    pending: String,
    pictures: Vec<String>,
}

impl Walker {
    fn block(&mut self, node: &Elem, container: &str) {
        for child in &node.children {
            match child {
                Child::Text(t) => self.pending.push_str(t),
                Child::Elem(e) => self.element(e, container),
            }
        }
    }

    fn element(&mut self, e: &Elem, container: &str) {
        let name = e.name.as_str();
        if SKIPPED.contains(&name) {
            return;
        }
        if name == "table" {
            self.flush(container);
            let rows = self.table(e);
            self.lines.table(&rows);
            self.flush_pictures();
        } else if PARAGRAPHS.contains(&name) {
            self.flush(container);
            let style = style_of(e);
            let mut text = String::new();
            let mut nested = Vec::new();
            self.inline(e, &mut text, &mut nested);
            self.paragraph(&style, &text, name == "pre");
            self.flush_pictures();
            // Listes et blocs imbriqués (dans un `li` par exemple).
            for n in nested {
                self.element(n, &style);
                self.flush(&style);
            }
        } else if BLOCKS.contains(&name) {
            self.flush(container);
            let style = style_of(e);
            self.block(e, &style);
            self.flush(&style);
        } else if name == "br" {
            self.pending.push('\n');
        } else if name == "img" {
            if let Some(src) = e.attr("src") {
                self.pictures.push(src.to_owned());
            }
        } else {
            let mut text = String::new();
            let mut nested = Vec::new();
            self.inline(e, &mut text, &mut nested);
            self.pending.push_str(&text);
            for n in nested {
                self.flush(container);
                self.element(n, container);
            }
        }
    }

    /// Texte d'un paragraphe ; les blocs imbriqués sont mis de côté.
    fn inline<'a>(&mut self, node: &'a Elem, out: &mut String, nested: &mut Vec<&'a Elem>) {
        for child in &node.children {
            match child {
                Child::Text(t) => out.push_str(t),
                Child::Elem(e) => {
                    let name = e.name.as_str();
                    if SKIPPED.contains(&name) {
                        continue;
                    }
                    if name == "br" {
                        out.push('\n');
                    } else if name == "img" {
                        if let Some(src) = e.attr("src") {
                            self.pictures.push(src.to_owned());
                        }
                    } else if name == "table"
                        || BLOCKS.contains(&name)
                        || PARAGRAPHS.contains(&name) && name != "pre"
                    {
                        nested.push(e);
                    } else {
                        self.inline(e, out, nested);
                    }
                }
            }
        }
    }

    fn table(&mut self, table: &Elem) -> Vec<Vec<Vec<Para>>> {
        let mut trs: Vec<&Elem> = Vec::new();
        for c in &table.children {
            if let Child::Elem(e) = c {
                match e.name.as_str() {
                    "tr" => trs.push(e),
                    "thead" | "tbody" | "tfoot" => trs.extend(e.elements("tr")),
                    _ => {}
                }
            }
        }
        let mut rows = Vec::new();
        for tr in trs {
            let mut row = Vec::new();
            for c in &tr.children {
                let Child::Elem(cell) = c else { continue };
                if cell.name != "td" && cell.name != "th" {
                    continue;
                }
                let style = style_of(cell);
                let mut sub = Walker::default();
                sub.block(cell, &style);
                sub.flush(&style);
                self.pictures.append(&mut sub.pictures);
                let paras = sub
                    .lines
                    .finish()
                    .lines()
                    .filter_map(|l| l.split_once('\t'))
                    .map(|(s, t)| (s.to_owned(), t.to_owned()))
                    .collect();
                row.push(paras);
            }
            rows.push(row);
        }
        rows
    }

    fn paragraph(&mut self, style: &str, text: &str, preformatted: bool) {
        if preformatted {
            for line in text.lines() {
                self.lines.para(style, line);
            }
            return;
        }
        for part in text.split('\n') {
            self.lines.para(style, &collapse(part));
        }
    }

    fn flush(&mut self, container: &str) {
        let text = std::mem::take(&mut self.pending);
        self.paragraph(container, &text, false);
        self.flush_pictures();
    }

    fn flush_pictures(&mut self) {
        for src in std::mem::take(&mut self.pictures) {
            self.lines.raw(&format!("<Picture<{src}>"));
        }
    }
}

/// Nom de balise en majuscules, suivi de `.<classe>` s'il y a lieu.
fn style_of(e: &Elem) -> String {
    let mut style = e.name.to_ascii_uppercase();
    if let Some(class) = e.attr("class") {
        let class = class.split_whitespace().collect::<Vec<_>>().join(".");
        if !class.is_empty() {
            style.push('.');
            style.push_str(&class);
        }
    }
    style
}

/// Espaces HTML : toute suite de blancs devient un espace.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_blocks_tables_and_images() {
        let html = r#"<!DOCTYPE html><html><head><title>x</title><style>p{color:red}</style></head><body>
            <h1 class="chapter">1 Introduction</h1>
            <p>REQ_1 : Le   syst&egrave;me <b>doit</b> démarrer.<br>[Covers: A1]
            <ul><li>premier<ul><li>imbriqué</li></ul><li>second</ul>
            <table><tr><th>Id<th>Texte<tr><td>R1</td><td><p>a</p><p>b</p></td></tr></table>
            <div>libre <img src="img/x.png"> suite</div>
            <script>document.write("<p>non</p>")</script>
            <pre>ligne 1
ligne 2</pre>
        </body></html>"#;
        assert_eq!(
            convert_str(html),
            "H1.chapter\t1 Introduction\n\
             P\tREQ_1 : Le système doit démarrer.\n\
             P\t[Covers: A1]\n\
             LI\tpremier\n\
             LI\timbriqué\n\
             LI\tsecond\n\
             #TABLE\n#TR|1 TH\tId|2 TH\tTexte\n#TR|1 TD\tR1|2 P\ta|2 P\tb\n\
             DIV\tlibre suite\n\
             <Picture<img/x.png>\n\
             PRE\tligne 1\nPRE\tligne 2\n"
        );
    }
}
