//! Fichiers médias (images, vidéos) → XML de la forme produite par l'outil
//! MediaInfo (`mediainfo --Output=XML`), que lisent les types MediaInfo
//! (`docs/interop-formats.md` §5) :
//!
//! ```xml
//! <MediaInfo><media ref="chemin">
//!   <track type="General"><Format>PNG</Format><Comment>REQ1 [Covers: A1]</Comment>…</track>
//!   <track type="Image"><Format>PNG</Format><Width>640</Width><Height>480</Height></track>
//! </media></MediaInfo>
//! ```
//!
//! Si l'outil `mediainfo` est installé, sa sortie est utilisée telle quelle.
//! Sinon un lecteur intégré extrait le format, les dimensions, la durée et les
//! commentaires des PNG, JPEG, GIF, BMP et MP4/MOV ; les autres formats n'ont
//! que la piste générale.

use crate::{xml_escape, ConvertError, ConvertOptions};
use std::path::Path;
use std::process::Command;

pub fn convert(path: &Path, options: &ConvertOptions) -> Result<String, ConvertError> {
    if !options.flag("builtinMediaReader") {
        if let Ok(out) = Command::new("mediainfo")
            .arg("--Output=XML")
            .arg(path)
            .output()
        {
            if out.status.success() && !out.stdout.is_empty() {
                let text = rc_interop::text::decode(&out.stdout);
                return Ok(crate::strip_xml_declaration(&text).to_owned());
            }
        }
    }
    let data = std::fs::read(path).map_err(|source| ConvertError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(describe(&path.display().to_string(), &data))
}

#[derive(Default, Debug, PartialEq)]
struct Info {
    format: String,
    kind: &'static str,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<f64>,
    title: Option<String>,
    comment: Option<String>,
}

pub fn describe(reference: &str, data: &[u8]) -> String {
    let info = probe(data);
    let mut out = format!(
        "<MediaInfo><media ref=\"{}\"><track type=\"General\">",
        xml_escape(reference)
    );
    let mut field = |name: &str, value: &str| {
        out.push_str(&format!("<{name}>{}</{name}>", xml_escape(value)));
    };
    field("CompleteName", reference);
    if !info.format.is_empty() {
        field("Format", &info.format);
    }
    field("FileSize", &data.len().to_string());
    if let Some(d) = info.duration {
        field("Duration", &format!("{d:.3}"));
    }
    if let Some(t) = &info.title {
        field("Title", t);
    }
    if let Some(c) = &info.comment {
        field("Comment", c);
    }
    out.push_str("</track>");
    if !info.kind.is_empty() {
        out.push_str(&format!(
            "<track type=\"{}\"><Format>{}</Format>",
            info.kind,
            xml_escape(&info.format)
        ));
        if let (Some(w), Some(h)) = (info.width, info.height) {
            out.push_str(&format!("<Width>{w}</Width><Height>{h}</Height>"));
        }
        out.push_str("</track>");
    }
    out.push_str("</media></MediaInfo>");
    out
}

fn be16(d: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes(d.get(i..i + 2)?.try_into().ok()?) as u32)
}
fn be32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}
fn le16(d: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?) as u32)
}
fn le32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

fn probe(d: &[u8]) -> Info {
    if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(d)
    } else if d.starts_with(&[0xff, 0xd8]) {
        jpeg(d)
    } else if d.starts_with(b"GIF8") {
        gif(d)
    } else if d.starts_with(b"BM") {
        Info {
            format: "BMP".into(),
            kind: "Image",
            width: le32(d, 18),
            height: le32(d, 22).map(|h| (h as i32).unsigned_abs()),
            ..Default::default()
        }
    } else if d
        .get(4..8)
        .is_some_and(|t| t == b"ftyp" || t == b"moov" || t == b"mdat")
    {
        mp4(d)
    } else if d.starts_with(b"RIFF") && d.get(8..12) == Some(b"AVI ") {
        Info {
            format: "AVI".into(),
            ..Default::default()
        }
    } else if d.starts_with(b"OggS") {
        Info {
            format: "Ogg".into(),
            ..Default::default()
        }
    } else {
        Info::default()
    }
}

fn png(d: &[u8]) -> Info {
    let mut info = Info {
        format: "PNG".into(),
        kind: "Image",
        ..Default::default()
    };
    let mut i = 8;
    while let (Some(len), Some(kind)) = (be32(d, i), d.get(i + 4..i + 8)) {
        let body = d.get(i + 8..i + 8 + len as usize).unwrap_or(&[]);
        match kind {
            b"IHDR" => {
                info.width = be32(body, 0);
                info.height = be32(body, 4);
            }
            b"tEXt" | b"iTXt" => {
                if let Some(nul) = body.iter().position(|&b| b == 0) {
                    let key = String::from_utf8_lossy(&body[..nul]).to_string();
                    let value = if kind == b"tEXt" {
                        body[nul + 1..]
                            .iter()
                            .map(|&b| b as char)
                            .collect::<String>()
                    } else {
                        // drapeau, méthode, langue\0, mot-clé traduit\0, texte UTF-8.
                        let rest = body.get(nul + 3..).unwrap_or(&[]);
                        let mut parts = rest.splitn(3, |&b| b == 0);
                        let _lang = parts.next();
                        let _translated = parts.next();
                        String::from_utf8_lossy(parts.next().unwrap_or(&[])).to_string()
                    };
                    match key.as_str() {
                        "Comment" | "Description" => info.comment = Some(value),
                        "Title" => info.title = Some(value),
                        _ => {}
                    }
                }
            }
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len as usize;
    }
    info
}

fn jpeg(d: &[u8]) -> Info {
    let mut info = Info {
        format: "JPEG".into(),
        kind: "Image",
        ..Default::default()
    };
    let mut i = 2;
    while i + 4 <= d.len() && d[i] == 0xff {
        let marker = d[i + 1];
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        let Some(len) = be16(d, i + 2) else { break };
        let body = d.get(i + 4..i + 2 + len as usize).unwrap_or(&[]);
        match marker {
            0xfe => {
                info.comment = Some(
                    String::from_utf8_lossy(body)
                        .trim_end_matches('\0')
                        .to_owned(),
                )
            }
            0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => {
                info.height = be16(body, 1);
                info.width = be16(body, 3);
            }
            0xe1 if body.starts_with(b"Exif\0\0") && info.comment.is_none() => {
                info.comment = exif_description(&body[6..]);
            }
            _ => {}
        }
        i += 2 + len as usize;
    }
    info
}

/// Étiquette TIFF `ImageDescription` (0x010E) de l'IFD0.
fn exif_description(tiff: &[u8]) -> Option<String> {
    let little = tiff.starts_with(b"II");
    let u16_at = |i| if little { le16(tiff, i) } else { be16(tiff, i) };
    let u32_at = |i| if little { le32(tiff, i) } else { be32(tiff, i) };
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    for n in 0..count {
        let e = ifd + 2 + n * 12;
        if u16_at(e)? == 0x010e {
            let len = u32_at(e + 4)? as usize;
            let bytes = if len <= 4 {
                tiff.get(e + 8..e + 8 + len)?
            } else {
                let off = u32_at(e + 8)? as usize;
                tiff.get(off..off + len)?
            };
            return Some(
                String::from_utf8_lossy(bytes)
                    .trim_end_matches('\0')
                    .to_owned(),
            );
        }
    }
    None
}

fn gif(d: &[u8]) -> Info {
    let mut info = Info {
        format: "GIF".into(),
        kind: "Image",
        width: le16(d, 6),
        height: le16(d, 8),
        ..Default::default()
    };
    // Extension de commentaire : 0x21 0xFE puis sous-blocs.
    if let Some(pos) = d.windows(2).position(|w| w == [0x21, 0xfe]) {
        let mut i = pos + 2;
        let mut text = Vec::new();
        while let Some(&n) = d.get(i) {
            if n == 0 {
                break;
            }
            text.extend_from_slice(d.get(i + 1..i + 1 + n as usize).unwrap_or(&[]));
            i += 1 + n as usize;
        }
        info.comment = Some(String::from_utf8_lossy(&text).into_owned());
    }
    info
}

/// MP4 / QuickTime : durée (`mvhd`), titre et commentaire (`©nam`, `©cmt`).
fn mp4(d: &[u8]) -> Info {
    let mut info = Info {
        format: "MPEG-4".into(),
        kind: "Video",
        ..Default::default()
    };
    if d.get(8..12) == Some(b"qt  ") {
        info.format = "QuickTime".into();
    }
    walk_boxes(d, 0, d.len(), &mut info);
    info
}

fn walk_boxes(d: &[u8], start: usize, end: usize, info: &mut Info) {
    let mut i = start;
    while i + 8 <= end {
        let Some(size) = be32(d, i) else { return };
        let kind = &d[i + 4..i + 8];
        let (header, size) = match size {
            0 => (8, end - i),
            1 => match d.get(i + 8..i + 16) {
                Some(b) => (
                    16,
                    u64::from_be_bytes(b.try_into().unwrap_or([0; 8])) as usize,
                ),
                None => return,
            },
            s => (8, s as usize),
        };
        if size < header || i + size > end {
            return;
        }
        let body = i + header;
        match kind {
            b"moov" | b"udta" | b"ilst" | b"trak" | b"mdia" => walk_boxes(d, body, i + size, info),
            // `meta` a 4 octets de version et drapeaux avant ses boîtes.
            b"meta" => walk_boxes(d, body + 4, i + size, info),
            b"mvhd" => {
                let version = d.get(body).copied().unwrap_or(0);
                let (scale, duration) = if version == 1 {
                    (
                        be32(d, body + 20),
                        d.get(body + 24..body + 32)
                            .map(|b| u64::from_be_bytes(b.try_into().unwrap_or([0; 8])) as f64),
                    )
                } else {
                    (be32(d, body + 12), be32(d, body + 16).map(f64::from))
                };
                if let (Some(s), Some(du)) = (scale, duration) {
                    if s > 0 {
                        info.duration = Some(du / f64::from(s));
                    }
                }
            }
            [0xa9, b'c', b'm', b't'] | [0xa9, b'n', b'a', b'm'] | [0xa9, b'd', b'e', b's'] => {
                let value = item_text(d, body, i + size);
                if kind[1] == b'n' {
                    info.title = Some(value);
                } else if info.comment.is_none() || kind[1] == b'c' {
                    info.comment = Some(value);
                }
            }
            _ => {}
        }
        i += size;
    }
}

/// Texte d'une entrée `ilst` (boîte `data`) ou d'une chaîne QuickTime `udta`.
fn item_text(d: &[u8], start: usize, end: usize) -> String {
    if d.get(start + 4..start + 8) == Some(b"data") {
        let len = be32(d, start).unwrap_or(0) as usize;
        let text = d.get(start + 16..(start + len).min(end)).unwrap_or(&[]);
        return String::from_utf8_lossy(text).into_owned();
    }
    // Forme QuickTime : longueur (2 octets), langue (2 octets), texte.
    let len = be16(d, start).unwrap_or(0) as usize;
    String::from_utf8_lossy(d.get(start + 4..(start + 4 + len).min(end)).unwrap_or(&[]))
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = (body.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v.extend_from_slice(&[0; 4]);
        v
    }

    #[test]
    fn reads_png_dimensions_and_comment() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = 640u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&480u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"tEXt", b"Comment\0IMG_REQ1 [Covers: THS_REQ1]"));
        png.extend(chunk(b"IEND", b""));
        assert_eq!(
            describe("a.png", &png),
            format!(
                "<MediaInfo><media ref=\"a.png\"><track type=\"General\"><CompleteName>a.png</CompleteName><Format>PNG</Format><FileSize>{}</FileSize><Comment>IMG_REQ1 [Covers: THS_REQ1]</Comment></track><track type=\"Image\"><Format>PNG</Format><Width>640</Width><Height>480</Height></track></media></MediaInfo>",
                png.len()
            )
        );
    }

    fn mp4_box(kind: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn reads_mp4_duration_and_comment() {
        let mut mvhd = vec![0u8; 4];
        mvhd.extend_from_slice(&[0; 8]);
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        mvhd.extend_from_slice(&12_500u32.to_be_bytes());
        mvhd.extend_from_slice(&[0; 80]);
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(b"VID_REQ1 [Covers: A1]");
        let cmt = mp4_box(&[0xa9, b'c', b'm', b't'], &mp4_box(b"data", &data));
        let mut meta = vec![0u8; 4];
        meta.extend(mp4_box(b"ilst", &cmt));
        let udta = mp4_box(b"udta", &mp4_box(b"meta", &meta));
        let mut moov_body = mp4_box(b"mvhd", &mvhd);
        moov_body.extend(udta);
        let mut file = mp4_box(b"ftyp", b"isom\0\0\0\0");
        file.extend(mp4_box(b"moov", &moov_body));
        let info = probe(&file);
        assert_eq!(info.duration, Some(12.5));
        assert_eq!(info.comment.as_deref(), Some("VID_REQ1 [Covers: A1]"));
        assert_eq!(info.kind, "Video");
    }
}
