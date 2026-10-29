//! Connecteurs vers des sources qui ne sont pas un simple fichier local.
//!
//! Deux familles, comme dans l'outil d'origine (`docs/interop-formats.md` §12) :
//!
//! - les **types d'accès** ([`access`]) rapatrient une copie locale (archive ZIP,
//!   URL HTTP/FTP, dépôt Git, Subversion, CVS, Perforce, Google Drive, OneDrive,
//!   commande externe), qui est ensuite convertie par le type d'analyse du
//!   document ;
//! - les **connecteurs** ([`Connector`]) interrogent directement un outil
//!   (Jira, Redmine, Bugzilla, codebeamer, Jama, Polarion, DOORS Next, OSLC,
//!   journaux Git/Subversion/Perforce, bases de données) et produisent un
//!   intermédiaire XML.
//!
//! Les échanges HTTP passent par `curl` ([`http`]) ; les identifiants viennent
//! des profils de connexion ([`Profiles`]), jamais du fichier de projet.

pub mod access;
pub mod alm;
pub mod command;
pub mod db;
pub mod http;
pub mod vcs;
pub mod xml;

use rc_convert::Intermediate;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("{0}")]
    Message(String),
    #[error("outil « {tool} » introuvable ou en échec : {detail}")]
    Tool { tool: String, detail: String },
    #[error("HTTP {status} sur {url} : {body}")]
    Http {
        status: u16,
        url: String,
        body: String,
    },
    #[error("profil de connexion « {0} » introuvable")]
    Profile(String),
    #[error(transparent)]
    Convert(#[from] rc_convert::ConvertError),
    #[error("{path} : {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl ConnectError {
    pub fn msg(text: impl Into<String>) -> ConnectError {
        ConnectError::Message(text.into())
    }

    pub(crate) fn io(path: &Path, source: std::io::Error) -> ConnectError {
        ConnectError::Io {
            path: path.to_owned(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, ConnectError>;

/// Profil de connexion : serveur et identifiants. Les secrets ne sont jamais
/// écrits dans le fichier de profils : celui-ci nomme la variable
/// d'environnement qui les contient (`tokenEnv`, `passwordEnv`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profile {
    pub name: String,
    pub server: String,
    pub user: Option<String>,
    pub token_env: Option<String>,
    pub password_env: Option<String>,
    /// Autres réglages (`command`, `database`, `auth`…).
    pub options: Vec<(String, String)>,
}

impl Profile {
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn token(&self) -> Option<String> {
        self.token_env
            .as_deref()
            .and_then(|v| std::env::var(v).ok())
            .filter(|t| !t.is_empty())
    }

    pub fn password(&self) -> Option<String> {
        self.password_env
            .as_deref()
            .and_then(|v| std::env::var(v).ok())
    }

    /// Authentification HTTP déduite du profil.
    pub fn auth(&self) -> http::Auth {
        match (
            self.option("auth"),
            self.token(),
            &self.user,
            self.password(),
        ) {
            (Some("basic"), Some(token), Some(user), _) => http::Auth::Basic {
                user: user.clone(),
                password: token,
            },
            (Some(header), Some(token), _, _) if header.starts_with("header:") => {
                http::Auth::Header(header["header:".len()..].to_owned(), token)
            }
            (_, Some(token), _, _) => http::Auth::Bearer(token),
            (_, None, Some(user), Some(password)) => http::Auth::Basic {
                user: user.clone(),
                password,
            },
            _ => http::Auth::None,
        }
    }
}

/// Profils de connexion, lus dans des fichiers INI :
///
/// ```ini
/// [jira-prod]
/// server=https://jira.example.com
/// user=alice
/// tokenEnv=JIRA_TOKEN
/// ```
#[derive(Debug, Clone, Default)]
pub struct Profiles {
    pub profiles: Vec<Profile>,
}

impl Profiles {
    pub fn parse(text: &str) -> Profiles {
        let ini = rc_interop::ini::Ini::parse(text);
        let profiles = ini
            .sections
            .iter()
            .filter(|s| !s.name.is_empty())
            .map(|s| {
                let mut p = Profile {
                    name: s.name.clone(),
                    ..Default::default()
                };
                for (k, v) in &s.entries {
                    let v = v.trim().to_owned();
                    match k.to_ascii_lowercase().as_str() {
                        "server" | "url" => p.server = v.trim_end_matches('/').to_owned(),
                        "user" | "login" => p.user = Some(v),
                        "tokenenv" => p.token_env = Some(v),
                        "passwordenv" => p.password_env = Some(v),
                        _ => p.options.push((k.clone(), v)),
                    }
                }
                p
            })
            .collect();
        Profiles { profiles }
    }

    /// Profils du projet (`reqticraft-profiles.ini` à côté du projet), puis de
    /// l'utilisateur (`~/.reqticraft/profiles.ini`), puis du fichier désigné par
    /// `REQTICRAFT_PROFILES`.
    pub fn load(project_dir: &Path) -> Profiles {
        let mut files = vec![project_dir.join("reqticraft-profiles.ini")];
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            files.push(PathBuf::from(home).join(".reqticraft").join("profiles.ini"));
        }
        if let Some(extra) = std::env::var_os("REQTICRAFT_PROFILES") {
            files.push(PathBuf::from(extra));
        }
        let mut all = Profiles::default();
        for f in files {
            if let Ok(text) = rc_interop::read_text(&f) {
                all.profiles.extend(Profiles::parse(&text).profiles);
            }
        }
        all
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }
}

/// Contexte d'un document à rapatrier ou à interroger.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// Répertoire du projet.
    pub base_dir: &'a Path,
    /// Répertoire de travail réservé au document (copies locales, caches).
    pub work_dir: PathBuf,
    pub profiles: &'a Profiles,
    /// Paramètres du document.
    pub params: &'a [(String, String)],
}

impl Context<'_> {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.trim().is_empty())
    }

    /// Profil désigné par le paramètre `profile` (obligatoire si `required`).
    pub fn profile(&self, required: bool) -> Result<Option<&Profile>> {
        match self.param("profile") {
            Some(name) => self
                .profiles
                .get(name)
                .map(Some)
                .ok_or_else(|| ConnectError::Profile(name.to_owned())),
            None if required => Err(ConnectError::msg(
                "paramètre « profile » requis : nom d'un profil de connexion",
            )),
            None => Ok(None),
        }
    }
}

/// Connecteurs qui produisent eux-mêmes l'intermédiaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connector {
    Jira,
    Redmine,
    Bugzilla,
    CodeBeamer,
    Jama,
    Polarion,
    DoorsNext,
    /// OSLC Change Management (gestion des modifications).
    Oslc,
    GitLog,
    SvnLog,
    P4Log,
    Sql,
    Command,
}

impl Connector {
    /// Connecteur correspondant à un `convertTool`.
    pub fn for_tool(tool: &str) -> Option<Connector> {
        Some(match tool.to_ascii_lowercase().as_str() {
            "jira" => Connector::Jira,
            "redmine" => Connector::Redmine,
            "bugzilla" => Connector::Bugzilla,
            "codebeamer" => Connector::CodeBeamer,
            "jamarest" | "jama" | "jamafilter" => Connector::Jama,
            "polarion" => Connector::Polarion,
            "doorsng" | "doorsngview" | "doorsnext" => Connector::DoorsNext,
            "ccm" | "oslc" | "oslccm" => Connector::Oslc,
            "gitlog" | "gitlogxml" => Connector::GitLog,
            "svnlogxml" | "svnlog" => Connector::SvnLog,
            "p4log" | "perforcelog" => Connector::P4Log,
            "accessxml" | "oracle" | "sql" | "odbc" | "database" => Connector::Sql,
            "command" | "externalcommand" => Connector::Command,
            _ => return None,
        })
    }

    /// Le document désigne-t-il une requête plutôt qu'un fichier ?
    pub fn is_query(self) -> bool {
        !matches!(
            self,
            Connector::GitLog | Connector::SvnLog | Connector::P4Log
        )
    }

    /// Produit l'intermédiaire du document situé en `location` (requête,
    /// URL ou chemin selon le connecteur).
    pub fn run(self, location: &str, ctx: &Context) -> Result<Intermediate> {
        match self {
            Connector::Jira => alm::jira(location, ctx).map(Intermediate::Xml),
            Connector::Redmine => alm::redmine(location, ctx).map(Intermediate::Xml),
            Connector::Bugzilla => alm::bugzilla(location, ctx).map(Intermediate::Xml),
            Connector::CodeBeamer => alm::codebeamer(location, ctx).map(Intermediate::Xml),
            Connector::Jama => alm::jama(location, ctx).map(Intermediate::Xml),
            Connector::Polarion => alm::polarion(location, ctx).map(Intermediate::Xml),
            Connector::DoorsNext => alm::doors_next(location, ctx).map(Intermediate::Xml),
            Connector::Oslc => alm::oslc_cm(location, ctx).map(Intermediate::Xml),
            Connector::GitLog => vcs::git_log(location, ctx).map(Intermediate::Xml),
            Connector::SvnLog => vcs::svn_log(location, ctx).map(Intermediate::Xml),
            Connector::P4Log => vcs::p4_log(location, ctx).map(Intermediate::Xml),
            Connector::Sql => db::query(location, ctx).map(Intermediate::Xml),
            Connector::Command => command::run(location, ctx),
        }
    }
}

/// Lance un outil externe et renvoie sa sortie standard.
pub(crate) fn run_tool(
    program: &str,
    args: &[&str],
    dir: Option<&Path>,
    env: &[(&str, &str)],
) -> Result<Vec<u8>> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| ConnectError::Tool {
        tool: program.to_owned(),
        detail: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(ConnectError::Tool {
            tool: program.to_owned(),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profiles_without_secrets() {
        let p = Profiles::parse(
            "; profils\n[jira]\nserver=https://jira.example.com/\nuser=alice\ntokenEnv=RC_TEST_TOKEN_UNSET\nauth=basic\n",
        );
        let jira = p.get("jira").unwrap();
        assert_eq!(jira.server, "https://jira.example.com");
        assert_eq!(jira.user.as_deref(), Some("alice"));
        assert_eq!(jira.option("auth"), Some("basic"));
        assert_eq!(jira.token(), None);
        assert_eq!(jira.auth(), http::Auth::None);
    }

    #[test]
    fn maps_reqtify_tools_to_connectors() {
        assert_eq!(Connector::for_tool("JIRA"), Some(Connector::Jira));
        assert_eq!(Connector::for_tool("JamaRest"), Some(Connector::Jama));
        assert_eq!(Connector::for_tool("SVNLogXml"), Some(Connector::SvnLog));
        assert_eq!(Connector::for_tool("AccessXML"), Some(Connector::Sql));
        assert_eq!(Connector::for_tool("Word"), None);
    }
}
