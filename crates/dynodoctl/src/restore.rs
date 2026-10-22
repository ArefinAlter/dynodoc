//! `restore-verify` (into a throwaway database, never the live one) and `restore --into-live`.

use crate::backup;
use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::db::{self, DbFacts, LIVE_DB};
use crate::error::{CliError, Result};
use crate::manifest::{self, BackupInfo};
use crate::report::{render_steps, Recorder, Step};
use crate::util::{human_duration, iso, set_private, stamp, write_private};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const LIVE_PHRASE: &str = "REPLACE LIVE DATA";

#[derive(Clone, Debug, Serialize)]
pub struct RestoreReport {
    pub mode: String,
    pub backup: String,
    pub backup_created_utc: String,
    pub database: String,
    pub steps: Vec<Step>,
    pub expected: DbFacts,
    pub restored: Option<DbFacts>,
    pub counts_match: bool,
    pub documents_verified: u64,
    pub history_verified: bool,
    pub scratch_removed: Option<bool>,
    pub duration_ms: u64,
    pub ok: bool,
    pub notes: Vec<String>,
}

impl RestoreReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Restore report ({})\n", self.mode));
        out.push_str(&format!(
            "  backup            {} (taken {})\n",
            self.backup, self.backup_created_utc
        ));
        out.push_str(&format!("  database          {}\n", self.database));
        out.push_str(&render_steps(&self.steps));
        if let Some(r) = &self.restored {
            out.push_str(&format!(
                "  counts            events {} (backup {}), documents {} ({}), identities {} ({}), migrations {} ({})\n",
                r.events, self.expected.events, r.documents, self.expected.documents, r.identities, self.expected.identities, r.migrations_applied, self.expected.migrations_applied
            ));
        }
        out.push_str(&format!(
            "  counts match      {}\n",
            if self.counts_match { "yes" } else { "NO" }
        ));
        out.push_str(&format!(
            "  hash chain        {} ({} documents verified)\n",
            if self.history_verified {
                "verified"
            } else {
                "NOT verified"
            },
            self.documents_verified
        ));
        if let Some(removed) = self.scratch_removed {
            out.push_str(&format!(
                "  scratch database  {}\n",
                if removed {
                    "removed"
                } else {
                    "NOT removed (see notes)"
                }
            ));
        }
        out.push_str(&format!(
            "  duration          {}\n",
            human_duration(self.duration_ms as u128)
        ));
        for note in &self.notes {
            out.push_str(&format!("  note              {note}\n"));
        }
        out.push_str(&format!(
            "  result            {}\n",
            if self.ok { "PASSED" } else { "FAILED" }
        ));
        out
    }
}

/// `latest`, a directory, or the name of a directory under backups/.
pub fn resolve_backup(ctx: &Ctx<'_>, arg: Option<&str>) -> Result<BackupInfo> {
    let backups = ctx.layout.backups_dir();
    let chosen = match arg {
        None | Some("latest") => {
            let all = manifest::list_backups(&backups);
            all.into_iter().last().ok_or_else(|| {
                CliError::refused(
                    format!("there is no backup in {}", backups.display()),
                    "take one with `dynodoctl backup`, or pass --backup with a backup directory",
                )
            })?
        }
        Some(path) => {
            let candidate = if Path::new(path).is_dir() {
                PathBuf::from(path)
            } else {
                backups.join(path)
            };
            let manifest = manifest::read_manifest(&candidate)?;
            BackupInfo {
                path: candidate,
                manifest,
            }
        }
    };
    Ok(chosen)
}

/// Differences between a restored database and the counts the backup recorded. The dump is one
/// consistent snapshot while the counts are separate queries, so on a busy system the restored
/// value may lie anywhere between the count before and the count after the dump.
pub fn compare(manifest: &manifest::Manifest, got: &DbFacts) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut cmp = |name: &str, a: i64, b: i64, got: i64| {
        let (low, high) = (a.min(b), a.max(b));
        if got < low || got > high {
            diffs.push(format!("{name}: backup {low}..{high}, restored {got}"));
        }
    };
    let (x, y) = (&manifest.facts, &manifest.facts_after);
    cmp("events", x.events, y.events, got.events);
    cmp("documents", x.documents, y.documents, got.documents);
    cmp("identities", x.identities, y.identities, got.identities);
    cmp(
        "migrations applied",
        x.migrations_applied,
        y.migrations_applied,
        got.migrations_applied,
    );
    diffs
}

/// Check the files of the backup, load its dump into `db` and compare the counts with the manifest.
pub fn restore_into(
    c: &Compose<'_>,
    info: &BackupInfo,
    config_from: Option<&Path>,
    db: &str,
    rec: &mut Recorder,
) -> Result<DbFacts> {
    rec.step("backup files match their checksums", || {
        manifest::verify_files(info, config_from).map_err(|p| {
            CliError::refused(
                format!("the backup is damaged or incomplete: {}", p.join("; ")),
                "use an earlier backup, or take a new one; nothing was changed",
            )
        })?;
        Ok(((), format!("{} bytes", info.manifest.database.bytes)))
    })?;
    rec.step("create database", || {
        db::create_database(c, db)?;
        Ok(((), db.to_string()))
    })?;
    rec.step("pg_restore", || {
        let out = db::restore_dump(c, db, &info.dump_path())?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("pg_restore failed: {}", short(&out.text())),
                "the dump may be damaged or from an incompatible PostgreSQL; try an earlier backup",
            ));
        }
        Ok(((), "loaded with --no-owner --no-acl".to_string()))
    })?;
    let facts = rec.step("read the restored database", || {
        let f = db::read_facts(c, db)?;
        Ok((
            f.clone(),
            format!(
                "{} events, {} documents, schema {}",
                f.events, f.documents, f.latest_migration
            ),
        ))
    })?;
    rec.step("counts equal the manifest", || {
        let diffs = compare(&info.manifest, &facts);
        if diffs.is_empty() {
            Ok(((), "events, documents, identities, migrations".to_string()))
        } else {
            Err(CliError::failed(
                format!(
                    "the restored database differs from the backup: {}",
                    diffs.join("; ")
                ),
                "do not use this backup",
            ))
        }
    })?;
    Ok(facts)
}

/// `verify-snapshots --all` and `verify` for every document of `db`. Returns the documents checked.
pub fn verify_history(c: &Compose<'_>, db: &str, rec: &mut Recorder) -> Result<u64> {
    rec.step("engine-cli verify-snapshots --all", || {
        let out = db::engine_cli_owner(c, db, &["verify-snapshots", "--all"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("verify-snapshots failed: {}", short(&out.text())),
                "the history in this copy does not verify; do not use it",
            ));
        }
        let n = out
            .stdout_text()
            .lines()
            .filter(|l| l.starts_with("OK:"))
            .count();
        Ok(((), format!("{n} documents")))
    })?;
    let ids = db::document_ids(c, db)?;
    rec.step("engine-cli verify (each document)", || {
        if ids.is_empty() {
            return Ok((0, "no documents".to_string()));
        }
        let out = db::verify_documents(c, db, &ids)?;
        let ok_lines = out
            .stdout_text()
            .lines()
            .filter(|l| l.starts_with("OK:"))
            .count();
        if !out.ok() || ok_lines != ids.len() {
            return Err(CliError::failed(
                format!(
                    "verify failed ({ok_lines} of {} documents passed): {}",
                    ids.len(),
                    short(&out.text())
                ),
                "the event chain of this copy does not verify; do not use it",
            ));
        }
        Ok((
            ids.len() as u64,
            format!(
                "{} documents, event chain and snapshots verified",
                ids.len()
            ),
        ))
    })
}

fn scratch_name(ctx: &Ctx<'_>, prefix: &str) -> String {
    format!("{prefix}{}", stamp((ctx.now)()).to_ascii_lowercase())
}

fn require_postgres(ctx: &Ctx<'_>, c: &Compose<'_>) -> Result<()> {
    let services = c.ps()?;
    if services.iter().any(|s| s.service == "postgres" && s.up()) {
        Ok(())
    } else {
        let _ = ctx;
        Err(CliError::refused(
            "PostgreSQL is not running, so there is nothing to restore into",
            "start the installation with `dynodoctl install` (or `docker compose up -d --wait`); to recover a lost database use `dynodoctl restore --into-live`",
        ))
    }
}

/// `restore-verify`: load the backup into a throwaway database, verify it, and remove it.
pub fn restore_verify(
    ctx: &Ctx<'_>,
    backup_arg: Option<&str>,
    config_from: Option<&Path>,
) -> Result<RestoreReport> {
    ctx.require_installed()?;
    let started = Instant::now();
    let info = resolve_backup(ctx, backup_arg)?;
    let c = Compose::new(ctx);
    require_postgres(ctx, &c)?;
    let db = scratch_name(ctx, "dynodoc_restore_");
    let mut rec = Recorder::new();
    let mut notes = Vec::new();
    ctx.say(&format!(
        "Restoring {} into the throwaway database {db} (the live database is not touched)...",
        info.name()
    ));
    let outcome = (|| -> Result<(DbFacts, u64)> {
        let facts = restore_into(&c, &info, config_from, &db, &mut rec)?;
        let documents = verify_history(&c, &db, &mut rec)?;
        Ok((facts, documents))
    })();
    // Teardown runs whatever happened above.
    let removed = match db::drop_database(&c, &db) {
        Ok(()) => true,
        Err(e) => {
            notes.push(e.to_string());
            false
        }
    };
    rec.note("remove the throwaway database", removed, &db);
    let (restored, documents, history) = match &outcome {
        Ok((f, n)) => (Some(f.clone()), *n, true),
        Err(e) => {
            notes.push(format!("{}  {}", e.what, e.todo));
            (None, 0, false)
        }
    };
    let counts_match = restored
        .as_ref()
        .map(|r| compare(&info.manifest, r).is_empty())
        .unwrap_or(false);
    let ok = outcome.is_ok() && removed;
    Ok(RestoreReport {
        mode: "restore-verify (throwaway database)".into(),
        backup: info.name(),
        backup_created_utc: info.manifest.created_utc.clone(),
        database: db,
        steps: rec.steps,
        expected: info.manifest.facts.clone(),
        restored,
        counts_match,
        documents_verified: documents,
        history_verified: history,
        scratch_removed: Some(removed),
        duration_ms: started.elapsed().as_millis() as u64,
        ok,
        notes,
    })
}

#[derive(Default)]
pub struct LiveOptions {
    pub backup: Option<String>,
    pub config_from: Option<PathBuf>,
    pub no_safety_backup: bool,
    pub keep_previous: bool,
    /// The caller (rollback) has already asked for the typed confirmation.
    pub confirmed: bool,
}

/// Put the settings and secrets of a backup back where they belong, but only the files that are
/// missing: an installation's own files are never overwritten here.
fn restore_config_files(
    ctx: &Ctx<'_>,
    info: &BackupInfo,
    root: &Path,
    notes: &mut Vec<String>,
) -> Result<usize> {
    let mut written = 0;
    for entry in &info.manifest.config.files {
        let target = ctx.layout.dir.join(&entry.path);
        let source = root.join(&entry.path);
        if target.exists() {
            let (have, _) = crate::util::sha256_file(&target)?;
            if have != entry.sha256 {
                notes.push(format!(
                    "{} differs from the backup's copy; the installation's file was kept",
                    entry.path
                ));
            }
            continue;
        }
        let bytes = std::fs::read(&source)?;
        write_private(&target, &bytes)?;
        written += 1;
    }
    Ok(written)
}

pub fn restore_live(ctx: &Ctx<'_>, opts: &LiveOptions) -> Result<RestoreReport> {
    ctx.require_bundle()?;
    let started = Instant::now();
    let info = resolve_backup(ctx, opts.backup.as_deref())?;
    let mut rec = Recorder::new();
    let mut notes = Vec::new();
    // Nothing is changed before these checks and the confirmation.
    manifest::verify_files(&info, opts.config_from.as_deref()).map_err(|p| {
        CliError::refused(
            format!("the backup is damaged or incomplete: {}", p.join("; ")),
            "use an earlier backup; nothing was changed",
        )
    })?;
    let config_root = info.config_root(opts.config_from.as_deref());
    if !ctx.layout.is_installed() && config_root.is_none() {
        return Err(CliError::refused(
            "this directory has no .env and secrets/, and the backup does not carry a copy of them here",
            "put the settings and secrets back first, or pass --config-from with the directory you sent them to (`backup --secrets-to`)",
        ));
    }
    if !opts.confirmed
        && !ctx.confirm.confirm(
            &format!(
                "This REPLACES the live Dynodoc database with the backup {} ({} events, {} documents). Everything written after {} is lost. The current database is kept until the restored one verifies.",
                info.name(),
                info.manifest.facts.events,
                info.manifest.facts.documents,
                info.manifest.created_utc
            ),
            LIVE_PHRASE,
        )
    {
        return Err(CliError::refused(
            "restoring over the live database was not confirmed; nothing was changed",
            format!("run again and type the phrase, or pass --confirm \"{LIVE_PHRASE}\""),
        ));
    }
    if !ctx.layout.is_installed() {
        if let Some(root) = &config_root {
            let n = restore_config_files(ctx, &info, root, &mut notes)?;
            rec.note(
                "restore settings and secrets from the backup",
                true,
                &format!("{n} files"),
            );
        }
    } else if let Some(root) = &config_root {
        restore_config_files(ctx, &info, root, &mut notes)?;
    }
    let c = Compose::new(ctx);
    let live_stamp = scratch_name(ctx, "");
    let restoring = format!("dynodoc_restoring_{live_stamp}");
    let previous = format!("dynodoc_prev_{live_stamp}");

    rec.step("start PostgreSQL", || {
        let out = c.run(&["up", "-d", "--wait", "--wait-timeout", "180", "postgres"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("PostgreSQL did not become healthy: {}", short(&out.text())),
                "run `docker compose logs postgres` in the bundle directory; nothing was changed",
            ));
        }
        Ok(((), "healthy".to_string()))
    })?;

    if !opts.no_safety_backup && db::has_schema(&c, LIVE_DB).unwrap_or(false) {
        let options = backup::Options {
            kind: manifest::KIND_PRE_RESTORE.into(),
            label: Some("before-restore".into()),
            keep: 3,
            ..backup::Options::default()
        };
        let made = rec.step("safety backup of the current database", || {
            let report = backup::run(ctx, &options)?;
            Ok((report.path.clone(), report.path.display().to_string()))
        })?;
        notes.push(format!(
            "the database as it was before this restore is saved in {}",
            made.display()
        ));
    } else {
        rec.note(
            "safety backup of the current database",
            true,
            "skipped (empty database or --no-safety-backup)",
        );
    }

    rec.step("stop proxy, web and api", || {
        let out = c.run(&["stop", "proxy", "web", "api"])?;
        if !out.ok() {
            notes.push(format!(
                "stopping the services reported: {}",
                short(&out.text())
            ));
        }
        Ok(((), "writers stopped".to_string()))
    })?;

    // Load into a new database first: a damaged dump fails here and the live database is untouched.
    let restored_facts = match restore_into(
        &c,
        &info,
        opts.config_from.as_deref(),
        &restoring,
        &mut rec,
    ) {
        Ok(f) => f,
        Err(e) => {
            let _ = db::drop_database(&c, &restoring);
            notes.push("the live database was NOT changed; restart the services with `docker compose up -d --wait`".to_string());
            return Ok(failed_report(&info, &previous, rec, notes, e, started));
        }
    };

    rec.step("swap the restored database in", || {
        let terminate = c.psql("postgres", "select pg_terminate_backend(pid) from pg_stat_activity where datname = 'dynodoc' and pid <> pg_backend_pid()")?;
        if !terminate.ok() {
            return Err(CliError::failed(format!("could not close the open connections: {}", short(&terminate.text())), "the live database was not changed"));
        }
        let exists = c.psql("postgres", "select 1 from pg_database where datname = 'dynodoc'")?;
        if exists.stdout_text().trim() == "1" {
            let rename = c.psql("postgres", &format!("alter database {LIVE_DB} rename to {previous}"))?;
            if !rename.ok() {
                return Err(CliError::failed(format!("could not set the current database aside: {}", short(&rename.text())), "the live database was not changed"));
            }
        }
        let rename = c.psql("postgres", &format!("alter database {restoring} rename to {LIVE_DB}"))?;
        if !rename.ok() {
            // Put the old one back so the installation is as it was.
            let _ = c.psql("postgres", &format!("alter database {previous} rename to {LIVE_DB}"));
            return Err(CliError::failed(format!("could not put the restored database in place: {}", short(&rename.text())), "the previous database was put back"));
        }
        Ok(((), format!("previous database kept as {previous} until the restore verifies")))
    })?;

    rec.step("start the stack (db-roles, init, services)", || {
        let out = c.run(&["up", "-d", "--wait", "--wait-timeout", "300"])?;
        if !out.ok() {
            return Err(CliError::failed(format!("the stack did not come up: {}", short(&out.text())), format!("the previous database is still there as {previous}; see docs/SELF-HOSTED-OPERATIONS.md, \"Undo a restore\"")));
        }
        Ok(((), "healthy".to_string()))
    })
    .inspect_err(|_| notes.push(format!("previous database kept as {previous}")))?;

    let verified = (|| -> Result<u64> {
        let after = rec.step("read the live database", || {
            let f = db::read_facts(&c, LIVE_DB)?;
            Ok((
                f.clone(),
                format!(
                    "{} events, {} documents, schema {}",
                    f.events, f.documents, f.latest_migration
                ),
            ))
        })?;
        if !compare(&info.manifest, &after).is_empty() {
            return Err(CliError::failed(
                "the live database differs from the backup after the restore",
                format!("the previous database is kept as {previous}"),
            ));
        }
        let documents = verify_history(&c, LIVE_DB, &mut rec)?;
        rec.step("engine-cli check-runtime (runtime login)", || {
            let out = c.run(&[
                "--profile",
                "maintenance",
                "run",
                "--rm",
                "--no-deps",
                "-T",
                "database-admin",
                "check-runtime",
            ])?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!("check-runtime failed: {}", short(&out.text())),
                    "run `docker compose logs init` in the bundle directory",
                ));
            }
            Ok(((), "runtime login and schema verified".to_string()))
        })?;
        Ok(documents)
    })();

    let (ok, documents, history) = match verified {
        Ok(n) => (true, n, true),
        Err(e) => {
            notes.push(format!("{}  {}", e.what, e.todo));
            (false, 0, false)
        }
    };
    if ok && !opts.keep_previous {
        match db::drop_database(&c, &previous) {
            Ok(()) => rec.note("remove the previous database", true, &previous),
            Err(e) => {
                rec.note("remove the previous database", false, &previous);
                notes.push(e.to_string());
            }
        }
    } else if ok {
        notes.push(format!("the previous database is kept as {previous} (--keep-previous); drop it when you no longer need it"));
    }
    let _ = restored_facts;
    let live_facts = db::read_facts(&c, LIVE_DB).ok();
    let counts_match = live_facts
        .as_ref()
        .map(|f| compare(&info.manifest, f).is_empty())
        .unwrap_or(false);
    Ok(RestoreReport {
        mode: "restore into the live installation".into(),
        backup: info.name(),
        backup_created_utc: info.manifest.created_utc.clone(),
        database: LIVE_DB.into(),
        steps: rec.steps,
        expected: info.manifest.facts.clone(),
        restored: live_facts,
        counts_match,
        documents_verified: documents,
        history_verified: history,
        scratch_removed: None,
        duration_ms: started.elapsed().as_millis() as u64,
        ok: ok && counts_match,
        notes,
    })
}

fn failed_report(
    info: &BackupInfo,
    db: &str,
    rec: Recorder,
    mut notes: Vec<String>,
    error: CliError,
    started: Instant,
) -> RestoreReport {
    notes.push(format!("{}  {}", error.what, error.todo));
    RestoreReport {
        mode: "restore into the live installation".into(),
        backup: info.name(),
        backup_created_utc: info.manifest.created_utc.clone(),
        database: db.into(),
        steps: rec.steps,
        expected: info.manifest.facts.clone(),
        restored: None,
        counts_match: false,
        documents_verified: 0,
        history_verified: false,
        scratch_removed: None,
        duration_ms: started.elapsed().as_millis() as u64,
        ok: false,
        notes,
    }
}

/// Write the report next to the other records so an audit can find it later.
pub fn save_report(ctx: &Ctx<'_>, report: &RestoreReport, kind: &str) -> Result<PathBuf> {
    let dir = ctx.layout.state_dir();
    crate::util::create_private_dir(&dir)?;
    let path = dir.join(format!("{kind}-{}.json", stamp((ctx.now)())));
    let mut text = serde_json::to_string_pretty(report)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    set_private(&path)?;
    let _ = iso;
    Ok(path)
}

#[cfg(test)]
#[path = "tests/restore.rs"]
mod tests;
