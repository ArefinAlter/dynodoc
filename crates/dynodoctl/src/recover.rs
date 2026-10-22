//! Recovery from an interrupted or failed database swap.
//!
//! `restore --into-live` (and `rollback`, which uses it) loads the backup into a NEW database beside
//! the live one, then renames the two in one transaction. Before that rename it writes
//! `state/restore-journal.json`; it removes the file only when the installation is consistent again.
//! If dynodoctl dies in between (power cut, kill -9, a bug) the journal stays, every destructive
//! command refuses to run until `dynodoctl recover` has looked at the databases and put things right,
//! and `dynodoctl status` says so.
//!
//! The decision is a pure function of the journal and the list of database names, so every state is
//! unit-tested; `recover` only executes the plan.

use crate::compose::{short, Compose};
use crate::ctx::{Ctx, Layout};
use crate::db::{self, LIVE_DB};
use crate::error::{CliError, Result};
use crate::report::{render_steps, Recorder, Step};
use crate::util::{create_private_dir, iso, write_private};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const JOURNAL_NAME: &str = "restore-journal.json";

/// Phases, in order. `verified` means the restored copy passed every check and only the clean-up is left.
pub const PHASE_PREPARED: &str = "prepared";
pub const PHASE_SWAPPED: &str = "swapped";
pub const PHASE_VERIFIED: &str = "verified";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    pub started_utc: String,
    pub operation: String,
    pub backup: String,
    pub live: String,
    /// The restored copy, before the swap; after a swap back, the copy that was set aside.
    pub restoring: String,
    /// The database that was live before the swap.
    pub previous: String,
    pub phase: String,
}

pub fn journal_path(layout: &Layout) -> PathBuf {
    layout.state_dir().join(JOURNAL_NAME)
}

pub fn write_journal(layout: &Layout, journal: &Journal) -> Result<()> {
    create_private_dir(&layout.state_dir())?;
    let mut text = serde_json::to_string_pretty(journal)?;
    text.push('\n');
    write_private(&journal_path(layout), text.as_bytes())
}

pub fn read_journal(layout: &Layout) -> Result<Option<Journal>> {
    match std::fs::read_to_string(journal_path(layout)) {
        // A byte order mark is what a Windows editor or `Set-Content -Encoding utf8` adds.
        Ok(text) => Ok(Some(serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
            CliError::failed(format!("state/{JOURNAL_NAME} is damaged ({e})"), "do not delete it: its contents tell `dynodoctl recover` which databases to look at; send it to support")
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn clear_journal(layout: &Layout) {
    let _ = std::fs::remove_file(journal_path(layout));
}

/// Destructive commands call this first: an unfinished swap must be recovered before anything else
/// changes the databases.
pub fn require_no_journal(ctx: &Ctx<'_>) -> Result<()> {
    match read_journal(&ctx.layout) {
        Ok(None) => Ok(()),
        Ok(Some(j)) => Err(CliError::refused(
            format!(
                "an interrupted database swap is recorded in state/{JOURNAL_NAME} ({} started {}, backup {}, phase {})",
                j.operation, j.started_utc, j.backup, j.phase
            ),
            "run `dynodoctl recover` first; it looks at the databases and puts the installation back in a consistent state",
        )),
        Err(e) => Err(e),
    }
}

/// What recovery will do. Executed in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Drop a scratch database that is not needed.
    Drop(String),
    /// Put the previous database back as live (one transaction), setting the restored copy aside under `restoring`.
    SwapBack {
        live: String,
        previous: String,
        restoring: String,
    },
    /// The restored copy was verified: only the replaced database is left to remove.
    Finalize { drop: String },
    /// Rename a database (used when only the previous one is left, between two renames).
    Rename { from: String, to: String },
    /// Start the stack again.
    StartStack,
    /// Nothing safe can be decided automatically: say what to do.
    Manual(String),
}

/// Decide from the journal and the database names that exist.
pub fn plan(j: &Journal, existing: &[String]) -> Vec<Action> {
    let has = |n: &str| existing.iter().any(|e| e == n);
    let (live, restoring, previous) = (j.live.as_str(), j.restoring.as_str(), j.previous.as_str());
    let (l, r, p) = (has(live), has(restoring), has(previous));
    match (l, r, p) {
        // The swap never happened: the live database is still the old one.
        (true, true, false) => vec![Action::Drop(restoring.into()), Action::StartStack],
        (true, false, false) => vec![Action::StartStack],
        // The swap happened. If the copy was verified, finish; otherwise put the old data back.
        (true, false, true) if j.phase == PHASE_VERIFIED => vec![Action::Finalize { drop: previous.into() }, Action::StartStack],
        (true, false, true) => vec![Action::SwapBack { live: live.into(), previous: previous.into(), restoring: restoring.into() }, Action::StartStack],
        // Between the two renames (only possible if the swap was not one transaction).
        (false, _, true) => {
            let mut v = vec![Action::Rename { from: previous.into(), to: live.into() }];
            if r {
                v.push(Action::Drop(restoring.into()));
            }
            v.push(Action::StartStack);
            v
        }
        (false, true, false) => vec![Action::Manual(format!(
            "the live database `{live}` does not exist but the restored copy `{restoring}` does: if it is the copy you want, run `alter database {restoring} rename to {live}` in psql (as the owner, connected to `postgres`); otherwise restore a backup with `dynodoctl restore --into-live`"
        ))],
        (false, false, false) => vec![Action::Manual(format!(
            "no `{live}`, `{restoring}` or `{previous}` database exists: restore a backup into a fresh volume with `dynodoctl restore --into-live --backup <dir> --config-from <settings copy>`"
        ))],
        (true, true, true) => vec![Action::Manual(format!(
            "all three databases `{live}`, `{restoring}` and `{previous}` exist, which should not happen; keep all of them, check which one holds the data you want with `dynodoctl status`, and ask support before dropping any"
        ))],
    }
}

/// The exact commands to put things back by hand (printed when an automatic unwind did not finish).
pub fn manual_commands(j: &Journal) -> Vec<String> {
    let psql = "docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) psql -h 127.0.0.1 -U dynodoc -d postgres -v ON_ERROR_STOP=1 -c";
    vec![
        "docker compose stop proxy web api".to_string(),
        format!("{psql} \"select datname from pg_database order by 1\"'"),
        format!(
            "{psql} \"begin; alter database {} rename to {}; alter database {} rename to {}; commit;\"'   # only if {} exists and holds the data you want live",
            j.live, j.restoring, j.previous, j.live, j.previous
        ),
        "docker compose up -d --wait".to_string(),
        format!("rm state/{JOURNAL_NAME}   # only when the databases are right"),
    ]
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryReport {
    pub journal: Option<Journal>,
    pub actions: Vec<String>,
    pub steps: Vec<Step>,
    pub ok: bool,
}

impl RecoveryReport {
    pub fn render(&self) -> String {
        let mut o = String::new();
        match &self.journal {
            None => o.push_str("Nothing to recover: no interrupted database swap is recorded.\n"),
            Some(j) => {
                o.push_str(&format!(
                    "Recovering the interrupted {} of backup {} (started {}, phase {})\n",
                    j.operation, j.backup, j.started_utc, j.phase
                ));
                for a in &self.actions {
                    o.push_str(&format!("  plan: {a}\n"));
                }
                o.push_str(&render_steps(&self.steps));
                o.push_str(if self.ok {
                    "  result: the installation is consistent again\n"
                } else {
                    "  result: NOT finished; see above\n"
                });
            }
        }
        o
    }
}

fn describe(a: &Action) -> String {
    match a {
        Action::Drop(n) => format!("drop the unneeded database {n}"),
        Action::SwapBack { live, previous, restoring } => format!("put the previous database {previous} back as {live} and set the unverified copy aside as {restoring}"),
        Action::Finalize { drop } => format!("the restored copy was verified: drop the replaced database {drop}"),
        Action::Rename { from, to } => format!("rename {from} to {to}"),
        Action::StartStack => "start the services".to_string(),
        Action::Manual(text) => format!("MANUAL: {text}"),
    }
}

/// `dynodoctl recover`.
pub fn recover(ctx: &Ctx<'_>) -> Result<RecoveryReport> {
    ctx.require_installed()?;
    let Some(journal) = read_journal(&ctx.layout)? else {
        return Ok(RecoveryReport {
            journal: None,
            actions: Vec::new(),
            steps: Vec::new(),
            ok: true,
        });
    };
    let c = Compose::new(ctx).shielded();
    let mut rec = Recorder::new(); // recovery must not be stopped by an interrupt
    rec.step("start PostgreSQL", || {
        let out = c.run(&["up", "-d", "--wait", "--wait-timeout", "180", "postgres"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("PostgreSQL did not become healthy: {}", short(&out.text())),
                "run `docker compose logs postgres`",
            ));
        }
        Ok(((), "healthy".to_string()))
    })?;
    let names = db::database_names(&c)?;
    let actions = plan(&journal, &names);
    let described: Vec<String> = actions.iter().map(describe).collect();
    let mut ok = true;
    for action in &actions {
        let name = describe(action);
        let result = match action {
            Action::Drop(n) => rec.step(&name, || db::drop_database(&c, n).map(|()| ((), n.clone()))),
            Action::Finalize { drop } => rec.step(&name, || db::drop_database(&c, drop).map(|()| ((), drop.clone()))),
            Action::SwapBack { live, previous, restoring } => rec.step(&name, || {
                stop_writers(&c)?;
                db::terminate_sessions(&c, live)?;
                db::swap_names(&c, live, previous, restoring)?;
                Ok(((), "one transaction".to_string()))
            }),
            Action::Rename { from, to } => rec.step(&name, || {
                let out = c.psql("postgres", &format!("alter database {from} rename to {to}"))?;
                if out.ok() {
                    Ok(((), format!("{from} -> {to}")))
                } else {
                    Err(CliError::failed(format!("rename failed: {}", short(&out.text())), "see the manual commands in the documentation"))
                }
            }),
            Action::StartStack => rec.step(&name, || {
                let out = c.run(&["up", "-d", "--wait", "--wait-timeout", "300"])?;
                if out.ok() {
                    Ok(((), "healthy".to_string()))
                } else {
                    Err(CliError::failed(format!("the stack did not come up: {}", short(&out.text())), "run `docker compose logs`; the databases are right, so this is not a data problem"))
                }
            }),
            Action::Manual(text) => {
                rec.note("manual step needed", false, text);
                Err(CliError::refused(text.clone(), "do it by hand; the journal is kept"))
            }
        };
        if result.is_err() {
            ok = false;
            break;
        }
    }
    if ok {
        clear_journal(&ctx.layout);
    }
    Ok(RecoveryReport {
        journal: Some(journal),
        actions: described,
        steps: rec.steps,
        ok,
    })
}

fn stop_writers(c: &Compose<'_>) -> Result<()> {
    let out = c.run(&["stop", "proxy", "web", "api"])?;
    if out.ok() {
        Ok(())
    } else {
        Err(CliError::failed(
            format!("could not stop the services: {}", short(&out.text())),
            "run `docker compose stop proxy web api`",
        ))
    }
}

/// A journal for a new swap.
pub fn new_journal(ctx: &Ctx<'_>, backup: &str, restoring: &str, previous: &str) -> Journal {
    Journal {
        version: 1,
        started_utc: iso((ctx.now)()),
        operation: "restore-into-live".into(),
        backup: backup.into(),
        live: LIVE_DB.into(),
        restoring: restoring.into(),
        previous: previous.into(),
        phase: PHASE_PREPARED.into(),
    }
}

#[cfg(test)]
#[path = "tests/recover.rs"]
mod tests;
