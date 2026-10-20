//! Retention of expired authentication rows, old web metrics and expired idempotency
//! records (docs/SELF-HOSTED-EDITION.md 6.5, P0-8).
//!
//! A background loop prunes in bounded batches, only rows that no code path can use
//! any more. It touches no history, event, snapshot, audit or erasure table, and
//! needs only the DELETE privilege the restricted runtime login already has on these
//! mutable tables (`engine_core::database`), so it adds no migration.
//!
//! Every replica runs the loop; each pass takes a PostgreSQL session advisory lock on
//! a connection of its own, so one replica at a time prunes and the others skip that
//! pass. Closing the connection (normally, on error, or when the task is cancelled)
//! ends the session and releases the lock, so a crashed holder never blocks the next.
//!
//! What counts as prunable:
//! - `magic_link`: expired, or consumed, more than `RETENTION_AUTH_DAYS` ago.
//! - `refresh_token`: expired, or revoked, more than `RETENTION_AUTH_DAYS` ago. A
//!   rotated token that has not expired is kept: `auth::store::revoke_login` still
//!   accepts it to end its login.
//! - `auth_request_window`: past its 15-minute window. A pruned row and a reset row
//!   behave the same (the next request starts a fresh window).
//! - `device_enrollment_challenge`: expired more than `RETENTION_AUTH_DAYS` ago.
//! - `web_metric`: recorded more than `RETENTION_METRICS_DAYS` ago.
//! - `idempotency_key`: older than the replay window ([`crate::idempotency::TTL`]);
//!   `idempotency::reserve` already treats such a row as absent.
use crate::{secrets::Lookup, AppState};
use serde_json::{json, Map, Value};
use sqlx::{Connection, PgConnection, PgPool};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Session advisory-lock key of the retention pass ("DYNORETN"). Advisory locks are
/// per database, so this cannot collide with another database's.
pub const LOCK_KEY: i64 = 0x4459_4E4F_5245_544E;

/// Time a single pass may spend before it stops and leaves the rest to the next one.
const PASS_BUDGET: Duration = Duration::from_secs(300);
/// Pause between full batches, so a large backlog does not starve normal requests.
const BATCH_PAUSE: Duration = Duration::from_millis(25);
/// A failed pass is retried sooner than a normal interval, but not in a tight loop.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(300);
/// The sign-in and device-enrollment request window (`auth/routes.rs`, `devices.rs`).
const REQUEST_WINDOW_SECONDS: f64 = 15.0 * 60.0;
const DAY_SECONDS: f64 = 86_400.0;

/// Number of tables the pass prunes.
pub const TABLE_COUNT: usize = 6;

/// One table's prune statement. `$1` is the batch size and `$2` the number of seconds
/// a row must be past its expiry (or older than, for metrics and idempotency keys).
/// Rows are locked with `skip locked`, so a row a request is touching is left for the
/// next pass instead of making the pass wait.
struct Spec {
    table: &'static str,
    sql: &'static str,
    seconds: fn(&RetentionConfig) -> f64,
}

static SPECS: [Spec; TABLE_COUNT] = [
    Spec {
        table: "magic_link",
        sql: "with doomed as (select ctid as row_id from magic_link
                where expires_at < now() - make_interval(secs => $2)
                   or consumed_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from magic_link using doomed where magic_link.ctid = doomed.row_id",
        seconds: |c| f64::from(c.auth_days) * DAY_SECONDS,
    },
    Spec {
        table: "refresh_token",
        sql: "with doomed as (select ctid as row_id from refresh_token
                where expires_at < now() - make_interval(secs => $2)
                   or revoked_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from refresh_token using doomed where refresh_token.ctid = doomed.row_id",
        seconds: |c| f64::from(c.auth_days) * DAY_SECONDS,
    },
    Spec {
        table: "auth_request_window",
        sql: "with doomed as (select ctid as row_id from auth_request_window
                where started_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from auth_request_window using doomed where auth_request_window.ctid = doomed.row_id",
        seconds: |_| REQUEST_WINDOW_SECONDS,
    },
    Spec {
        table: "device_enrollment_challenge",
        sql: "with doomed as (select ctid as row_id from device_enrollment_challenge
                where expires_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from device_enrollment_challenge using doomed where device_enrollment_challenge.ctid = doomed.row_id",
        seconds: |c| f64::from(c.auth_days) * DAY_SECONDS,
    },
    Spec {
        table: "web_metric",
        sql: "with doomed as (select ctid as row_id from web_metric
                where recorded_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from web_metric using doomed where web_metric.ctid = doomed.row_id",
        seconds: |c| f64::from(c.metrics_days) * DAY_SECONDS,
    },
    Spec {
        table: "idempotency_key",
        sql: "with doomed as (select ctid as row_id from idempotency_key
                where created_at < now() - make_interval(secs => $2)
                limit $1 for update skip locked)
              delete from idempotency_key using doomed where idempotency_key.ctid = doomed.row_id",
        seconds: |_| crate::idempotency::TTL.num_seconds() as f64,
    },
];

/// The tables the pass prunes, in reporting order.
pub fn table_names() -> [&'static str; TABLE_COUNT] {
    std::array::from_fn(|i| SPECS[i].table)
}

/// Retention settings. The environment names and bounds are in [`RetentionConfig::load`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionConfig {
    /// Master switch (`RETENTION_JOBS`). Off: nothing is pruned and no loop runs.
    pub enabled: bool,
    /// Time between passes.
    pub interval: Duration,
    /// Most rows one DELETE statement removes.
    pub batch_rows: i64,
    /// Days a sign-in link, refresh token or enrollment challenge is kept after it
    /// expired or was used up.
    pub auth_days: i32,
    /// Days a `web_metric` row is kept.
    pub metrics_days: i32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(3600),
            batch_rows: 1000,
            auth_days: 7,
            metrics_days: 30,
        }
    }
}

impl RetentionConfig {
    /// Read `RETENTION_JOBS` (`on`/`off`, default `on`), `RETENTION_INTERVAL_SECONDS`
    /// (60..=86400, default 3600), `RETENTION_BATCH_ROWS` (10..=10000, default 1000),
    /// `RETENTION_AUTH_DAYS` (1..=365, default 7) and `RETENTION_METRICS_DAYS`
    /// (1..=3650, default 30). Unset or empty means the default; anything else outside
    /// the bounds is appended to `problems` and the default is used.
    pub fn load(lookup: Lookup, problems: &mut Vec<String>) -> Self {
        let default = Self::default();
        let enabled = match lookup("RETENTION_JOBS")
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string_lossy().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => {
                problems.push("RETENTION_JOBS must be on or off".into());
                default.enabled
            }
        };
        let mut whole = |name: &str, default: i64, min: i64, max: i64| -> i64 {
            let Some(raw) = lookup(name).filter(|v| !v.is_empty()) else {
                return default;
            };
            raw.to_str()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .filter(|n| (min..=max).contains(n))
                .unwrap_or_else(|| {
                    problems.push(format!("{name} must be a whole number from {min} to {max}"));
                    default
                })
        };
        Self {
            enabled,
            interval: Duration::from_secs(
                whole("RETENTION_INTERVAL_SECONDS", 3600, 60, 86_400) as u64
            ),
            batch_rows: whole("RETENTION_BATCH_ROWS", 1000, 10, 10_000),
            auth_days: whole("RETENTION_AUTH_DAYS", 7, 1, 365) as i32,
            metrics_days: whole("RETENTION_METRICS_DAYS", 30, 1, 3650) as i32,
        }
    }
}

/// Rows pruned since this process started, for the admin overview. Each process
/// counts its own; these are not fleet-wide totals.
#[derive(Clone, Default)]
pub struct Stats(Arc<StatsInner>);

#[derive(Default)]
struct StatsInner {
    enabled: AtomicBool,
    pruned: [AtomicU64; TABLE_COUNT],
    /// Passes that held the lock and finished without error.
    runs: AtomicU64,
    /// Passes that found another replica holding the lock.
    skipped: AtomicU64,
    failures: AtomicU64,
    /// Unix seconds of the last finished pass; 0 when none yet.
    last_run: AtomicI64,
}

impl Stats {
    fn add(&self, table: usize, rows: u64) {
        self.0.pruned[table].fetch_add(rows, Ordering::Relaxed);
    }

    /// Counts per table and pass totals. Table names and numbers only.
    pub fn snapshot(&self) -> Value {
        let inner = &self.0;
        let pruned: Map<String, Value> = SPECS
            .iter()
            .zip(&inner.pruned)
            .map(|(spec, n)| (spec.table.to_string(), n.load(Ordering::Relaxed).into()))
            .collect();
        let last_run = chrono::DateTime::from_timestamp(inner.last_run.load(Ordering::Relaxed), 0)
            .filter(|_| inner.last_run.load(Ordering::Relaxed) > 0);
        json!({
            "enabled": inner.enabled.load(Ordering::Relaxed),
            "runs": inner.runs.load(Ordering::Relaxed),
            "skipped_lock_held": inner.skipped.load(Ordering::Relaxed),
            "failures": inner.failures.load(Ordering::Relaxed),
            "last_run_at": last_run,
            "pruned": pruned,
        })
    }
}

/// What one pass did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Another replica held the lock; nothing was pruned here.
    Skipped,
    /// Rows deleted per table, in [`table_names`] order.
    Ran([u64; TABLE_COUNT]),
}

/// Delete at most `config.batch_rows` prunable rows of `table` and return how many
/// went. Public for tests; the loop calls it until a batch comes back short.
pub async fn prune_batch(
    connection: &mut PgConnection,
    table: &str,
    config: &RetentionConfig,
) -> Result<u64, sqlx::Error> {
    let spec = SPECS
        .iter()
        .find(|spec| spec.table == table)
        .ok_or_else(|| sqlx::Error::Protocol(format!("{table} is not a retention table")))?;
    sqlx::query(spec.sql)
        .bind(config.batch_rows)
        .bind((spec.seconds)(config))
        .execute(connection)
        .await
        .map(|done| done.rows_affected())
}

/// One pass: take the lock on a dedicated connection, prune every table in batches,
/// release. A table that fails does not stop the others; the first error is returned
/// once the pass is done, and the rows already deleted stay counted.
pub async fn run_once(
    pool: &PgPool,
    config: &RetentionConfig,
    stats: &Stats,
) -> Result<Outcome, sqlx::Error> {
    // Detached: the connection leaves the pool for this pass and is closed at the end,
    // which ends the session and so releases the advisory lock whatever happens.
    let mut connection = pool.acquire().await?.detach();
    let result = locked_pass(&mut connection, config, stats).await;
    let _ = connection.close().await;
    result
}

async fn locked_pass(
    connection: &mut PgConnection,
    config: &RetentionConfig,
    stats: &Stats,
) -> Result<Outcome, sqlx::Error> {
    let locked: bool = sqlx::query_scalar("select pg_try_advisory_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut *connection)
        .await?;
    if !locked {
        stats.0.skipped.fetch_add(1, Ordering::Relaxed);
        return Ok(Outcome::Skipped);
    }
    let deadline = Instant::now() + PASS_BUDGET;
    let mut pruned = [0u64; TABLE_COUNT];
    let mut first_error = None;
    for (index, spec) in SPECS.iter().enumerate() {
        loop {
            match prune_batch(&mut *connection, spec.table, config).await {
                Ok(rows) => {
                    pruned[index] += rows;
                    stats.add(index, rows);
                    if rows < config.batch_rows as u64 || Instant::now() >= deadline {
                        break;
                    }
                    tokio::time::sleep(BATCH_PAUSE).await;
                }
                Err(error) => {
                    tracing::warn!(table = spec.table, error = %error, "retention batch failed");
                    first_error.get_or_insert(error);
                    break;
                }
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    stats.0.runs.fetch_add(1, Ordering::Relaxed);
    stats
        .0
        .last_run
        .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
    Ok(Outcome::Ran(pruned))
}

/// Run a pass, record and log its result, and return how long to wait for the next.
async fn tick(pool: &PgPool, config: &RetentionConfig, stats: &Stats) -> Duration {
    match run_once(pool, config, stats).await {
        Ok(Outcome::Ran(pruned)) => {
            if pruned.iter().sum::<u64>() > 0 {
                tracing::info!(
                    magic_link = pruned[0],
                    refresh_token = pruned[1],
                    auth_request_window = pruned[2],
                    device_enrollment_challenge = pruned[3],
                    web_metric = pruned[4],
                    idempotency_key = pruned[5],
                    "retention pass pruned expired rows"
                );
            } else {
                tracing::debug!("retention pass found nothing to prune");
            }
            config.interval
        }
        Ok(Outcome::Skipped) => {
            tracing::debug!("retention pass skipped; another replica holds the lock");
            config.interval
        }
        Err(error) => {
            stats.0.failures.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(error = %error, "retention pass failed; will retry");
            config.interval.min(RETRY_AFTER_FAILURE)
        }
    }
}

/// The loop behind [`spawn`]: wait, prune, repeat. It never returns and survives
/// database errors by retrying.
pub async fn run_loop(pool: PgPool, config: RetentionConfig, stats: Stats) {
    // The first pass waits a little so startup work is not competing with it.
    let mut wait = config.interval.min(Duration::from_secs(60));
    loop {
        tokio::time::sleep(wait).await;
        wait = tick(&pool, &config, &stats).await;
    }
}

/// Start the retention loop beside the request path, unless the master switch is off.
pub fn spawn(state: AppState, config: RetentionConfig) -> Option<tokio::task::JoinHandle<()>> {
    state
        .retention
        .0
        .enabled
        .store(config.enabled, Ordering::Relaxed);
    if !config.enabled {
        tracing::info!("retention jobs are off (RETENTION_JOBS=off)");
        return None;
    }
    tracing::info!(
        interval_seconds = config.interval.as_secs(),
        batch_rows = config.batch_rows,
        auth_days = config.auth_days,
        metrics_days = config.metrics_days,
        "retention jobs are on"
    );
    Some(tokio::spawn(run_loop(
        state.pool.clone(),
        config,
        state.retention.clone(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, ffi::OsString};

    fn load(pairs: &[(&str, &str)]) -> (RetentionConfig, Vec<String>) {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        let mut problems = Vec::new();
        let config = RetentionConfig::load(&|name| map.get(name).cloned(), &mut problems);
        (config, problems)
    }

    #[test]
    fn defaults_apply_when_nothing_is_set() {
        let (config, problems) = load(&[]);
        assert_eq!(config, RetentionConfig::default());
        assert!(problems.is_empty());
        assert!(config.enabled);
        // Empty values (a compose `${VAR:-}` pass-through) mean the default too.
        let (config, problems) = load(&[("RETENTION_JOBS", ""), ("RETENTION_AUTH_DAYS", "")]);
        assert_eq!(config, RetentionConfig::default());
        assert!(problems.is_empty());
    }

    #[test]
    fn valid_values_and_bounds_are_accepted() {
        let (config, problems) = load(&[
            ("RETENTION_JOBS", "OFF"),
            ("RETENTION_INTERVAL_SECONDS", "60"),
            ("RETENTION_BATCH_ROWS", "10000"),
            ("RETENTION_AUTH_DAYS", "365"),
            ("RETENTION_METRICS_DAYS", " 1 "),
        ]);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(!config.enabled);
        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.batch_rows, 10_000);
        assert_eq!(config.auth_days, 365);
        assert_eq!(config.metrics_days, 1);
    }

    #[test]
    fn nonsense_is_reported_by_name_and_falls_back_to_defaults() {
        let (config, problems) = load(&[
            ("RETENTION_JOBS", "maybe"),
            ("RETENTION_INTERVAL_SECONDS", "59"),
            ("RETENTION_BATCH_ROWS", "9"),
            ("RETENTION_AUTH_DAYS", "0"),
            ("RETENTION_METRICS_DAYS", "3651"),
        ]);
        assert_eq!(problems.len(), 5, "{problems:?}");
        for name in [
            "RETENTION_JOBS",
            "RETENTION_INTERVAL_SECONDS",
            "RETENTION_BATCH_ROWS",
            "RETENTION_AUTH_DAYS",
            "RETENTION_METRICS_DAYS",
        ] {
            assert!(problems.iter().any(|p| p.starts_with(name)), "{name}");
        }
        assert_eq!(config, RetentionConfig::default());
        for bad in ["-1", "1.5", "ten", "1e3", "99999999999999999999"] {
            let (_, problems) = load(&[("RETENTION_AUTH_DAYS", bad)]);
            assert_eq!(problems.len(), 1, "{bad}");
        }
    }

    #[test]
    fn every_statement_binds_batch_and_seconds_and_names_its_table() {
        for spec in &SPECS {
            assert!(
                spec.sql.contains("$1") && spec.sql.contains("$2"),
                "{}",
                spec.table
            );
            assert!(spec.sql.contains("skip locked"), "{}", spec.table);
            assert!(
                spec.sql
                    .contains(&format!("delete from {} using", spec.table)),
                "{}",
                spec.table
            );
        }
        assert_eq!(table_names()[0], "magic_link");
    }

    #[test]
    fn windows_scale_with_the_configuration() {
        let config = RetentionConfig {
            auth_days: 2,
            metrics_days: 3,
            ..RetentionConfig::default()
        };
        let seconds: Vec<f64> = SPECS.iter().map(|s| (s.seconds)(&config)).collect();
        assert_eq!(
            seconds,
            vec![172_800.0, 172_800.0, 900.0, 172_800.0, 259_200.0, 86_400.0]
        );
    }
}
