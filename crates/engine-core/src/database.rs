//! Deployment-time schema/privilege contract. The HTTP server never migrates.
//!
//! A dedicated, non-member login gets an explicit allowlist in this database.
//! This constrains history rewrites/DDL, not application-level authorization or
//! malicious appends by a compromised writer. Owners remain trusted operators.

use anyhow::{bail, Context};
use sqlx::{PgConnection, PgPool};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

const HISTORY: &[&str] = &[
    "event",
    "snapshot",
    "snapshot_verification",
    "document_version",
    "draft_edit",
    "operation_audit",
    "erasure_receipt",
    "provenance_bundle",
    "checkpoint_object",
    "shared_checkpoint",
    "change_request_review",
    "device_signing_key",
    "device_key_revocation",
    "project_staged_object",
    "project_staged_commit",
    "project_published_commit",
    "project_published_object",
    "project_history_review",
    "project_ref_event",
    "project_issue_comment",
];
const MUTABLE: &[&str] = &[
    "document",
    "node",
    "identity",
    "magic_link",
    "document_access",
    "refresh_token",
    "idempotency_key",
    "workspace_draft",
    "document_upload",
    "auth_request_window",
    "web_metric",
    "access_space",
    "space_member",
    "public_document_view",
    "document_policy",
    "notification",
    "document_fingerprint",
    "document_fingerprint_band",
    "document_block_hash",
    "document_match",
    "copy_detection_baseline",
    "project_settings",
    "project_watch",
    "connector_grant",
    "device_enrollment_challenge",
    "project_label",
];
/// Rows are created and pruned but never edited: a replay ledger, not a record.
const INSERT_DELETE: &[&str] = &["device_session", "project_issue_label", "project_star"];
/// Rows are created and edited but never deleted by the runtime login. Device key
/// access mode is edited in place; a grant row only ever moves `revoked_at` (a
/// trigger refuses any other change, so a changed scope set is revoke plus insert).
const INSERT_UPDATE: &[&str] = &[
    "project_issue",
    "project_issue_counter",
    "device_key_access",
    "device_key_project_grant",
];
const FUNCTIONS: &[&str] = &[
    "access_role_rank(text)",
    "member_role_order(text)",
    "inherited_space_role(uuid,uuid)",
    "effective_document_role(uuid,uuid,boolean)",
    "can_manage_document(uuid,uuid)",
    "document_member_role(uuid,uuid,boolean)",
    "document_project(uuid)",
];
const SEQUENCES: &[&str] = &[
    "draft_edit_id_seq",
    "operation_audit_id_seq",
    "project_history_review_id_seq",
];
const READ_ONLY: &[&str] = &[
    "_sqlx_migrations",
    "current_documents",
    "device_authority",
    "checkpoint_storage_account",
    "checkpoint_storage_document",
    "project_staging_storage",
    "project_history_storage",
    "project_ref_authority",
    "project_ref_authority_epoch",
    "project_ref_authority_revocation",
];

/// Read-only exact version/checksum check. Both missing and newer schemas fail.
pub async fn check_schema(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<(i64, bool, Vec<u8>)> = sqlx::query_as(
        "select version,success,checksum from public._sqlx_migrations order by version",
    )
    .fetch_all(pool)
    .await
    .context("schema unavailable; run engine-cli migrate with separate migration credentials")?;
    if rows.len() != MIGRATOR.iter().count()
        || rows
            .iter()
            .zip(MIGRATOR.iter())
            .any(|((version, success, checksum), migration)| {
                !success
                    || *version != migration.version
                    || checksum.as_slice() != migration.checksum.as_ref()
            })
    {
        bail!("database schema differs from this binary; use a compatible build and the separate migration command");
    }
    Ok(())
}

fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

async fn check_login(connection: &mut PgConnection, role: &str) -> anyhow::Result<()> {
    let safe: bool = sqlx::query_scalar(
        "select coalesce((select rolcanlogin and not (rolsuper or rolcreatedb or rolcreaterole or rolreplication or rolbypassrls)
         and not exists(select 1 from pg_catalog.pg_auth_members where member=r.oid)
         and not exists(select 1 from pg_catalog.pg_database where datdba=r.oid)
         and not exists(select 1 from pg_catalog.pg_namespace where nspowner=r.oid)
         and not exists(select 1 from pg_catalog.pg_class where relowner=r.oid)
         and not exists(select 1 from pg_catalog.pg_proc where proowner=r.oid)
         and not exists(select 1 from pg_catalog.pg_type where typowner=r.oid)
         from pg_catalog.pg_roles r where rolname=$1),false)",
    ).bind(role).fetch_one(connection).await?;
    if !safe {
        bail!("runtime database role must be an existing nonprivileged login without memberships or ownership");
    }
    Ok(())
}

/// Provision an existing dedicated login; never creates a role or accepts a password.
/// Run only with migration/operator credentials, while API traffic is stopped.
pub async fn grant_runtime(pool: &PgPool, role: &str) -> anyhow::Result<()> {
    check_schema(pool).await?;
    let mut tx = pool.begin().await?;
    check_login(&mut tx, role).await?;
    let database: String = sqlx::query_scalar("select current_database()")
        .fetch_one(&mut *tx)
        .await?;
    let name = ident(role);
    sqlx::raw_sql(&format!(
        "revoke all on database {} from {name}; grant connect on database {} to {name};
         revoke all on schema public from {name}; grant usage on schema public to {name};
         revoke all on all tables in schema public from {name};
         revoke all on all sequences in schema public from {name};
         revoke all on all functions in schema public from {name};",
        ident(&database),
        ident(&database)
    ))
    .execute(&mut *tx)
    .await?;
    for (tables, privileges) in [
        (READ_ONLY, "select"),
        (HISTORY, "select,insert"),
        (INSERT_DELETE, "select,insert,delete"),
        (INSERT_UPDATE, "select,insert,update"),
        (MUTABLE, "select,insert,update,delete"),
    ] {
        for table in tables {
            sqlx::raw_sql(&format!(
                "grant {privileges} on public.{} to {name}",
                ident(table)
            ))
            .execute(&mut *tx)
            .await?;
        }
    }
    for function in FUNCTIONS {
        sqlx::raw_sql(&format!(
            "grant execute on function public.{function} to {name}"
        ))
        .execute(&mut *tx)
        .await?;
    }
    for sequence in SEQUENCES {
        sqlx::raw_sql(&format!(
            "grant usage on sequence public.{} to {name}",
            ident(sequence)
        ))
        .execute(&mut *tx)
        .await?;
    }
    // Refuse inherited/PUBLIC or column-level privileges that table REVOKE cannot
    // remove. No partially provisioned role is committed if the contract fails.
    check_privileges(&mut tx, role).await?;
    tx.commit().await?;
    Ok(())
}

async fn check_privileges(connection: &mut PgConnection, role: &str) -> anyhow::Result<()> {
    check_login(connection, role).await?;
    let unsafe_access: bool = sqlx::query_scalar(
        "select has_database_privilege($1,current_database(),'CREATE,TEMP')
         or has_parameter_privilege($1,'session_replication_role','SET')
         or exists(select 1 from pg_catalog.pg_namespace where has_schema_privilege($1,oid,'CREATE'))
         or exists(select 1 from pg_catalog.pg_proc p join pg_catalog.pg_namespace n on n.oid=p.pronamespace
            where p.prosecdef and n.nspname not in ('pg_catalog','information_schema') and has_function_privilege($1,p.oid,'EXECUTE'))",
    ).bind(role).fetch_one(&mut *connection).await?;
    if unsafe_access {
        bail!("runtime role has DDL, temporary-object, trigger-bypass or security-definer execution privileges");
    }
    let tables: Vec<String> = sqlx::query_scalar(
        "select c.relname from pg_catalog.pg_class c join pg_catalog.pg_namespace n on n.oid=c.relnamespace
         where n.nspname='public' and c.relkind in ('r','p','v','m','f') order by c.relname",
    ).fetch_all(&mut *connection).await?;
    if tables.len()
        != HISTORY.len()
            + MUTABLE.len()
            + READ_ONLY.len()
            + INSERT_DELETE.len()
            + INSERT_UPDATE.len()
    {
        bail!("runtime table inventory differs from this binary");
    }
    for table in tables {
        let mutable = MUTABLE.contains(&table.as_str());
        let history = HISTORY.contains(&table.as_str());
        let prunable = INSERT_DELETE.contains(&table.as_str());
        let editable = INSERT_UPDATE.contains(&table.as_str());
        if !mutable && !history && !prunable && !editable && !READ_ONLY.contains(&table.as_str()) {
            bail!("unexpected runtime table {table}");
        }
        let relation = format!("public.{}", ident(&table));
        let (read, insert, update, delete, column_insert, column_update, dangerous): (bool, bool, bool, bool, bool, bool, bool) = sqlx::query_as(
            "select has_table_privilege($1,$2,'SELECT'), has_table_privilege($1,$2,'INSERT'),
             has_table_privilege($1,$2,'UPDATE'), has_table_privilege($1,$2,'DELETE'),
             has_any_column_privilege($1,$2,'INSERT'), has_any_column_privilege($1,$2,'UPDATE'),
             has_table_privilege($1,$2,'TRUNCATE,TRIGGER,REFERENCES,SELECT WITH GRANT OPTION,INSERT WITH GRANT OPTION,UPDATE WITH GRANT OPTION,DELETE WITH GRANT OPTION')
             or has_any_column_privilege($1,$2,'REFERENCES,SELECT WITH GRANT OPTION,INSERT WITH GRANT OPTION,UPDATE WITH GRANT OPTION')",
        ).bind(role).bind(&relation).fetch_one(&mut *connection).await?;
        if !read
            || insert != (mutable || history || prunable || editable)
            || update != (mutable || editable)
            || delete != (mutable || prunable)
            || column_insert != (mutable || history || prunable || editable)
            || column_update != (mutable || editable)
            || dangerous
        {
            bail!("runtime privileges do not match the allowlist for {table}");
        }
    }
    for sequence in SEQUENCES {
        let safe: bool = sqlx::query_scalar(
            "select has_sequence_privilege($1,$2,'USAGE') and not has_sequence_privilege($1,$2,'UPDATE')",
        ).bind(role).bind(format!("public.{sequence}")).fetch_one(&mut *connection).await?;
        if !safe {
            bail!("runtime sequence privileges differ for {sequence}");
        }
    }
    for function in FUNCTIONS {
        let allowed: bool = sqlx::query_scalar("select has_function_privilege($1,$2,'EXECUTE')")
            .bind(role)
            .bind(format!("public.{function}"))
            .fetch_one(&mut *connection)
            .await?;
        if !allowed {
            bail!("missing runtime function permission: {function}");
        }
    }
    Ok(())
}

/// API startup gate. No migrations, grants, role switching or owner fallback.
pub async fn check_runtime(pool: &PgPool) -> anyhow::Result<()> {
    check_schema(pool).await?;
    let mut connection = pool.acquire().await?;
    let (session, current): (String, String) =
        sqlx::query_as("select session_user::text,current_user::text")
            .fetch_one(&mut *connection)
            .await?;
    if session != current || session != pool.connect_options().get_username() {
        bail!("connect as the runtime login directly; SET ROLE/SESSION AUTHORIZATION is not a credential boundary");
    }
    check_privileges(&mut connection, &current).await
}
