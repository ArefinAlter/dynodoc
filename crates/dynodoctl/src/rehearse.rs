//! `dynodoctl rehearse-lite`: restore the latest backup into a throwaway database, run the
//! TARGET release's migrations against it, verify the history, check the runtime login and start
//! the target API against the copy. It follows the routine of docs/operations/2026-10-13-schema38-release.md
//! and encodes its two pitfalls: the runtime role must exist before a dump with GRANTs is
//! restored (db-roles has already run on the live server), and the database-level ACL is not part
//! of a dump, so it is mirrored on the copy before `grant-runtime`.
//!
//! "Lite" means: one host, the same PostgreSQL server as the live database (a scratch database,
//! never the live one), no separate network or second host, no load test.

use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::db;
use crate::error::{CliError, Result};
use crate::manifest::BackupInfo;
use crate::report::{render_steps, Recorder, Step};
use crate::restore::{resolve_backup, restore_into, verify_history};
use crate::util::{create_private_dir, human_duration, stamp, write_private};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

/// The release to test. All empty means "the version that is installed now".
#[derive(Clone, Debug, Default)]
pub struct Target {
    pub version: Option<String>,
    pub registry: Option<String>,
    pub overlay: Option<PathBuf>,
}

impl Target {
    pub fn is_current(&self) -> bool {
        self.version.is_none() && self.registry.is_none() && self.overlay.is_none()
    }

    /// A Compose wrapper that uses the target's images. Process variables win over `.env`, so
    /// nothing is written to the installation.
    pub fn compose<'a>(&self, ctx: &'a Ctx<'a>) -> Compose<'a> {
        let mut c = Compose::new(ctx);
        if let Some(v) = &self.version {
            c = c.with_env("DYNODOC_VERSION", v);
        }
        if let Some(r) = &self.registry {
            c = c.with_env("DYNODOC_IMAGE_REGISTRY", r);
        }
        if let Some(overlay) = &self.overlay {
            c = c.with_files(vec![ctx.layout.compose_file(), overlay.clone()]);
        }
        c
    }

    pub fn label(&self, current: &str) -> String {
        self.version.clone().unwrap_or_else(|| current.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RehearsalReport {
    pub kind: String,
    pub created_utc: String,
    pub backup: String,
    pub backup_sha256: String,
    pub installed_version: String,
    pub target_version: String,
    pub steps: Vec<Step>,
    pub migrations_before: i64,
    pub migrations_after: i64,
    pub documents_verified: u64,
    pub api_smoke: bool,
    pub scratch_database: String,
    pub scratch_removed: bool,
    pub duration_ms: u64,
    pub ok: bool,
    pub notes: Vec<String>,
}

impl RehearsalReport {
    pub fn render(&self) -> String {
        let mut o = format!(
            "Rehearsal (lite) of {} -> {}\n",
            self.installed_version, self.target_version
        );
        o.push_str(&format!(
            "  backup            {} (sha256 {})\n",
            self.backup,
            &self.backup_sha256[..self.backup_sha256.len().min(16)]
        ));
        o.push_str(&format!(
            "  scratch database  {} ({})\n",
            self.scratch_database,
            if self.scratch_removed {
                "removed"
            } else {
                "NOT removed"
            }
        ));
        o.push_str(&render_steps(&self.steps));
        o.push_str(&format!(
            "  schema level      {} -> {}\n",
            self.migrations_before, self.migrations_after
        ));
        o.push_str(&format!(
            "  hash chain        {} documents verified\n",
            self.documents_verified
        ));
        o.push_str(&format!(
            "  api smoke         {}\n",
            if self.api_smoke {
                "the target API answered /healthz on the migrated copy"
            } else {
                "NOT passed"
            }
        ));
        o.push_str(&format!(
            "  duration          {}\n",
            human_duration(self.duration_ms as u128)
        ));
        for n in &self.notes {
            o.push_str(&format!("  note              {n}\n"));
        }
        o.push_str(&format!(
            "  result            {}\n",
            if self.ok { "PASSED" } else { "FAILED" }
        ));
        o
    }
}

fn rehearse_on(
    ctx: &Ctx<'_>,
    info: &BackupInfo,
    config_from: Option<&std::path::Path>,
    target: &Target,
) -> Result<RehearsalReport> {
    let started = Instant::now();
    let env = ctx.env()?;
    let live = Compose::new(ctx);
    let tc = target.compose(ctx);
    let installed = env.get("DYNODOC_VERSION").unwrap_or("dev").to_string();
    let db_name = ctx.scratch_name("dynodoc_rehearse_");
    let mut rec = Recorder::for_ctx(ctx);
    let mut notes = Vec::new();
    let mut created = false;
    ctx.say(&format!("Rehearsing {} on a copy of {} in the scratch database {db_name} (the live database is not touched)...", target.label(&installed), info.name()));

    let outcome = (|| -> Result<(i64, i64, u64)> {
        let restored = restore_into(
            ctx,
            &live,
            info,
            config_from,
            &db_name,
            &mut rec,
            &mut created,
        )?;
        rec.step("mirror the database ACL on the copy", || {
            // The same two statements the db-roles job applies to the live database.
            for sql in [format!("revoke all on database {db_name} from public"), format!("grant connect on database {db_name} to dynodoc_runtime")] {
                let out = live.psql("postgres", &sql)?;
                if !out.ok() {
                    return Err(CliError::failed(format!("could not prepare the copy: {}", short(&out.text())), "has `docker compose up` ever run here? the runtime login is created by the db-roles job"));
                }
            }
            Ok(((), "PUBLIC revoked, CONNECT granted to dynodoc_runtime".to_string()))
        })?;
        rec.step(&format!("engine-cli migrate ({})", target.label(&installed)), || {
            let out = db::engine_cli(&tc, &db_name, &["migrate"])?;
            if !out.ok() {
                return Err(CliError::failed(format!("the target release cannot migrate this data: {}", short(&out.text())), "do not upgrade; this is exactly what the rehearsal is for. Report the message and keep running the current version"));
            }
            Ok(((), short(&out.text())))
        })?;
        let after = rec.step("read the migrated copy", || {
            let f = db::read_facts(&live, &db_name)?;
            Ok((
                f.clone(),
                format!(
                    "schema level {} ({} migrations)",
                    f.latest_migration, f.migrations_applied
                ),
            ))
        })?;
        rec.step("engine-cli grant-runtime and check-runtime", || {
            let grant = db::engine_cli(&tc, &db_name, &["grant-runtime", "dynodoc_runtime"])?;
            if !grant.ok() {
                return Err(CliError::failed(format!("grant-runtime failed on the copy: {}", short(&grant.text())), "the target release does not accept this database's privileges"));
            }
            let check = db::engine_cli(&tc, &db_name, &["check-runtime"])?;
            if !check.ok() {
                return Err(CliError::failed(format!("check-runtime failed on the copy: {}", short(&check.text())), "the runtime login would have too much or too little access with the target release"));
            }
            Ok(((), "the runtime login passes".to_string()))
        })?;
        let documents = verify_history(&tc, &db_name, &mut rec)?;
        rec.step("target API starts on the copy and answers /healthz", || {
            let out = db::api_smoke(&tc, &db_name)?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!(
                        "the target API did not come up on the migrated copy: {}",
                        short(&out.text())
                    ),
                    "do not upgrade; read the API log lines above",
                ));
            }
            Ok(((), short(&out.text())))
        })?;
        Ok((restored.latest_migration, after.latest_migration, documents))
    })();

    // Teardown runs whatever happened above, but only for a database THIS run created (M2).
    rec.set_recovering(true);
    let removed = if created {
        match db::drop_database(&live, &db_name) {
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
        "remove the scratch database",
        removed,
        if created {
            &db_name
        } else {
            "none was created"
        },
    );
    let (before, after, documents, smoke, ok) = match &outcome {
        Ok((b, a, d)) => (*b, *a, *d, true, removed),
        Err(e) => {
            notes.push(format!("{}  {}", e.what, e.todo));
            (0, 0, 0, false, false)
        }
    };
    Ok(RehearsalReport {
        kind: "rehearse-lite".into(),
        created_utc: crate::util::iso((ctx.now)()),
        backup: info.name(),
        backup_sha256: info.manifest.database.sha256.clone(),
        installed_version: installed.clone(),
        target_version: target.label(&installed),
        steps: rec.steps,
        migrations_before: before,
        migrations_after: after,
        documents_verified: documents,
        api_smoke: smoke,
        scratch_database: db_name,
        scratch_removed: removed,
        duration_ms: started.elapsed().as_millis() as u64,
        ok,
        notes,
    })
}

/// Run a rehearsal of `target` on the latest (or the named) backup and save the report.
pub fn run(
    ctx: &Ctx<'_>,
    backup_arg: Option<&str>,
    config_from: Option<&std::path::Path>,
    target: &Target,
) -> Result<RehearsalReport> {
    ctx.require_installed()?;
    let info = resolve_backup(ctx, backup_arg)?;
    let live = Compose::new(ctx);
    if !live.ps()?.iter().any(|s| s.service == "postgres" && s.up()) {
        return Err(CliError::refused(
            "PostgreSQL is not running, so there is no server to rehearse on",
            "start the installation first (`dynodoctl status`, `docker compose up -d --wait`)",
        ));
    }
    let report = rehearse_on(ctx, &info, config_from, target)?;
    save(ctx, &report)?;
    Ok(report)
}

pub fn save(ctx: &Ctx<'_>, report: &RehearsalReport) -> Result<PathBuf> {
    let dir = ctx.layout.state_dir();
    create_private_dir(&dir)?;
    let path = dir.join(format!("rehearsal-{}.json", stamp((ctx.now)())));
    let mut text = serde_json::to_string_pretty(report)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    Ok(path)
}

#[cfg(test)]
#[path = "tests/rehearse.rs"]
mod tests;
