//! The engine: `engine-cli migrate` (data upgrades, on every start), then `engine-api` on a
//! loopback port chosen at each start, kept running by a supervisor that restarts it if it dies.
//!
//! The engine's own rules hold unchanged: it never holds the database owner's login, it refuses
//! to start unless the schema is the one it was built for, and it answers only with a bearer token
//! the app minted for the local person (`session.rs`).

use super::layout::{Layout, Programs};
use super::pg::{self, ENGINE_DB, OWNER, RUNTIME};
use super::procs;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use zeroize::Zeroizing;

/// The three secrets of one local workspace, as the credential store holds them.
pub struct Keys {
    pub owner_password: Zeroizing<String>,
    pub runtime_password: Zeroizing<String>,
    /// The engine's token key: 32 random bytes as hex.
    pub paseto: Zeroizing<String>,
    /// What the app presents to the engine to mint a local session (at least 32 characters).
    pub service: Zeroizing<String>,
}

pub fn database_url(user: &str, password: &str, port: u16, database: &str) -> Zeroizing<String> {
    Zeroizing::new(format!("postgres://{user}:{password}@127.0.0.1:{port}/{database}"))
}

/// Applies the engine's migrations and gives the runtime login its explicit privileges. Both are
/// idempotent; the second also runs after an update that added tables.
pub async fn migrate(
    programs: &Programs,
    pg_port: u16,
    keys: &Keys,
) -> Result<(), String> {
    let owner_url = database_url(OWNER, &keys.owner_password, pg_port, ENGINE_DB);
    let runtime_url = database_url(RUNTIME, &keys.runtime_password, pg_port, ENGINE_DB);
    let secrets = [
        keys.owner_password.as_str(),
        keys.runtime_password.as_str(),
    ];
    for args in [vec!["migrate"], vec!["grant-runtime", RUNTIME], vec!["check-runtime"]] {
        let mut cmd = procs::command(&programs.engine_cli);
        cmd.args(&args)
            .env("MIGRATION_DATABASE_URL", owner_url.as_str())
            .env("DATABASE_URL", runtime_url.as_str())
            .env("RUST_LOG", "warn");
        if args[0] == "check-runtime" {
            // The runtime check must see only the restricted login.
            cmd.env_remove("MIGRATION_DATABASE_URL");
        }
        let out = procs::run_to_end(cmd, Duration::from_secs(180))
            .await
            .map_err(|e| e.to_string())?;
        if !out.success {
            let detail = procs::scrub(&format!("{} {}", out.stderr, out.stdout), &secrets);
            super::applog::note(&format!("engine-cli {} failed: {}", args[0], detail.chars().take(300).collect::<String>()));
            return Err(format!(
                "The workspace's data could not be brought up to date (step: {}).",
                args[0]
            ));
        }
    }
    Ok(())
}

/// Where the running engine answers, as the rest of the app sees it.
#[derive(Clone, Default)]
pub struct Base(Arc<RwLock<Option<String>>>);

impl Base {
    pub fn get(&self) -> Option<String> {
        self.0.read().ok().and_then(|g| g.clone())
    }
    pub fn set(&self, value: Option<String>) {
        if let Ok(mut g) = self.0.write() {
            *g = value;
        }
    }
}

/// Everything one start of the engine needs.
pub struct Launch {
    pub programs: Programs,
    pub layout: Layout,
    pub pg_port: u16,
    pub keys: Arc<Keys>,
}

pub struct Running {
    pub child: tokio::process::Child,
    pub port: u16,
}

/// Starts the engine and waits until it answers `/healthz`.
pub async fn spawn(launch: &Launch) -> Result<Running, String> {
    stop_orphan(&launch.layout).await;
    let port = pg::free_port().map_err(|_| "No local port is free.".to_string())?;
    procs::rotate(&launch.layout.engine_log(), 5 * 1024 * 1024);
    let mut cmd = procs::command(&launch.programs.engine_api);
    cmd.env(
        "DATABASE_URL",
        database_url(RUNTIME, &launch.keys.runtime_password, launch.pg_port, ENGINE_DB).as_str(),
    )
    .env("PASETO_LOCAL_KEY", launch.keys.paseto.as_str())
    .env("WEB_SERVICE_KEY", launch.keys.service.as_str())
    .env("API_HOST", "127.0.0.1")
    .env("API_PORT", port.to_string())
    .env("APP_ENV", "production")
    .env("TOKEN_TTL_SECONDS", "3600")
    .env("PUBLIC_API_BASE_URL", format!("http://127.0.0.1:{port}"))
    .env("RUST_LOG", "warn")
    .env_remove("MIGRATION_DATABASE_URL")
    .env_remove("MIGRATION_DATABASE_URL_FILE")
    .kill_on_drop(true);
    procs::log_to(&mut cmd, &launch.layout.engine_log()).map_err(|_| "The engine's log could not be opened.".to_string())?;
    let child = cmd
        .spawn()
        .map_err(|_| "The engine could not be started.".to_string())?;
    procs::bind_to_app(&child);
    if let Some(pid) = child.id() {
        let _ = std::fs::write(launch.layout.engine_pid_file(), pid.to_string());
    }
    let mut running = Running { child, port };
    let url = format!("http://127.0.0.1:{port}/healthz");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "The engine could not be reached.".to_string())?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(Some(status)) = running.child.try_wait() {
            return Err(format!(
                "The engine stopped while starting (exit {}). See the engine log in the workspace folder.",
                status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into())
            ));
        }
        if let Ok(response) = http.get(&url).send().await {
            if response.status().is_success() {
                return Ok(running);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = running.child.kill().await;
            return Err("The engine did not answer in time.".to_string());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// An engine that an earlier run of the app left behind is ended before a new one starts.
pub async fn stop_orphan(layout: &Layout) {
    let path = layout.engine_pid_file();
    let Some(pid) = std::fs::read_to_string(&path).ok().and_then(|t| t.trim().parse::<u32>().ok()) else {
        return;
    };
    if procs::alive(pid).await {
        // Only a process that is still the engine is ended; a reused id is left alone.
        if let Some(name) = procs::program_of(pid).await {
            if name.starts_with("engine-api") {
                procs::kill(pid).await;
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

/// What the supervisor tells the rest of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Up,
    Down { restarting: bool },
    GaveUp(String),
}

/// Keeps the engine running until `shutdown` turns true. `base` always names the live engine (or
/// nothing while it restarts). `first` is the engine the caller already started.
pub async fn supervise(
    launch: Launch,
    first: Running,
    base: Base,
    mut shutdown: watch::Receiver<bool>,
    events: impl Fn(Event) + Send + 'static,
) {
    let mut running = Some(first);
    let mut failures: Vec<tokio::time::Instant> = Vec::new();
    loop {
        let Some(mut current) = running.take() else {
            match spawn(&launch).await {
                Ok(next) => {
                    running = Some(next);
                    continue;
                }
                Err(message) => {
                    super::applog::note(&format!("engine restart failed: {message}"));
                    failures.push(tokio::time::Instant::now());
                    if give_up(&mut failures) {
                        events(Event::GaveUp(message));
                        return;
                    }
                    tokio::time::sleep(backoff(failures.len())).await;
                    continue;
                }
            }
        };
        base.set(Some(format!("http://127.0.0.1:{}", current.port)));
        events(Event::Up);
        tokio::select! {
            _ = current.child.wait() => {
                base.set(None);
                super::applog::note("the engine stopped; starting it again");
                failures.push(tokio::time::Instant::now());
                if give_up(&mut failures) {
                    events(Event::GaveUp("The engine keeps stopping. See the engine log in the workspace folder.".into()));
                    return;
                }
                events(Event::Down { restarting: true });
                tokio::time::sleep(backoff(failures.len())).await;
            }
            _ = shutdown.changed() => {
                base.set(None);
                let _ = current.child.kill().await;
                let _ = std::fs::remove_file(launch.layout.engine_pid_file());
                return;
            }
        }
    }
}

/// Five stops inside a minute is a fault, not a hiccup.
fn give_up(failures: &mut Vec<tokio::time::Instant>) -> bool {
    let now = tokio::time::Instant::now();
    failures.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    failures.len() >= 5
}

fn backoff(n: usize) -> Duration {
    Duration::from_millis((250u64 << n.min(5)).min(5000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_grows_and_stops_growing() {
        assert_eq!(backoff(1), Duration::from_millis(500));
        assert_eq!(backoff(3), Duration::from_millis(2000));
        assert_eq!(backoff(9), Duration::from_millis(5000));
    }

    #[tokio::test]
    async fn five_quick_failures_give_up_and_old_ones_do_not_count() {
        let mut v = Vec::new();
        for _ in 0..4 {
            v.push(tokio::time::Instant::now());
        }
        assert!(!give_up(&mut v));
        v.push(tokio::time::Instant::now());
        assert!(give_up(&mut v));
    }

    #[test]
    fn the_connection_url_names_loopback_and_the_login() {
        let url = database_url("dynodoc_runtime", "abc123", 5433, "dynodoc");
        assert_eq!(&**url, "postgres://dynodoc_runtime:abc123@127.0.0.1:5433/dynodoc");
    }
}
