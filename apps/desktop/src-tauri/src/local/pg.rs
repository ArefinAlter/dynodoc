//! The managed PostgreSQL: one cluster in the app's data folder, on a loopback port chosen at
//! each start, with generated passwords that only the credential store holds.
//!
//! The engine's schema (47 migrations, triggers that keep the event log append-only, a restricted
//! runtime login) is PostgreSQL-specific, so the app ships PostgreSQL and runs it itself
//! (decision 019). Everything here goes through `pg_ctl` and `initdb`: on Windows they start the
//! server with a restricted token, which is what lets the app work when it was started elevated
//! (PostgreSQL itself refuses to run with administrator rights).
//!
//! A cluster that a crashed app left running is adopted, not started twice, and one that was killed
//! mid-write recovers from its write-ahead log at the next start (`pg_ctl start` waits for it).

use super::layout::{Layout, Programs};
use super::procs;
use std::time::Duration;
use tokio_postgres::{Client, NoTls};

pub const OWNER: &str = "dynodoc_owner";
pub const RUNTIME: &str = "dynodoc_runtime";
pub const ENGINE_DB: &str = "dynodoc";
pub const APP_DB: &str = "dynodoc_app";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgError {
    /// What to tell the person.
    Failed(String),
}

impl std::fmt::Display for PgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Failed(m) = self;
        f.write_str(m)
    }
}

impl std::error::Error for PgError {}

fn fail<T>(message: impl Into<String>) -> Result<T, PgError> {
    Err(PgError::Failed(message.into()))
}

pub struct Pg {
    programs: Programs,
    layout: Layout,
}

/// A free loopback port (bound and released; the server takes it a moment later).
pub fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// The port in a `postmaster.pid` file (its fourth line), when the file says one.
pub fn port_in_pid_file(text: &str) -> Option<u16> {
    text.lines().nth(3)?.trim().parse().ok()
}

/// The process id in a `postmaster.pid` file (its first line).
pub fn pid_in_pid_file(text: &str) -> Option<u32> {
    text.lines().next()?.trim().parse().ok()
}

/// The settings every start passes (not written to the cluster's config, so an old setting
/// can never outlive an update).
pub fn server_options(port: u16) -> String {
    [
        format!("-c port={port}"),
        "-c listen_addresses=127.0.0.1".to_string(),
        "-c max_connections=40".to_string(),
        "-c shared_buffers=64MB".to_string(),
        "-c fsync=on".to_string(),
        "-c synchronous_commit=on".to_string(),
        "-c full_page_writes=on".to_string(),
        "-c password_encryption=scram-sha-256".to_string(),
        "-c max_wal_size=256MB".to_string(),
        "-c log_min_messages=warning".to_string(),
        "-c timezone=UTC".to_string(),
        "-c lc_messages=C".to_string(),
    ]
    .join(" ")
        + if cfg!(windows) {
            ""
        } else {
            " -c unix_socket_directories="
        }
}

impl Pg {
    pub fn new(programs: Programs, layout: Layout) -> Self {
        Self { programs, layout }
    }

    fn cmd(&self, tool: &str) -> tokio::process::Command {
        let mut cmd = procs::command(&self.programs.pg_bin(tool));
        // The server's own messages stay in English so the log is readable by anyone.
        cmd.env("LC_MESSAGES", "C").env("LANG", "C");
        cmd
    }

    /// Creates the cluster (first start only). Takes tens of seconds on a busy computer.
    pub async fn init_cluster(&self, owner_password: &str) -> Result<(), PgError> {
        let pwfile = self.layout.run.join("owner.pw");
        std::fs::write(&pwfile, owner_password)
            .or_else(|_| fail("The local database could not be prepared."))?;
        let mut cmd = self.cmd("initdb");
        cmd.arg("-D")
            .arg(&self.layout.pgdata)
            .args(["-U", OWNER])
            .arg(format!("--pwfile={}", pwfile.display()))
            .args(["--auth-local=scram-sha-256", "--auth-host=scram-sha-256"])
            .args([
                "-E",
                "UTF8",
                "--locale=C",
                "--data-checksums",
                "--no-instructions",
                "--no-sync",
            ]);
        let out = procs::run_to_end(cmd, Duration::from_secs(300)).await;
        let _ = std::fs::remove_file(&pwfile);
        match out {
            Ok(o) if o.success => Ok(()),
            Ok(o) => {
                tracing_note("initdb failed", &procs::scrub(&o.stderr, &[owner_password]));
                // A half-made cluster must not be mistaken for a finished one.
                if !self.layout.has_cluster() {
                    let _ = std::fs::remove_dir_all(&self.layout.pgdata);
                }
                fail(
                    "The local database could not be created. See the log in the workspace folder.",
                )
            }
            Err(e) => fail(e.to_string()),
        }
    }

    /// The port of a cluster that is running now (this app's or an orphan's), if any.
    pub async fn running_port(&self) -> Option<u16> {
        let pid_file = self.layout.pgdata.join("postmaster.pid");
        let text = std::fs::read_to_string(pid_file).ok()?;
        let pid = pid_in_pid_file(&text)?;
        if !procs::alive(pid).await {
            return None;
        }
        // A pid can be reused by something else after a crash; ask pg_ctl before trusting it.
        let mut cmd = self.cmd("pg_ctl");
        cmd.arg("status").arg("-D").arg(&self.layout.pgdata);
        match procs::run_to_end(cmd, Duration::from_secs(20)).await {
            Ok(o) if o.success => port_in_pid_file(&text),
            _ => None,
        }
    }

    /// Starts the server and waits until it accepts connections (recovery included).
    pub async fn start(&self, port: u16) -> Result<(), PgError> {
        procs::rotate(&self.layout.postgres_log(), 5 * 1024 * 1024);
        let mut cmd = self.cmd("pg_ctl");
        cmd.arg("start")
            .arg("-D")
            .arg(&self.layout.pgdata)
            .args(["-w", "-t", "120", "-s"])
            .arg("-l")
            .arg(self.layout.postgres_log())
            .arg("-o")
            .arg(server_options(port));
        let log = self.layout.logs.join("pg_ctl.log");
        match procs::run_detaching(cmd, Duration::from_secs(150), &log).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                let detail = std::fs::read_to_string(&log).unwrap_or_default();
                tracing_note("pg_ctl start failed", &detail);
                fail("The local database did not start. See the log in the workspace folder.")
            }
            Err(e) => fail(e.to_string()),
        }
    }

    /// Stops the server cleanly (a checkpoint, then exit). Quiet when it is not running.
    pub async fn stop(&self) -> Result<(), PgError> {
        let mut cmd = self.cmd("pg_ctl");
        cmd.arg("stop")
            .arg("-D")
            .arg(&self.layout.pgdata)
            .args(["-m", "fast", "-w", "-t", "60", "-s"]);
        match procs::run_to_end(cmd, Duration::from_secs(90)).await {
            Ok(_) => Ok(()),
            Err(e) => fail(e.to_string()),
        }
    }

    /// A connection as `user` to `database` on the running server.
    pub async fn connect(
        port: u16,
        user: &str,
        password: &str,
        database: &str,
    ) -> Result<Client, PgError> {
        let mut config = tokio_postgres::Config::new();
        config
            .host("127.0.0.1")
            .port(port)
            .user(user)
            .password(password)
            .dbname(database)
            .connect_timeout(Duration::from_secs(10))
            .application_name("dynodoc-desktop");
        let (client, connection) = config
            .connect(NoTls)
            .await
            .or_else(|_| fail("The local database did not accept the connection."))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(client)
    }

    /// Waits until the owner login can run a statement.
    pub async fn wait_ready(
        port: u16,
        owner_password: &str,
        limit: Duration,
    ) -> Result<(), PgError> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            if let Ok(client) = Self::connect(port, OWNER, owner_password, "postgres").await {
                if client.simple_query("select 1").await.is_ok() {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return fail("The local database started but did not answer.");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Creates the roles and databases when they are not there, and brings the runtime login's
    /// password in step with the credential store. Safe to run at every start.
    pub async fn provision(
        port: u16,
        owner_password: &str,
        runtime_password: &str,
    ) -> Result<(), PgError> {
        // The passwords are hexadecimal text this app generated, so they need no quoting.
        if !is_hex(owner_password) || !is_hex(runtime_password) {
            return fail("The local database passwords are not in the expected form.");
        }
        let client = Self::connect(port, OWNER, owner_password, "postgres").await?;
        let present = |sql: &'static str, name: &'static str| {
            let client = &client;
            async move {
                client
                    .query_opt(sql, &[&name])
                    .await
                    .map(|row| row.is_some())
                    .or_else(|_| fail("The local database could not be checked."))
            }
        };
        let role = if present("select 1 from pg_roles where rolname = $1", RUNTIME).await? {
            "alter"
        } else {
            "create"
        };
        run(
            &client,
            &format!("{role} role {RUNTIME} login nosuperuser nocreatedb nocreaterole noinherit password '{runtime_password}'"),
        )
        .await?;
        for db in [ENGINE_DB, APP_DB] {
            if !present("select 1 from pg_database where datname = $1", db).await? {
                run(
                    &client,
                    &format!("create database {db} owner {OWNER} encoding 'UTF8' template template0 lc_collate 'C' lc_ctype 'C'"),
                )
                .await?;
            }
            run(&client, &format!("revoke all on database {db} from public")).await?;
        }
        run(
            &client,
            &format!("grant connect on database {ENGINE_DB} to {RUNTIME}"),
        )
        .await?;
        Ok(())
    }
}

async fn run(client: &Client, sql: &str) -> Result<(), PgError> {
    client
        .batch_execute(sql)
        .await
        .or_else(|_| fail("The local database could not be set up."))
}

fn is_hex(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A line for the log. Never given a secret (callers scrub first).
fn tracing_note(what: &str, detail: &str) {
    let first: String = detail.lines().take(6).collect::<Vec<_>>().join(" | ");
    super::applog::note(&format!(
        "{what}: {}",
        first.chars().take(400).collect::<String>()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_and_pid_come_from_the_pid_file() {
        let text = "4242\nD:/data/pgdata\n1790000000\n54329\n\n127.0.0.1\n 1234 5678\nready   \n";
        assert_eq!(port_in_pid_file(text), Some(54329));
        assert_eq!(pid_in_pid_file(text), Some(4242));
        assert_eq!(port_in_pid_file("1\n2\n"), None);
        assert_eq!(pid_in_pid_file(""), None);
    }

    #[test]
    fn the_server_listens_on_loopback_only_and_keeps_durability_on() {
        let options = server_options(54329);
        assert!(options.contains("listen_addresses=127.0.0.1"));
        assert!(options.contains("fsync=on"));
        assert!(options.contains("full_page_writes=on"));
        assert!(options.contains("synchronous_commit=on"));
        assert!(options.contains("port=54329"));
        assert!(!options.contains("0.0.0.0") && !options.contains('*'));
    }

    #[test]
    fn a_free_port_is_usable() {
        let port = free_port().unwrap();
        assert!(port > 1023);
        assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
    }

    #[test]
    fn only_generated_hex_passwords_reach_a_statement() {
        assert!(is_hex("00ff9a"));
        assert!(!is_hex("a'; drop"));
        assert!(!is_hex(""));
    }
}
