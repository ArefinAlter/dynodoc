//! `engine-cli` verification commands as the restricted runtime login.

use std::process::Command;

use engine_core::log;
use engine_shared::{DocumentId, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::json;
use sqlx::PgPool;

async fn actor(pool: &PgPool) -> IdentityId {
    let id: uuid::Uuid = sqlx::query_scalar(
        "insert into identity(email,display_name) values('cli@example.test','CLI') returning id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    IdentityId(id)
}

/// A document with one root node, written as the API does: event plus node row.
async fn document(pool: &PgPool, actor: IdentityId, title: &str) -> (DocumentId, String) {
    let doc: uuid::Uuid =
        sqlx::query_scalar("insert into document(title,created_by) values($1,$2) returning id")
            .bind(title)
            .bind(actor.0)
            .fetch_one(pool)
            .await
            .unwrap();
    let root = format!("01CLIROOT{}", title.to_uppercase());
    sqlx::query(
        "insert into node(id,document_id,type,pos,current_fields) values($1,$2,'form','a0',$3)",
    )
    .bind(&root)
    .bind(doc)
    .bind(json!({ "title": title }))
    .execute(pool)
    .await
    .unwrap();
    let created = EventPayload::NodeCreated {
        node_id: NodeId(root.clone()),
        node_type: NodeType::Form,
        parent_id: None,
        pos: "a0".into(),
        fields: json!({ "title": title }),
        var_name: None,
    };
    log::append(pool, DocumentId(doc), &created, actor)
        .await
        .unwrap();
    (DocumentId(doc), root)
}

/// A fresh login with the runtime grants, and its database URL.
async fn runtime_login(pool: &PgPool) -> (String, String) {
    let role = format!("verify_cli_{}", uuid::Uuid::new_v4().simple());
    let password = uuid::Uuid::new_v4().simple().to_string();
    sqlx::raw_sql(&format!(
        "create role {role} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls"
    ))
    .execute(pool)
    .await
    .unwrap();
    engine_core::database::grant_runtime(pool, &role)
        .await
        .unwrap();
    let options = pool.connect_options();
    let url = format!(
        "postgres://{role}:{password}@{}:{}/{}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap()
    );
    (url, role)
}

async fn drop_login(pool: &PgPool, role: &str) {
    sqlx::raw_sql(&format!("drop owned by {role}; drop role {role}"))
        .execute(pool)
        .await
        .unwrap();
}

fn run(url: &str, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(args)
        .env("DATABASE_URL", url)
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap() + &String::from_utf8(out.stderr).unwrap();
    (out.status.success(), text)
}

#[sqlx::test(migrations = "../../migrations")]
async fn all_mode_reports_only_the_drifted_document(pool: PgPool) {
    let actor = actor(&pool).await;
    let (clean, _) = document(&pool, actor, "Clean").await;
    let (drifted, root) = document(&pool, actor, "Drifted").await;
    sqlx::query("update node set pos='zz' where id=$1")
        .bind(&root)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into node(id,document_id,parent_id,type,pos) values('01CLIEXTRA',$1,$2,'item','b0')")
        .bind(drifted.0)
        .bind(&root)
        .execute(&pool)
        .await
        .unwrap();
    let (url, role) = runtime_login(&pool).await;

    let (ok, out) = run(&url, &["verify-projection", "--all"]);
    assert!(!ok, "{out}");
    assert_eq!(
        out.lines().filter(|l| l.starts_with("FAIL:")).count(),
        1,
        "{out}"
    );
    assert!(out.contains(&format!("OK: document {}", clean.0)), "{out}");
    let drift = format!("DIFF: document {} node", drifted.0);
    assert!(
        out.contains(&format!("{drift} {root} pos: replay=\"a0\" table=\"zz\"")),
        "{out}"
    );
    assert!(
        out.contains(&format!("{drift} 01CLIEXTRA row: replay=null")),
        "{out}"
    );
    assert!(run(&url, &["verify-projection", &clean.0.to_string()]).0);

    drop_login(&pool, &role).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_accepts_only_a_head_in_the_verified_chain(pool: PgPool) {
    let actor = actor(&pool).await;
    let (doc, root) = document(&pool, actor, "Pinned").await;
    let edit = EventPayload::FieldEdited {
        node_id: NodeId(root),
        field: "title".into(),
        value: json!("Pinned again"),
    };
    log::append(&pool, doc, &edit, actor).await.unwrap();
    let heads: Vec<Vec<u8>> =
        sqlx::query_scalar("select chain_hash from event where document_id=$1 order by seq")
            .bind(doc.0)
            .fetch_all(&pool)
            .await
            .unwrap();
    let to_hex = |h: &[u8]| h.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let (url, role) = runtime_login(&pool).await;
    let id = doc.0.to_string();

    let (ok, out) = run(&url, &["verify", &id, "--expect-head", &to_hex(&heads[1])]);
    assert!(
        ok && out.contains(&format!("; head {}", to_hex(&heads[1]))),
        "{out}"
    );
    let (ok, out) = run(&url, &["verify", &id, "--expect-head", &to_hex(&heads[0])]);
    assert!(ok && out.contains("chain hash at seq 1"), "{out}");
    let unknown = "ab".repeat(32);
    let (ok, out) = run(&url, &["verify", &id, "--expect-head", &unknown]);
    assert!(!ok, "{out}");
    assert!(
        out.contains(&unknown) && out.contains(&to_hex(&heads[1])),
        "{out}"
    );

    drop_login(&pool, &role).await;
}

/// Exercise the Linux/GNU-timeout scheduler against a disposable DB, not a mock CLI.
#[cfg(target_os = "linux")]
#[sqlx::test(migrations = "../../migrations")]
async fn scheduled_checks_notify_on_projection_or_snapshot_failure(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;

    let actor = actor(&pool).await;
    let (doc, root) = document(&pool, actor, "Scheduled").await;
    let snapshot = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .take(doc, engine_core::snapshot::SnapshotReason::Periodic)
        .await
        .unwrap();
    // Model a historical snapshot without a receipt. Runtime backfill must work.
    sqlx::raw_sql("alter table snapshot_verification disable trigger snapshot_verification_no_delete; delete from snapshot_verification; alter table snapshot_verification enable trigger snapshot_verification_no_delete")
        .execute(&pool).await.unwrap();
    let (url, role) = runtime_login(&pool).await;
    let dir = std::env::temp_dir().join(format!("dynodoc-verification-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let alert = dir.join("capture-alert");
    let capture = dir.join("alerts");
    std::fs::write(&alert, "#!/bin/sh\ncat >> \"$ALERT_CAPTURE\"\n").unwrap();
    std::fs::set_permissions(&alert, std::fs::Permissions::from_mode(0o700)).unwrap();
    let check = || {
        Command::new("sh")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/verify-scheduled.sh"
            ))
            .env("DATABASE_URL", &url)
            .env("ENGINE_CLI", env!("CARGO_BIN_EXE_engine-cli"))
            .env("DYNODOC_VERIFY_ALERT", &alert)
            .env("DYNODOC_VERIFY_TIMEOUT", "30s")
            .env("ALERT_CAPTURE", &capture)
            .env("RUST_LOG", "off")
            .output()
            .unwrap()
    };
    let clean = check();
    assert!(clean.status.success(), "{clean:?}");
    assert!(!capture.exists(), "a successful run must stay quiet");
    let receipts: i64 = sqlx::query_scalar("select count(*) from snapshot_verification")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(receipts, 1);

    sqlx::query("update node set pos='zz' where id=$1")
        .bind(&root)
        .execute(&pool)
        .await
        .unwrap();
    let drift = check();
    assert_eq!(drift.status.code(), Some(1), "{drift:?}");
    let alert_text = std::fs::read_to_string(&capture).unwrap();
    assert!(
        alert_text.contains("verify-projection=1 verify-snapshots=0"),
        "{alert_text}"
    );
    assert!(!alert_text.contains(&doc.0.to_string()));
    assert!(!alert_text.contains(&root));
    assert!(!alert_text.contains(&url));

    sqlx::query("update node set pos='a0' where id=$1")
        .bind(&root)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("alter table snapshot disable trigger snapshot_no_update")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "update snapshot set state=jsonb_set(state,$2,'\"Corrupted snapshot\"') where id=$1",
    )
    .bind(snapshot.id.0)
    .bind(vec!["nodes", &root, "current_fields", "title"])
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("alter table snapshot enable trigger snapshot_no_update")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let corrupt = check();
    assert_eq!(corrupt.status.code(), Some(1), "{corrupt:?}");
    let alerts = std::fs::read_to_string(&capture).unwrap();
    assert_eq!(alerts.lines().count(), 2, "{alerts}");
    assert!(
        alerts
            .lines()
            .last()
            .unwrap()
            .contains("verify-snapshots=1"),
        "{alerts}"
    );
    drop_login(&pool, &role).await;
    std::fs::remove_dir_all(&dir).unwrap();
}
