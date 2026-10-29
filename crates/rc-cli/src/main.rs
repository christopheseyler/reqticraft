//! `reqticraft` : analyse de traçabilité sans interface graphique.
//!
//! Sous-commandes : `docs/architecture-proposal.md` §5.6. `report` et `snapshot`
//! ne sont pas encore implémentées et le disent.

mod output;
mod serve;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
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
    },
    /// Génère un rapport.
    Report { project: PathBuf },
    /// Crée, liste ou compare des instantanés.
    Snapshot { project: PathBuf },
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
        } => {
            let run = rc_core::analyze_project(&project, &cli.config)
                .with_context(|| format!("analyse de {}", project.display()))?;
            print!("{}", output::analysis_report(&run, details));
            if let Some(path) = json {
                let report = output::json_report(&run);
                std::fs::write(&path, serde_json::to_string_pretty(&report)?)
                    .with_context(|| format!("écriture de {}", path.display()))?;
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
        Command::Report { .. } => bail!("la commande `report` n'est pas encore implémentée"),
        Command::Snapshot { .. } => bail!("la commande `snapshot` n'est pas encore implémentée"),
    }
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
