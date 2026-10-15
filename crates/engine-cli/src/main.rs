//! dynodoc admin CLI.
//!
//! - `verify` (stage 14) — replay the log and confirm hash-chain integrity.
//! - `replay` (stage 15) — fold the whole log and print the materialized state.
//! - `snapshot` (stage 15) — take a snapshot of current state and print its digest.
//! - `openapi` (stage 18) — emit the generated OpenAPI document (`openapi.yaml`).

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use engine_api::ApiDoc;
use engine_core::log;
use engine_core::materializer::Materializer;
use engine_core::snapshot::{SnapshotEngine, SnapshotReason};
use engine_shared::DocumentId;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use utoipa::OpenApi;
use uuid::Uuid;

mod authority;
mod device;
mod keyfile;
mod keystore;
mod passphrase;
mod private_file;
mod receipt;
mod repository;
mod signing;
mod sync;

#[derive(Parser)]
#[command(name = "engine-cli", version, about = "dynodoc admin tools")]
struct Cli {
    /// Accept a signing-key or token file whose access cannot be restricted to the
    /// current user (Windows: the access list could not be applied; Unix: a key
    /// file readable by group or others). Off by default.
    #[arg(long, global = true)]
    allow_insecure_key_file: bool,
    /// File holding the passphrase of an encrypted key file (one trailing newline
    /// is ignored). Without it the DYNODOC_KEY_PASSPHRASE_FILE environment variable
    /// (a file path) and then a hidden terminal prompt are tried. A passphrase is
    /// never accepted as an argument.
    #[arg(long, global = true, value_name = "FILE")]
    passphrase_file: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Selected-history transfer. Remote heads are pinned separately from local branches.
    Sync(sync::Options),
    /// Enroll a local signing key in an authenticated Dynodoc account.
    Device {
        #[command(subcommand)]
        action: device::Action,
    },
    /// Signed ref receipts and a separate durable client trust journal (offline).
    Receipt {
        journal: std::path::PathBuf,
        authority: Uuid,
        project: Uuid,
        branch: String,
        #[command(subcommand)]
        action: receipt::Action,
    },
    /// Durable local project history (no network or installed-editor session required).
    Repository {
        path: std::path::PathBuf,
        project: Uuid,
        #[command(subcommand)]
        action: repository::Action,
    },
    /// Apply migrations using MIGRATION_DATABASE_URL in a separate operator process.
    Migrate,
    /// Give an existing restricted login the explicit runtime allowlist (operator only).
    GrantRuntime { role: String },
    /// Install the public receipt key using operator credentials; never replaces a key.
    InstallRefAuthority { public_key: String },
    /// Offline: dual-sign a receipt-authority rotation (or, with --recovery, sign a
    /// recorded break with the new key). Writes a new statement file; never overwrites.
    RotateRefAuthority(authority::Rotate),
    /// Append a dual-signed rotation using operator credentials. An exact retry is a no-op.
    InstallRefAuthorityRotation { statement: std::path::PathBuf },
    /// Append a recovery statement (operator credentials). Records a break that every
    /// client must re-pin explicitly; requires --confirm-break.
    InstallRefAuthorityRecovery {
        statement: std::path::PathBuf,
        #[arg(long)]
        confirm_break: bool,
    },
    /// Print the receipt-authority epochs, seal and revocation state. --out saves the
    /// state file `rotate-ref-authority` signs (created exclusively).
    ShowRefAuthority {
        #[arg(long)]
        out: Option<std::path::PathBuf>,
    },
    /// Record a revocation marker for the active epoch (or --epoch). It stops receipt
    /// issuance at once and cannot be edited; recover with a new epoch.
    RevokeRefAuthority {
        #[arg(long)]
        reason: String,
        /// Best-known time of compromise or loss, RFC 3339 with a zone. Defaults to now.
        #[arg(long)]
        effective_at: Option<String>,
        #[arg(long)]
        epoch: Option<u32>,
    },
    /// Check schema and runtime privileges using DATABASE_URL; makes no changes.
    CheckRuntime,
    /// Independently verify the event chain and all legacy snapshots.
    Verify {
        /// Document id (UUID).
        document_id: String,
        /// Also fail unless the verified chain passes through this head (hex), such as
        /// one recorded at an earlier check.
        #[arg(long)]
        expect_head: Option<String>,
    },
    /// Backfill legacy snapshot receipts by verified replay (schema 28). Atomic per document.
    VerifySnapshots {
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        document_id: Option<String>,
        /// Visit every document in keyset order; stop at the first integrity error.
        #[arg(long)]
        all: bool,
    },
    /// Compare the node table with a verified replay (read-only); exits non-zero on an
    /// unexplained difference.
    VerifyProjection {
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        document_id: Option<String>,
        /// Visit every document in keyset order and report every failing one.
        #[arg(long)]
        all: bool,
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
    /// Publish an opt-in shared checkpoint at an exact event revision (schema 25).
    SharedSnapshot {
        document_id: String,
        #[arg(long)]
        through_seq: i64,
        /// Use typed chunked state objects (requires schema 30).
        #[arg(long)]
        typed: bool,
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
    private_file::allow_insecure(cli.allow_insecure_key_file);
    passphrase::set_file(cli.passphrase_file);
    match cli.command {
        Command::Sync(options) => sync::run(options).await,
        Command::Device { action } => device::run(action),
        Command::Receipt {
            journal,
            authority,
            project,
            branch,
            action,
        } => {
            receipt::run(
                journal,
                engine_core::repository::attestation::Scope {
                    authority,
                    project,
                    branch,
                },
                action,
            )
            .await
        }
        Command::Repository {
            path,
            project,
            action,
        } => repository::run(path, project, action).await,
        Command::Migrate => {
            let pool = connect_migration().await?;
            engine_core::database::MIGRATOR.run(&pool).await?;
            println!("Database migrations complete; provision/check the runtime role before starting the API.");
            Ok(())
        }
        Command::GrantRuntime { role } => {
            engine_core::database::grant_runtime(&connect_migration().await?, &role).await?;
            println!("Runtime grants verified for {role}; connect directly using its separate credentials.");
            Ok(())
        }
        Command::InstallRefAuthority { public_key } => {
            let id = engine_core::repository::server_authority::install(
                &connect_migration().await?,
                &public_key,
            )
            .await?;
            println!("Installed receipt authority {id}; distribute this identity and public key through a trusted channel.");
            Ok(())
        }
        Command::RotateRefAuthority(args) => authority::rotate(args),
        Command::InstallRefAuthorityRotation { statement } => {
            authority::install_rotation(&connect_migration().await?, &statement).await
        }
        Command::InstallRefAuthorityRecovery {
            statement,
            confirm_break,
        } => {
            authority::install_recovery(&connect_migration().await?, &statement, confirm_break)
                .await
        }
        Command::ShowRefAuthority { out } => {
            authority::show(&connect_migration().await?, out.as_deref()).await
        }
        Command::RevokeRefAuthority {
            reason,
            effective_at,
            epoch,
        } => {
            authority::revoke(
                &connect_migration().await?,
                &reason,
                effective_at.as_deref(),
                epoch,
            )
            .await
        }
        Command::CheckRuntime => {
            engine_core::database::check_runtime(&connect().await?).await?;
            println!("Runtime schema and database privileges verified.");
            Ok(())
        }
        Command::Verify {
            document_id,
            expect_head,
        } => verify(document_id, expect_head).await,
        Command::VerifySnapshots { document_id, all } => verify_snapshots(document_id, all).await,
        Command::VerifyProjection { document_id, all } => verify_projection(document_id, all).await,
        Command::Replay { document_id } => replay(document_id).await,
        Command::Snapshot { document_id } => snapshot(document_id).await,
        Command::SharedSnapshot {
            document_id,
            through_seq,
            typed,
        } => shared_snapshot(document_id, through_seq, typed).await,
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

async fn connect_migration() -> anyhow::Result<PgPool> {
    let url = std::env::var("MIGRATION_DATABASE_URL").context(
        "MIGRATION_DATABASE_URL is required for this operator command; never pass it to the API",
    )?;
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .context("could not connect for database administration")
}

async fn verify(document_id: String, expect_head: Option<String>) -> anyhow::Result<()> {
    let doc = parse_document_id(&document_id)?;
    let pool = connect().await?;

    let report = engine_core::snapshot_verification::verify_document(&pool, doc, false)
        .await
        .context("event/snapshot verification failed")?;
    // Event rows are immutable, so the verified head can be read after the check.
    let head: Vec<u8> =
        sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
            .bind(doc.0)
            .bind(report.through_seq)
            .fetch_optional(&pool)
            .await?
            .unwrap_or_else(|| log::ZERO_HASH.to_vec());
    println!("OK: event chain and {} legacy snapshots verified through seq {} for document {document_id}; head {}", report.snapshots, report.through_seq, to_hex(&head));
    if let Some(expected) = expect_head {
        let seq: Option<i64> = sqlx::query_scalar(
            "select seq from event where document_id=$1 and encode(chain_hash,'hex')=lower($2) and seq<=$3",
        )
        .bind(doc.0)
        .bind(expected.trim())
        .bind(report.through_seq)
        .fetch_optional(&pool)
        .await?;
        let Some(seq) = seq else {
            anyhow::bail!(
                "expected head {expected} is not in the verified chain; its head is {}",
                to_hex(&head)
            );
        };
        println!("OK: expected head is the chain hash at seq {seq}");
    }
    Ok(())
}

async fn verify_snapshots(document_id: Option<String>, all: bool) -> anyhow::Result<()> {
    let pool = connect().await?;
    let mut cursor: Option<Uuid> = None;
    loop {
        let document = if all {
            sqlx::query_scalar::<_, Uuid>(
                "select id from document where ($1::uuid is null or id>$1) order by id limit 1",
            )
            .bind(cursor)
            .fetch_optional(&pool)
            .await?
        } else {
            Some(parse_document_id(document_id.as_deref().context("document id required")?)?.0)
        };
        let Some(id) = document else {
            break;
        };
        let report =
            engine_core::snapshot_verification::verify_document(&pool, DocumentId(id), true)
                .await
                .with_context(|| {
                    format!(
                        "snapshot backfill failed for document {id}; its receipts were rolled back"
                    )
                })?;
        println!(
            "OK: document {id}; {} snapshots; {} replayed events; through seq {}",
            report.snapshots, report.replayed_events, report.through_seq
        );
        if !all {
            break;
        }
        cursor = Some(id);
    }
    Ok(())
}

async fn verify_projection(document_id: Option<String>, all: bool) -> anyhow::Result<()> {
    let pool = connect().await?;
    let mut cursor: Option<Uuid> = None;
    let mut failed = 0usize;
    loop {
        let document = if all {
            sqlx::query_scalar::<_, Uuid>(
                "select id from document where ($1::uuid is null or id>$1) order by id limit 1",
            )
            .bind(cursor)
            .fetch_optional(&pool)
            .await?
        } else {
            Some(parse_document_id(document_id.as_deref().context("document id required")?)?.0)
        };
        let Some(id) = document else {
            break;
        };
        match engine_core::projection::verify_projection(&pool, DocumentId(id)).await {
            Err(err) => {
                failed += 1;
                println!("FAIL: document {id}: {err}");
            }
            Ok(report) => {
                for d in &report.differences {
                    let status = d
                        .known
                        .map_or("DIFF".to_string(), |why| format!("KNOWN ({why})"));
                    println!(
                        "{status}: document {id} node {} {}: replay={} table={}",
                        d.node_id.0, d.field, d.replayed, d.stored
                    );
                }
                let unexplained = report.unexplained().count();
                let known = report.differences.len() - unexplained;
                let verdict = if unexplained > 0 { "FAIL" } else { "OK" };
                failed += usize::from(unexplained > 0);
                println!(
                    "{verdict}: document {id}; {} rows; {unexplained} unexplained, {known} known difference(s); through seq {}",
                    report.stored_rows, report.through_seq
                );
            }
        }
        if !all {
            break;
        }
        cursor = Some(id);
    }
    if failed > 0 {
        anyhow::bail!("{failed} document(s) failed the projection check");
    }
    Ok(())
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

async fn shared_snapshot(document_id: String, through_seq: i64, typed: bool) -> anyhow::Result<()> {
    let doc = parse_document_id(&document_id)?;
    let pool = connect().await?;
    use engine_core::shared_checkpoint::postgres::Codec;
    let codec = if typed { Codec::V2 } else { Codec::V1 };
    let result = engine_core::shared_checkpoint::postgres::take_with_codec(
        &pool,
        doc,
        through_seq,
        engine_core::shared_checkpoint::Limits::default(),
        codec,
    )
    .await
    .context("publishing shared checkpoint failed")?;
    println!(
        "shared checkpoint {codec:?} at revision {}: {}",
        result.through_seq,
        result.root.as_str()
    );
    println!(
        "new objects: {}; new canonical bytes: {}",
        result.stats.new_objects, result.stats.new_bytes
    );
    Ok(())
}
