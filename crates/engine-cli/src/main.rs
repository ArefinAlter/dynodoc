//! dynodoc admin CLI.
//!
//! - `verify` (stage 14) — replay the log and confirm hash-chain integrity.
//! - `replay` (stage 15) — fold the whole log and print the materialized state.
//! - `snapshot` (stage 15) — take a snapshot of current state and print its digest.
//! - `openapi` (stage 18) — emit the generated OpenAPI document (`openapi.yaml`).

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use engine_api::ApiDoc;
use engine_core::log::{self, verify_chain, EventError};
use engine_core::materializer::Materializer;
use engine_core::snapshot::{SnapshotEngine, SnapshotReason};
use engine_shared::DocumentId;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use utoipa::OpenApi;
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
    Replay {
        /// Document id (UUID).
        document_id: String,
    },
    /// Take a snapshot of a document's current state.
    Snapshot {
        /// Document id (UUID).
        document_id: String,
    },
    /// Emit the generated OpenAPI document. With `--out`, write it there; otherwise
    /// print to stdout. CI compares the committed `openapi.yaml` against `--out -` to
    /// fail on drift (see SETUP.md).
    Openapi {
        /// File to write the spec to (defaults to stdout).
        #[arg(long)]
        out: Option<String>,
    },
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
        Command::Replay { document_id } => replay(document_id).await,
        Command::Snapshot { document_id } => snapshot(document_id).await,
        Command::Openapi { out } => openapi(out),
    }
}

/// Emit the OpenAPI document generated from the annotated handlers. No DB needed.
fn openapi(out: Option<String>) -> anyhow::Result<()> {
    let yaml = ApiDoc::openapi()
        .to_yaml()
        .context("serializing the OpenAPI document to YAML failed")?;
    match out {
        Some(path) => {
            std::fs::write(&path, yaml).with_context(|| format!("writing {path} failed"))?;
            eprintln!("wrote OpenAPI spec to {path}");
        }
        None => print!("{yaml}"),
    }
    Ok(())
}

/// Parse the UUID argument into a `DocumentId`.
fn parse_document_id(document_id: &str) -> anyhow::Result<DocumentId> {
    Uuid::parse_str(document_id)
        .map(DocumentId)
        .with_context(|| format!("invalid document id (expected a UUID): {document_id}"))
}

/// Connect to Postgres from `DATABASE_URL`.
async fn connect() -> anyhow::Result<PgPool> {
    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is not set (see .env / SETUP.md)")?;
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .context("could not connect to Postgres")
}

async fn verify(document_id: String) -> anyhow::Result<()> {
    let doc = parse_document_id(&document_id)?;
    let pool = connect().await?;

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

async fn replay(document_id: String) -> anyhow::Result<()> {
    let doc = parse_document_id(&document_id)?;
    let pool = connect().await?;

    // A true replay folds the whole log from genesis (snapshot_seq 0 = no snapshot).
    let events = log::read_since_snapshot(&pool, doc, 0)
        .await
        .context("reading the event log failed")?;
    let state = Materializer::fold(&events).context("folding the event log failed")?;

    println!(
        "replayed {} event(s) for document {document_id}",
        events.len()
    );
    println!("{}", serde_json::to_string_pretty(&state)?);
    Ok(())
}

async fn snapshot(document_id: String) -> anyhow::Result<()> {
    let doc = parse_document_id(&document_id)?;
    let pool = connect().await?;

    let snapshot = SnapshotEngine::new(pool)
        .take(doc, SnapshotReason::Periodic)
        .await
        .context("taking a snapshot failed")?;

    println!(
        "snapshot {} taken for document {document_id}",
        snapshot.id.0
    );
    println!("  through_seq:  {}", snapshot.through_seq);
    println!("  merkle_root:  {}", to_hex(&snapshot.merkle_root));
    println!("  chain_hash:   {}", to_hex(&snapshot.event_chain_hash));
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
