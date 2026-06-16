//! dynodoc admin CLI.
//!
//! Stage 12 scaffolding: defines the command surface (`verify`, `replay`,
//! `snapshot`) with stubs. The `verify` command — replay the log and confirm
//! hash-chain integrity — is implemented against the event log in stage 14/21.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "engine-cli", version, about = "dynodoc admin tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Replay a document's event log and verify hash-chain integrity.
    Verify {
        /// Document id (UUID).
        document_id: String,
    },
    /// Replay a document's event log and print the materialized state.
    Replay { document_id: String },
    /// Take a snapshot of a document's current state.
    Snapshot { document_id: String },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Verify { document_id } => {
            anyhow::bail!("verify {document_id}: not implemented until stage 14");
        }
        Command::Replay { document_id } => {
            anyhow::bail!("replay {document_id}: not implemented until stage 15");
        }
        Command::Snapshot { document_id } => {
            anyhow::bail!("snapshot {document_id}: not implemented until stage 15");
        }
    }
}
