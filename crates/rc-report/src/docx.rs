//! Rendu DOCX : un paquet WordprocessingML minimal (document, styles, relations)
//! écrit directement, avec des titres et des tableaux à bordures.

use crate::Report;
use std::io::Write;
use zip::write::SimpleFileOptions;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="21"/><w:lang w:val="fr-FR"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="120"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="60"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:rPr><w:color w:val="6B6B66"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="320" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/><w:color w:val="2F6F5E"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Caption"><w:name w:val="caption"/><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/></w:pPr><w:rPr><w:b/></w:rPr></w:style>
<w:style w:type="table" w:styleId="Grid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders>
<w:top w:val="single" w:sz="4" w:color="BFBFBA"/><w:left w:val="single" w:sz="4" w:color="BFBFBA"/><w:bottom w:val="single" w:sz="4" w:color="BFBFBA"/><w:right w:val="single" w:sz="4" w:color="BFBFBA"/>
<w:insideH w:val="single" w:sz="4" w:color="BFBFBA"/><w:insideV w:val="single" w:sz="4" w:color="BFBFBA"/></w:tblBorders>
<w:tblCellMar><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
</w:styles>"#;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn paragraph(out: &mut String, style: Option<&str>, text: &str, bold: bool) {
    out.push_str("<w:p>");
    if let Some(s) = style {
        out.push_str(&format!("<w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr>"));
    }
    for (i, line) in text.split('\n').enumerate() {
        out.push_str("<w:r>");
        if bold {
            out.push_str("<w:rPr><w:b/></w:rPr>");
        }
        if i > 0 {
            out.push_str("<w:br/>");
        }
        out.push_str(&format!(
            "<w:t xml:space=\"preserve\">{}</w:t></w:r>",
            esc(line)
        ));
    }
    out.push_str("</w:p>");
}

pub fn render(r: &Report) -> Result<Vec<u8>, String> {
    let mut body = String::new();
    paragraph(&mut body, Some("Title"), &r.title, false);
    paragraph(&mut body, Some("Subtitle"), &r.subtitle, false);
    for s in &r.sections {
        paragraph(&mut body, Some("Heading1"), &s.heading, false);
        for p in &s.paragraphs {
            paragraph(&mut body, None, p, false);
        }
        for t in &s.tables {
            if t.title != s.heading {
                paragraph(&mut body, Some("Caption"), &t.title, false);
            }
            body.push_str(
                "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr>",
            );
            body.push_str("<w:tr><w:trPr><w:tblHeader/></w:trPr>");
            for c in &t.columns {
                body.push_str("<w:tc><w:tcPr><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F0F0EC\"/></w:tcPr>");
                paragraph(&mut body, None, c, true);
                body.push_str("</w:tc>");
            }
            body.push_str("</w:tr>");
            for row in &t.rows {
                body.push_str("<w:tr>");
                for cell in row {
                    body.push_str("<w:tc>");
                    paragraph(&mut body, None, cell, false);
                    body.push_str("</w:tc>");
                }
                body.push_str("</w:tr>");
            }
            body.push_str("</w:tbl>");
            paragraph(&mut body, None, "", false);
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}\
         <w:sectPr><w:pgSz w:w=\"16838\" w:h=\"11906\" w:orient=\"landscape\"/>\
         <w:pgMar w:top=\"1000\" w:right=\"1000\" w:bottom=\"1000\" w:left=\"1000\" w:header=\"500\" w:footer=\"500\" w:gutter=\"0\"/></w:sectPr>\
         </w:body></w:document>"
    );
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        for (name, content) in [
            ("[Content_Types].xml", CONTENT_TYPES),
            ("_rels/.rels", RELS),
            ("word/_rels/document.xml.rels", DOCUMENT_RELS),
            ("word/styles.xml", STYLES),
            ("word/document.xml", document.as_str()),
        ] {
            zip.start_file(name, opts).map_err(|e| e.to_string())?;
            zip.write_all(content.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}
