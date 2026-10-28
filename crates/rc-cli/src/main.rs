//! `reqticraft` : analyse de traçabilité sans interface graphique.
//!
//! Les sous-commandes reprennent `docs/architecture-proposal.md` §5.6. Celles qui
//! ne sont pas encore implémentées le disent et sortent en erreur.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "reqticraft", version, about = "Traçabilité des exigences")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Importe un projet Reqtify (.rqtf) et produit un rapport d'import.
    Import { project: PathBuf },
    /// Analyse un projet : couverture, impact, règles.
    Analyze { project: PathBuf },
    /// Génère un rapport.
    Report { project: PathBuf },
    /// Crée, liste ou compare des instantanés.
    Snapshot { project: PathBuf },
    /// Teste un type d'analyse sur un fichier.
    Types { file: PathBuf },
    /// Affiche le fichier intermédiaire produit par un convertisseur.
    Convert { file: PathBuf },
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Command::Import { .. } => "import",
            Command::Analyze { .. } => "analyze",
            Command::Report { .. } => "report",
            Command::Snapshot { .. } => "snapshot",
            Command::Types { .. } => "types",
            Command::Convert { .. } => "convert",
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    bail!(
        "la commande `{}` n'est pas encore implémentée",
        cli.command.name()
    )
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
