//! Starting the programs the workspace runs, without a console window and without a shell.
//!
//! Arguments are always a fixed list (never a string a page chose), secrets travel in the
//! environment of the child only (never in an argument), and a child's output goes to a log file
//! or nowhere. On Windows the engine is put in a job object that ends it when the app ends, so a
//! crash of the app leaves no engine behind.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command with no console window, no standard input and no inherited secrets of ours.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    // Never let a variable meant for another program reach the database or the engine.
    for name in [
        "DATABASE_URL",
        "MIGRATION_DATABASE_URL",
        "PGPASSWORD",
        "PGPASSFILE",
        "PGSERVICE",
    ] {
        cmd.env_remove(name);
    }
    cmd
}

/// Runs a short program to its end and returns (success, stdout) with a time limit. The
/// program's standard error is returned as text for the log only after `scrub`.
pub async fn run_to_end(mut cmd: Command, limit: Duration) -> Result<Output, RunError> {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn().map_err(|_| RunError::Spawn)?;
    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(out)) => Ok(Output {
            success: out.status.success(),
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Ok(Err(_)) => Err(RunError::Spawn),
        Err(_) => Err(RunError::TimedOut),
    }
}

/// Runs a program that starts a long-lived child (such as `pg_ctl start`) and waits only for the
/// program itself: its output goes to a file, never to a pipe, because the child it leaves
/// running would keep a pipe open and the wait would never end.
pub async fn run_detaching(
    mut cmd: Command,
    limit: Duration,
    log: &Path,
) -> Result<bool, RunError> {
    log_to(&mut cmd, log).map_err(|_| RunError::Spawn)?;
    cmd.kill_on_drop(false);
    let mut child = cmd.spawn().map_err(|_| RunError::Spawn)?;
    match tokio::time::timeout(limit, child.wait()).await {
        Ok(Ok(status)) => Ok(status.success()),
        Ok(Err(_)) => Err(RunError::Spawn),
        Err(_) => Err(RunError::TimedOut),
    }
}

#[derive(Debug)]
pub struct Output {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    Spawn,
    TimedOut,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Spawn => "A program of this installation could not be started.",
            Self::TimedOut => "A program of this installation did not finish in time.",
        })
    }
}

/// Appends a child's standard output and error to a log file (created if need be).
pub fn log_to(cmd: &mut Command, log: &Path) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    let err = file.try_clone()?;
    cmd.stdout(Stdio::from(file)).stderr(Stdio::from(err));
    Ok(())
}

/// A log that has grown past `limit` bytes is moved aside (one old copy is kept).
pub fn rotate(log: &Path, limit: u64) {
    if std::fs::metadata(log)
        .map(|m| m.len() > limit)
        .unwrap_or(false)
    {
        let old = log.with_extension("log.old");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(log, old);
    }
}

/// Removes a secret's characters from text that is about to reach a log or an error: a program's
/// own message might quote a connection URL.
pub fn scrub(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        if s.len() >= 6 {
            out = out.replace(s, "[secret]");
        }
    }
    // A connection URL's password, whatever it was.
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(at) = rest.find("://") {
        result.push_str(&rest[..at + 3]);
        rest = &rest[at + 3..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '/')
            .unwrap_or(rest.len());
        let authority = &rest[..end];
        match authority.rfind('@') {
            Some(i) if authority[..i].contains(':') => {
                let user = authority[..i].split(':').next().unwrap_or("");
                result.push_str(user);
                result.push_str(":[secret]");
                result.push_str(&authority[i..]);
            }
            _ => result.push_str(authority),
        }
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

/// True when a process with this id exists and is running (any program).
pub async fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("tasklist");
        cmd.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW);
        match run_to_end(cmd, Duration::from_secs(10)).await {
            Ok(out) => out.stdout.contains(&format!("\"{pid}\"")),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}

/// The name of a running process's program (lower case, without folders), if it can be read.
pub async fn program_of(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("tasklist");
        cmd.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = run_to_end(cmd, Duration::from_secs(10)).await.ok()?;
        let first = out.stdout.lines().next()?;
        let name = first.split(',').next()?.trim().trim_matches('"');
        (!name.is_empty() && !name.starts_with("INFO")).then(|| name.to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        let link = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
        link.file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
    }
}

/// Ends a process by id (forcefully). Used only for an engine this app started in an earlier run.
pub async fn kill(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = run_to_end(cmd, Duration::from_secs(10)).await;
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("kill");
        cmd.args(["-9", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = run_to_end(cmd, Duration::from_secs(10)).await;
    }
}

/// Puts a started child in a job that ends it when this process ends (Windows). The job handle is
/// kept for the life of the process; elsewhere this does nothing (the supervisor stops the child
/// on a clean exit, and a stale one is removed at the next start by its recorded process id).
pub fn bind_to_app(child: &Child) {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        static JOB: OnceLock<Option<win32job::Job>> = OnceLock::new();
        let job = JOB.get_or_init(|| {
            let job = win32job::Job::create().ok()?;
            let mut info = job.query_extended_limit_info().ok()?;
            info.limit_kill_on_job_close();
            job.set_extended_limit_info(&info).ok()?;
            Some(job)
        });
        if let (Some(job), Some(handle)) = (job.as_ref(), child.raw_handle()) {
            let _ = job.assign_process(handle as isize);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = child;
    }
}

pub fn as_os<S: AsRef<OsStr> + ?Sized>(s: &S) -> &OsStr {
    s.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_url_loses_its_password_and_known_secrets_vanish() {
        let text = "failed: postgres://dynodoc_owner:s3cr3tvalue@127.0.0.1:5432/dynodoc and TOKEN-abcdef123456 too";
        let out = scrub(text, &["TOKEN-abcdef123456"]);
        assert!(!out.contains("s3cr3tvalue"), "{out}");
        assert!(!out.contains("TOKEN-abcdef123456"), "{out}");
        assert!(
            out.contains("postgres://dynodoc_owner:[secret]@127.0.0.1:5432"),
            "{out}"
        );
        assert_eq!(scrub("no urls here", &[]), "no urls here");
        assert_eq!(scrub("https://example.org/a", &[]), "https://example.org/a");
    }

    #[test]
    fn a_log_over_the_limit_is_moved_aside_once() {
        let dir = std::env::temp_dir().join(format!("dynodoc-rotate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("x.log");
        std::fs::write(&log, vec![b'a'; 100]).unwrap();
        rotate(&log, 1000);
        assert!(log.exists());
        rotate(&log, 10);
        assert!(!log.exists());
        assert!(dir.join("x.log.old").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
