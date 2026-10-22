//! `dynodoctl upgrade` and `dynodoctl rollback`.
//!
//! An upgrade is: verify the new images against the release overlay (digests), take a mandatory
//! pre-upgrade backup, rehearse the target release on a copy of the data, then switch the
//! installation to the new version (`up` runs the migrations in the `init` job), check health and
//! print how to go back. Rollback is a restore of the pre-upgrade backup into the previous
//! images: migrations only move forward, and an older image refuses a newer database.

use crate::backup;
use crate::compose::{docker, short, Compose, LONG_RUNNING};
use crate::ctx::Ctx;
use crate::db::{self, DbFacts, LIVE_DB};
use crate::envfile::EnvFile;
use crate::error::{CliError, Result};
use crate::manifest::{KIND_PRE_UPGRADE};
use crate::rehearse::{self, RehearsalReport, Target};
use crate::report::{render_steps, Recorder, Step};
use crate::restore::{self, LiveOptions};
use crate::util::{create_private_dir, iso, stamp, write_private};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SKIP_BACKUP_PHRASE: &str = "I HAVE NO BACKUP";
pub const SKIP_REHEARSAL_PHRASE: &str = "SKIP REHEARSAL";
pub const ROLLBACK_PHRASE: &str = "ROLLBACK";

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub version: Option<String>,
    pub registry: Option<String>,
    /// The release's `compose.release.generated.yml` (images pinned by digest).
    pub overlay: Option<PathBuf>,
    /// No new images: exercise the mechanism with the images already installed (development, or a
    /// configuration-only upgrade). Nothing is pulled.
    pub local: bool,
    pub allow_unpinned: bool,
    pub skip_backup: bool,
    pub skip_rehearsal: bool,
    pub plan_only: bool,
    /// Roll back without asking when the new version does not become healthy.
    pub auto_rollback: bool,
    pub backup_to: Option<PathBuf>,
    pub secrets_to: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayImage {
    pub service: String,
    pub reference: String,
    pub digest: Option<String>,
}

/// The `service: image:` pairs of a release overlay.
pub fn parse_overlay(text: &str) -> Vec<OverlayImage> {
    let mut out = Vec::new();
    let mut service: Option<String> = None;
    for line in text.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if indent == 2 && t.ends_with(':') {
            service = Some(t.trim_end_matches(':').to_string());
        } else if indent >= 4 {
            if let (Some(s), Some(image)) = (&service, t.strip_prefix("image:")) {
                let reference = image.trim().trim_matches('"').to_string();
                let digest = reference.split_once("@sha256:").map(|(_, d)| format!("sha256:{d}"));
                out.push(OverlayImage { service: s.clone(), reference, digest });
            }
        }
    }
    out
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub from_version: String,
    pub from_registry: String,
    pub to_version: String,
    pub to_registry: String,
    pub images: Vec<OverlayImage>,
    pub steps: Vec<String>,
    pub warnings: Vec<String>,
}

/// Decide what an upgrade would do. Pure apart from the overlay text it is given.
pub fn plan(opts: &Options, env: &EnvFile, overlay_text: Option<&str>) -> Result<Plan> {
    let from_version = env.get("DYNODOC_VERSION").unwrap_or("dev").to_string();
    let from_registry = env.get("DYNODOC_IMAGE_REGISTRY").unwrap_or("").to_string();
    let mut warnings = Vec::new();
    let (to_version, to_registry) = if opts.local {
        if opts.registry.is_some() || opts.overlay.is_some() {
            return Err(CliError::usage("--local means the images already installed", "remove --registry/--overlay, or remove --local"));
        }
        warnings.push("--local: no new images. This exercises the backup, rehearsal, restart and rollback mechanism with the version already installed; it is not an upgrade to a newer release.".to_string());
        (opts.version.clone().unwrap_or_else(|| from_version.clone()), from_registry.clone())
    } else {
        let version = opts.version.clone().ok_or_else(|| CliError::usage("which release? --version is missing", "pass --version <tag> (and --overlay compose.release.generated.yml from the release), or --local to exercise the mechanism with the installed images"))?;
        (version, opts.registry.clone().unwrap_or_else(|| from_registry.clone()))
    };
    let images = overlay_text.map(parse_overlay).unwrap_or_default();
    if !opts.local {
        if to_registry.is_empty() {
            return Err(CliError::usage("no image registry is known", "pass --registry <host/path>, or install from a registry first"));
        }
        if images.is_empty() && !opts.allow_unpinned {
            return Err(CliError::refused(
                "the new images are not pinned by digest",
                "pass --overlay with the release's compose.release.generated.yml so the pulled images can be verified, or --allow-unpinned to trust the registry tag (not recommended)",
            ));
        }
        if images.is_empty() {
            warnings.push("--allow-unpinned: the images are trusted by tag; nothing proves they are the tested ones.".to_string());
        }
        if !images.is_empty() && images.iter().any(|i| i.digest.is_none()) {
            return Err(CliError::refused("the overlay names an image without a digest", "use the overlay exactly as scripts/make-release-overlay.mjs wrote it"));
        }
    }
    let mut steps = Vec::new();
    if !opts.local {
        steps.push(format!("pull the images of {to_registry} {to_version} and check each digest against the overlay"));
    }
    steps.push(if opts.skip_backup { "NO pre-upgrade backup (--skip-backup, confirmed)".to_string() } else { "take a pre-upgrade backup (mandatory) and keep it for the rollback".to_string() });
    steps.push(if opts.skip_rehearsal { "NO rehearsal (--skip-rehearsal, confirmed)".to_string() } else { "rehearse the new release on a copy of the backup: migrations, history verification, runtime login, API start".to_string() });
    steps.push(format!("switch to {to_version}: save .env, write the new version, docker compose up -d --wait (the init job migrates)"));
    steps.push("check the services, then print how to roll back".to_string());
    Ok(Plan { from_version, from_registry, to_version, to_registry, images, steps, warnings })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpgradeRecord {
    pub id: String,
    pub started_utc: String,
    pub finished_utc: String,
    pub from_version: String,
    pub from_registry: String,
    pub to_version: String,
    pub to_registry: String,
    /// `plan`, `completed`, `rehearsal-failed`, `failed`, `failed-rolled-back`, `rolled-back`.
    pub result: String,
    pub backup_dir: Option<String>,
    pub backup_name: Option<String>,
    pub saved_state_dir: Option<String>,
    pub skipped_backup: bool,
    pub skipped_rehearsal: bool,
    pub rehearsal_ok: Option<bool>,
    pub schema_before: Option<i64>,
    pub schema_after: Option<i64>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpgradeReport {
    pub plan: Plan,
    pub record: UpgradeRecord,
    pub record_path: Option<PathBuf>,
    pub rehearsal: Option<RehearsalReport>,
    pub rollback_hint: String,
    pub ok: bool,
}

impl UpgradeReport {
    pub fn render(&self) -> String {
        let p = &self.plan;
        let mut o = format!("Upgrade {} -> {}\n\nPlan\n", p.from_version, p.to_version);
        for (i, s) in p.steps.iter().enumerate() {
            o.push_str(&format!("  {}. {s}\n", i + 1));
        }
        for w in &p.warnings {
            o.push_str(&format!("\nWARNING: {w}\n"));
        }
        if !self.record.steps.is_empty() {
            o.push_str("\nDone\n");
            o.push_str(&render_steps(&self.record.steps));
        }
        if let Some(r) = &self.rehearsal {
            o.push_str(&format!("\nRehearsal: {}\n", if r.ok { "passed" } else { "FAILED" }));
        }
        o.push_str(&format!("\nResult: {}\n", self.record.result));
        if !self.rollback_hint.is_empty() {
            o.push_str(&format!("\n{}\n", self.rollback_hint));
        }
        if let Some(path) = &self.record_path {
            o.push_str(&format!("\nRecord: {}\n", path.display()));
        }
        o
    }
}

fn digest_check(ctx: &Ctx<'_>, images: &[OverlayImage]) -> Result<usize> {
    let mut checked = 0;
    for image in images {
        let Some(digest) = &image.digest else { continue };
        let out = docker(ctx, &["image", "inspect", "--format", "{{json .RepoDigests}}", &image.reference])?;
        if !out.ok() || !out.stdout_text().contains(digest.as_str()) {
            return Err(CliError::refused(
                format!("the image {} was not pulled with the digest the overlay names ({digest})", image.service),
                "do not upgrade with it: pull again from the release registry, or ask for a new overlay; nothing was changed",
            ));
        }
        checked += 1;
    }
    Ok(checked)
}

fn write_record(ctx: &Ctx<'_>, record: &UpgradeRecord) -> Result<PathBuf> {
    let dir = ctx.layout.state_dir();
    create_private_dir(&dir)?;
    let path = dir.join(format!("upgrade-{}.json", record.id));
    let mut text = serde_json::to_string_pretty(record)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    Ok(path)
}

fn healthy(ctx: &Ctx<'_>) -> Result<Vec<String>> {
    let services = Compose::new(ctx).ps()?;
    let mut bad: Vec<String> = LONG_RUNNING.iter().filter(|n| !services.iter().any(|s| s.service == **n && s.up())).map(|n| n.to_string()).collect();
    for job in ["secrets-init", "preflight", "db-roles", "init"] {
        if services.iter().any(|s| s.service == job && s.state == "exited" && s.exit_code != 0) {
            bad.push(format!("job {job}"));
        }
    }
    Ok(bad)
}

pub fn rollback_hint(record_name: &str, backup: &Option<String>) -> String {
    match backup {
        Some(b) => format!("To go back: `dynodoctl rollback` restores the pre-upgrade backup {b} into the previous version (data written after {record_name} is lost; migrations only move forward, so the old images cannot run on the new database)."),
        None => "There is no pre-upgrade backup, so there is nothing to roll back to; restore from an older backup with `dynodoctl restore --into-live`.".to_string(),
    }
}

pub fn run(ctx: &Ctx<'_>, opts: &Options) -> Result<UpgradeReport> {
    ctx.require_installed()?;
    let env = ctx.env()?;
    let overlay_text = match &opts.overlay {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| CliError::usage(format!("cannot read the overlay {}: {e}", p.display()), "pass the release's compose.release.generated.yml"))?),
        None => None,
    };
    let plan = plan(opts, &env, overlay_text.as_deref())?;
    let started = (ctx.now)();
    let id = stamp(started);
    let mut record = UpgradeRecord {
        id: id.clone(),
        started_utc: iso(started),
        finished_utc: String::new(),
        from_version: plan.from_version.clone(),
        from_registry: plan.from_registry.clone(),
        to_version: plan.to_version.clone(),
        to_registry: plan.to_registry.clone(),
        result: "plan".into(),
        backup_dir: None,
        backup_name: None,
        saved_state_dir: None,
        skipped_backup: false,
        skipped_rehearsal: false,
        rehearsal_ok: None,
        schema_before: None,
        schema_after: None,
        steps: Vec::new(),
    };
    if opts.plan_only {
        return Ok(UpgradeReport { plan, record, record_path: None, rehearsal: None, rollback_hint: String::new(), ok: true });
    }

    // The two things that must never happen silently.
    if opts.skip_backup && !ctx.confirm.confirm("You are about to upgrade WITHOUT a backup. If the upgrade damages the data there is nothing to go back to.", SKIP_BACKUP_PHRASE) {
        return Err(CliError::refused("the upgrade without a backup was not confirmed; nothing was changed", format!("take the backup (drop --skip-backup), or pass --confirm \"{SKIP_BACKUP_PHRASE}\"")));
    }
    if opts.skip_rehearsal && !ctx.confirm.confirm("You are about to upgrade WITHOUT rehearsing the new release on a copy of your data.", SKIP_REHEARSAL_PHRASE) {
        return Err(CliError::refused("the upgrade without a rehearsal was not confirmed; nothing was changed", format!("rehearse (drop --skip-rehearsal), or pass --confirm \"{SKIP_REHEARSAL_PHRASE}\"")));
    }
    record.skipped_backup = opts.skip_backup;
    record.skipped_rehearsal = opts.skip_rehearsal;

    let c = Compose::new(ctx);
    if !c.ps()?.iter().any(|s| s.service == "postgres" && s.up()) {
        return Err(CliError::refused("PostgreSQL is not running, so the installation cannot be upgraded safely", "start it and check `dynodoctl status` first"));
    }
    let mut rec = Recorder::new();
    let changes_version = !opts.local || opts.version.is_some();
    let target = Target {
        version: changes_version.then(|| plan.to_version.clone()),
        registry: (!plan.to_registry.is_empty() && plan.to_registry != plan.from_registry).then(|| plan.to_registry.clone()),
        overlay: opts.overlay.clone(),
    };

    // 1. Pull and verify.
    if !opts.local {
        let tc = target.compose(ctx);
        let pulled = rec.step("docker compose pull (new images)", || {
            let out = tc.run(&["pull"])?;
            if !out.ok() {
                return Err(CliError::failed(format!("pulling the new images failed: {}", short(&out.text())), "check the network, the registry name and `docker login`; nothing was changed"));
            }
            Ok(((), "pulled".to_string()))
        });
        if let Err(e) = pulled {
            return finish(ctx, plan, record, rec, None, "failed", Some(e), String::new());
        }
        if !plan.images.is_empty() {
            let r = rec.step("each image matches its digest in the overlay", || {
                let n = digest_check(ctx, &plan.images)?;
                Ok((n, format!("{n} images")))
            });
            if let Err(e) = r {
                return finish(ctx, plan, record, rec, None, "failed", Some(e), String::new());
            }
        }
    }

    // 2. The backup the rollback depends on.
    let mut backup_info = None;
    if opts.skip_backup {
        rec.note("pre-upgrade backup", true, "SKIPPED (confirmed)");
    } else {
        let options = backup::Options { to: opts.backup_to.clone(), secrets_to: opts.secrets_to.clone(), kind: KIND_PRE_UPGRADE.into(), label: Some("before-upgrade".into()), keep: 3, ..backup::Options::default() };
        match rec.step("pre-upgrade backup", || {
            let r = backup::run(ctx, &options)?;
            Ok((r.clone(), format!("{} ({} events, {} documents)", r.name, r.events, r.documents)))
        }) {
            Ok(r) => {
                record.backup_dir = Some(r.path.display().to_string());
                record.backup_name = Some(r.name.clone());
                backup_info = Some(r);
            }
            Err(e) => return finish(ctx, plan, record, rec, None, "failed", Some(e), String::new()),
        }
    }
    record.schema_before = db::read_facts(&c, LIVE_DB).ok().map(|f: DbFacts| f.latest_migration);

    // 3. Rehearsal on a copy of that backup.
    let mut rehearsal = None;
    if opts.skip_rehearsal {
        rec.note("rehearsal", true, "SKIPPED (confirmed)");
    } else if let Some(info) = &backup_info {
        let config_root = opts.secrets_to.as_ref().map(|d| d.join(format!("{}-config", info.name)));
        match rehearse::run(ctx, Some(&info.path.display().to_string()), config_root.as_deref(), &target) {
            Ok(r) => {
                rec.note("rehearsal on a copy of the backup", r.ok, &format!("schema {} -> {}, {} documents verified, API smoke {}", r.migrations_before, r.migrations_after, r.documents_verified, if r.api_smoke { "ok" } else { "FAILED" }));
                record.rehearsal_ok = Some(r.ok);
                let ok = r.ok;
                rehearsal = Some(r);
                if !ok {
                    let err = CliError::failed("the rehearsal failed, so the installation was NOT upgraded", "read the rehearsal report above; the current version keeps running and the backup is kept");
                    return finish(ctx, plan, record, rec, rehearsal, "rehearsal-failed", Some(err), String::new());
                }
            }
            Err(e) => return finish(ctx, plan, record, rec, None, "rehearsal-failed", Some(e), String::new()),
        }
    } else {
        // Skipped backup but a rehearsal wanted: rehearse on the newest backup there is.
        match rehearse::run(ctx, None, None, &target) {
            Ok(r) => {
                rec.note("rehearsal on the newest existing backup", r.ok, &r.target_version);
                record.rehearsal_ok = Some(r.ok);
                let ok = r.ok;
                rehearsal = Some(r);
                if !ok {
                    let err = CliError::failed("the rehearsal failed, so the installation was NOT upgraded", "the current version keeps running");
                    return finish(ctx, plan, record, rec, rehearsal, "rehearsal-failed", Some(err), String::new());
                }
            }
            Err(e) => {
                let err = if e.what.contains("no backup in") {
                    CliError::refused(
                        "the rehearsal needs a backup to copy, and there is none because the pre-upgrade backup was skipped",
                        "take a backup first (`dynodoctl backup`), or skip the rehearsal as well with --skip-rehearsal (confirmed separately); nothing was changed",
                    )
                } else {
                    e
                };
                return finish(ctx, plan, record, rec, None, "rehearsal-failed", Some(err), String::new());
            }
        }
    }

    // 4. Switch.
    let saved = ctx.layout.state_dir().join(format!("pre-upgrade-{id}"));
    let switch = rec.step("save .env and the overlay, write the new version", || {
        create_private_dir(&saved)?;
        write_private(&saved.join("env"), std::fs::read(ctx.layout.env_file())?.as_slice())?;
        if ctx.layout.overlay_file().is_file() {
            write_private(&saved.join(crate::ctx::OVERLAY_NAME), std::fs::read(ctx.layout.overlay_file())?.as_slice())?;
        }
        let mut new_env = ctx.env()?;
        if changes_version {
            new_env.set("DYNODOC_VERSION", &plan.to_version)?;
        }
        if !plan.to_registry.is_empty() {
            new_env.set("DYNODOC_IMAGE_REGISTRY", &plan.to_registry)?;
        }
        write_private(&ctx.layout.env_file(), new_env.render().as_bytes())?;
        if let Some(overlay) = &opts.overlay {
            write_private(&ctx.layout.overlay_file(), std::fs::read(overlay)?.as_slice())?;
        }
        Ok(((), saved.display().to_string()))
    });
    if let Err(e) = switch {
        return finish(ctx, plan, record, rec, rehearsal, "failed", Some(e), String::new());
    }
    record.saved_state_dir = Some(saved.display().to_string());

    let up = rec.step("docker compose up -d --wait (migrations run in init)", || {
        let out = Compose::new(ctx).run(&["up", "-d", "--wait", "--wait-timeout", "300"])?;
        if !out.ok() {
            return Err(CliError::failed(format!("the new version did not become healthy: {}", short(&out.text())), "see below"));
        }
        let bad = healthy(ctx)?;
        if !bad.is_empty() {
            return Err(CliError::failed(format!("not healthy after the upgrade: {}", bad.join(", ")), "see below"));
        }
        Ok(((), "all services healthy".to_string()))
    });
    if let Err(e) = up {
        let hint = rollback_hint(&record.id, &record.backup_name);
        let from_version = plan.from_version.clone();
        // Offer the way back at once.
        let wants = record.backup_dir.is_some()
            && (opts.auto_rollback || ctx.confirm.confirm("The new version is not healthy. Roll back now: restore the pre-upgrade backup into the previous version? Data written since the backup is lost.", ROLLBACK_PHRASE));
        if wants {
            let rolled = rollback_with(ctx, &record, true);
            match rolled {
                Ok(report) if report.ok => {
                    rec.note("rollback to the pre-upgrade backup", true, &record.backup_name.clone().unwrap_or_default());
                    return finish(ctx, plan, record, rec, rehearsal, "failed-rolled-back", Some(CliError::failed(format!("{}; the installation was rolled back to {} and is running the previous version", e.what, from_version), "find out why before trying again: `docker compose logs`")), String::new());
                }
                Ok(report) => {
                    rec.note("rollback to the pre-upgrade backup", false, "did not verify");
                    return finish(ctx, plan, record, rec, rehearsal, "failed", Some(CliError::failed(format!("{}; the rollback ran but did not verify:\n{}", e.what, report.render()), "keep the pre-upgrade backup; see docs/SELF-HOSTED-OPERATIONS.md, \"If an upgrade fails\"")), hint);
                }
                Err(re) => {
                    rec.note("rollback to the pre-upgrade backup", false, &re.what);
                    return finish(ctx, plan, record, rec, rehearsal, "failed", Some(CliError::failed(format!("{}; the rollback failed too: {}", e.what, re.what), re.todo)), hint);
                }
            }
        }
        return finish(ctx, plan, record, rec, rehearsal, "failed", Some(e), hint);
    }
    record.schema_after = db::read_facts(&Compose::new(ctx), LIVE_DB).ok().map(|f| f.latest_migration);
    let hint = rollback_hint(&record.id, &record.backup_name);
    finish(ctx, plan, record, rec, rehearsal, "completed", None, hint)
}

#[allow(clippy::too_many_arguments)]
fn finish(ctx: &Ctx<'_>, plan: Plan, mut record: UpgradeRecord, rec: Recorder, rehearsal: Option<RehearsalReport>, result: &str, error: Option<CliError>, hint: String) -> Result<UpgradeReport> {
    record.result = result.to_string();
    record.finished_utc = iso((ctx.now)());
    record.steps = rec.steps;
    let path = write_record(ctx, &record).ok();
    match error {
        Some(e) => {
            let mut message = e;
            if let Some(p) = &path {
                message.todo = format!("{} (record: {})", message.todo, p.display());
            }
            if !hint.is_empty() {
                message.todo = format!("{} {}", message.todo, hint);
            }
            Err(message)
        }
        None => Ok(UpgradeReport { plan, record, record_path: path, rehearsal, rollback_hint: hint, ok: true }),
    }
}

/// The most recent upgrade record that can still be rolled back.
pub fn latest_record(ctx: &Ctx<'_>, named: Option<&str>) -> Result<(UpgradeRecord, PathBuf)> {
    let dir = ctx.layout.state_dir();
    let mut found: Vec<(UpgradeRecord, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("upgrade-") && name.ends_with(".json") {
                if let Ok(text) = std::fs::read_to_string(e.path()) {
                    if let Ok(r) = serde_json::from_str::<UpgradeRecord>(&text) {
                        found.push((r, e.path()));
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let pick = match named {
        Some(n) => found.into_iter().find(|(r, p)| r.id == n || p.to_string_lossy() == n || p.file_name().map(|f| f.to_string_lossy() == n).unwrap_or(false)),
        None => found.into_iter().rev().find(|(r, _)| matches!(r.result.as_str(), "completed" | "failed")),
    };
    pick.ok_or_else(|| CliError::refused("there is no upgrade to roll back", "an upgrade leaves a record in state/; to restore any backup use `dynodoctl restore --into-live`"))
}

pub struct RollbackReport {
    pub restore: restore::RestoreReport,
    pub ok: bool,
}

impl RollbackReport {
    pub fn render(&self) -> String {
        self.restore.render()
    }
}

fn rollback_with(ctx: &Ctx<'_>, record: &UpgradeRecord, confirmed: bool) -> Result<RollbackReport> {
    let backup_dir = record.backup_dir.clone().ok_or_else(|| CliError::refused("that upgrade was made without a backup, so there is nothing to roll back to", "restore an older backup with `dynodoctl restore --into-live --backup <dir>`"))?;
    if !confirmed
        && !ctx.confirm.confirm(
            &format!("Roll back to the pre-upgrade backup {} and the previous version {}. Everything written since is lost.", record.backup_name.clone().unwrap_or_default(), record.from_version),
            ROLLBACK_PHRASE,
        )
    {
        return Err(CliError::refused("the rollback was not confirmed; nothing was changed", format!("run again and type the phrase, or pass --confirm {ROLLBACK_PHRASE}")));
    }
    // The previous version's settings and overlay come back first, so `up` starts the old images.
    if let Some(saved) = &record.saved_state_dir {
        let saved = Path::new(saved);
        if let Ok(bytes) = std::fs::read(saved.join("env")) {
            write_private(&ctx.layout.env_file(), &bytes)?;
        }
        match std::fs::read(saved.join(crate::ctx::OVERLAY_NAME)) {
            Ok(bytes) => write_private(&ctx.layout.overlay_file(), &bytes)?,
            Err(_) => {
                let _ = std::fs::remove_file(ctx.layout.overlay_file());
            }
        }
    }
    let config_from = None;
    let report = restore::restore_live(ctx, &LiveOptions { backup: Some(backup_dir), config_from, no_safety_backup: false, keep_previous: false, confirmed: true })?;
    let ok = report.ok;
    Ok(RollbackReport { restore: report, ok })
}

/// `dynodoctl rollback`.
pub fn rollback(ctx: &Ctx<'_>, record_arg: Option<&str>) -> Result<RollbackReport> {
    ctx.require_installed()?;
    let (mut record, path) = latest_record(ctx, record_arg)?;
    let report = rollback_with(ctx, &record, false)?;
    record.result = if report.ok { "rolled-back".into() } else { record.result };
    let mut text = serde_json::to_string_pretty(&record)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    Ok(report)
}

#[cfg(test)]
#[path = "tests/upgrade.rs"]
mod tests;
