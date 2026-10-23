//! The retention job under the restricted runtime login, against a real database:
//! only expired rows go, batches are bounded, history is untouched, one replica at a
//! time prunes, and a database error is survived.
mod common;

use common::RuntimeDatabase;
use engine_api::{
    auth::AuthConfig,
    retention::{self, Outcome, RetentionConfig, Stats, LOCK_KEY},
    AppState,
};
use engine_core::{
    log,
    snapshot::{SnapshotEngine, SnapshotReason},
};
use engine_shared::{DocumentId, EventPayload, IdentityId};
use sqlx::{Connection, PgPool};
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

const TARGETS: [&str; 8] = [
    "magic_link",
    "refresh_token",
    "auth_request_window",
    "device_enrollment_challenge",
    "app_handover_code",
    "web_metric",
    "idempotency_key",
    "operator_sign_in_link",
];

async fn new_identity(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("insert into identity(email) values($1) returning id")
        .bind(format!("{}@retention.test", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn tag(n: u8) -> String {
    let hex = format!("{n:02x}").repeat(32);
    format!("decode('{hex}','hex')")
}

fn uuid_tag(n: u8) -> String {
    format!("'00000000-0000-0000-0000-00000000000{n}'")
}

/// One expired-or-live row of every kind, labelled so survivors can be named.
/// Labels in the comments: P = pruned by the default configuration, K = kept.
async fn seed(pool: &PgPool, identity: Uuid) {
    let mut sql = String::new();
    // (tag, expires_at, consumed_at)
    for (n, expires, consumed) in [
        (1, "now() - interval '30 days'", "null"), // P expired long ago
        (2, "now() - interval '1 day'", "null"),   // K expired inside the 7-day window
        (3, "now() + interval '10 minutes'", "null"), // K live
        (4, "now() + interval '1 hour'", "now() - interval '10 days'"), // P consumed long ago
        (5, "now() + interval '1 hour'", "now() - interval '1 hour'"), // K consumed just now
    ] {
        sql += &format!("insert into magic_link(identity_id,token_hash,expires_at,consumed_at) values('{identity}',{},{expires},{consumed});", tag(n));
    }
    // (tag, expires_at, rotated_at, revoked_at)
    for (n, expires, rotated, revoked) in [
        (1, "now() - interval '30 days'", "null", "null"), // P expired long ago
        (2, "now() - interval '1 day'", "null", "null"),   // K expired inside the window
        (3, "now() + interval '20 days'", "null", "null"), // K live
        (
            4,
            "now() + interval '20 days'",
            "null",
            "now() - interval '10 days'",
        ), // P revoked long ago
        (
            5,
            "now() + interval '20 days'",
            "null",
            "now() - interval '1 hour'",
        ), // K revoked just now
        (
            6,
            "now() + interval '10 days'",
            "now() - interval '20 days'",
            "null",
        ), // K rotated, unexpired
    ] {
        sql += &format!("insert into refresh_token(identity_id,token_hash,expires_at,rotated_at,revoked_at) values('{identity}',{},{expires},{rotated},{revoked});", tag(n));
    }
    for (n, started) in [
        (1, "now() - interval '1 hour'"),     // P window long over
        (2, "now() - interval '14 minutes'"), // K window still open
        (3, "now() - interval '16 minutes'"), // P window just over
        (4, "now()"),                         // K
    ] {
        sql += &format!(
            "insert into auth_request_window(key_hash,started_at) values({},{started});",
            tag(n)
        );
    }
    for (n, expires) in [
        (1, "now() - interval '30 days'"),   // P
        (2, "now() - interval '1 day'"),     // K inside the window
        (3, "now() + interval '5 minutes'"), // K live
    ] {
        sql += &format!("insert into device_enrollment_challenge(id,identity_id,expires_at,challenge) values({},'{identity}',{expires},'{{}}');", uuid_tag(n));
    }
    // (tag, created, expires, consumed): a code lives 60 seconds, used up or not.
    for (n, created, expires, consumed, outcome) in [
        (
            1,
            "now() - interval '30 days'",
            "now() - interval '30 days' + interval '60 seconds'",
            "null",
            "null",
        ), // P expired long ago, never used
        (
            2,
            "now() - interval '40 days'",
            "now() - interval '40 days' + interval '60 seconds'",
            "now() - interval '40 days'",
            "'redeemed'",
        ), // P used long ago
        (
            3,
            "now() - interval '1 day'",
            "now() - interval '1 day' + interval '60 seconds'",
            "null",
            "null",
        ), // K expired inside the window
        (
            4,
            "now() - interval '10 seconds'",
            "now() + interval '50 seconds'",
            "null",
            "null",
        ), // K live
    ] {
        sql += &format!("insert into app_handover_code(identity_id,code_hash,challenge,state,platform,session_generation,created_at,expires_at,consumed_at,outcome) values('{identity}',{},{},'retention-test-state-0000','desktop',0,{created},{expires},{consumed},{outcome});", tag(n), tag(n));
    }
    for (n, recorded) in [
        (1, "now() - interval '40 days'"), // P
        (2, "now() - interval '29 days'"), // K
        (3, "now()"),                      // K
    ] {
        sql += &format!("insert into web_metric(page_id,metric,host,page,value,recorded_at) values({},'visit','homepage','home',1,{recorded});", uuid_tag(n));
    }
    for (key, status, created) in [
        ("old-done", 201, "now() - interval '25 hours'"), // P past the replay window
        ("old-pending", 0, "now() - interval '25 hours'"), // P abandoned reservation
        ("fresh-done", 201, "now() - interval '1 hour'"), // K
        ("fresh-pending", 0, "now() - interval '1 hour'"), // K
        ("edge", 201, "now() - interval '23 hours'"),     // K still replayable
    ] {
        sql += &format!("insert into idempotency_key(identity_id,idempotency_key,request_path,response_status,response_body,created_at) values('{identity}','{key}','/x',{status},'{{}}',{created});");
    }
    sqlx::raw_sql(&sql).execute(pool).await.unwrap();
}

/// Operator sign-in links (schema 47). Only the owner credential can make one, so this takes the
/// owner pool. Labels: P = pruned by the default configuration, K = kept, S = swept to `expired`
/// (with an audit row) by the pass before the prune. Rows that "were used long ago" are made with
/// the table's triggers off, because the table rightly refuses to mark an expired link used.
async fn seed_operator_links(owner: &PgPool, identity: Uuid) {
    let mut sql = String::from("alter table operator_sign_in_link disable trigger user;");
    // (tag, created, expires, consumed, outcome)
    for (n, created, expires, consumed, outcome) in [
        (
            1,
            "now() - interval '30 days'",
            "now() - interval '30 days' + interval '15 minutes'",
            "null",
            "null",
        ), // P and S: expired long ago, never used
        (
            2,
            "now() - interval '1 day'",
            "now() - interval '1 day' + interval '15 minutes'",
            "null",
            "null",
        ), // K and S: expired inside the window
        (
            3,
            "now() - interval '1 minute'",
            "now() + interval '14 minutes'",
            "null",
            "null",
        ), // K live
        (
            4,
            "now() - interval '10 days'",
            "now() - interval '10 days' + interval '15 minutes'",
            "now() - interval '10 days'",
            "'used'",
        ), // P used long ago
        (
            5,
            "now() - interval '2 minutes'",
            "now() + interval '13 minutes'",
            "now() - interval '1 minute'",
            "'used'",
        ), // K used just now
    ] {
        sql += &format!("insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,created_at,expires_at,consumed_at,outcome) values('{identity}',{},0,'retention-test',{created},{expires},{consumed},{outcome});", tag(n));
    }
    sql += "alter table operator_sign_in_link enable trigger user;";
    sqlx::raw_sql(&sql).execute(owner).await.unwrap();
}

async fn survivors(pool: &PgPool, query: &str) -> Vec<String> {
    sqlx::query_scalar(query).fetch_all(pool).await.unwrap()
}

async fn row_counts(pool: &PgPool) -> BTreeMap<String, i64> {
    let tables: Vec<String> = sqlx::query_scalar(
        "select c.relname::text from pg_class c join pg_namespace n on n.oid=c.relnamespace
         where n.nspname='public' and c.relkind='r' order by 1",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut counts = BTreeMap::new();
    for table in tables {
        let count = sqlx::query_scalar(&format!("select count(*) from public.\"{table}\""))
            .fetch_one(pool)
            .await
            .unwrap();
        counts.insert(table, count);
    }
    counts
}

fn config(batch_rows: i64) -> RetentionConfig {
    RetentionConfig {
        batch_rows,
        interval: Duration::from_millis(40),
        ..RetentionConfig::default()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_expired_rows_are_pruned_under_the_restricted_login(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let identity = new_identity(&runtime.pool).await;
    seed(&runtime.pool, identity).await;
    seed_operator_links(&runtime.owner, identity).await;
    let stats = Stats::default();

    let outcome = retention::run_once(&runtime.pool, &config(1000), &stats)
        .await
        .unwrap();
    // Table order: magic_link, refresh_token, auth_request_window,
    // device_enrollment_challenge, app_handover_code, web_metric, idempotency_key,
    // operator_sign_in_link.
    assert_eq!(outcome, Outcome::Ran([2, 2, 2, 1, 2, 1, 2, 2]));
    // The unused links past their time (1 and 2) were marked expired, each with one audit row,
    // before the old one (1) was pruned; the others are as they were.
    assert_eq!(
        survivors(
            &runtime.pool,
            "select get_byte(token_hash,0)::text || ':' || coalesce(outcome,'live') from operator_sign_in_link order by 1"
        )
        .await,
        ["2:expired", "3:live", "5:used"]
    );
    assert_eq!(
        survivors(
            &runtime.pool,
            "select action from operation_audit where action like 'auth.operator_link.%'"
        )
        .await,
        ["auth.operator_link.expired", "auth.operator_link.expired"]
    );

    let tags = |table: &str, column: &str| {
        format!("select get_byte({column},0)::text from {table} order by 1")
    };
    assert_eq!(
        survivors(&runtime.pool, &tags("magic_link", "token_hash")).await,
        ["2", "3", "5"]
    );
    assert_eq!(
        survivors(&runtime.pool, &tags("refresh_token", "token_hash")).await,
        ["2", "3", "5", "6"]
    );
    assert_eq!(
        survivors(&runtime.pool, &tags("auth_request_window", "key_hash")).await,
        ["2", "4"]
    );
    assert_eq!(
        survivors(
            &runtime.pool,
            "select right(id::text,1) from device_enrollment_challenge order by 1"
        )
        .await,
        ["2", "3"]
    );
    assert_eq!(
        survivors(
            &runtime.pool,
            "select get_byte(code_hash,0)::text from app_handover_code order by 1"
        )
        .await,
        ["3", "4"]
    );
    assert_eq!(
        survivors(
            &runtime.pool,
            "select right(page_id::text,1) from web_metric order by 1"
        )
        .await,
        ["2", "3"]
    );
    assert_eq!(
        survivors(
            &runtime.pool,
            "select idempotency_key from idempotency_key order by 1"
        )
        .await,
        ["edge", "fresh-done", "fresh-pending"]
    );

    // A second pass finds nothing, and the counters carry the first pass's totals.
    let outcome = retention::run_once(&runtime.pool, &config(1000), &stats)
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ran([0; 8]));
    // ... and no second audit row for a link that was already swept.
    let swept: i64 = sqlx::query_scalar(
        "select count(*) from operation_audit where action='auth.operator_link.expired'",
    )
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(swept, 2);
    let snapshot = stats.snapshot();
    assert_eq!(snapshot["runs"], 2);
    assert_eq!(snapshot["pruned"]["magic_link"], 2);
    assert_eq!(snapshot["pruned"]["idempotency_key"], 2);
    assert_eq!(snapshot["failures"], 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn windows_are_configurable(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let identity = new_identity(&runtime.pool).await;
    seed(&runtime.pool, identity).await;
    seed_operator_links(&runtime.owner, identity).await;
    // One-day windows still keep the 1-day-old row only if it has not passed a full
    // day yet; the 29-day-old metric goes with a 10-day metrics window.
    let tight = RetentionConfig {
        auth_days: 365,
        metrics_days: 10,
        ..config(1000)
    };
    let outcome = retention::run_once(&runtime.pool, &tight, &Stats::default())
        .await
        .unwrap();
    // A year of grace keeps every auth row except the request windows (always 15
    // minutes) and the 25-hour-old idempotency keys; metrics older than 10 days go.
    assert_eq!(outcome, Outcome::Ran([0, 0, 2, 0, 0, 2, 2, 0]));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn batches_are_bounded_and_the_pass_drains_the_backlog(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let backlog = |n: i32| {
        format!("insert into web_metric(page_id,metric,host,page,value,recorded_at) select gen_random_uuid(),'visit','homepage','home',1,now()-interval '90 days' from generate_series(1,{n})")
    };
    sqlx::raw_sql(&backlog(25))
        .execute(&runtime.pool)
        .await
        .unwrap();
    sqlx::raw_sql("insert into web_metric(page_id,metric,host,page,value) values(gen_random_uuid(),'visit','homepage','home',1)")
        .execute(&runtime.pool)
        .await
        .unwrap();

    let mut connection = runtime.pool.acquire().await.unwrap();
    let cfg = config(10);
    let mut batches = Vec::new();
    for _ in 0..4 {
        batches.push(
            retention::prune_batch(&mut connection, "web_metric", &cfg)
                .await
                .unwrap(),
        );
    }
    assert_eq!(batches, [10, 10, 5, 0]);
    drop(connection);
    let live: i64 = sqlx::query_scalar("select count(*) from web_metric")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(live, 1, "the current row is never pruned");

    // The loop of one pass keeps going until a batch comes back short.
    sqlx::raw_sql(&backlog(25))
        .execute(&runtime.pool)
        .await
        .unwrap();
    let outcome = retention::run_once(&runtime.pool, &config(10), &Stats::default())
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ran([0, 0, 0, 0, 0, 25, 0, 0]));
    // An unknown table name is refused rather than interpolated.
    let mut connection = runtime.pool.acquire().await.unwrap();
    assert!(retention::prune_batch(&mut connection, "event", &cfg)
        .await
        .is_err());
    drop(connection);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn history_and_every_other_table_are_untouched(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let identity = new_identity(&runtime.pool).await;
    let document: Uuid = sqlx::query_scalar(
        "insert into document(title,created_by) values('History',$1) returning id",
    )
    .bind(identity)
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    for body in ["first", "second", "third"] {
        log::append(
            &runtime.pool,
            DocumentId(document),
            &EventPayload::CommentAdded {
                node_id: None,
                body: body.into(),
            },
            IdentityId(identity),
        )
        .await
        .unwrap();
    }
    SnapshotEngine::new(runtime.pool.clone())
        .take(DocumentId(document), SnapshotReason::Periodic)
        .await
        .unwrap();
    sqlx::query(
        "insert into operation_audit(actor_id,action,resource_id) values($1,'retention.test',$2)",
    )
    .bind(identity)
    .bind(document)
    .execute(&runtime.pool)
    .await
    .unwrap();
    seed(&runtime.pool, identity).await;
    seed_operator_links(&runtime.owner, identity).await;

    let before = row_counts(&runtime.pool).await;
    for history in ["event", "snapshot", "operation_audit", "document"] {
        assert!(before[history] > 0, "{history} was not seeded");
    }
    retention::run_once(&runtime.pool, &config(3), &Stats::default())
        .await
        .unwrap();
    let after = row_counts(&runtime.pool).await;
    for (table, count) in &before {
        if TARGETS.contains(&table.as_str()) {
            assert!(after[table] < *count, "{table} should have lost rows");
        } else if table == "operation_audit" {
            // The one thing the pass adds to history: an audit row for each operator sign-in
            // link it marks expired. Nothing is edited or removed.
            let swept: i64 = sqlx::query_scalar(
                "select count(*) from operation_audit where action='auth.operator_link.expired'",
            )
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
            assert_eq!(swept, 2);
            assert_eq!(
                after[table],
                *count + swept,
                "{table} only gains the expiry rows"
            );
        } else {
            assert_eq!(after[table], *count, "{table} must not change");
        }
    }

    // The privileges the job relies on: DELETE on exactly the mutable tables it
    // prunes, none on history.
    for table in TARGETS {
        let can: bool = sqlx::query_scalar("select has_table_privilege(current_user,$1,'DELETE')")
            .bind(format!("public.{table}"))
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
        assert!(can, "{table}");
    }
    for table in [
        "event",
        "snapshot",
        "operation_audit",
        "erasure_receipt",
        "document_version",
    ] {
        let can: bool = sqlx::query_scalar("select has_table_privilege(current_user,$1,'DELETE')")
            .bind(format!("public.{table}"))
            .fetch_one(&runtime.pool)
            .await
            .unwrap();
        assert!(!can, "{table}");
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_one_instance_prunes_at_a_time(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let expired = "insert into web_metric(page_id,metric,host,page,value,recorded_at) select gen_random_uuid(),'visit','homepage','home',1,now()-interval '90 days' from generate_series(1,40)";
    sqlx::raw_sql(expired).execute(&runtime.pool).await.unwrap();

    // Another replica is mid-pass: it holds the session lock on its own connection.
    let mut holder = runtime.pool.acquire().await.unwrap().detach();
    sqlx::query("select pg_advisory_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut holder)
        .await
        .unwrap();
    let stats = Stats::default();
    let outcome = retention::run_once(&runtime.pool, &config(1000), &stats)
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Skipped);
    assert_eq!(stats.snapshot()["skipped_lock_held"], 1);
    assert_eq!(stats.snapshot()["runs"], 0);
    let left: i64 = sqlx::query_scalar("select count(*) from web_metric")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(left, 40, "a skipped pass must not delete");

    // A holder that dies (here: closes) releases the lock with its session.
    holder.close().await.unwrap();
    let outcome = retention::run_once(&runtime.pool, &config(1000), &stats)
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ran([0, 0, 0, 0, 0, 40, 0, 0]));

    // Two passes started together: one runs, the other skips, nothing is deleted twice.
    sqlx::raw_sql(expired).execute(&runtime.pool).await.unwrap();
    let small = config(5);
    let (first, second) = tokio::join!(
        retention::run_once(&runtime.pool, &small, &stats),
        retention::run_once(&runtime.pool, &small, &stats),
    );
    let outcomes = [first.unwrap(), second.unwrap()];
    assert!(outcomes.contains(&Outcome::Skipped), "{outcomes:?}");
    assert!(
        outcomes.contains(&Outcome::Ran([0, 0, 0, 0, 0, 40, 0, 0])),
        "{outcomes:?}"
    );
    runtime.close().await;
}

fn state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: [7u8; 32],
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    )
}

async fn expired_metrics(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "select count(*) from web_metric where recorded_at < now() - interval '30 days'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn eventually<F: std::future::Future<Output = bool>>(
    what: &str,
    mut check: impl FnMut() -> F,
) {
    for _ in 0..200 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn master_switch_off_prunes_nothing_and_on_prunes_in_the_background(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let expired = "insert into web_metric(page_id,metric,host,page,value,recorded_at) select gen_random_uuid(),'visit','homepage','home',1,now()-interval '90 days' from generate_series(1,12)";
    sqlx::raw_sql(expired).execute(&runtime.pool).await.unwrap();

    let off = state(runtime.pool.clone());
    let none = retention::spawn(
        off.clone(),
        RetentionConfig {
            enabled: false,
            ..config(1000)
        },
    );
    assert!(none.is_none());
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(expired_metrics(&runtime.pool).await, 12);
    assert_eq!(off.retention.snapshot()["enabled"], false);
    assert_eq!(off.retention.snapshot()["runs"], 0);

    let on = state(runtime.pool.clone());
    let task = retention::spawn(on.clone(), config(1000)).expect("switch is on");
    eventually("the background pass", || async {
        expired_metrics(&runtime.pool).await == 0
    })
    .await;
    assert_eq!(on.retention.snapshot()["enabled"], true);
    assert_eq!(on.retention.snapshot()["pruned"]["web_metric"], 12);
    task.abort();
    let _ = task.await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_loop_survives_a_database_error_and_retries(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    let identity = new_identity(&runtime.pool).await;
    seed(&runtime.pool, identity).await;
    seed_operator_links(&runtime.owner, identity).await;
    // web_metric disappears (a failing table); the other tables must still be pruned.
    sqlx::raw_sql("alter table web_metric rename to web_metric_away")
        .execute(&runtime.owner)
        .await
        .unwrap();

    let stats = Stats::default();
    let task = tokio::spawn(retention::run_loop(
        runtime.pool.clone(),
        config(1000),
        stats.clone(),
    ));
    eventually("a counted failure", || async {
        stats.snapshot()["failures"].as_u64() > Some(1)
    })
    .await;
    let magic_left: i64 = sqlx::query_scalar("select count(*) from magic_link")
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
    assert_eq!(magic_left, 3, "one failing table must not stop the others");
    assert_eq!(stats.snapshot()["runs"], 0);

    // The table returns; the next pass completes and prunes it.
    sqlx::raw_sql("alter table web_metric_away rename to web_metric")
        .execute(&runtime.owner)
        .await
        .unwrap();
    eventually("a successful retry", || async {
        stats.snapshot()["runs"].as_u64() >= Some(1)
    })
    .await;
    assert_eq!(stats.snapshot()["pruned"]["web_metric"], 1);
    task.abort();
    let _ = task.await;
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_closed_pool_is_an_error_not_a_panic(pool: PgPool) {
    let runtime = RuntimeDatabase::new(pool).await;
    runtime.pool.close().await;
    let stats = Stats::default();
    assert!(retention::run_once(&runtime.pool, &config(10), &stats)
        .await
        .is_err());
    runtime.close().await;
}
