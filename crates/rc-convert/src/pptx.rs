//! PowerPoint `.pptx` (et Google Slides exporté) → intermédiaire texte, diapositive
//! par diapositive (format décrit par le connecteur PowerPoint,
//! `docs/interop-formats.md` §5) :
//!
//! ```text
//! SLIDE <n>
//! SHAPES
//! <un paragraphe par ligne ; PICTURE <fichier> pour une image>
//! NOTES
//! <notes>
//! COMMENTS            (seulement s'il y en a)
//! <auteur> : <commentaire>
//! ```
//!
//! Le paramètre `slides` (`1-10,15`) restreint les diapositives converties. Les
//! textes des masques et dispositions ne sont pas repris.

use crate::archive::{relationships, Archive};
use crate::lines::clean;
use crate::{ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::path::Path;

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ppt"))
    {
        return Err(ConvertError::format(
            path,
            "format PowerPoint 97-2003 (.ppt) non pris en charge : enregistrer en .pptx",
        ));
    }
    let mut archive = Archive::open(path)?;
    let presentation = archive.require("ppt/presentation.xml")?;
    let pres = Document::parse(&presentation).map_err(|e| ConvertError::format(path, e))?;
    let rels = relationships(&mut archive, "ppt/presentation.xml")?;
    let slides: Vec<String> = pres
        .descendants()
        .filter(|n| n.has_tag_name((P, "sldId")))
        .filter_map(|n| n.attribute((R, "id")))
        .filter_map(|id| rels.iter().find(|(rid, _, _)| rid == id))
        .map(|(_, _, target)| target.clone())
        .collect();
    let selection = options.param("slides").map(parse_ranges);

    let mut out = String::new();
    for (i, part) in slides.iter().enumerate() {
        let number = i + 1;
        if selection
            .as_ref()
            .is_some_and(|ranges| !ranges.iter().any(|(a, b)| (*a..=*b).contains(&number)))
        {
            continue;
        }
        let xml = archive.require(part)?;
        let doc = Document::parse(&xml).map_err(|e| ConvertError::format(path, e))?;
        let slide_rels = relationships(&mut archive, part)?;
        out.push_str(&format!("SLIDE {number}\nSHAPES\n"));
        if let Some(tree) = doc.descendants().find(|n| n.has_tag_name((P, "spTree"))) {
            shapes(tree, &slide_rels, false, &mut out);
        }
        out.push_str("NOTES\n");
        if let Some((_, _, notes)) = slide_rels.iter().find(|(_, k, _)| k == "notesSlide") {
            if let Some(xml) = archive.text(notes)? {
                let doc = Document::parse(&xml).map_err(|e| ConvertError::format(path, e))?;
                if let Some(tree) = doc.descendants().find(|n| n.has_tag_name((P, "spTree"))) {
                    shapes(tree, &[], true, &mut out);
                }
            }
        }
        let mut comments = Vec::new();
        for (_, kind, target) in &slide_rels {
            if kind != "comments" {
                continue;
            }
            if let Some(xml) = archive.text(target)? {
                comments.extend(read_comments(&xml));
            }
        }
        if !comments.is_empty() {
            out.push_str("COMMENTS\n");
            for c in comments {
                out.push_str(&c);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

/// Texte des formes, dans l'ordre de l'arbre ; en mode notes, seul l'espace
/// réservé du corps est retenu (ni miniature ni numéro de diapositive).
fn shapes(tree: Node, rels: &[(String, String, String)], notes: bool, out: &mut String) {
    for shape in tree.children().filter(|n| n.is_element()) {
        let name = shape.tag_name().name();
        match name {
            "grpSp" => shapes(shape, rels, notes, out),
            "sp" => {
                if notes {
                    let ph = shape
                        .descendants()
                        .find(|n| n.has_tag_name((P, "ph")))
                        .and_then(|ph| ph.attribute("type"));
                    if ph != Some("body") {
                        continue;
                    }
                }
                if let Some(body) = shape.children().find(|n| n.has_tag_name((P, "txBody"))) {
                    paragraphs(body, out);
                }
            }
            "graphicFrame" => {
                for cell in shape.descendants().filter(|n| n.has_tag_name((A, "tc"))) {
                    if let Some(body) = cell.children().find(|n| n.has_tag_name((A, "txBody"))) {
                        paragraphs(body, out);
                    }
                }
            }
            "pic" if !notes => {
                let file = shape
                    .descendants()
                    .find(|n| n.has_tag_name((A, "blip")))
                    .and_then(|b| b.attribute((R, "embed")))
                    .and_then(|id| rels.iter().find(|(rid, _, _)| rid == id))
                    .map(|(_, _, t)| t.rsplit('/').next().unwrap_or(t).to_owned());
                if let Some(file) = file {
                    out.push_str(&format!("PICTURE {file}\n"));
                }
            }
            _ => {}
        }
    }
}

fn paragraphs(body: Node, out: &mut String) {
    for p in body.children().filter(|n| n.has_tag_name((A, "p"))) {
        let mut text = String::new();
        for r in p.children().filter(|n| n.is_element()) {
            match r.tag_name().name() {
                "r" | "fld" => {
                    for t in r.children().filter(|n| n.has_tag_name((A, "t"))) {
                        text.push_str(t.text().unwrap_or(""));
                    }
                }
                "br" => text.push(' '),
                _ => {}
            }
        }
        let text = clean(&text);
        if !text.trim().is_empty() {
            out.push_str(text.trim_end());
            out.push('\n');
        }
    }
}

/// Commentaires « hérités » (`p:cm/p:text`) ou modernes (`a:t`).
fn read_comments(xml: &str) -> Vec<String> {
    let Ok(doc) = Document::parse(xml) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.tag_name().name() == "cm")
        .filter_map(|cm| {
            let text: String = match cm.children().find(|n| n.has_tag_name((P, "text"))) {
                Some(t) => t.text().unwrap_or("").to_owned(),
                None => cm
                    .descendants()
                    .filter(|n| n.has_tag_name((A, "t")))
                    .filter_map(|n| n.text())
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            let text = clean(&text);
            (!text.trim().is_empty()).then(|| text.trim().to_owned())
        })
        .collect()
}

/// `1-10,15` → [(1, 10), (15, 15)].
fn parse_ranges(spec: &str) -> Vec<(usize, usize)> {
    spec.split([',', ';'])
        .filter_map(|part| {
            let part = part.trim();
            match part.split_once('-') {
                Some((a, b)) => Some((a.trim().parse().ok()?, b.trim().parse().ok()?)),
                None => part.parse().ok().map(|n| (n, n)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pptx() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rc-pptx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deck.pptx");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        let mut add = |name: &str, body: &str| {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        };
        let ns = format!(r#"xmlns:a="{A}" xmlns:p="{P}" xmlns:r="{R}""#);
        add(
            "ppt/presentation.xml",
            &format!(
                r#"<p:presentation {ns}><p:sldIdLst><p:sldId id="257" r:id="rId3"/><p:sldId id="256" r:id="rId2"/></p:sldIdLst></p:presentation>"#
            ),
        );
        add(
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships><Relationship Id="rId2" Type="x/slide" Target="slides/slide1.xml"/><Relationship Id="rId3" Type="x/slide" Target="slides/slide2.xml"/></Relationships>"#,
        );
        add(
            "ppt/slides/slide1.xml",
            &format!(
                r#"<p:sld {ns}><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Second</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
            ),
        );
        add(
            "ppt/slides/slide2.xml",
            &format!(
                r#"<p:sld {ns}><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>[REQ1: Démarrer]</a:t></a:r></a:p><a:p/></p:txBody></p:sp><p:grpSp><p:sp><p:txBody><a:p><a:r><a:t>groupe</a:t></a:r><a:br/><a:r><a:t>suite</a:t></a:r></a:p></p:txBody></p:sp></p:grpSp><p:pic><p:blipFill><a:blip r:embed="rId9"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#
            ),
        );
        add(
            "ppt/slides/_rels/slide2.xml.rels",
            r#"<Relationships><Relationship Id="rId9" Type="x/image" Target="../media/image1.png"/><Relationship Id="rId4" Type="x/notesSlide" Target="../notesSlides/notesSlide1.xml"/><Relationship Id="rId5" Type="x/comments" Target="../comments/comment1.xml"/></Relationships>"#,
        );
        add(
            "ppt/notesSlides/notesSlide1.xml",
            &format!(
                r#"<p:notes {ns}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp><p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>[Covers: A1]</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:nvPr><p:ph type="sldNum"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>2</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#
            ),
        );
        add(
            "ppt/comments/comment1.xml",
            &format!(
                r#"<p:cmLst {ns}><p:cm authorId="0"><p:text>À revoir</p:text></p:cm></p:cmLst>"#
            ),
        );
        zip.finish().unwrap();
        path
    }

    #[test]
    fn converts_slides_notes_and_comments() {
        let path = pptx();
        assert_eq!(
            convert(&path, &ConvertOptions::default()).unwrap(),
            "SLIDE 1\nSHAPES\n[REQ1: Démarrer]\ngroupe suite\nPICTURE image1.png\nNOTES\n[Covers: A1]\nCOMMENTS\nÀ revoir\n\
             SLIDE 2\nSHAPES\nSecond\nNOTES\n"
        );
        let only_two = ConvertOptions {
            params: vec![("slides".into(), "2".into())],
            ..Default::default()
        };
        assert_eq!(
            convert(&path, &only_two).unwrap(),
            "SLIDE 2\nSHAPES\nSecond\nNOTES\n"
        );
    }

    #[test]
    fn parses_slide_ranges() {
        assert_eq!(parse_ranges("1-3, 7"), vec![(1, 3), (7, 7)]);
    }
}
