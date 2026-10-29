//! Instantanés : enregistrement daté des résultats d'analyse, comparaison et
//! historique d'un élément.
//!
//! Les instantanés d'un projet sont stockés dans une base SQLite à côté du
//! projet (`<projet>.rcsnap`). Un instantané garde, pour chaque élément
//! structurel identifié, son texte, ses attributs, ses références et son état de
//! couverture : de quoi comparer deux états sans relire les documents.

use rc_analysis::{leaf_requirements, Analysis};
use rc_model::{CapturedDocument, ElementClass, Severity};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("base d'instantanés : {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("instantané : {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// État d'un élément au moment de l'instantané.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementState {
    pub document: String,
    pub type_name: String,
    pub class: ElementClass,
    pub ident: Option<String>,
    pub guid: Option<String>,
    pub label: Option<String>,
    pub text: String,
    /// `covered`, `uncovered`, `derived` ou `none`.
    pub status: String,
    pub attributes: Vec<(String, Option<String>)>,
    pub references: Vec<String>,
    /// Éléments couvrants, `document › nom`.
    pub covered_by: Vec<String>,
}

impl ElementState {
    /// Clé de rapprochement : document, type et GUID (ou identifiant sans casse).
    pub fn key(&self) -> Option<(String, String, String)> {
        let id = self
            .guid
            .clone()
            .or_else(|| self.ident.as_ref().map(|i| i.to_lowercase()))?;
        Some((self.document.clone(), self.type_name.clone(), id))
    }

    pub fn name(&self) -> String {
        match (&self.ident, &self.label) {
            (Some(i), Some(l)) if i != l => format!("{i} {l}"),
            (Some(i), _) => i.clone(),
            (None, Some(l)) => l.clone(),
            (None, None) => self.type_name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentState {
    pub name: String,
    pub requirements: usize,
    pub covered: Option<usize>,
}

/// Résultats d'analyse réduits à ce qu'un instantané conserve.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub documents: Vec<DocumentState>,
    pub elements: Vec<ElementState>,
    pub errors: usize,
    pub warnings: usize,
}

impl State {
    pub fn from_analysis(a: &Analysis) -> State {
        let mut covered_by: HashMap<(usize, usize), Vec<String>> = HashMap::new();
        for l in &a.links {
            if let Some(e) = l.to.element {
                covered_by
                    .entry((l.to.document, e))
                    .or_default()
                    .push(format!(
                        "{} › {}",
                        a.documents[l.from.document].name,
                        a.element_name(l.from)
                    ));
            }
        }
        let mut elements = Vec::new();
        for (d, doc) in a.documents.iter().enumerate() {
            let leaves: std::collections::HashSet<usize> =
                leaf_requirements(doc).into_iter().collect();
            for (e, el) in doc.elements.iter().enumerate() {
                if !el.class.is_structural() || (el.ident.is_none() && el.guid.is_none()) {
                    continue;
                }
                let status = if a.derived.contains(&(d, e)) {
                    "derived"
                } else if a.uncovered.contains(&(d, e)) {
                    "uncovered"
                } else if leaves.contains(&e) && a.summaries[d].covered.is_some() {
                    "covered"
                } else {
                    "none"
                };
                elements.push(ElementState {
                    document: doc.name.clone(),
                    type_name: el.type_name.clone(),
                    class: el.class,
                    ident: el.ident.clone(),
                    guid: el.guid.clone(),
                    label: el.label.clone(),
                    text: element_text(doc, e),
                    status: status.into(),
                    attributes: el
                        .attributes
                        .iter()
                        .map(|x| (x.name.clone(), x.value.clone()))
                        .collect(),
                    references: el.references.iter().map(|r| r.target.clone()).collect(),
                    covered_by: covered_by.remove(&(d, e)).unwrap_or_default(),
                });
            }
        }
        State {
            documents: a
                .documents
                .iter()
                .zip(&a.summaries)
                .map(|(doc, s)| DocumentState {
                    name: doc.name.clone(),
                    requirements: s.requirements,
                    covered: s.covered,
                })
                .collect(),
            elements,
            errors: a.count(Severity::Error),
            warnings: a.count(Severity::Warning),
        }
    }

    /// Exigences couvertes et couvrables, tous documents couverts confondus.
    pub fn coverage(&self) -> (usize, usize) {
        self.documents
            .iter()
            .filter_map(|d| d.covered.map(|c| (c, d.requirements)))
            .fold((0, 0), |(a, b), (c, t)| (a + c, b + t))
    }
}

fn element_text(doc: &CapturedDocument, e: usize) -> String {
    let el = &doc.elements[e];
    let mut parts = vec![el.text.trim().to_owned()];
    parts.extend(
        el.children
            .iter()
            .map(|&c| &doc.elements[c])
            .filter(|c| !c.class.is_structural() && c.class != ElementClass::Reference)
            .map(|c| c.text.trim().to_owned()),
    );
    parts.retain(|p| !p.is_empty());
    parts.dedup();
    parts.join("\n")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: i64,
    pub name: String,
    /// Date UTC, ISO 8601.
    pub created: String,
    pub requirements: usize,
    pub covered: usize,
    pub coverable: usize,
    pub errors: usize,
    pub warnings: usize,
}

/// Base d'instantanés d'un projet.
pub struct Store {
    conn: Connection,
}

/// Fichier de la base d'instantanés d'un projet.
pub fn store_path(project: &Path) -> PathBuf {
    project.with_extension("rcsnap")
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, SnapshotError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snapshot (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 created TEXT NOT NULL,
                 requirements INTEGER NOT NULL,
                 covered INTEGER NOT NULL,
                 coverable INTEGER NOT NULL,
                 errors INTEGER NOT NULL,
                 warnings INTEGER NOT NULL,
                 documents TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS element (
                 snapshot INTEGER NOT NULL REFERENCES snapshot(id) ON DELETE CASCADE,
                 document TEXT NOT NULL,
                 type TEXT NOT NULL,
                 key TEXT NOT NULL,
                 data TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS element_key ON element(document, key);
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(Store { conn })
    }

    pub fn create(&mut self, name: &str, state: &State) -> Result<SnapshotInfo, SnapshotError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SnapshotError::Other(
                "l'instantané doit avoir un nom".into(),
            ));
        }
        if self.find(name)?.is_some() {
            return Err(SnapshotError::Other(format!(
                "un instantané « {name} » existe déjà"
            )));
        }
        let (covered, coverable) = state.coverage();
        let requirements: usize = state.documents.iter().map(|d| d.requirements).sum();
        let created = now_iso();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshot (name, created, requirements, covered, coverable, errors, warnings, documents)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                name,
                created,
                requirements as i64,
                covered as i64,
                coverable as i64,
                state.errors as i64,
                state.warnings as i64,
                serde_json::to_string(&state.documents)?
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut insert = tx.prepare(
                "INSERT INTO element (snapshot, document, type, key, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in &state.elements {
                let Some((doc, ty, key)) = e.key() else {
                    continue;
                };
                insert.execute(params![id, doc, ty, key, serde_json::to_string(e)?])?;
            }
        }
        tx.commit()?;
        Ok(SnapshotInfo {
            id,
            name: name.to_owned(),
            created,
            requirements,
            covered,
            coverable,
            errors: state.errors,
            warnings: state.warnings,
        })
    }

    pub fn list(&self) -> Result<Vec<SnapshotInfo>, SnapshotError> {
        let mut q = self.conn.prepare(
            "SELECT id, name, created, requirements, covered, coverable, errors, warnings
             FROM snapshot ORDER BY created, id",
        )?;
        let rows = q.query_map([], |r| {
            Ok(SnapshotInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                created: r.get(2)?,
                requirements: r.get::<_, i64>(3)? as usize,
                covered: r.get::<_, i64>(4)? as usize,
                coverable: r.get::<_, i64>(5)? as usize,
                errors: r.get::<_, i64>(6)? as usize,
                warnings: r.get::<_, i64>(7)? as usize,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Instantané désigné par son nom ou son numéro.
    pub fn find(&self, name_or_id: &str) -> Result<Option<SnapshotInfo>, SnapshotError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|s| s.name == name_or_id || s.id.to_string() == name_or_id))
    }

    pub fn load(&self, id: i64) -> Result<State, SnapshotError> {
        let row: Option<(String, i64, i64)> = self
            .conn
            .query_row(
                "SELECT documents, errors, warnings FROM snapshot WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((documents, errors, warnings)) = row else {
            return Err(SnapshotError::Other(format!("instantané {id} introuvable")));
        };
        let mut q = self
            .conn
            .prepare("SELECT data FROM element WHERE snapshot = ?1 ORDER BY rowid")?;
        let elements = q
            .query_map([id], |r| r.get::<_, String>(0))?
            .map(|d| Ok(serde_json::from_str(&d?)?))
            .collect::<Result<Vec<ElementState>, SnapshotError>>()?;
        Ok(State {
            documents: serde_json::from_str(&documents)?,
            elements,
            errors: errors as usize,
            warnings: warnings as usize,
        })
    }

    pub fn delete(&mut self, id: i64) -> Result<(), SnapshotError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM element WHERE snapshot = ?1", [id])?;
        tx.execute("DELETE FROM snapshot WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// État d'un élément dans chaque instantané (`None` : absent).
    pub fn history(
        &self,
        document: &str,
        key: &str,
    ) -> Result<Vec<(SnapshotInfo, Option<ElementState>)>, SnapshotError> {
        let mut q = self.conn.prepare(
            "SELECT data FROM element WHERE snapshot = ?1 AND document = ?2 AND key = ?3",
        )?;
        self.list()?
            .into_iter()
            .map(|s| {
                let data: Option<String> = q
                    .query_row(params![s.id, document, key], |r| r.get(0))
                    .optional()?;
                let state = data.map(|d| serde_json::from_str(&d)).transpose()?;
                Ok((s, state))
            })
            .collect()
    }
}

/// Élément modifié entre deux états.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Modified {
    pub before: ElementState,
    pub after: ElementState,
    /// `text`, `label`, `attributes`, `references`, `status`.
    pub fields: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDelta {
    pub name: String,
    pub before: Option<DocumentState>,
    pub after: Option<DocumentState>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub added: Vec<ElementState>,
    pub removed: Vec<ElementState>,
    pub modified: Vec<Modified>,
    /// Exigences couvertes avant et plus maintenant.
    pub coverage_lost: Vec<ElementState>,
    pub coverage_gained: Vec<ElementState>,
    pub documents: Vec<DocumentDelta>,
    pub before: (usize, usize),
    pub after: (usize, usize),
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

/// Compare deux états : `old` (instantané) et `new` (instantané ou état courant).
pub fn diff(old: &State, new: &State) -> Diff {
    let index = |s: &State| -> BTreeMap<(String, String, String), ElementState> {
        s.elements
            .iter()
            .filter_map(|e| Some((e.key()?, e.clone())))
            .collect()
    };
    let (a, b) = (index(old), index(new));
    let mut d = Diff {
        before: old.coverage(),
        after: new.coverage(),
        ..Default::default()
    };
    for (k, after) in &b {
        match a.get(k) {
            None => d.added.push(after.clone()),
            Some(before) => {
                let mut fields = Vec::new();
                if before.text != after.text {
                    fields.push("text");
                }
                if before.label != after.label {
                    fields.push("label");
                }
                if before.attributes != after.attributes {
                    fields.push("attributes");
                }
                if before.references != after.references {
                    fields.push("references");
                }
                if before.status != after.status {
                    fields.push("status");
                    if before.status == "covered" {
                        d.coverage_lost.push(after.clone());
                    } else if after.status == "covered" {
                        d.coverage_gained.push(after.clone());
                    }
                }
                if !fields.is_empty() {
                    d.modified.push(Modified {
                        before: before.clone(),
                        after: after.clone(),
                        fields,
                    });
                }
            }
        }
    }
    d.removed = a
        .iter()
        .filter(|(k, _)| !b.contains_key(*k))
        .map(|(_, e)| e.clone())
        .collect();
    let mut names: Vec<&String> = old.documents.iter().map(|x| &x.name).collect();
    for n in new.documents.iter().map(|x| &x.name) {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    d.documents = names
        .into_iter()
        .map(|n| DocumentDelta {
            name: n.clone(),
            before: old.documents.iter().find(|x| &x.name == n).cloned(),
            after: new.documents.iter().find(|x| &x.name == n).cloned(),
        })
        .collect();
    d
}

/// Date et heure UTC courantes, `AAAA-MM-JJTHH:MM:SSZ`.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Conversion jours → date civile (calendrier grégorien proleptique).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(ident: &str, text: &str, status: &str) -> ElementState {
        ElementState {
            document: "Spec".into(),
            type_name: "Req".into(),
            class: ElementClass::Requirement,
            ident: Some(ident.into()),
            guid: None,
            label: None,
            text: text.into(),
            status: status.into(),
            attributes: Vec::new(),
            references: Vec::new(),
            covered_by: Vec::new(),
        }
    }

    fn state(elements: Vec<ElementState>) -> State {
        let covered = elements.iter().filter(|e| e.status == "covered").count();
        State {
            documents: vec![DocumentState {
                name: "Spec".into(),
                requirements: elements.len(),
                covered: Some(covered),
            }],
            elements,
            errors: 0,
            warnings: 0,
        }
    }

    #[test]
    fn diffs_states() {
        let v1 = state(vec![
            element("R1", "a", "covered"),
            element("R2", "b", "covered"),
            element("R3", "c", "uncovered"),
        ]);
        let v2 = state(vec![
            element("r1", "a", "covered"),
            element("R2", "b2", "uncovered"),
            element("R4", "d", "covered"),
        ]);
        let d = diff(&v1, &v2);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.removed[0].ident.as_deref(), Some("R3"));
        assert_eq!(d.modified.len(), 1);
        assert_eq!(d.modified[0].fields, ["text", "status"]);
        assert_eq!(d.coverage_lost[0].ident.as_deref(), Some("R2"));
        assert_eq!((d.before, d.after), ((2, 3), (2, 3)));
    }

    #[test]
    fn stores_snapshots() {
        let path = std::env::temp_dir().join(format!("rc-snap-{}.rcsnap", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut store = Store::open(&path).unwrap();
        let v1 = state(vec![
            element("R1", "a", "covered"),
            element("R2", "b", "uncovered"),
        ]);
        let info = store.create("V1", &v1).unwrap();
        assert_eq!((info.covered, info.coverable), (1, 2));
        assert!(store.create("V1", &v1).is_err());
        let v2 = state(vec![element("R1", "a2", "covered")]);
        store.create("V2", &v2).unwrap();
        assert_eq!(store.load(info.id).unwrap(), v1);
        let h = store.history("Spec", "r2").unwrap();
        assert_eq!(h.len(), 2);
        assert!(h[0].1.is_some() && h[1].1.is_none());
        store.delete(info.id).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn formats_dates() {
        let d = now_iso();
        assert_eq!(d.len(), 20);
        assert!(d.starts_with("20") && d.ends_with('Z'));
    }
}
