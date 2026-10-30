//! `reqticraft` : analyse de traçabilité sans interface graphique.
//!
//! Sous-commandes : `docs/architecture-proposal.md` §5.6. `report` et `snapshot`
//! ne sont pas encore implémentées et le disent.

mod output;
mod serve;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use rc_core::outputs::{self, rc_report::Format, rc_report::Template, rc_snapshot};
use rc_core::rc_model::{DocumentType, Severity};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "reqticraft", version, about = "Traçabilité des exigences")]
struct Cli {
    /// Répertoire de configuration supplémentaire (contenant `types/`), répétable.
    /// Permet de réutiliser une bibliothèque de types existante.
    #[arg(long = "config", global = true, value_name = "RÉPERTOIRE")]
    config: Vec<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lit un projet Reqtify (.rqtf) et indique ce qui sera repris.
    Import { project: PathBuf },
    /// Analyse un projet : couverture, exigences dérivées, règles.
    Analyze {
        project: PathBuf,
        /// Écrit le résultat complet en JSON.
        #[arg(long, value_name = "FICHIER")]
        json: Option<PathBuf>,
        /// Liste chaque message (sinon : un résumé par règle).
        #[arg(long)]
        details: bool,
        /// Code de sortie 2 si un message atteint ce niveau.
        #[arg(long, value_enum)]
        fail_on: Option<FailOn>,
        /// Écrit aussi un rapport (format d'après l'extension : html, xlsx, docx, csv, md).
        #[arg(long, value_name = "FICHIER")]
        report: Option<PathBuf>,
        /// Modèle du rapport écrit par `--report`.
        #[arg(long, value_enum, default_value = "matrix")]
        template: ReportTemplate,
        /// Exécute les scripts Python du projet (`scripts/*.py`) même s'il n'est pas approuvé.
        #[arg(long)]
        scripts: bool,
    },
    /// Génère un rapport.
    Report {
        project: PathBuf,
        /// Modèle de rapport.
        #[arg(long, value_enum, default_value = "matrix")]
        template: ReportTemplate,
        /// Format (par défaut : d'après l'extension du fichier de sortie).
        #[arg(long, value_enum)]
        format: Option<ReportFormat>,
        /// Fichier de sortie.
        #[arg(short, long, value_name = "FICHIER")]
        output: PathBuf,
    },
    /// Crée, liste ou compare des instantanés (base `<projet>.rcsnap`).
    Snapshot {
        project: PathBuf,
        #[command(subcommand)]
        action: SnapshotAction,
    },
    /// Types d'analyse : liste, ou test d'un type sur un fichier.
    Types {
        #[command(subcommand)]
        action: TypesAction,
    },
    /// Affiche le fichier intermédiaire produit pour un document.
    Convert {
        file: PathBuf,
        /// Type d'analyse (par défaut : d'après l'extension).
        #[arg(long = "type")]
        type_name: Option<String>,
        /// Paramètre du document, `nom=valeur`, répétable.
        #[arg(long = "param", value_parser = parse_param)]
        params: Vec<(String, String)>,
    },
    /// Serveur local de l'interface (développement, accès en lecture).
    #[command(hide = true)]
    Serve {
        /// Projet ouvert au démarrage.
        project: Option<PathBuf>,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Répertoire de l'interface compilée (`app/dist`).
        #[arg(long, value_name = "RÉPERTOIRE")]
        ui: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum SnapshotAction {
    /// Enregistre l'état courant sous un nom.
    Create { name: String },
    /// Liste les instantanés.
    List,
    /// Compare un instantané à l'état courant, ou à un second instantané.
    Diff { from: String, to: Option<String> },
    /// État d'un élément dans chaque instantané.
    History {
        /// Identifiant de l'élément.
        ident: String,
        /// Document de l'élément (par défaut : le premier qui le contient).
        #[arg(long)]
        document: Option<String>,
    },
    /// Supprime un instantané.
    Delete { name: String },
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportTemplate {
    /// Matrice de traçabilité.
    Matrix,
    /// Résultats d'analyse.
    Analysis,
    /// Description du projet.
    Description,
    /// Contrôle des règles.
    Rules,
}

impl ReportTemplate {
    fn core(self) -> Template {
        match self {
            ReportTemplate::Matrix => Template::Matrix,
            ReportTemplate::Analysis => Template::Analysis,
            ReportTemplate::Description => Template::Description,
            ReportTemplate::Rules => Template::Rules,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportFormat {
    Html,
    Xlsx,
    Docx,
    Csv,
    Md,
}

impl ReportFormat {
    fn core(self) -> Format {
        match self {
            ReportFormat::Html => Format::Html,
            ReportFormat::Xlsx => Format::Xlsx,
            ReportFormat::Docx => Format::Docx,
            ReportFormat::Csv => Format::Csv,
            ReportFormat::Md => Format::Markdown,
        }
    }
}

#[derive(Subcommand)]
enum TypesAction {
    /// Liste les types disponibles (configuration puis bibliothèque intégrée).
    List,
    /// Capture un fichier avec un type et affiche l'arbre des éléments.
    Test {
        file: PathBuf,
        #[arg(long = "type")]
        type_name: Option<String>,
        #[arg(long = "param", value_parser = parse_param)]
        params: Vec<(String, String)>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum FailOn {
    Error,
    Warning,
}

fn parse_param(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| format!("paramètre attendu sous la forme nom=valeur : {s}"))
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // Lancé par un double-clic (sans argument) ou par un projet déposé sur
    // l'exécutable : la console se refermerait aussitôt la sortie écrite.
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let code = match args.len() {
        1 if interactive => {
            println!("ReqtiCraft {}", env!("CARGO_PKG_VERSION"));
            println!(
                "Outil en ligne de commande : l'interface graphique n'est pas encore disponible.\n\
                 Ouvrez un terminal (PowerShell ou cmd) dans ce répertoire et lancez par exemple :\n\
                 \x20 reqticraft analyze projet.rqtf --details\n\
                 Vous pouvez aussi déposer un projet .rqtf sur reqticraft.exe pour l'analyser.\n"
            );
            Cli::command().print_help().ok();
            ExitCode::from(2)
        }
        2 if is_project(Path::new(&args[1])) => {
            let cli = Cli {
                config: Vec::new(),
                command: Command::Analyze {
                    project: PathBuf::from(&args[1]),
                    json: None,
                    details: true,
                    fail_on: None,
                    report: None,
                    template: ReportTemplate::Matrix,
                    scripts: false,
                },
            };
            execute(cli)
        }
        _ => return execute(Cli::parse()),
    };
    if interactive {
        print!("\nAppuyez sur Entrée pour fermer.");
        std::io::stdout().flush().ok();
        std::io::stdin().read_line(&mut String::new()).ok();
    }
    code
}

fn is_project(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rqtf"))
}

fn execute(cli: Cli) -> ExitCode {
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Import { project } => {
            let loaded = rc_core::load_project(&project, &cli.config)
                .with_context(|| format!("lecture de {}", project.display()))?;
            let flat = rc_core::flatten(&loaded, &cli.config);
            print!("{}", output::import_report(&loaded, &flat));
            Ok(ExitCode::SUCCESS)
        }
        Command::Analyze {
            project,
            json,
            details,
            fail_on,
            report,
            template,
            scripts,
        } => {
            let mut run = rc_core::analyze_project(&project, &cli.config)
                .with_context(|| format!("analyse de {}", project.display()))?;
            let scripts = rc_core::apply_scripts(&mut run, scripts);
            if scripts.ran {
                println!(
                    "Scripts : {} vérification(s), {} problème(s), {} erreur(s) ({} ms)",
                    scripts.checks, scripts.problems, scripts.errors, scripts.duration_ms
                );
            } else if !scripts.scripts.is_empty() {
                println!("Scripts non exécutés (projet non approuvé) : relancez avec --scripts.");
            }
            print!("{}", output::analysis_report(&run, details));
            if let Some(path) = json {
                let report = output::json_report(&run);
                std::fs::write(&path, serde_json::to_string_pretty(&report)?)
                    .with_context(|| format!("écriture de {}", path.display()))?;
            }
            if let Some(path) = report {
                outputs::write_report(&run, template.core(), None, &path)
                    .map_err(anyhow::Error::msg)?;
                println!("Rapport : {}", path.display());
            }
            let failed = match fail_on {
                Some(FailOn::Error) => run.analysis.count(Severity::Error) > 0,
                Some(FailOn::Warning) => {
                    run.analysis.count(Severity::Error) + run.analysis.count(Severity::Warning) > 0
                }
                None => false,
            };
            Ok(if failed {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Serve { project, port, ui } => {
            serve::serve(port, ui, project, &cli.config)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Convert {
            file,
            type_name,
            params,
        } => {
            let types = library(&cli.config);
            let doc_type = pick_type(&types, type_name.as_deref(), &file)?;
            let entry = entry_for(&file, doc_type, params);
            let out = rc_core::intermediate(&entry, doc_type)
                .map_err(|(rule, text)| anyhow::anyhow!("{rule} : {text}"))?;
            print!("{}", out.as_str());
            Ok(ExitCode::SUCCESS)
        }
        Command::Types {
            action: TypesAction::List,
        } => {
            for t in library(&cli.config) {
                let source = t
                    .source
                    .as_ref()
                    .map(|s| s.display().to_string())
                    .unwrap_or_default();
                println!("{:<32} {:<22} {}", t.name, t.converter, source);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Types {
            action:
                TypesAction::Test {
                    file,
                    type_name,
                    params,
                },
        } => {
            let types = library(&cli.config);
            let doc_type = pick_type(&types, type_name.as_deref(), &file)?;
            let entry = entry_for(&file, doc_type, params);
            let doc = rc_core::capture_document(&entry);
            print!("{}", output::element_tree(&doc));
            Ok(ExitCode::SUCCESS)
        }
        Command::Report {
            project,
            template,
            format,
            output: out,
        } => {
            let run = rc_core::analyze_project(&project, &cli.config)
                .with_context(|| format!("analyse de {}", project.display()))?;
            outputs::write_report(&run, template.core(), format.map(ReportFormat::core), &out)
                .map_err(anyhow::Error::msg)?;
            println!("Rapport : {}", out.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Snapshot { project, action } => {
            let run = rc_core::analyze_project(&project, &cli.config)
                .with_context(|| format!("analyse de {}", project.display()))?;
            let mut store = outputs::snapshots(&run).map_err(anyhow::Error::msg)?;
            snapshot(&run, &mut store, action)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn snapshot(
    run: &rc_core::ProjectRun,
    store: &mut rc_snapshot::Store,
    action: SnapshotAction,
) -> Result<()> {
    let find = |store: &rc_snapshot::Store, name: &str| -> Result<rc_snapshot::SnapshotInfo> {
        store
            .find(name)?
            .with_context(|| format!("instantané « {name} » introuvable"))
    };
    match action {
        SnapshotAction::Create { name } => {
            let info = store.create(&name, &outputs::current_state(run))?;
            println!(
                "Instantané « {} » créé : {} exigences, couverture {}/{}",
                info.name, info.requirements, info.covered, info.coverable
            );
        }
        SnapshotAction::List => {
            let list = store.list()?;
            if list.is_empty() {
                println!("Aucun instantané.");
            }
            for s in list {
                println!(
                    "{:>3}  {:<24} {}  exigences {:>5}  couverture {:>5}/{:<5}  erreurs {}",
                    s.id, s.name, s.created, s.requirements, s.covered, s.coverable, s.errors
                );
            }
        }
        SnapshotAction::Diff { from, to } => {
            let a = find(store, &from)?;
            let old = store.load(a.id)?;
            let (label, new) = match to {
                Some(t) => {
                    let b = find(store, &t)?;
                    (b.name.clone(), store.load(b.id)?)
                }
                None => ("état courant".to_owned(), outputs::current_state(run)),
            };
            print!(
                "{}",
                output::snapshot_diff(&a.name, &label, &rc_snapshot::diff(&old, &new))
            );
        }
        SnapshotAction::History { ident, document } => {
            let state = outputs::current_state(run);
            let element = state
                .elements
                .iter()
                .find(|e| {
                    e.ident
                        .as_deref()
                        .is_some_and(|i| i.eq_ignore_ascii_case(&ident))
                        && document.as_ref().is_none_or(|d| *d == e.document)
                })
                .with_context(|| format!("élément « {ident} » introuvable dans l'état courant"))?;
            let (doc, _, key) = element.key().context("élément sans identifiant")?;
            for (s, e) in store.history(&doc, &key)? {
                match e {
                    Some(e) => println!(
                        "{:<24} {}  {:<10} {}",
                        s.name,
                        s.created,
                        e.status,
                        e.text.replace('\n', " ")
                    ),
                    None => println!("{:<24} {}  absent", s.name, s.created),
                }
            }
            println!(
                "{:<24} {:<20}  {:<10} {}",
                "(courant)",
                "",
                element.status,
                element.text.replace('\n', " ")
            );
        }
        SnapshotAction::Delete { name } => {
            let s = find(store, &name)?;
            store.delete(s.id)?;
            println!("Instantané « {} » supprimé.", s.name);
        }
    }
    Ok(())
}

/// Document isolé (commandes `convert` et `types test`) : chemin, URL ou
/// requête relatifs au répertoire courant.
fn entry_for(
    file: &Path,
    doc_type: &DocumentType,
    params: Vec<(String, String)>,
) -> rc_core::DocumentEntry {
    rc_core::DocumentEntry {
        name: file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into()),
        decl: rc_core::rc_model::DocumentDecl {
            name: String::new(),
            doc_type: doc_type.name.clone(),
            path: file.to_owned(),
            params,
            ..Default::default()
        },
        base_dir: PathBuf::from("."),
        types: {
            let mut lib = rc_core::rc_interop::TypeLibrary::default();
            lib.extend([doc_type.clone()]);
            lib
        },
    }
}

/// Types disponibles hors projet : configuration, puis bibliothèque intégrée.
fn library(config: &[PathBuf]) -> Vec<DocumentType> {
    let mut lib = rc_core::rc_interop::TypeLibrary::default();
    for dir in config {
        let types_dir = if dir.join("types").is_dir() {
            dir.join("types")
        } else {
            dir.clone()
        };
        lib.extend(rc_core::rc_interop::load_types_dir(&types_dir, true).0);
    }
    lib.extend(rc_core::builtin_types());
    lib.visible().into_iter().cloned().collect()
}

fn pick_type<'a>(
    types: &'a [DocumentType],
    name: Option<&str>,
    file: &Path,
) -> Result<&'a DocumentType> {
    if let Some(name) = name {
        return types
            .iter()
            .find(|t| t.name == name)
            .with_context(|| format!("type d'analyse inconnu : {name}"));
    }
    if file.is_dir() {
        return types
            .iter()
            .find(|t| t.name == "Code")
            .context("type Code introuvable");
    }
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    types
        .iter()
        .find(|t| {
            t.file_extensions
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&ext))
        })
        .with_context(|| format!("aucun type pour l'extension « {ext} » : préciser --type"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
