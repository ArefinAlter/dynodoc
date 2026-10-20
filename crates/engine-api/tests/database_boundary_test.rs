//! Real login, catalog/DDL denial, upgrade compatibility and startup regressions.
mod common;

use common::RuntimeDatabase;
use engine_core::{
    database, log,
    snapshot::{SnapshotEngine, SnapshotReason},
    snapshot_verification,
};
use engine_shared::{DocumentId, EventPayload, IdentityId};
use sqlx::{ConnectOptions, PgPool};

async fn seed(pool: &PgPool) -> (DocumentId, IdentityId) {
    let actor = sqlx::query_scalar("insert into identity(email,display_name) values('runtime@example.test','Runtime fixture') returning id")
        .fetch_one(pool).await.unwrap();
    let doc = sqlx::query_scalar(
        "insert into document(title,created_by) values('Runtime history',$1) returning id",
    )
    .bind(actor)
    .fetch_one(pool)
    .await
    .unwrap();
    (DocumentId(doc), IdentityId(actor))
}

async fn comment(pool: &PgPool, doc: DocumentId, actor: IdentityId) {
    log::append(
        pool,
        doc,
        &EventPayload::CommentAdded {
            node_id: None,
            body: "Preserved comment".into(),
        },
        actor,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_cannot_rewrite_history_disable_guards_or_escalate(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let (doc, actor) = seed(&runtime.pool).await;
    comment(&runtime.pool, doc, actor).await;
    SnapshotEngine::new(runtime.pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await
        .unwrap();
    // Immutable-table writes must fail at the ACL even when there are no matching
    // rows (row triggers alone cannot reject these zero-row statements).
    for (table, column) in [
        ("event", "seq"),
        ("snapshot", "through_seq"),
        ("snapshot_verification", "through_seq"),
        ("document_version", "name"),
        ("draft_edit", "actor_id"),
        ("operation_audit", "actor_id"),
        ("erasure_receipt", "kind"),
        ("provenance_bundle", "body"),
        ("agent_call", "outcome"),
        ("agent_proposal_receipt", "outcome"),
        ("device_key_class", "class"),
        ("checkpoint_object", "bytes"),
        ("shared_checkpoint", "through_seq"),
        ("change_request_review", "verdict"),
        ("_sqlx_migrations", "success"),
    ] {
        for statement in [
            format!("update {table} set {column}={column} where false"),
            format!("delete from {table} where false"),
            format!("truncate {table} cascade"),
        ] {
            let error = sqlx::raw_sql(&statement)
                .execute(&runtime.pool)
                .await
                .unwrap_err();
            assert_eq!(
                error.as_database_error().and_then(|e| e.code()).as_deref(),
                Some("42501"),
                "{statement}: {error}"
            );
        }
    }
    let owner = runtime.owner.connect_options().get_username().to_owned();
    for statement in [
        "alter table event disable trigger all".to_owned(),
        "drop trigger event_no_update on event".into(),
        "create schema attacker".into(), "create table public.attacker(id integer)".into(),
        "create temp table event(id integer)".into(),
        "set session_replication_role=replica".into(),
        "select setval('operation_audit_id_seq',1)".into(),
        "create trigger attacker before insert on document for each row execute function event_immutable()".into(),
        format!("alter role {} superuser", runtime.role), format!("set role \"{owner}\""),
    ] {
        assert!(sqlx::raw_sql(&statement).execute(&runtime.pool).await.is_err(), "unexpected permission: {statement}");
    }
    // PostgreSQL may report an unauthorized GRANT as a warning/no-op rather than
    // an error. Verify the effective permission did not change in either case.
    let _ = sqlx::raw_sql("grant update on event to public")
        .execute(&runtime.pool)
        .await;
    assert!(!sqlx::query_scalar::<_, bool>(
        "select has_table_privilege(current_user,'event','UPDATE')"
    )
    .fetch_one(&runtime.pool)
    .await
    .unwrap());
    let report = snapshot_verification::verify_document(&runtime.pool, doc, false)
        .await
        .unwrap();
    assert_eq!((report.through_seq, report.snapshots), (1, 1));
    // Re-provisioning is idempotent and does not alter stored content.
    database::grant_runtime(&runtime.owner, &runtime.role)
        .await
        .unwrap();
    database::check_runtime(&runtime.pool).await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn startup_detects_memberships_public_grants_and_column_only_escalation(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let role = &runtime.role;
    for (grant, revoke) in [
        (
            format!("grant update(seq) on event to {role}"),
            format!("revoke update(seq) on event from {role}"),
        ),
        (
            "grant truncate on event to public".into(),
            "revoke truncate on event from public".into(),
        ),
        (
            format!("grant insert on event to {role} with grant option"),
            format!("revoke grant option for insert on event from {role}"),
        ),
        (
            format!("grant execute on function erase_document_children() to {role}"),
            format!("revoke execute on function erase_document_children() from {role}"),
        ),
        (
            format!("grant pg_read_all_data to {role}"),
            format!("revoke pg_read_all_data from {role}"),
        ),
    ] {
        sqlx::raw_sql(&grant).execute(&runtime.owner).await.unwrap();
        assert!(
            database::check_runtime(&runtime.pool).await.is_err(),
            "{grant}"
        );
        sqlx::raw_sql(&revoke)
            .execute(&runtime.owner)
            .await
            .unwrap();
        database::check_runtime(&runtime.pool).await.unwrap();
    }
    assert!(database::check_runtime(&runtime.owner).await.is_err());
    assert!(database::grant_runtime(
        &runtime.owner,
        runtime.owner.connect_options().get_username()
    )
    .await
    .is_err());
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_operator_functions_are_not_executable_by_runtime_or_public(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    sqlx::raw_sql("create function public.future_operator_function() returns integer language sql security definer set search_path=pg_catalog,public,pg_temp as 'select 1'")
        .execute(&runtime.owner).await.unwrap();
    assert!(sqlx::query("select public.future_operator_function()")
        .execute(&runtime.pool)
        .await
        .is_err());
    let public_execute: bool = sqlx::query_scalar("select exists(select 1 from pg_proc p, lateral aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a where p.oid='public.future_operator_function()'::regprocedure and a.grantee=0 and a.privilege_type='EXECUTE')")
        .fetch_one(&runtime.owner).await.unwrap();
    assert!(!public_execute);
    database::check_runtime(&runtime.pool).await.unwrap();
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_connection_disguised_by_session_authorization_is_rejected(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let role = runtime.role.clone();
    let disguised = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let statement = format!("set session authorization {role}");
            Box::pin(async move {
                sqlx::query(&statement).execute(connection).await?;
                Ok(())
            })
        })
        .connect_with(runtime.owner.connect_options().as_ref().clone())
        .await
        .unwrap();
    let (session, current): (String, String) =
        sqlx::query_as("select session_user::text,current_user::text")
            .fetch_one(&disguised)
            .await
            .unwrap();
    assert_eq!(session, runtime.role);
    assert_eq!(current, runtime.role);
    assert!(database::check_runtime(&disguised)
        .await
        .unwrap_err()
        .to_string()
        .contains("SESSION AUTHORIZATION"));
    disguised.close().await;
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn schema_28_upgrade_preserves_history_and_requires_a_new_runtime_login(pool: PgPool) {
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            database::MIGRATOR
                .iter()
                .filter(|m| m.version <= 28)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    let (doc, actor) = seed(&pool).await;
    comment(&pool, doc, actor).await;
    let snapshot = SnapshotEngine::new(pool.clone())
        .take(doc, SnapshotReason::Periodic)
        .await
        .unwrap();
    assert!(database::check_schema(&pool).await.is_err());
    database::MIGRATOR.run(&pool).await.unwrap();
    let runtime = RuntimeDatabase::new(pool).await;
    let mut connection = runtime.pool.acquire().await.unwrap();
    assert_eq!(
        snapshot_verification::load(&mut connection, doc, snapshot.id.0)
            .await
            .unwrap(),
        Some(snapshot)
    );
    drop(connection);
    comment(&runtime.pool, doc, actor).await;
    assert_eq!(
        snapshot_verification::verify_document(&runtime.pool, doc, false)
            .await
            .unwrap()
            .through_seq,
        2
    );
    // A retained build using this same schema/role contract can reopen the existing
    // data. Pre-boundary binaries are not declared safe rollback.
    runtime.pool.close().await;
    let reopened = PgPool::connect_with(runtime.pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    database::check_runtime(&reopened).await.unwrap();
    snapshot_verification::verify_document(&reopened, doc, false)
        .await
        .unwrap();
    reopened.close().await;
    runtime.close().await;
}

async fn rejected_binary(pool: &PgPool) -> String {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_engine-api"))
            .kill_on_drop(true)
            .env_remove("MIGRATION_DATABASE_URL")
            .env(
                "DATABASE_URL",
                pool.connect_options().to_url_lossy().as_str(),
            )
            .env("PASETO_LOCAL_KEY", "07".repeat(32))
            .env("WEB_SERVICE_KEY", "isolated-runtime-boundary-service-key")
            .output(),
    )
    .await
    .expect("unsafe API startup did not exit")
    .unwrap();
    assert!(!output.status.success());
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[sqlx::test(migrations = "../../migrations")]
async fn api_refuses_owner_credentials_and_schema_drift_without_migrating(pool: PgPool) {
    assert!(rejected_binary(&pool)
        .await
        .contains("runtime database role"));
    let runtime = RuntimeDatabase::new(pool).await;
    for statement in [
        "update _sqlx_migrations set checksum=decode('00','hex') where version=29",
        "delete from _sqlx_migrations where version=29",
        "insert into _sqlx_migrations(version,description,success,checksum,execution_time) values(999,'future',true,decode('00','hex'),0)",
    ] {
        sqlx::raw_sql(statement).execute(&runtime.owner).await.unwrap();
        let before: serde_json::Value = sqlx::query_scalar("select coalesce(jsonb_agg(to_jsonb(m) order by version),'[]') from _sqlx_migrations m")
            .fetch_one(&runtime.owner).await.unwrap();
        assert!(rejected_binary(&runtime.pool).await.contains("database schema differs"));
        let after: serde_json::Value = sqlx::query_scalar("select coalesce(jsonb_agg(to_jsonb(m) order by version),'[]') from _sqlx_migrations m")
            .fetch_one(&runtime.owner).await.unwrap();
        assert_eq!(before,after,"startup must not repair/write migration metadata");
    }
    runtime.close().await;
}
