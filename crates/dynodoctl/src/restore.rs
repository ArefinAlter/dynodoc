//! `restore-verify` (into a throwaway database, never the live one) and `restore --into-live`.
//!
//! TRUST: restoring a backup runs its SQL as the database owner, a superuser, inside the database
//! container. A backup is code. dynodoctl checks the files against two records of their hashes
//! (`manifest`), confines every path the manifest names, and screens the SQL before it runs
//! (`dumpscan`), but none of that authenticates a backup or sandboxes it: restore only backups you
//! made or can account for. See docs/SELF-HOSTED-OPERATIONS.md, "Trust".
//!
//! The live restore is built so that no failure leaves the installation worse than it found it: the
//! backup is loaded into a NEW database, a journal is written, the two databases are swapped in one
//! transaction, and anything that goes wrong afterwards (a failed start, a failed check, an
//! interrupt) swaps back. A crash in the middle leaves the journal for `dynodoctl recover`.

use crate::backup;
use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::db::{self, DbFacts, SchemaState, LIVE_DB};
use crate::dumpscan;
use crate::error::{CliError, Result};
use crate::manifest::{self, BackupInfo};
use crate::recover::{self, Journal};
use crate::report::{render_steps, Recorder, Step};
use crate::util::{
    create_private_dir, human_duration, set_private, stamp, write_private, write_private_new,
};
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
    /// The restored copy is the live database now.
    pub live_changed: bool,
    /// A swap was made and then undone because the restored copy did not start or verify.
    pub swapped_back: bool,
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
        if self.swapped_back {
            out.push_str(
                "  swap              made and UNDONE: the previous database is live again\n",
            );
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

/// `pg_restore` messages can quote rows of the data, so they are neither stored in a report nor put
/// in the error: only the class, the exit status and how many lines there were.
fn pg_restore_failure(out: &crate::runner::Output) -> CliError {
    let lines = out.text().lines().count();
    CliError::failed(
        format!("pg_restore failed (exit status {}; {lines} line(s) of messages, not shown or stored because they can quote your data)", out.status),
        "the dump may be damaged or from an incompatible PostgreSQL; try an earlier backup. To read the messages, run the restore by hand into a scratch database",
    )
}

/// Render the dump's SQL without running it and screen it (see `dumpscan`). The rendered script is
/// a plain-text copy of the whole database schema and data, so it is written 0600 into `state/`
/// and removed at once, whatever the outcome.
pub fn review_dump(ctx: &Ctx<'_>, c: &Compose<'_>, dump: &Path) -> Result<String> {
    create_private_dir(&ctx.layout.state_dir())?;
    let script = ctx
        .layout
        .state_dir()
        .join(format!(".review-{}.sql", (ctx.nonce)()));
    write_private(&script, b"")?;
    let result = (|| -> Result<String> {
        let out = db::dump_script(c, dump, &script)?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("pg_restore could not render the dump (exit status {}): it is damaged or not a custom-format dump", out.status),
                "do not use this backup; nothing was loaded",
            ));
        }
        let file = std::fs::File::open(&script)?;
        let findings = dumpscan::scan_script(std::io::BufReader::new(file))
            .map_err(|e| CliError::refused(e, "do not use this backup; nothing was loaded"))?;
        if findings.is_empty() {
            Ok("no program execution, server settings, unknown extensions, role changes or foreign code".to_string())
        } else {
            let shown: Vec<String> = findings
                .iter()
                .take(3)
                .map(|f| format!("{} (`{}`)", f.rule, f.excerpt))
                .collect();
            Err(CliError::refused(
                format!("the dump contains SQL that a Dynodoc backup never has: {}", shown.join("; ")),
                "do not restore it. A backup you made yourself never trips this; if you are sure this one is yours, find out how it was changed. Nothing was loaded",
            ))
        }
    })();
    let _ = std::fs::remove_file(&script);
    result
}

/// Check the files of the backup, screen its SQL, load the dump into `db` and compare the counts
/// with the manifest. `created` is set once this call has created `db`: only then may the caller drop it.
pub fn restore_into(
    ctx: &Ctx<'_>,
    c: &Compose<'_>,
    info: &BackupInfo,
    config_from: Option<&Path>,
    db: &str,
    rec: &mut Recorder,
    created: &mut bool,
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
    rec.step("screen the dump's SQL before it runs", || {
        let detail = review_dump(ctx, c, &info.dump_path())?;
        Ok(((), detail))
    })?;
    rec.step("create database", || {
        db::create_database(c, db)?;
        *created = true;
        Ok(((), db.to_string()))
    })?;
    rec.step("pg_restore", || {
        let out = db::restore_dump(c, db, &info.dump_path())?;
        if !out.ok() {
            return Err(pg_restore_failure(&out));
        }
        Ok((
            (),
            "loaded with its privileges (--no-owner only)".to_string(),
        ))
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

fn require_postgres(c: &Compose<'_>) -> Result<()> {
    let services = c.ps()?;
    if services.iter().any(|s| s.service == "postgres" && s.up()) {
        Ok(())
    } else {
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
    require_postgres(&c)?;
    let db = ctx.scratch_name("dynodoc_restore_");
    let mut rec = Recorder::for_ctx(ctx);
    let mut notes = Vec::new();
    let mut created = false;
    ctx.say(&format!(
        "Restoring {} into the throwaway database {db} (the live database is not touched)...",
        info.name()
    ));
    let outcome = (|| -> Result<(DbFacts, u64)> {
        let facts = restore_into(ctx, &c, &info, config_from, &db, &mut rec, &mut created)?;
        let documents = verify_history(&c, &db, &mut rec)?;
        Ok((facts, documents))
    })();
    // Teardown runs whatever happened above, but only for a database THIS run created.
    rec.set_recovering(true);
    let removed = if created {
        match db::drop_database(&c, &db) {
            Ok(()) => true,
            Err(e) => {
                notes.push(e.to_string());
                false
            }
        }
    } else {
        true
    };
    rec.note(
        "remove the throwaway database",
        removed,
        if created { &db } else { "none was created" },
    );
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
        live_changed: false,
        swapped_back: false,
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
///
/// Every path was validated when the manifest was read and is checked again here; nothing is
/// followed through a symbolic link (the source must be a regular file, the directory and the
/// target must not be links, and the target is created exclusively).
fn restore_config_files(
    ctx: &Ctx<'_>,
    info: &BackupInfo,
    root: &Path,
    notes: &mut Vec<String>,
) -> Result<usize> {
    let mut written = 0;
    for entry in &info.manifest.config.files {
        if !manifest::safe_config_path(&entry.path) {
            return Err(CliError::refused(
                format!(
                    "the settings file `{}` is not at an allowed path; nothing was written",
                    entry.path
                ),
                "do not use this backup",
            ));
        }
        let source = root.join(&entry.path);
        if !std::fs::symlink_metadata(&source)
            .map(|m| m.file_type().is_file())
            .unwrap_or(false)
        {
            return Err(CliError::refused(
                format!(
                    "{} in the backup is not a regular file; nothing was written",
                    entry.path
                ),
                "do not use this backup",
            ));
        }
        let target = ctx.layout.dir.join(&entry.path);
        if let Some(parent) = target.parent().filter(|p| *p != ctx.layout.dir.as_path()) {
            match std::fs::symlink_metadata(parent) {
                Ok(m) if m.file_type().is_symlink() || !m.is_dir() => {
                    return Err(CliError::refused(
                        format!(
                            "{} is a link or not a directory; refusing to write through it",
                            parent.display()
                        ),
                        "nothing was written",
                    ));
                }
                Ok(_) => {}
                Err(_) => create_private_dir(parent)?,
            }
        }
        match std::fs::symlink_metadata(&target) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(CliError::refused(
                    format!(
                        "{} is a symbolic link; refusing to write through it",
                        target.display()
                    ),
                    "nothing was written",
                ));
            }
            Ok(_) => {
                if std::fs::read(&target)? != std::fs::read(&source)? {
                    notes.push(format!(
                        "{} differs from the backup's copy; the installation's file was kept",
                        entry.path
                    ));
                }
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        write_private_new(&target, &std::fs::read(&source)?)?;
        written += 1;
    }
    Ok(written)
}

/// What a live restore has done so far, so a failure knows what to undo.
struct State {
    restoring: String,
    previous: String,
    stopped: bool,
    created: bool,
    swapped: bool,
    swapped_back: bool,
    journal: Option<Journal>,
    notes: Vec<String>,
}

fn stop_writers(c: &Compose<'_>) -> Result<String> {
    let out = c.run(&["stop", "proxy", "web", "api"])?;
    if out.ok() {
        Ok("writers stopped".to_string())
    } else {
        Err(CliError::failed(
            format!("could not stop the services: {}", short(&out.text())),
            "run `docker compose stop proxy web api`; nothing else was changed",
        ))
    }
}

fn start_stack(c: &Compose<'_>) -> Result<()> {
    let out = c.run(&["up", "-d", "--wait", "--wait-timeout", "300"])?;
    if out.ok() {
        Ok(())
    } else {
        Err(CliError::failed(
            format!("the stack did not come up: {}", short(&out.text())),
            "run `docker compose logs <service>` for the failed job",
        ))
    }
}

/// The destructive part. Any `Err` makes the caller unwind.
fn destructive(
    ctx: &Ctx<'_>,
    opts: &LiveOptions,
    info: &BackupInfo,
    schema: SchemaState,
    rec: &mut Recorder,
    st: &mut State,
) -> Result<u64> {
    let c = Compose::new(ctx);
    let cs = Compose::new(ctx).shielded();

    // Writers stop BEFORE the safety backup, so the backup is of a database nobody is changing.
    st.stopped = true;
    rec.step("stop proxy, web and api", || Ok(((), stop_writers(&cs)?)))?;

    if !opts.no_safety_backup && schema == SchemaState::Present {
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
        st.notes.push(format!(
            "the database as it was before this restore is saved in {}",
            made.display()
        ));
    } else {
        let why = if opts.no_safety_backup {
            "--no-safety-backup"
        } else if schema == SchemaState::Missing {
            "no live database yet"
        } else {
            "the live database is empty"
        };
        rec.note(
            "safety backup of the current database",
            true,
            &format!("skipped ({why})"),
        );
    }

    // The live name must exist for the swap; on a fresh server it may not.
    if schema == SchemaState::Missing {
        rec.step("create the empty live database", || {
            let out = c.psql("postgres", &format!("create database {LIVE_DB}"))?;
            if out.ok() {
                Ok(((), LIVE_DB.to_string()))
            } else {
                Err(CliError::failed(
                    format!("could not create `{LIVE_DB}`: {}", short(&out.text())),
                    "check `docker compose logs postgres`",
                ))
            }
        })?;
    }

    // Load beside the live database: a damaged dump fails here and the live database is untouched.
    restore_into(
        ctx,
        &c,
        info,
        opts.config_from.as_deref(),
        &st.restoring,
        rec,
        &mut st.created,
    )?;

    let journal = recover::new_journal(ctx, &info.name(), &st.restoring, &st.previous);
    rec.step("record the swap in state/restore-journal.json", || {
        recover::write_journal(&ctx.layout, &journal)?;
        Ok((
            (),
            "removed again when the installation is consistent".to_string(),
        ))
    })?;
    st.journal = Some(journal);

    rec.step("swap the restored database in (one transaction)", || {
        db::terminate_sessions(&cs, LIVE_DB)?;
        db::swap_names(&cs, LIVE_DB, &st.restoring, &st.previous)?;
        Ok((
            (),
            format!(
                "previous database kept as {} until the restore verifies",
                st.previous
            ),
        ))
    })?;
    st.swapped = true;
    if let Some(j) = st.journal.as_mut() {
        j.phase = recover::PHASE_SWAPPED.into();
        recover::write_journal(&ctx.layout, j)?;
    }

    rec.step("start the stack (db-roles, init, services)", || {
        start_stack(&cs)?;
        Ok(((), "healthy".to_string()))
    })?;
    st.stopped = false;

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
            "the previous database is put back",
        ));
    }
    let documents = verify_history(&c, LIVE_DB, rec)?;
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

    // Verified: only the clean-up is left.
    if let Some(j) = st.journal.as_mut() {
        j.phase = recover::PHASE_VERIFIED.into();
        recover::write_journal(&ctx.layout, j)?;
    }
    Ok(documents)
}

/// Undo what a failed live restore did, as far as it can be undone, and say what is left.
fn unwind(ctx: &Ctx<'_>, rec: &mut Recorder, st: &mut State, cause: &CliError) {
    rec.set_recovering(true);
    let cs = Compose::new(ctx).shielded();
    st.notes.push(format!("{}  {}", cause.what, cause.todo));
    let mut consistent = true;
    if st.swapped {
        let (previous, restoring) = (st.previous.clone(), st.restoring.clone());
        let back = rec.step("put the previous database back (one transaction)", || {
            stop_writers(&cs)?;
            db::terminate_sessions(&cs, LIVE_DB)?;
            db::swap_names(&cs, LIVE_DB, &previous, &restoring)?;
            Ok(((), format!("the restored copy is set aside as {restoring}")))
        });
        match back {
            Ok(()) => {
                st.swapped = false;
                st.swapped_back = true;
                st.stopped = true;
                st.notes.push(format!("the restored copy did not start or verify, so the previous database is live again. The rejected copy is kept as {restoring} for inspection; drop it with: docker compose exec -T postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc {restoring}'"));
            }
            Err(e) => {
                consistent = false;
                if let Some(j) = &st.journal {
                    st.notes.push(format!(
                        "COULD NOT put the previous database back automatically ({}). The restored copy is live. Run `dynodoctl recover`, or by hand:\n    {}",
                        e.what,
                        recover::manual_commands(j).join("\n    ")
                    ));
                }
            }
        }
    } else if st.created {
        let restoring = st.restoring.clone();
        match rec.step("remove the partly loaded copy", || {
            db::drop_database(&Compose::new(ctx), &restoring).map(|()| ((), restoring.clone()))
        }) {
            Ok(()) => st
                .notes
                .push("the live database was NOT changed".to_string()),
            Err(e) => st.notes.push(format!(
                "the live database was NOT changed, but the copy {} could not be removed: {}",
                st.restoring, e.what
            )),
        }
    } else {
        st.notes
            .push("the live database was NOT changed".to_string());
    }
    if consistent && st.stopped {
        match rec.step("start the stack again", || start_stack(&cs).map(|()| ((), "healthy".to_string()))) {
            Ok(()) => st.stopped = false,
            Err(e) => st.notes.push(format!("the services are stopped and could not be started: {}. The databases are right; run `docker compose up -d --wait`", e.what)),
        }
    }
    if consistent {
        recover::clear_journal(&ctx.layout);
    } else {
        st.notes.push(format!("state/{} is kept: `dynodoctl status` and every destructive command will say so until `dynodoctl recover` has run", recover::JOURNAL_NAME));
    }
}

pub fn restore_live(ctx: &Ctx<'_>, opts: &LiveOptions) -> Result<RestoreReport> {
    ctx.require_bundle()?;
    recover::require_no_journal(ctx)?;
    let started = Instant::now();
    let info = resolve_backup(ctx, opts.backup.as_deref())?;
    let mut notes: Vec<String> = Vec::new();
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
                info.name(), info.manifest.facts.events, info.manifest.facts.documents, info.manifest.created_utc
            ),
            LIVE_PHRASE,
        )
    {
        return Err(CliError::refused(
            "restoring over the live database was not confirmed; nothing was changed",
            format!("run again and type the phrase, or pass --confirm \"{LIVE_PHRASE}\""),
        ));
    }
    crate::compose::guard_project(ctx)?;
    let mut rec = Recorder::for_ctx(ctx);
    if let Some(root) = &config_root {
        let n = restore_config_files(ctx, &info, root, &mut notes)?;
        if !ctx.layout.is_installed() || n > 0 {
            rec.note(
                "restore settings and secrets from the backup",
                true,
                &format!("{n} files"),
            );
        }
    }
    let c = Compose::new(ctx);
    let cs = Compose::new(ctx).shielded();

    // Not yet destructive: if these fail, nothing needs undoing.
    rec.step("start PostgreSQL", || {
        let out = cs.run(&["up", "-d", "--wait", "--wait-timeout", "180", "postgres"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("PostgreSQL did not become healthy: {}", short(&out.text())),
                "run `docker compose logs postgres` in the bundle directory; nothing was changed",
            ));
        }
        Ok(((), "healthy".to_string()))
    })?;
    // The dump's GRANTs name the runtime login. On a fresh volume (disaster recovery) it does not
    // exist yet: the db-roles job creates it (idempotent; harmless on a running install).
    rec.step("create the runtime login (db-roles)", || {
        let out = cs.run(&["run", "--rm", "--no-deps", "-T", "db-roles"])?;
        if !out.ok() {
            return Err(CliError::failed(
                format!("the db-roles job failed: {}", short(&out.text())),
                "run `docker compose logs db-roles`; nothing was changed",
            ));
        }
        Ok(((), "dynodoc_runtime is in place".to_string()))
    })?;
    // "Cannot tell" is an error here, never "empty": a failed query must not skip the safety backup.
    let schema = db::schema_state(&c, LIVE_DB)?;

    let mut st = State {
        restoring: ctx.scratch_name("dynodoc_restoring_"),
        previous: String::new(),
        stopped: false,
        created: false,
        swapped: false,
        swapped_back: false,
        journal: None,
        notes,
    };
    st.previous = st
        .restoring
        .replacen("dynodoc_restoring_", "dynodoc_prev_", 1);

    let outcome = destructive(ctx, opts, &info, schema, &mut rec, &mut st);
    let documents = match &outcome {
        Ok(n) => *n,
        Err(cause) => {
            unwind(ctx, &mut rec, &mut st, cause);
            0
        }
    };
    let mut ok = outcome.is_ok();
    if ok {
        if opts.keep_previous {
            st.notes.push(format!("the previous database is kept as {} (--keep-previous); drop it when you no longer need it", st.previous));
        } else {
            match rec.step("remove the previous database", || {
                db::drop_database(&c, &st.previous).map(|()| ((), st.previous.clone()))
            }) {
                Ok(()) => {}
                Err(e) => st.notes.push(format!(
                    "the restore verified, but the previous database {} could not be removed: {}",
                    st.previous, e.what
                )),
            }
        }
        recover::clear_journal(&ctx.layout);
    }
    let live_facts = db::read_facts(&c, LIVE_DB).ok();
    let counts_match = live_facts
        .as_ref()
        .map(|f| compare(&info.manifest, f).is_empty())
        .unwrap_or(false);
    ok = ok && counts_match;
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
        history_verified: outcome.is_ok(),
        scratch_removed: None,
        live_changed: st.swapped,
        swapped_back: st.swapped_back,
        duration_ms: started.elapsed().as_millis() as u64,
        ok,
        notes: st.notes,
    })
}

/// Write the report next to the other records so an audit can find it later.
pub fn save_report(ctx: &Ctx<'_>, report: &RestoreReport, kind: &str) -> Result<PathBuf> {
    let dir = ctx.layout.state_dir();
    create_private_dir(&dir)?;
    let path = dir.join(format!("{kind}-{}.json", stamp((ctx.now)())));
    let mut text = serde_json::to_string_pretty(report)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    set_private(&path)?;
    Ok(path)
}

#[cfg(test)]
#[path = "tests/restore.rs"]
mod tests;
