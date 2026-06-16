//! dynodoc admin CLI.
//!
//! `verify` (stage 14) replays a document's event log and confirms hash-chain
//! integrity — the audit guarantee made operational. `replay`/`snapshot` land in
//! stage 15.

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use engine_core::log::{verify_chain, EventError};
use engine_shared::DocumentId;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Verify { document_id } => verify(document_id).await,
        Command::Replay { document_id } => {
            anyhow::bail!("replay {document_id}: not implemented until stage 15");
        }
        Command::Snapshot { document_id } => {
            anyhow::bail!("snapshot {document_id}: not implemented until stage 15");
        }
    }
}

async fn verify(document_id: String) -> anyhow::Result<()> {
    let doc = DocumentId(
        Uuid::parse_str(&document_id)
            .with_context(|| format!("invalid document id (expected a UUID): {document_id}"))?,
    );

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is not set (see .env / SETUP.md)")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .context("could not connect to Postgres")?;

    match verify_chain(&pool, doc).await {
        Ok(()) => {
            println!("OK: hash chain verified for document {document_id}");
            Ok(())
        }
        Err(EventError::ChainBroken { seq, detail }) => {
            // A failed audit is an operational signal, not a crash: report the exact
            // event and exit non-zero so scripts/CI can gate on it.
            eprintln!("CHAIN BROKEN at event seq {seq}: {detail}");
            std::process::exit(1);
        }
        Err(other) => Err(other).context("verify failed"),
    }
}
