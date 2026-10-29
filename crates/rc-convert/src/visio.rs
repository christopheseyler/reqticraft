//! Visio `.vsdx` → XML intermédiaire de la forme documentée par le connecteur
//! Visio (`docs/interop-formats.md` §5) :
//!
//! ```xml
//! <Document>
//!   <Page name="Page-1">
//!     <Shape name="Sheet.1" text="…">
//!       <Property name="Requirements Traceability" value="Covers: REQ_001"/>
//!     </Shape>
//!     <Connector name="…" sourceName="…" targetName="…"/>
//!   </Page>
//! </Document>
//! ```
//!
//! Les propriétés sont les données de forme (section `Property`), désignées par
//! leur libellé. Les propriétés héritées d'un modèle sans valeur locale ne sont
//! pas reprises. Le paramètre `page` restreint la conversion à une page.

use crate::archive::{relationships, Archive};
use crate::{xml_escape, ConvertError, ConvertOptions};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::Path;

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vsd"))
    {
        return Err(ConvertError::format(
            path,
            "format Visio 2003-2010 (.vsd) non pris en charge : enregistrer en .vsdx",
        ));
    }
    let mut archive = Archive::open(path)?;
    let pages_xml = archive.require("visio/pages/pages.xml")?;
    let pages = Document::parse(&pages_xml).map_err(|e| ConvertError::format(path, e))?;
    let rels = relationships(&mut archive, "visio/pages/pages.xml")?;
    let wanted = options
        .param("page")
        .map(str::trim)
        .filter(|p| !p.is_empty());

    let mut out = String::from("<Document>");
    for page in pages
        .descendants()
        .filter(|n| n.tag_name().name() == "Page")
    {
        let name = page
            .attribute("NameU")
            .or_else(|| page.attribute("Name"))
            .unwrap_or("");
        if wanted.is_some_and(|w| w != name && page.attribute("Name") != Some(w)) {
            continue;
        }
        // Les pages d'arrière-plan ne portent pas d'éléments de traçabilité.
        if page.attribute("Background") == Some("1") {
            continue;
        }
        let Some(target) = page
            .descendants()
            .find(|n| n.tag_name().name() == "Rel")
            .and_then(|n| n.attribute((R, "id")))
            .and_then(|id| rels.iter().find(|(rid, _, _)| rid == id))
            .map(|(_, _, t)| t.clone())
        else {
            continue;
        };
        let xml = archive.require(&target)?;
        let doc = Document::parse(&xml).map_err(|e| ConvertError::format(path, e))?;
        out.push_str(&format!("<Page name=\"{}\">", xml_escape(name)));
        out.push_str(&page_content(&doc));
        out.push_str("</Page>");
    }
    out.push_str("</Document>");
    Ok(out)
}

fn page_content(doc: &Document) -> String {
    let mut out = String::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for shape in doc.descendants().filter(|n| n.tag_name().name() == "Shape") {
        if let Some(id) = shape.attribute("ID") {
            names.insert(id.to_owned(), shape_name(shape));
        }
    }
    if let Some(shapes) = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "Shapes")
    {
        write_shapes(shapes, &mut out);
    }
    // Connexions : début et fin de chaque connecteur.
    let mut ends: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for c in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Connect")
    {
        let (Some(from), Some(to)) = (c.attribute("FromSheet"), c.attribute("ToSheet")) else {
            continue;
        };
        let entry = ends.entry(from.to_owned()).or_default();
        match c.attribute("FromCell") {
            Some("BeginX") => entry.0 = Some(to.to_owned()),
            Some("EndX") => entry.1 = Some(to.to_owned()),
            _ => {}
        }
    }
    let mut connectors: Vec<_> = ends.into_iter().collect();
    connectors.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, (source, target)) in connectors {
        let name = |id: &Option<String>| {
            id.as_ref()
                .and_then(|i| names.get(i))
                .cloned()
                .unwrap_or_default()
        };
        out.push_str(&format!(
            "<Connector name=\"{}\" sourceName=\"{}\" targetName=\"{}\"/>",
            xml_escape(names.get(&id).map(String::as_str).unwrap_or("")),
            xml_escape(&name(&source)),
            xml_escape(&name(&target)),
        ));
    }
    out
}

fn shape_name(shape: Node) -> String {
    shape
        .attribute("NameU")
        .or_else(|| shape.attribute("Name"))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Sheet.{}", shape.attribute("ID").unwrap_or("")))
}

fn write_shapes(shapes: Node, out: &mut String) {
    for shape in shapes.children().filter(|n| n.tag_name().name() == "Shape") {
        let text = shape
            .children()
            .find(|n| n.tag_name().name() == "Text")
            .map(|t| {
                t.descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect::<String>()
            })
            .unwrap_or_default();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push_str(&format!(
            "<Shape name=\"{}\" text=\"{}\">",
            xml_escape(&shape_name(shape)),
            xml_escape(&text)
        ));
        for section in shape
            .children()
            .filter(|n| n.tag_name().name() == "Section" && n.attribute("N") == Some("Property"))
        {
            for row in section.children().filter(|n| n.tag_name().name() == "Row") {
                let cell = |name: &str| {
                    row.children()
                        .find(|c| c.tag_name().name() == "Cell" && c.attribute("N") == Some(name))
                        .and_then(|c| c.attribute("V"))
                };
                let label = cell("Label")
                    .map(str::to_owned)
                    .or_else(|| {
                        row.attribute("N")
                            .map(|n| n.trim_start_matches("Prop.").to_owned())
                    })
                    .unwrap_or_default();
                if let Some(value) = cell("Value") {
                    out.push_str(&format!(
                        "<Property name=\"{}\" value=\"{}\"/>",
                        xml_escape(&label),
                        xml_escape(value)
                    ));
                }
            }
        }
        if let Some(sub) = shape.children().find(|n| n.tag_name().name() == "Shapes") {
            write_shapes(sub, out);
        }
        out.push_str("</Shape>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn converts_pages_shapes_properties_and_connectors() {
        let dir = std::env::temp_dir().join(format!("rc-vsdx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.vsdx");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        let mut add = |name: &str, body: &str| {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        };
        add(
            "visio/pages/pages.xml",
            &format!(
                r#"<Pages xmlns:r="{R}"><Page ID="0" NameU="Page-1"><Rel r:id="rId1"/></Page></Pages>"#
            ),
        );
        add(
            "visio/pages/_rels/pages.xml.rels",
            r#"<Relationships><Relationship Id="rId1" Type="x/page" Target="page1.xml"/></Relationships>"#,
        );
        add(
            "visio/pages/page1.xml",
            r#"<PageContents><Shapes>
              <Shape ID="1" NameU="Capteur"><Section N="Property"><Row N="Prop.Trace"><Cell N="Label" V="Requirements Traceability"/><Cell N="Value" V="Covers: REQ_001"/></Row></Section><Text>Mesure <cp IX="0"/>la température</Text></Shape>
              <Shape ID="2"><Text>Régulateur</Text></Shape>
              <Shape ID="3" NameU="Dynamic connector"/>
            </Shapes><Connects><Connect FromSheet="3" FromCell="BeginX" ToSheet="1"/><Connect FromSheet="3" FromCell="EndX" ToSheet="2"/></Connects></PageContents>"#,
        );
        zip.finish().unwrap();
        assert_eq!(
            convert(&path, &ConvertOptions::default()).unwrap(),
            "<Document><Page name=\"Page-1\">\
             <Shape name=\"Capteur\" text=\"Mesure la température\"><Property name=\"Requirements Traceability\" value=\"Covers: REQ_001\"/></Shape>\
             <Shape name=\"Sheet.2\" text=\"Régulateur\"></Shape>\
             <Shape name=\"Dynamic connector\" text=\"\"></Shape>\
             <Connector name=\"Dynamic connector\" sourceName=\"Capteur\" targetName=\"Sheet.2\"/>\
             </Page></Document>"
        );
    }
}
