//! A thin wrapper over `docker` and `docker compose` for one bundle directory.

use crate::ctx::Ctx;
use crate::error::{CliError, Result};
use crate::runner::{Cmd, Output};
use std::path::PathBuf;

/// Runs a command as the owner inside the PostgreSQL container: the password is read from the
/// container's own secret file and never leaves it.
pub const PG_AUTH: &str = "PGPASSWORD=$(cat /run/secrets/postgres_password); export PGPASSWORD; ";
pub const PG_CONNECT: &str = "-h 127.0.0.1 -U dynodoc";

/// One service as `docker compose ps` reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ServiceState {
    pub service: String,
    pub state: String,
    pub health: String,
    pub exit_code: i64,
    pub image: String,
}

impl ServiceState {
    pub fn running(&self) -> bool {
        self.state == "running"
    }
    /// Running, and healthy when the service has a health check.
    pub fn up(&self) -> bool {
        self.running() && (self.health.is_empty() || self.health == "healthy")
    }
}

/// Database names we create must be plain identifiers (they are put into shell and SQL text).
pub fn check_ident(name: &str) -> Result<()> {
    if !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        Ok(())
    } else {
        Err(CliError::failed(
            format!("`{name}` is not a safe database name"),
            "this is a bug in dynodoctl; please report it",
        ))
    }
}

pub struct Compose<'a> {
    ctx: &'a Ctx<'a>,
    env: Vec<(String, String)>,
    files: Option<Vec<PathBuf>>,
}

impl<'a> Compose<'a> {
    pub fn new(ctx: &'a Ctx<'a>) -> Self {
        Self {
            ctx,
            env: Vec::new(),
            files: None,
        }
    }

    /// Set a variable for the commands of this wrapper only (Compose interpolation: it wins over `.env`).
    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    /// Use these compose files instead of compose.yml plus the installed overlay.
    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files = Some(files);
        self
    }

    pub fn cmd(&self, args: &[&str]) -> Cmd {
        let mut all: Vec<String> = vec!["compose".into()];
        if let Some(project) = &self.ctx.project {
            all.push("-p".into());
            all.push(project.clone());
        }
        all.push("--project-directory".into());
        all.push(self.ctx.layout.dir.display().to_string());
        let files = self
            .files
            .clone()
            .unwrap_or_else(|| self.ctx.layout.compose_files(&self.ctx.extra_files));
        for file in files {
            all.push("-f".into());
            all.push(file.display().to_string());
        }
        all.extend(args.iter().map(|a| a.to_string()));
        Cmd {
            program: "docker".into(),
            args: all,
            cwd: Some(self.ctx.layout.dir.clone()),
            env: self.env.clone(),
            scrub_env: true,
            ..Cmd::default()
        }
    }

    pub fn run(&self, args: &[&str]) -> Result<Output> {
        run_cmd(self.ctx, &self.cmd(args))
    }

    /// `docker compose ps -a`, parsed.
    pub fn ps(&self) -> Result<Vec<ServiceState>> {
        let out = self.run(&["ps", "-a", "--format", "json"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("`docker compose ps` failed: {}", short(&out.text())),
                "check that Docker is running and that you are in the bundle directory",
            ));
        }
        Ok(parse_ps(&out.stdout_text()))
    }

    /// A shell script run as the database owner in the PostgreSQL container.
    pub fn pg_sh(&self, script: &str) -> Result<Output> {
        let full = format!("{PG_AUTH}{script}");
        self.run(&["exec", "-T", "postgres", "sh", "-c", &full])
    }

    /// Same, with a file on standard input and/or standard output (a dump going in or out).
    pub fn pg_sh_io(
        &self,
        script: &str,
        stdin: Option<PathBuf>,
        stdout: Option<PathBuf>,
    ) -> Result<Output> {
        let full = format!("{PG_AUTH}{script}");
        let mut cmd = self.cmd(&["exec", "-T", "postgres", "sh", "-c", &full]);
        cmd.stdin_file = stdin;
        cmd.stdout_file = stdout;
        run_cmd(self.ctx, &cmd)
    }

    /// A one-off container of `service` (profile `maintenance` services included) running `script`
    /// with `sh -c`; `args` become `$1`, `$2`, ... of the script. Standard input is optional.
    pub fn run_script(
        &self,
        service: &str,
        script: &str,
        args: &[&str],
        stdin: Option<Vec<u8>>,
    ) -> Result<Output> {
        let mut all = vec![
            "--profile",
            "maintenance",
            "run",
            "--rm",
            "--no-deps",
            "-T",
            "--entrypoint",
            "/bin/sh",
            service,
            "-c",
            script,
            "sh",
        ];
        all.extend_from_slice(args);
        let mut cmd = self.cmd(&all);
        cmd.stdin_bytes = stdin;
        run_cmd(self.ctx, &cmd)
    }

    /// Run SQL against `db` as the owner; returns trimmed unaligned tuples.
    pub fn psql(&self, db: &str, sql: &str) -> Result<Output> {
        check_ident(db)?;
        if sql.contains('"') || sql.contains('`') || sql.contains('$') {
            return Err(CliError::failed(
                "internal: refusing SQL with shell metacharacters",
                "this is a bug in dynodoctl; please report it",
            ));
        }
        self.pg_sh(&format!(
            "psql {PG_CONNECT} -d {db} -v ON_ERROR_STOP=1 -tA -F '|' -c \"{sql}\""
        ))
    }
}

/// Run a command; a missing program becomes an error that says what to install.
pub fn run_cmd(ctx: &Ctx<'_>, cmd: &Cmd) -> Result<Output> {
    ctx.runner.run(cmd).map_err(|e| {
        CliError::failed(
            format!("could not start `{}`: {e}", cmd.program),
            if cmd.program == "docker" {
                "install Docker Engine or Docker Desktop with Compose v2.24 or newer, and make sure `docker` is on the PATH"
            } else {
                "check that the program is installed and on the PATH"
            },
        )
    })
}

pub fn docker(ctx: &Ctx<'_>, args: &[&str]) -> Result<Output> {
    let mut cmd = Cmd::new("docker", args);
    cmd.scrub_env = true;
    run_cmd(ctx, &cmd)
}

/// Remove terminal colour sequences (`ESC [ ... m`), which tools such as engine-cli print.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Compose's own progress lines (`Container x Created`), which say nothing about a failure.
fn is_progress_line(line: &str) -> bool {
    let t = line.trim();
    ["Container ", "Network ", "Volume "]
        .iter()
        .any(|p| t.starts_with(p))
        && [
            "Creating", "Created", "Starting", "Started", "Running", "Healthy", "Waiting",
            "Removing", "Removed", "Stopping", "Stopped",
        ]
        .iter()
        .any(|s| t.ends_with(s))
}

/// Keep messages short: the last lines of a long output, without colour codes or progress noise.
pub fn short(text: &str) -> String {
    let cleaned = strip_ansi(text);
    let lines: Vec<&str> = cleaned
        .lines()
        .filter(|l| !l.trim().is_empty() && !is_progress_line(l))
        .collect();
    let start = lines.len().saturating_sub(6);
    lines[start..].join(" / ")
}

/// Parse `docker compose ps --format json`: one JSON object per line (Compose 2.21+) or one array.
pub fn parse_ps(text: &str) -> Vec<ServiceState> {
    let mut values: Vec<serde_json::Value> = Vec::new();
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(trimmed) {
            values = items;
        }
    } else {
        for line in trimmed.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                values.push(v);
            }
        }
    }
    let get = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let mut services: Vec<ServiceState> = values
        .iter()
        .map(|v| ServiceState {
            service: get(v, "Service"),
            state: get(v, "State"),
            health: get(v, "Health"),
            exit_code: v.get("ExitCode").and_then(|x| x.as_i64()).unwrap_or(0),
            image: get(v, "Image"),
        })
        .filter(|s| !s.service.is_empty())
        .collect();
    services.sort_by(|a, b| a.service.cmp(&b.service));
    services
}

/// The services that must be up for the installation to work.
pub const LONG_RUNNING: [&str; 4] = ["postgres", "api", "web", "proxy"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{Layout, ScriptedConfirm};
    use crate::runner::FakeRunner;

    #[test]
    fn parses_both_json_shapes() {
        let ndjson = "{\"Service\":\"web\",\"State\":\"running\",\"Health\":\"healthy\",\"Image\":\"dynodoc-local/web:dev\"}\n{\"Service\":\"init\",\"State\":\"exited\",\"ExitCode\":0}\n";
        let array = "[{\"Service\":\"web\",\"State\":\"running\",\"Health\":\"healthy\",\"Image\":\"dynodoc-local/web:dev\"},{\"Service\":\"init\",\"State\":\"exited\",\"ExitCode\":0}]";
        let a = parse_ps(ndjson);
        let b = parse_ps(array);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2);
        assert!(a.iter().find(|s| s.service == "web").unwrap().up());
        assert!(!a.iter().find(|s| s.service == "init").unwrap().running());
    }

    #[test]
    fn unhealthy_or_starting_is_not_up() {
        let s = ServiceState {
            service: "api".into(),
            state: "running".into(),
            health: "starting".into(),
            ..Default::default()
        };
        assert!(!s.up());
    }

    #[test]
    fn compose_command_names_the_project_directory_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        let confirm = ScriptedConfirm::new(&[]);
        let mut ctx = Ctx::new(&runner, Layout::new(dir.path()), &confirm);
        ctx.project = Some("p1".into());
        let cmd = Compose::new(&ctx)
            .with_env("DYNODOC_VERSION", "9")
            .cmd(&["ps"]);
        let line = cmd.line();
        assert!(line.starts_with("docker compose -p p1 --project-directory "));
        assert!(line.contains("compose.yml"));
        assert!(line.ends_with(" ps"));
        assert!(cmd.scrub_env);
        assert_eq!(
            cmd.env,
            vec![("DYNODOC_VERSION".to_string(), "9".to_string())]
        );
    }

    #[test]
    fn psql_refuses_shell_metacharacters_and_bad_database_names() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        let confirm = ScriptedConfirm::new(&[]);
        let ctx = Ctx::new(&runner, Layout::new(dir.path()), &confirm);
        let c = Compose::new(&ctx);
        assert!(c.psql("dynodoc", "select \"x\"").is_err());
        assert!(c.psql("dyno;doc", "select 1").is_err());
        assert!(runner.calls().is_empty());
        assert!(c.psql("dynodoc", "select 1").is_ok());
        assert!(runner.ran("psql -h 127.0.0.1 -U dynodoc -d dynodoc"));
    }

    #[test]
    fn colour_codes_and_compose_progress_lines_are_dropped_from_summaries() {
        let text = "\u{1b}[2m2026-10-22T16:00:20Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m migrations complete\n Container x-database-admin-run-1 Creating \n Container x-database-admin-run-1 Created \nError: boom\n";
        assert_eq!(
            short(text),
            "2026-10-22T16:00:20Z  INFO migrations complete / Error: boom"
        );
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("a\u{1b}[1;31mb\u{1b}[0mc"), "abc");
    }

    #[test]
    fn short_keeps_the_last_lines() {
        assert_eq!(short("a\n\nb\nc"), "a / b / c");
        assert_eq!(
            short(
                &(1..=10)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            "5 / 6 / 7 / 8 / 9 / 10"
        );
    }
}
