//! Outils de gestion des exigences et des demandes (ALM), interrogés par leur
//! API REST. Chaque connecteur produit un XML de la forme lue par les types
//! d'analyse de l'outil d'origine (`docs/interop-formats.md` §12.3).
//!
//! Le document désigne une requête (JQL, cbQL, requête Lucene, paramètres
//! d'URL…) et le paramètre `profile` le serveur et les identifiants. Si le
//! document désigne un fichier local existant (réponse d'API enregistrée ou
//! export), il est converti sans connexion : c'est aussi le mode des tests.

use crate::http::{self, encode, Auth, Request};
use crate::xml::{attributes, elements, name, scalar};
use crate::{ConnectError, Context, Profile, Result};
use rc_convert::xml_escape;
use serde_json::Value;
use std::path::PathBuf;

/// Source d'un document : fichier local, ou requête sur un serveur.
enum Source<'a> {
    File(String, PathBuf),
    Server(&'a Profile, &'a str),
}

fn source<'a>(location: &'a str, ctx: &'a Context) -> Result<Source<'a>> {
    let local = ctx.base_dir.join(location);
    if !location.trim().is_empty() && local.is_file() {
        let text = rc_interop::read_text(&local).map_err(|e| ConnectError::msg(e.to_string()))?;
        return Ok(Source::File(text, local));
    }
    let profile = ctx.profile(true)?.expect("profil requis");
    if profile.server.is_empty() {
        return Err(ConnectError::msg(format!(
            "profil « {} » : serveur non renseigné",
            profile.name
        )));
    }
    Ok(Source::Server(profile, location.trim()))
}

fn parse_json(text: &str, path: &std::path::Path) -> Result<Value> {
    serde_json::from_str(text)
        .map_err(|e| ConnectError::msg(format!("{} : JSON invalide : {e}", path.display())))
}

fn is_xml(text: &str) -> bool {
    text.trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with('<')
}

fn without_declaration(xml: &str) -> &str {
    let t = xml.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with("<?xml") {
        t.find("?>").map(|i| &t[i + 2..]).unwrap_or(t)
    } else {
        t
    }
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

// ------------------------------------------------------------------- Jira

/// Jira : requête JQL. Forme `<issues key="…"><fields summary="…" description="…">`
/// (types Jira, deuxième variante) ; un export XML (flux RSS `<item><key>`) est
/// repris tel quel.
pub fn jira(location: &str, ctx: &Context) -> Result<String> {
    let issues: Vec<Value> = match source(location, ctx)? {
        Source::File(text, path) => {
            if is_xml(&text) {
                return Ok(without_declaration(&text).to_owned());
            }
            let v = parse_json(&text, &path)?;
            match v {
                Value::Array(a) => a,
                v => array(&v, "issues").to_vec(),
            }
        }
        Source::Server(p, jql) => {
            let auth = p.auth();
            let path = ctx.param("searchPath").unwrap_or("/rest/api/2/search");
            let mut all = Vec::new();
            let mut start = 0usize;
            let mut token: Option<String> = None;
            loop {
                let mut url = format!(
                    "{}{path}?jql={}&maxResults=100&fields=*navigable",
                    p.server,
                    encode(jql)
                );
                match &token {
                    Some(t) => url.push_str(&format!("&nextPageToken={}", encode(t))),
                    None => url.push_str(&format!("&startAt={start}")),
                }
                let page = http::get_json(&url, &auth)?;
                let got = array(&page, "issues");
                all.extend(got.iter().cloned());
                start += got.len();
                if let Some(t) = page.get("nextPageToken").and_then(Value::as_str) {
                    token = Some(t.to_owned());
                    continue;
                }
                let total = page.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                if got.is_empty()
                    || start >= total
                    || page.get("isLast") == Some(&Value::Bool(true))
                {
                    break;
                }
            }
            all
        }
    };
    let mut out = String::from("<Jira>");
    for mut issue in issues {
        // API v3 : description au format ADF, ramenée à son texte.
        if let Some(d) = issue.pointer_mut("/fields/description") {
            if d.is_object() {
                *d = Value::String(adf_text(d));
            }
        }
        attributes("issues", &issue, &mut out);
    }
    out.push_str("</Jira>");
    Ok(out)
}

/// Texte d'un document Atlassian (ADF) : paragraphes séparés par un saut de ligne.
fn adf_text(v: &Value) -> String {
    fn walk(v: &Value, out: &mut String) {
        if let Some(t) = v.get("text").and_then(Value::as_str) {
            out.push_str(t);
        }
        for c in array(v, "content") {
            walk(c, out);
        }
        if matches!(
            v.get("type").and_then(Value::as_str),
            Some("paragraph" | "heading" | "listItem")
        ) {
            out.push('\n');
        }
    }
    let mut out = String::new();
    walk(v, &mut out);
    out.trim_end().to_owned()
}

// ------------------------------------------------------------------ Redmine

/// Enregistrement en forme « mixte » : l'élément racine garde ses membres en
/// éléments enfants, même s'ils sont tous simples.
fn mixed_record(tag: &str, value: &Value, out: &mut String) {
    match value.as_object() {
        Some(map) => {
            out.push_str(&format!("<{tag}>"));
            for (k, v) in map {
                mixed(k, v, out);
            }
            out.push_str(&format!("</{tag}>"));
        }
        None => mixed(tag, value, out),
    }
}

/// Forme « mixte » : membres simples en éléments, objets simples (références
/// `{id, name}`) en éléments à attributs, comme l'API XML de Redmine.
pub fn mixed(tag: &str, value: &Value, out: &mut String) {
    let tag = name(tag);
    match value {
        Value::Array(items) => {
            for i in items {
                mixed(&tag, i, out);
            }
        }
        Value::Object(map) if map.values().all(|v| scalar(v).is_some()) && !map.is_empty() => {
            attributes(&tag, value, out);
        }
        Value::Object(map) => {
            out.push_str(&format!("<{tag}>"));
            for (k, v) in map {
                mixed(k, v, out);
            }
            out.push_str(&format!("</{tag}>"));
        }
        v => elements(&tag, v, out),
    }
}

/// Redmine : paramètres de la liste des demandes (`project_id=thermo&tracker_id=1`).
/// Forme `<issues><issue><id/><subject/><description/><priority name="…"/>`.
pub fn redmine(location: &str, ctx: &Context) -> Result<String> {
    match source(location, ctx)? {
        Source::File(text, path) => {
            if is_xml(&text) {
                return Ok(without_declaration(&text).to_owned());
            }
            let v = parse_json(&text, &path)?;
            let mut out = String::from("<issues>");
            for issue in array(&v, "issues") {
                mixed_record("issue", issue, &mut out);
            }
            out.push_str("</issues>");
            Ok(out)
        }
        Source::Server(p, query) => {
            let auth = match p.token() {
                Some(t) if p.option("auth").is_none() => {
                    Auth::Header("X-Redmine-API-Key".into(), t)
                }
                _ => p.auth(),
            };
            let mut out = String::from("<issues>");
            let mut offset = 0usize;
            loop {
                let url = format!(
                    "{}/issues.xml?{query}{}offset={offset}&limit=100",
                    p.server,
                    if query.is_empty() { "" } else { "&" }
                );
                let text = http::fetch(&Request {
                    url: &url,
                    auth: auth.clone(),
                    ..Default::default()
                })?
                .text();
                let doc = roxmltree::Document::parse(&text)
                    .map_err(|e| ConnectError::msg(format!("{url} : {e}")))?;
                let root = doc.root_element();
                let mut count = 0;
                for issue in root.children().filter(|n| n.has_tag_name("issue")) {
                    out.push_str(&text[issue.range()]);
                    count += 1;
                }
                offset += count;
                let total: usize = root
                    .attribute("total_count")
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                if count == 0 || offset >= total {
                    break;
                }
            }
            out.push_str("</issues>");
            Ok(out)
        }
    }
}

// ----------------------------------------------------------------- Bugzilla

/// Bugzilla : paramètres de recherche (`product=Thermostat`). Forme
/// `<Bug><id/><summary/><comment/><priority/>…</Bug>` ; `comment` est la
/// description (premier commentaire), sauf avec le paramètre `comments=0`.
pub fn bugzilla(location: &str, ctx: &Context) -> Result<String> {
    let bugs: Vec<Value> = match source(location, ctx)? {
        Source::File(text, path) => {
            if is_xml(&text) {
                return Ok(without_declaration(&text).to_owned());
            }
            array(&parse_json(&text, &path)?, "bugs").to_vec()
        }
        Source::Server(p, query) => {
            let auth = match p.token() {
                Some(t) if p.option("auth").is_none() => {
                    Auth::Header("X-BUGZILLA-API-KEY".into(), t)
                }
                _ => p.auth(),
            };
            let mut all = Vec::new();
            let mut offset = 0;
            loop {
                let url = format!(
                    "{}/rest/bug?{query}{}limit=100&offset={offset}",
                    p.server,
                    if query.is_empty() { "" } else { "&" }
                );
                let page = http::get_json(&url, &auth)?;
                let got = array(&page, "bugs").to_vec();
                offset += got.len();
                let done = got.len() < 100;
                all.extend(got);
                if done {
                    break;
                }
            }
            if ctx.param("comments") != Some("0") {
                for bug in &mut all {
                    let Some(id) = bug.get("id").and_then(scalar) else {
                        continue;
                    };
                    let c = http::get_json(&format!("{}/rest/bug/{id}/comment", p.server), &auth)?;
                    let first = c
                        .pointer(&format!("/bugs/{id}/comments/0/text"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    if let Some(map) = bug.as_object_mut() {
                        map.insert("comment".into(), Value::String(first));
                    }
                }
            }
            all
        }
    };
    let mut out = String::from("<bugzilla>");
    for bug in bugs {
        mixed_record("Bug", &bug, &mut out);
    }
    out.push_str("</bugzilla>");
    Ok(out)
}

// --------------------------------------------------------------- codebeamer

/// codebeamer : requête cbQL (`tracker.id IN (1234)`). Forme
/// `<items id="…" name="…" description="…"><parent id="…"/><status name="…"/>`.
pub fn codebeamer(location: &str, ctx: &Context) -> Result<String> {
    let items: Vec<Value> = match source(location, ctx)? {
        Source::File(text, path) => {
            let v = parse_json(&text, &path)?;
            match v {
                Value::Array(a) => a,
                v => array(&v, "items").to_vec(),
            }
        }
        Source::Server(p, query) => {
            let auth = p.auth();
            let mut all = Vec::new();
            let mut page = 1;
            loop {
                let url = format!(
                    "{}/api/v3/items/query?queryString={}&page={page}&pageSize=100",
                    p.server,
                    encode(query)
                );
                let v = http::get_json(&url, &auth)?;
                let got = array(&v, "items").to_vec();
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
                all.extend(got.iter().cloned());
                if got.is_empty() || all.len() >= total {
                    break;
                }
                page += 1;
            }
            all
        }
    };
    let mut out = String::from("<codeBeamer>");
    for item in items {
        attributes("items", &item, &mut out);
    }
    out.push_str("</codeBeamer>");
    Ok(out)
}

// --------------------------------------------------------------------- Jama

/// Jama : `project=<id>` (ou `filter=<id>`). Forme
/// `<Item id name itemType description parent><Field label value/>
/// <Upstreamrelationship from type/></Item>` ; `name` est la clé du document
/// (`documentKey`), `title` le nom de l'élément.
pub fn jama(location: &str, ctx: &Context) -> Result<String> {
    let (items, relations, types) = match source(location, ctx)? {
        Source::File(text, path) => {
            let v = parse_json(&text, &path)?;
            (
                array(&v, "data").to_vec(),
                array(&v, "relationships").to_vec(),
                array(&v, "itemTypes").to_vec(),
            )
        }
        Source::Server(p, query) => {
            let auth = p.auth();
            let api = format!("{}/rest/v1", p.server);
            let paged = |path: String| -> Result<Vec<Value>> {
                let mut all = Vec::new();
                let mut start = 0;
                loop {
                    let sep = if path.contains('?') { '&' } else { '?' };
                    let v = http::get_json(
                        &format!("{api}{path}{sep}startAt={start}&maxResults=50"),
                        &auth,
                    )?;
                    let got = array(&v, "data").to_vec();
                    let total = v
                        .pointer("/meta/pageInfo/totalResults")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    start += got.len();
                    all.extend(got.iter().cloned());
                    if got.is_empty() || start >= total {
                        break;
                    }
                }
                Ok(all)
            };
            let items = match query.split_once('=') {
                Some(("filter", id)) => paged(format!("/filters/{id}/results"))?,
                _ => paged(format!("/items?{query}"))?,
            };
            let relations = match query.split_once('=') {
                Some(("project", id)) => paged(format!("/relationships?project={id}"))?,
                _ => Vec::new(),
            };
            let types = paged("/itemtypes".into())?;
            (items, relations, types)
        }
    };
    let type_name = |id: &Value| -> String {
        types
            .iter()
            .find(|t| t.get("id") == Some(id))
            .and_then(|t| t.get("display").and_then(Value::as_str))
            .map(str::to_owned)
            .unwrap_or_else(|| scalar(id).unwrap_or_default())
    };
    let key_of = |id: &Value| -> String {
        items
            .iter()
            .find(|i| i.get("id") == Some(id))
            .and_then(|i| i.get("documentKey").and_then(scalar))
            .unwrap_or_else(|| scalar(id).unwrap_or_default())
    };
    let rel_types: Vec<(Value, String)> = Vec::new();
    let mut out = String::from("<Jama>");
    for item in &items {
        let fields = item.get("fields").cloned().unwrap_or(Value::Null);
        let field = |k: &str| fields.get(k).and_then(scalar).unwrap_or_default();
        let parent = item
            .pointer("/location/parent/item")
            .map(key_of)
            .unwrap_or_default();
        out.push_str(&format!(
            "<Item id=\"{}\" name=\"{}\" title=\"{}\" itemType=\"{}\" description=\"{}\" parent=\"{}\">",
            xml_escape(&item.get("id").and_then(scalar).unwrap_or_default()),
            xml_escape(&item.get("documentKey").and_then(scalar).unwrap_or_default()),
            xml_escape(&field("name")),
            xml_escape(&item.get("itemType").map(type_name).unwrap_or_default()),
            xml_escape(&html_text(&field("description"))),
            xml_escape(&parent),
        ));
        if let Some(map) = fields.as_object() {
            for (k, v) in map {
                if let Some(s) = scalar(v) {
                    out.push_str(&format!(
                        "<Field label=\"{}\" value=\"{}\"/>",
                        xml_escape(k),
                        xml_escape(&html_text(&s))
                    ));
                }
            }
        }
        for r in relations
            .iter()
            .filter(|r| r.get("toItem") == item.get("id"))
        {
            let kind = r
                .get("relationshipType")
                .map(|t| {
                    rel_types
                        .iter()
                        .find(|(id, _)| id == t)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| scalar(t).unwrap_or_default())
                })
                .unwrap_or_default();
            out.push_str(&format!(
                "<Upstreamrelationship from=\"{}\" type=\"{}\"/>",
                xml_escape(&r.get("fromItem").map(key_of).unwrap_or_default()),
                xml_escape(&kind)
            ));
        }
        out.push_str("</Item>");
    }
    out.push_str("</Jama>");
    Ok(out)
}

/// Texte d'un fragment HTML (descriptions riches des outils ALM).
pub fn html_text(s: &str) -> String {
    if !s.contains('<') {
        return s.to_owned();
    }
    let lines = rc_convert::html_to_lines(s);
    lines
        .lines()
        .map(|l| l.split_once('\t').map(|(_, t)| t).unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
}

// ----------------------------------------------------------------- Polarion

/// Polarion (API REST) : requête Lucene, paramètre `project`. Forme
/// `<WorkItem id title type status … description outlineNumber
/// linkedWorkItems="parent={projet}ID,verifies={projet}ID"><Hyperlink uri/>`.
pub fn polarion(location: &str, ctx: &Context) -> Result<String> {
    let items: Vec<Value> = match source(location, ctx)? {
        Source::File(text, path) => array(&parse_json(&text, &path)?, "data").to_vec(),
        Source::Server(p, query) => {
            let project = ctx
                .param("project")
                .ok_or_else(|| ConnectError::msg("paramètre « project » requis (Polarion)"))?;
            let auth = p.auth();
            let mut all = Vec::new();
            let mut number = 1;
            loop {
                let url = format!(
                    "{}/polarion/rest/v1/projects/{}/workitems?query={}&fields%5Bworkitems%5D=%40all&page%5Bsize%5D=100&page%5Bnumber%5D={number}",
                    p.server,
                    encode(project),
                    encode(query)
                );
                let v = http::get_json(&url, &auth)?;
                let got = array(&v, "data").to_vec();
                let more = v.pointer("/links/next").is_some() && !got.is_empty();
                all.extend(got);
                if !more {
                    break;
                }
                number += 1;
            }
            all
        }
    };
    let mut out = String::from("<Polarion>");
    for wi in items {
        let a = wi.get("attributes").cloned().unwrap_or(Value::Null);
        let attr = |k: &str| -> String {
            match a.get(k) {
                Some(Value::Object(o)) => o
                    .get("value")
                    .and_then(scalar)
                    .map(|v| html_text(&v))
                    .unwrap_or_default(),
                Some(v) => scalar(v).unwrap_or_default(),
                None => String::new(),
            }
        };
        let author = wi
            .pointer("/relationships/author/data/id")
            .and_then(scalar)
            .unwrap_or_default();
        // Identifiant REST d'un lien : projet/élément/rôle/projet cible/élément cible.
        let links: Vec<String> = wi
            .pointer("/relationships/linkedWorkItems/data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|l| l.get("id").and_then(Value::as_str))
            .filter_map(|id| {
                let parts: Vec<&str> = id.split('/').collect();
                (parts.len() >= 5).then(|| format!("{}={{{}}}{}", parts[2], parts[3], parts[4]))
            })
            .collect();
        out.push_str(&format!(
            "<WorkItem id=\"{}\" title=\"{}\" type=\"{}\" status=\"{}\" priority=\"{}\" severity=\"{}\" resolution=\"{}\" author=\"{}\" outlineNumber=\"{}\" description=\"{}\" linkedWorkItems=\"{}\">",
            xml_escape(&attr("id")),
            xml_escape(&attr("title")),
            xml_escape(&attr("type")),
            xml_escape(&attr("status")),
            xml_escape(&attr("priority")),
            xml_escape(&attr("severity")),
            xml_escape(&attr("resolution")),
            xml_escape(&author),
            xml_escape(&attr("outlineNumber")),
            xml_escape(&attr("description")),
            xml_escape(&links.join(",")),
        ));
        for h in a
            .get("hyperlinks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(uri) = h.get("uri").and_then(Value::as_str) {
                out.push_str(&format!("<Hyperlink uri=\"{}\"/>", xml_escape(uri)));
            }
        }
        out.push_str("</WorkItem>");
    }
    out.push_str("</Polarion>");
    Ok(out)
}

// --------------------------------------------------------------- DOORS Next

/// DOORS Next : URL de l'API de publication (`/rm/publish/modules?moduleURI=…`),
/// dont le XML (`ds:artifact`, `rrm:identifier`…) est repris tel quel.
/// Authentification par formulaire Jazz si le serveur la demande.
pub fn doors_next(location: &str, ctx: &Context) -> Result<String> {
    match source(location, ctx)? {
        Source::File(text, _) => Ok(without_declaration(&text).to_owned()),
        Source::Server(p, query) => {
            let url = absolute(&p.server, query);
            std::fs::create_dir_all(&ctx.work_dir)
                .map_err(|e| ConnectError::io(&ctx.work_dir, e))?;
            let jar = ctx.work_dir.join("cookies.txt");
            let request = Request {
                url: &url,
                headers: vec![("Accept".into(), "application/xml".into())],
                cookie_jar: Some(&jar),
                ..Default::default()
            };
            let mut resp = http::send(&request)?;
            if resp.header("X-com-ibm-team-repository-web-auth-msg") == Some("authrequired") {
                let (Some(user), Some(password)) = (&p.user, p.password().or_else(|| p.token()))
                else {
                    return Err(ConnectError::msg(
                        "DOORS Next demande une authentification : renseigner user et passwordEnv",
                    ));
                };
                let login = format!(
                    "{}{}",
                    p.server,
                    p.option("authPath").unwrap_or("/rm/j_security_check")
                );
                let body = format!(
                    "j_username={}&j_password={}",
                    encode(user),
                    encode(&password)
                );
                http::send(&Request {
                    url: &login,
                    method: Some("POST"),
                    headers: vec![(
                        "Content-Type".into(),
                        "application/x-www-form-urlencoded".into(),
                    )],
                    body: Some(body.as_bytes()),
                    cookie_jar: Some(&jar),
                    ..Default::default()
                })?;
                resp = http::send(&request)?;
            }
            let _ = std::fs::remove_file(&jar);
            if !(200..300).contains(&resp.status) {
                return Err(ConnectError::Http {
                    status: resp.status,
                    url,
                    body: resp.text().chars().take(500).collect(),
                });
            }
            Ok(without_declaration(&resp.text()).to_owned())
        }
    }
}

fn absolute(server: &str, path: &str) -> String {
    if path.contains("://") {
        path.to_owned()
    } else {
        format!(
            "{server}{}{path}",
            if path.starts_with('/') { "" } else { "/" }
        )
    }
}

// --------------------------------------------------------------------- OSLC

/// OSLC Change Management 2.0/3.0 : URL d'une requête (`…?oslc.where=…`).
/// Forme `<WorkItem url title description shortId severity status priority type>`
/// avec `<Link type target/>` pour les exigences et demandes liées.
pub fn oslc_cm(location: &str, ctx: &Context) -> Result<String> {
    let mut pages = Vec::new();
    match source(location, ctx)? {
        Source::File(text, _) => pages.push(text),
        Source::Server(p, query) => {
            let auth = p.auth();
            let mut next = Some(absolute(&p.server, query));
            while let Some(url) = next.take() {
                let text = http::fetch(&Request {
                    url: &url,
                    headers: vec![
                        ("Accept".into(), "application/rdf+xml".into()),
                        ("OSLC-Core-Version".into(), "2.0".into()),
                    ],
                    auth: auth.clone(),
                    ..Default::default()
                })?
                .text();
                next = roxmltree::Document::parse(&text).ok().and_then(|d| {
                    d.descendants()
                        .find(|n| n.tag_name().name() == "nextPage")
                        .and_then(|n| rdf_resource(n))
                        .map(str::to_owned)
                });
                pages.push(text);
            }
        }
    }
    let mut out = String::from("<ChangeManagement>");
    for text in pages {
        let doc =
            roxmltree::Document::parse(&text).map_err(|e| ConnectError::msg(e.to_string()))?;
        for cr in doc.descendants().filter(|n| {
            n.is_element()
                && (n.tag_name().name() == "ChangeRequest"
                    || n.tag_name().name() == "Description"
                        && n.children().any(|c| c.tag_name().name() == "identifier"))
        }) {
            let prop = |local: &str| -> String {
                cr.children()
                    .find(|c| c.is_element() && c.tag_name().name() == local)
                    .map(|c| {
                        c.text()
                            .map(str::to_owned)
                            .or_else(|| {
                                rdf_resource(c)
                                    .map(|r| r.rsplit(['/', '#']).next().unwrap_or(r).to_owned())
                            })
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
            };
            out.push_str(&format!(
                "<WorkItem url=\"{}\" title=\"{}\" description=\"{}\" shortId=\"{}\" severity=\"{}\" status=\"{}\" priority=\"{}\" type=\"{}\">",
                xml_escape(about(cr).unwrap_or("")),
                xml_escape(&html_text(&prop("title"))),
                xml_escape(&html_text(&prop("description"))),
                xml_escape(&prop("identifier")),
                xml_escape(&prop("severity")),
                xml_escape(&prop("status")),
                xml_escape(&prop("priority")),
                xml_escape(&prop("type")),
            ));
            for link in cr.children().filter(|c| {
                c.is_element()
                    && matches!(
                        c.tag_name().name(),
                        "implementsRequirement"
                            | "affectsRequirement"
                            | "tracksRequirement"
                            | "relatedChangeRequest"
                            | "affectedByDefect"
                            | "testedByTestCase"
                    )
            }) {
                if let Some(target) = rdf_resource(link) {
                    out.push_str(&format!(
                        "<Link type=\"{}\" target=\"{}\"/>",
                        link.tag_name().name(),
                        xml_escape(target)
                    ));
                }
            }
            out.push_str("</WorkItem>");
        }
    }
    out.push_str("</ChangeManagement>");
    Ok(out)
}

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

fn rdf_resource<'a>(n: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    n.attribute((RDF, "resource"))
}

fn about<'a>(n: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    n.attribute((RDF, "about"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock;
    use crate::Profiles;
    use std::path::Path;

    fn ctx<'a>(
        base: &'a Path,
        profiles: &'a Profiles,
        params: &'a [(String, String)],
    ) -> Context<'a> {
        Context {
            base_dir: base,
            work_dir: std::env::temp_dir().join(format!("rc-alm-{}", std::process::id())),
            profiles,
            params,
        }
    }

    fn with_server(url: &str) -> (Profiles, Vec<(String, String)>) {
        std::env::set_var("RC_TEST_ALM_TOKEN", "jeton");
        (
            Profiles::parse(&format!(
                "[srv]\nserver={url}\ntokenEnv=RC_TEST_ALM_TOKEN\n"
            )),
            vec![("profile".into(), "srv".into())],
        )
    }

    #[test]
    fn jira_paginates_and_flattens_adf() {
        let page1 = r#"{"startAt":0,"total":2,"issues":[{"key":"THS-1","fields":{"summary":"Démarrer","priority":{"name":"High"},"description":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"[Covers: SYS-1]"}]}]}}}]}"#;
        let page2 = r#"{"startAt":1,"total":2,"issues":[{"key":"THS-2","fields":{"summary":"Arrêter","description":null}}]}"#;
        let m = mock::serve(vec![
            ("/rest/api/2/search?jql=project%20%3D%20THS&maxResults=100&fields=*navigable&startAt=0".into(), 200, "application/json", page1.into()),
            ("/rest/api/2/search?jql=project%20%3D%20THS&maxResults=100&fields=*navigable&startAt=1".into(), 200, "application/json", page2.into()),
        ]);
        let (profiles, params) = with_server(&m.url);
        let out = jira("project = THS", &ctx(Path::new("."), &profiles, &params)).unwrap();
        assert_eq!(
            out,
            "<Jira><issues key=\"THS-1\"><fields summary=\"Démarrer\" description=\"[Covers: SYS-1]\"><priority name=\"High\"/></fields></issues>\
             <issues key=\"THS-2\"><fields summary=\"Arrêter\" description=\"\"/></issues></Jira>"
        );
        assert!(m.requests.lock().unwrap()[0].contains("Authorization: Bearer jeton"));
    }

    #[test]
    fn redmine_merges_xml_pages_with_api_key() {
        let p1 = r#"<?xml version="1.0"?><issues total_count="2" offset="0" limit="1" type="array"><issue><id>1</id><subject>A</subject><priority id="2" name="Normal"/></issue></issues>"#;
        let p2 = r#"<issues total_count="2" offset="1" limit="1" type="array"><issue><id>2</id><subject>B</subject></issue></issues>"#;
        let m = mock::serve(vec![
            (
                "/issues.xml?project_id=ths&offset=0".into(),
                200,
                "application/xml",
                p1.into(),
            ),
            (
                "/issues.xml?project_id=ths&offset=1".into(),
                200,
                "application/xml",
                p2.into(),
            ),
        ]);
        let (profiles, params) = with_server(&m.url);
        let out = redmine("project_id=ths", &ctx(Path::new("."), &profiles, &params)).unwrap();
        assert_eq!(
            out,
            "<issues><issue><id>1</id><subject>A</subject><priority id=\"2\" name=\"Normal\"/></issue><issue><id>2</id><subject>B</subject></issue></issues>"
        );
        assert!(m.requests.lock().unwrap()[0].contains("X-Redmine-API-Key: jeton"));
    }

    #[test]
    fn offline_files_are_converted_without_server() {
        let dir = std::env::temp_dir().join(format!("rc-alm-off-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("bugs.json"),
            r#"{"bugs":[{"id":7,"summary":"Capteur","whiteboard":"THS_REQ1","keywords":["k"],"assigned_to_detail":{"name":"bob"}}]}"#,
        )
        .unwrap();
        let profiles = Profiles::default();
        let c = ctx(&dir, &profiles, &[]);
        assert_eq!(
            bugzilla("bugs.json", &c).unwrap(),
            "<bugzilla><Bug><id>7</id><summary>Capteur</summary><whiteboard>THS_REQ1</whiteboard><keywords>k</keywords><assigned_to_detail name=\"bob\"/></Bug></bugzilla>"
        );
        std::fs::write(
            dir.join("wi.json"),
            r#"{"data":[{"type":"workitems","id":"P/WI-2","attributes":{"id":"WI-2","title":"Test","type":"testcase","description":{"type":"text/html","value":"<p>Vérifie <b>WI-1</b></p>"},"hyperlinks":[{"uri":"http://x"}]},"relationships":{"linkedWorkItems":{"data":[{"type":"linkedworkitems","id":"P/WI-2/verifies/P/WI-1"}]}}}]}"#,
        )
        .unwrap();
        assert_eq!(
            polarion("wi.json", &c).unwrap(),
            "<Polarion><WorkItem id=\"WI-2\" title=\"Test\" type=\"testcase\" status=\"\" priority=\"\" severity=\"\" resolution=\"\" author=\"\" outlineNumber=\"\" description=\"Vérifie WI-1\" linkedWorkItems=\"verifies={P}WI-1\"><Hyperlink uri=\"http://x\"/></WorkItem></Polarion>"
        );
        std::fs::write(
            dir.join("jama.json"),
            r#"{"data":[{"id":10,"documentKey":"THS-REQ-1","itemType":33,"fields":{"name":"Démarrer","description":"<p>Le système démarre</p>","status":"Approved"}},{"id":11,"documentKey":"THS-TC-1","itemType":34,"fields":{"name":"Test"}}],
                "relationships":[{"id":1,"fromItem":10,"toItem":11,"relationshipType":5}],
                "itemTypes":[{"id":33,"display":"Requirement"},{"id":34,"display":"Test Case"}]}"#,
        )
        .unwrap();
        let out = jama("jama.json", &c).unwrap();
        assert!(out.contains("<Item id=\"10\" name=\"THS-REQ-1\" title=\"Démarrer\" itemType=\"Requirement\" description=\"Le système démarre\" parent=\"\"><Field label=\"name\" value=\"Démarrer\"/>"), "{out}");
        assert!(out.contains("<Item id=\"11\" name=\"THS-TC-1\" title=\"Test\" itemType=\"Test Case\" description=\"\" parent=\"\"><Field label=\"name\" value=\"Test\"/><Upstreamrelationship from=\"THS-REQ-1\" type=\"5\"/></Item>"), "{out}");
    }

    #[test]
    fn oslc_reads_rdf_change_requests() {
        let rdf = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:oslc_cm="http://open-services.net/ns/cm#" xmlns:oslc_cmx="http://open-services.net/ns/cm-x#">
          <oslc_cm:ChangeRequest rdf:about="https://ccm/wi/42"><dcterms:identifier>42</dcterms:identifier><dcterms:title>Corriger la mesure</dcterms:title>
            <oslc_cm:status>Open</oslc_cm:status><oslc_cmx:severity rdf:resource="https://ccm/sev#major"/>
            <oslc_cm:implementsRequirement rdf:resource="https://rm/THS_REQ1"/></oslc_cm:ChangeRequest></rdf:RDF>"#;
        let dir = std::env::temp_dir().join(format!("rc-oslc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("q.rdf"), rdf).unwrap();
        let profiles = Profiles::default();
        assert_eq!(
            oslc_cm("q.rdf", &ctx(&dir, &profiles, &[])).unwrap(),
            "<ChangeManagement><WorkItem url=\"https://ccm/wi/42\" title=\"Corriger la mesure\" description=\"\" shortId=\"42\" severity=\"major\" status=\"Open\" priority=\"\" type=\"\"><Link type=\"implementsRequirement\" target=\"https://rm/THS_REQ1\"/></WorkItem></ChangeManagement>"
        );
    }
}
