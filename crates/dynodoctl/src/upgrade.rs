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
use crate::manifest::KIND_PRE_UPGRADE;
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

/// `sha256:` followed by exactly 64 lowercase hex digits, and nothing else.
pub fn valid_digest(text: &str) -> bool {
    text.strip_prefix("sha256:")
        .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
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
                // M1 (d): only a full digest counts (`sha256:` and exactly 64 lowercase hex).
                let digest = reference
                    .split_once('@')
                    .map(|(_, d)| d.to_string())
                    .filter(|d| valid_digest(d));
                out.push(OverlayImage {
                    service: s.clone(),
                    reference,
                    digest,
                });
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
        // M1 (a): `--local --version X` would switch DYNODOC_VERSION with no overlay, no digest
        // check and no --allow-unpinned: every gate of a release upgrade, bypassed with one flag.
        if opts.registry.is_some() || opts.overlay.is_some() || opts.version.is_some() {
            return Err(CliError::usage(
                "--local means the images already installed, so it cannot be combined with --version, --registry or --overlay",
                "remove --local to upgrade to a release (with --version and --overlay), or remove the other options",
            ));
        }
        warnings.push("--local: no new images. This exercises the backup, rehearsal, restart and rollback mechanism with the version already installed; it is not an upgrade to a newer release.".to_string());
        (from_version.clone(), from_registry.clone())
    } else {
        let version = opts.version.clone().ok_or_else(|| CliError::usage("which release? --version is missing", "pass --version <tag> (and --overlay compose.release.generated.yml from the release), or --local to exercise the mechanism with the installed images"))?;
        (
            version,
            opts.registry
                .clone()
                .unwrap_or_else(|| from_registry.clone()),
        )
    };
    let images = overlay_text.map(parse_overlay).unwrap_or_default();
    if !opts.local {
        if to_registry.is_empty() {
            return Err(CliError::usage(
                "no image registry is known",
                "pass --registry <host/path>, or install from a registry first",
            ));
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
            let names: Vec<&str> = images
                .iter()
                .filter(|i| i.digest.is_none())
                .map(|i| i.service.as_str())
                .collect();
            return Err(CliError::refused(
                format!("the overlay names an image without a full sha256 digest (`@sha256:` and 64 hex characters): {}", names.join(", ")),
                "use the overlay exactly as scripts/make-release-overlay.mjs wrote it",
            ));
        }
    }
    let mut steps = Vec::new();
    if !opts.local {
        steps.push(format!("pull the images of {to_registry} {to_version} and check each digest against the overlay"));
    }
    steps.push(if opts.skip_backup {
        "NO pre-upgrade backup (--skip-backup, confirmed)".to_string()
    } else {
        "take a pre-upgrade backup (mandatory) and keep it for the rollback".to_string()
    });
    steps.push(if opts.skip_rehearsal { "NO rehearsal (--skip-rehearsal, confirmed)".to_string() } else { "rehearse the new release on a copy of the backup: migrations, history verification, runtime login, API start".to_string() });
    steps.push(format!("switch to {to_version}: save .env, write the new version, docker compose up -d --wait (the init job migrates)"));
    steps.push("check the services, then print how to roll back".to_string());
    Ok(Plan {
        from_version,
        from_registry,
        to_version,
        to_registry,
        images,
        steps,
        warnings,
    })
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
    /// Where the copy of the settings and secrets went, if it was sent away (`--secrets-to`).
    #[serde(default)]
    pub config_dir: Option<String>,
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
            o.push_str(&format!(
                "\nRehearsal: {}\n",
                if r.ok { "passed" } else { "FAILED" }
            ));
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

/// The repository part of an image reference: no digest, no tag, and Docker Hub's default
/// prefixes removed, so `docker.io/library/postgres:16@sha256:..` and `postgres@sha256:..` agree.
fn repository(reference: &str) -> String {
    let no_digest = reference.split_once('@').map_or(reference, |(r, _)| r);
    let last_slash = no_digest.rfind('/').map_or(0, |i| i + 1);
    let no_tag = match no_digest[last_slash..].find(':') {
        Some(i) => &no_digest[..last_slash + i],
        None => no_digest,
    };
    let mut repo = no_tag;
    for prefix in ["index.docker.io/", "docker.io/"] {
        repo = repo.strip_prefix(prefix).unwrap_or(repo);
    }
    repo.strip_prefix("library/").unwrap_or(repo).to_string()
}

/// Does `RepoDigests` (a JSON array of `repo@sha256:..`) contain exactly this repository and digest?
fn repo_digests_contain(json: &str, reference: &str, digest: &str) -> bool {
    let Ok(entries) = serde_json::from_str::<Vec<String>>(json.trim()) else {
        return false;
    };
    let wanted = repository(reference);
    entries.iter().any(|entry| {
        entry
            .split_once('@')
            .is_some_and(|(repo, d)| d == digest && repository(repo) == wanted)
    })
}

/// The `service -> image` pairs Compose will actually run (overlay merged, variables resolved,
/// the `maintenance` jobs included), from `docker compose config --format json`.
fn effective_images(tc: &Compose<'_>) -> Result<Vec<(String, String)>> {
    let out = tc.run(&["--profile", "maintenance", "config", "--format", "json"])?;
    let parsed = out
        .ok()
        .then(|| serde_json::from_str::<serde_json::Value>(&out.stdout_text()).ok())
        .flatten();
    let services = parsed
        .as_ref()
        .and_then(|v| v.get("services"))
        .and_then(|s| s.as_object())
        .ok_or_else(|| {
            CliError::refused(
                format!("could not read the configuration Compose would run: {}", short(&out.text())),
                "the images cannot be verified without it; nothing was changed. Check `docker compose config` in the bundle directory",
            )
        })?;
    let mut images: Vec<(String, String)> = services
        .iter()
        .filter_map(|(name, def)| {
            def.get("image")
                .and_then(|i| i.as_str())
                .map(|i| (name.clone(), i.to_string()))
        })
        .collect();
    images.sort();
    Ok(images)
}

/// M1 (d). The images Compose will run must (1) ALL be pinned by a full digest, whichever file
/// pins them, (2) be the ones the overlay names, and (3) have been pulled with exactly that
/// repository and digest. Returns how many were checked.
fn digest_check(ctx: &Ctx<'_>, tc: &Compose<'_>, images: &[OverlayImage]) -> Result<usize> {
    let effective = effective_images(tc)?;
    let unpinned: Vec<String> = effective
        .iter()
        .filter(|(_, image)| !image.split_once('@').is_some_and(|(_, d)| valid_digest(d)))
        .map(|(service, image)| format!("{service} ({image})"))
        .collect();
    if !unpinned.is_empty() {
        return Err(CliError::refused(
            format!("these services would run an image that is not pinned by a full digest: {}", unpinned.join(", ")),
            "the overlay must cover every service image of the installation; ask for a complete compose.release.generated.yml. Nothing was changed",
        ));
    }
    for image in images {
        match effective
            .iter()
            .find(|(service, _)| *service == image.service)
        {
            Some((_, running)) if *running == image.reference => {}
            Some((_, running)) => {
                return Err(CliError::refused(
                    format!(
                        "the overlay names {} for {}, but Compose would run {running}",
                        image.reference, image.service
                    ),
                    "the overlay does not apply as intended; nothing was changed",
                ));
            }
            None => {
                return Err(CliError::refused(
                    format!(
                        "the overlay names a service this installation does not have: {}",
                        image.service
                    ),
                    "use the overlay of the release you are installing; nothing was changed",
                ));
            }
        }
    }
    let mut checked = 0;
    let mut seen: Vec<&str> = Vec::new();
    for (service, reference) in &effective {
        if seen.contains(&reference.as_str()) {
            continue;
        }
        seen.push(reference);
        let digest = reference
            .split_once('@')
            .map(|(_, d)| d)
            .unwrap_or_default();
        let out = docker(
            ctx,
            &[
                "image",
                "inspect",
                "--format",
                "{{json .RepoDigests}}",
                reference,
            ],
        )?;
        if !out.ok() || !repo_digests_contain(&out.stdout_text(), reference, digest) {
            return Err(CliError::refused(
                format!("the image of {service} was not pulled with the repository and digest the release names ({digest})"),
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
    let mut bad: Vec<String> = LONG_RUNNING
        .iter()
        .filter(|n| !services.iter().any(|s| s.service == **n && s.up()))
        .map(|n| n.to_string())
        .collect();
    for job in ["secrets-init", "preflight", "db-roles", "init"] {
        if services
            .iter()
            .any(|s| s.service == job && s.state == "exited" && s.exit_code != 0)
        {
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
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| {
            CliError::usage(
                format!("cannot read the overlay {}: {e}", p.display()),
                "pass the release's compose.release.generated.yml",
            )
        })?),
        None => None,
    };
    let plan = plan(opts, &env, overlay_text.as_deref())?;
    // M1 (c): with an overlay of the installed release in place, an upgrade by tag would keep
    // running the old pinned images while DYNODOC_VERSION moves on. Say so instead.
    if !opts.local && opts.overlay.is_none() && ctx.layout.overlay_file().is_file() {
        return Err(CliError::refused(
            format!(
                "{} from the installed release pins the images by digest, so changing the version by tag would keep running the old images",
                crate::ctx::OVERLAY_NAME
            ),
            "pass --overlay with the new release's file, or, if you really mean to trust the registry tag, move that overlay file out of the directory first; nothing was changed",
        ));
    }
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
        config_dir: None,
        saved_state_dir: None,
        skipped_backup: false,
        skipped_rehearsal: false,
        rehearsal_ok: None,
        schema_before: None,
        schema_after: None,
        steps: Vec::new(),
    };
    if opts.plan_only {
        return Ok(UpgradeReport {
            plan,
            record,
            record_path: None,
            rehearsal: None,
            rollback_hint: String::new(),
            ok: true,
        });
    }

    crate::recover::require_no_journal(ctx)?;

    // The two things that must never happen silently.
    if opts.skip_backup && !ctx.confirm.confirm("You are about to upgrade WITHOUT a backup. If the upgrade damages the data there is nothing to go back to.", SKIP_BACKUP_PHRASE) {
        return Err(CliError::refused("the upgrade without a backup was not confirmed; nothing was changed", format!("take the backup (drop --skip-backup), or pass --confirm \"{SKIP_BACKUP_PHRASE}\"")));
    }
    if opts.skip_rehearsal
        && !ctx.confirm.confirm(
            "You are about to upgrade WITHOUT rehearsing the new release on a copy of your data.",
            SKIP_REHEARSAL_PHRASE,
        )
    {
        return Err(CliError::refused(
            "the upgrade without a rehearsal was not confirmed; nothing was changed",
            format!(
                "rehearse (drop --skip-rehearsal), or pass --confirm \"{SKIP_REHEARSAL_PHRASE}\""
            ),
        ));
    }
    record.skipped_backup = opts.skip_backup;
    record.skipped_rehearsal = opts.skip_rehearsal;
    // M9: containers of this Compose project name must belong to this directory.
    crate::compose::guard_project(ctx)?;

    let c = Compose::new(ctx);
    if !c.ps()?.iter().any(|s| s.service == "postgres" && s.up()) {
        return Err(CliError::refused(
            "PostgreSQL is not running, so the installation cannot be upgraded safely",
            "start it and check `dynodoctl status` first",
        ));
    }
    let mut rec = Recorder::for_ctx(ctx);
    let changes_version = !opts.local;
    let target = Target {
        version: changes_version.then(|| plan.to_version.clone()),
        registry: (!plan.to_registry.is_empty() && plan.to_registry != plan.from_registry)
            .then(|| plan.to_registry.clone()),
        overlay: opts.overlay.clone(),
    };

    // 1. Pull and verify.
    if !opts.local {
        let tc = target.compose(ctx);
        let pulled = rec.step("docker compose pull (new images)", || {
            let out = tc.run(&["pull"])?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!("pulling the new images failed: {}", short(&out.text())),
                    "check the network, the registry name and `docker login`; nothing was changed",
                ));
            }
            Ok(((), "pulled".to_string()))
        });
        if let Err(e) = pulled {
            return finish(
                ctx,
                plan,
                record,
                rec,
                None,
                "failed",
                Some(e),
                String::new(),
            );
        }
        if !plan.images.is_empty() {
            let r = rec.step("each image matches its digest in the overlay", || {
                let n = digest_check(ctx, &tc, &plan.images)?;
                Ok((n, format!("{n} images")))
            });
            if let Err(e) = r {
                return finish(
                    ctx,
                    plan,
                    record,
                    rec,
                    None,
                    "failed",
                    Some(e),
                    String::new(),
                );
            }
        }
    }

    // 2. The backup the rollback depends on.
    let mut backup_info = None;
    if opts.skip_backup {
        rec.note("pre-upgrade backup", true, "SKIPPED (confirmed)");
    } else {
        let options = backup::Options {
            to: opts.backup_to.clone(),
            secrets_to: opts.secrets_to.clone(),
            kind: KIND_PRE_UPGRADE.into(),
            label: Some("before-upgrade".into()),
            keep: 3,
            ..backup::Options::default()
        };
        match rec.step("pre-upgrade backup", || {
            let r = backup::run(ctx, &options)?;
            Ok((
                r.clone(),
                format!(
                    "{} ({} events, {} documents)",
                    r.name, r.events, r.documents
                ),
            ))
        }) {
            Ok(r) => {
                record.backup_dir = Some(r.path.display().to_string());
                record.backup_name = Some(r.name.clone());
                record.config_dir = opts
                    .secrets_to
                    .as_ref()
                    .map(|d| d.join(format!("{}-config", r.name)).display().to_string());
                backup_info = Some(r);
            }
            Err(e) => {
                return finish(
                    ctx,
                    plan,
                    record,
                    rec,
                    None,
                    "failed",
                    Some(e),
                    String::new(),
                )
            }
        }
    }
    record.schema_before = db::read_facts(&c, LIVE_DB)
        .ok()
        .map(|f: DbFacts| f.latest_migration);

    // 3. Rehearsal on a copy of that backup.
    let mut rehearsal = None;
    if opts.skip_rehearsal {
        rec.note("rehearsal", true, "SKIPPED (confirmed)");
    } else if let Some(info) = &backup_info {
        let config_root = opts
            .secrets_to
            .as_ref()
            .map(|d| d.join(format!("{}-config", info.name)));
        match rehearse::run(
            ctx,
            Some(&info.path.display().to_string()),
            config_root.as_deref(),
            &target,
        ) {
            Ok(r) => {
                rec.note(
                    "rehearsal on a copy of the backup",
                    r.ok,
                    &format!(
                        "schema {} -> {}, {} documents verified, API smoke {}",
                        r.migrations_before,
                        r.migrations_after,
                        r.documents_verified,
                        if r.api_smoke { "ok" } else { "FAILED" }
                    ),
                );
                record.rehearsal_ok = Some(r.ok);
                let ok = r.ok;
                rehearsal = Some(r);
                if !ok {
                    let err = CliError::failed("the rehearsal failed, so the installation was NOT upgraded", "read the rehearsal report above; the current version keeps running and the backup is kept");
                    return finish(
                        ctx,
                        plan,
                        record,
                        rec,
                        rehearsal,
                        "rehearsal-failed",
                        Some(err),
                        String::new(),
                    );
                }
            }
            Err(e) => {
                return finish(
                    ctx,
                    plan,
                    record,
                    rec,
                    None,
                    "rehearsal-failed",
                    Some(e),
                    String::new(),
                )
            }
        }
    } else {
        // Skipped backup but a rehearsal wanted: rehearse on the newest backup there is.
        match rehearse::run(ctx, None, None, &target) {
            Ok(r) => {
                rec.note(
                    "rehearsal on the newest existing backup",
                    r.ok,
                    &r.target_version,
                );
                record.rehearsal_ok = Some(r.ok);
                let ok = r.ok;
                rehearsal = Some(r);
                if !ok {
                    let err = CliError::failed(
                        "the rehearsal failed, so the installation was NOT upgraded",
                        "the current version keeps running",
                    );
                    return finish(
                        ctx,
                        plan,
                        record,
                        rec,
                        rehearsal,
                        "rehearsal-failed",
                        Some(err),
                        String::new(),
                    );
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
                return finish(
                    ctx,
                    plan,
                    record,
                    rec,
                    None,
                    "rehearsal-failed",
                    Some(err),
                    String::new(),
                );
            }
        }
    }

    // 4. Switch. What can fail without changing the installation comes first, and the record is
    // written BEFORE `.env` changes: an upgrade that is interrupted after the switch must never
    // read as "nothing to roll back" (H3 c).
    let saved = ctx.layout.state_dir().join(format!("pre-upgrade-{id}"));
    let prepared = rec.step("save .env and the overlay; record the upgrade", || {
        create_private_dir(&saved)?;
        write_private(
            &saved.join("env"),
            std::fs::read(ctx.layout.env_file())?.as_slice(),
        )?;
        if ctx.layout.overlay_file().is_file() {
            write_private(
                &saved.join(crate::ctx::OVERLAY_NAME),
                std::fs::read(ctx.layout.overlay_file())?.as_slice(),
            )?;
        }
        record.saved_state_dir = Some(saved.display().to_string());
        record.result = "switching".into();
        write_record(ctx, &record)?;
        Ok(((), saved.display().to_string()))
    });
    if let Err(e) = prepared {
        record.saved_state_dir = None;
        return finish(
            ctx,
            plan,
            record,
            rec,
            rehearsal,
            "failed",
            Some(e),
            String::new(),
        );
    }
    let switch = rec.step("write the new version and overlay", || {
        let mut new_env = ctx.env()?;
        if changes_version {
            new_env.set("DYNODOC_VERSION", &plan.to_version)?;
        }
        if !plan.to_registry.is_empty() {
            new_env.set("DYNODOC_IMAGE_REGISTRY", &plan.to_registry)?;
        }
        write_private(&ctx.layout.env_file(), new_env.render().as_bytes())?;
        if let Some(overlay) = &opts.overlay {
            write_private(
                &ctx.layout.overlay_file(),
                std::fs::read(overlay)?.as_slice(),
            )?;
        }
        Ok(((), plan.to_version.clone()))
    });
    if let Err(mut e) = switch {
        if restore_saved(ctx, &saved).is_err() {
            e.todo = format!(
                "{} Your previous settings are saved in {}",
                e.todo,
                saved.display()
            );
        }
        return finish(
            ctx,
            plan,
            record,
            rec,
            rehearsal,
            "failed",
            Some(e),
            String::new(),
        );
    }

    let up = rec.step(
        "docker compose up -d --wait (migrations run in init)",
        || {
            let out = Compose::new(ctx).shielded().run(&[
                "up",
                "-d",
                "--wait",
                "--wait-timeout",
                "300",
            ])?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!(
                        "the new version did not become healthy: {}",
                        short(&out.text())
                    ),
                    "see below",
                ));
            }
            let bad = healthy(ctx)?;
            if !bad.is_empty() {
                return Err(CliError::failed(
                    format!("not healthy after the upgrade: {}", bad.join(", ")),
                    "see below",
                ));
            }
            Ok(((), "all services healthy".to_string()))
        },
    );
    if let Err(e) = up {
        let hint = rollback_hint(&record.id, &record.backup_name);
        let from_version = plan.from_version.clone();
        if e.what == crate::report::interrupted_error().what {
            // Interrupted before the services were touched: only the settings changed, so put them back.
            let back = restore_saved(ctx, &saved);
            let mut e = e;
            e.what = format!(
                "{}; the services were not touched and {}",
                e.what,
                if back.is_ok() {
                    "the previous settings were put back".to_string()
                } else {
                    format!(
                        "the previous settings could NOT be put back (copies are in {})",
                        saved.display()
                    )
                }
            );
            return finish(
                ctx,
                plan,
                record,
                rec,
                rehearsal,
                "interrupted",
                Some(e),
                String::new(),
            );
        }
        // Offer the way back at once (not while an interrupt is pending: nobody is at the prompt).
        let wants = record.backup_dir.is_some()
            && !ctx.interrupted()
            && (opts.auto_rollback || ctx.confirm.confirm("The new version is not healthy. Roll back now: restore the pre-upgrade backup into the previous version? Data written since the backup is lost.", ROLLBACK_PHRASE));
        if wants {
            let rolled = rollback_with(ctx, &record, true);
            match rolled {
                Ok(report) if report.ok => {
                    rec.note(
                        "rollback to the pre-upgrade backup",
                        true,
                        &record.backup_name.clone().unwrap_or_default(),
                    );
                    return finish(ctx, plan, record, rec, rehearsal, "failed-rolled-back", Some(CliError::failed(format!("{}; the installation was rolled back to {} and is running the previous version", e.what, from_version), "find out why before trying again: `docker compose logs`")), String::new());
                }
                Ok(report) => {
                    rec.note(
                        "rollback to the pre-upgrade backup",
                        false,
                        "did not verify",
                    );
                    return finish(ctx, plan, record, rec, rehearsal, "failed", Some(CliError::failed(format!("{}; the rollback ran but did not verify:\n{}", e.what, report.render()), "keep the pre-upgrade backup; see docs/SELF-HOSTED-OPERATIONS.md, \"If an upgrade fails\"")), hint);
                }
                Err(re) => {
                    rec.note("rollback to the pre-upgrade backup", false, &re.what);
                    return finish(
                        ctx,
                        plan,
                        record,
                        rec,
                        rehearsal,
                        "failed",
                        Some(CliError::failed(
                            format!("{}; the rollback failed too: {}", e.what, re.what),
                            re.todo,
                        )),
                        hint,
                    );
                }
            }
        }
        return finish(ctx, plan, record, rec, rehearsal, "failed", Some(e), hint);
    }
    record.schema_after = db::read_facts(&Compose::new(ctx), LIVE_DB)
        .ok()
        .map(|f| f.latest_migration);
    let hint = rollback_hint(&record.id, &record.backup_name);
    finish(ctx, plan, record, rec, rehearsal, "completed", None, hint)
}

#[allow(clippy::too_many_arguments)]
fn finish(
    ctx: &Ctx<'_>,
    plan: Plan,
    mut record: UpgradeRecord,
    rec: Recorder,
    rehearsal: Option<RehearsalReport>,
    result: &str,
    error: Option<CliError>,
    hint: String,
) -> Result<UpgradeReport> {
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
        None => Ok(UpgradeReport {
            plan,
            record,
            record_path: path,
            rehearsal,
            rollback_hint: hint,
            ok: true,
        }),
    }
}

fn read_records(ctx: &Ctx<'_>) -> Vec<(UpgradeRecord, PathBuf)> {
    let mut found: Vec<(UpgradeRecord, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ctx.layout.state_dir()) {
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
    found
}

/// The newest upgrade record, if it says the switch began and no end was recorded (H3 c).
pub fn unfinished(ctx: &Ctx<'_>) -> Option<UpgradeRecord> {
    read_records(ctx)
        .pop()
        .map(|(r, _)| r)
        .filter(|r| r.result == "switching")
}

/// The most recent upgrade record that can still be rolled back.
pub fn latest_record(ctx: &Ctx<'_>, named: Option<&str>) -> Result<(UpgradeRecord, PathBuf)> {
    let found = read_records(ctx);
    let pick = match named {
        Some(n) => found.into_iter().find(|(r, p)| {
            r.id == n
                || p.to_string_lossy() == n
                || p.file_name()
                    .map(|f| f.to_string_lossy() == n)
                    .unwrap_or(false)
        }),
        None => found
            .into_iter()
            .rev()
            .find(|(r, _)| matches!(r.result.as_str(), "completed" | "failed" | "switching")),
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

/// Put back the `.env` and overlay that were saved before a switch (the installation as it was).
fn restore_saved(ctx: &Ctx<'_>, saved: &Path) -> Result<()> {
    let env = std::fs::read(saved.join("env"))?;
    write_private(&ctx.layout.env_file(), &env)?;
    match std::fs::read(saved.join(crate::ctx::OVERLAY_NAME)) {
        Ok(bytes) => write_private(&ctx.layout.overlay_file(), &bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let _ = std::fs::remove_file(ctx.layout.overlay_file());
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn rollback_with(ctx: &Ctx<'_>, record: &UpgradeRecord, confirmed: bool) -> Result<RollbackReport> {
    let backup_dir = record.backup_dir.clone().ok_or_else(|| {
        CliError::refused(
            "that upgrade was made without a backup, so there is nothing to roll back to",
            "restore an older backup with `dynodoctl restore --into-live --backup <dir>`",
        )
    })?;
    crate::recover::require_no_journal(ctx)?;
    // H2: everything that can be refused is checked BEFORE a single file changes, so "nothing was
    // changed" is true when it is said. The backup is found and its files verified first.
    let config_from = record
        .config_dir
        .as_ref()
        .map(PathBuf::from)
        .filter(|p| p.is_dir());
    let info = restore::resolve_backup(ctx, Some(&backup_dir))?;
    crate::manifest::verify_files(&info, config_from.as_deref()).map_err(|p| {
        CliError::refused(
            format!("the pre-upgrade backup is damaged or incomplete: {}", p.join("; ")),
            "nothing was changed; use `dynodoctl restore --into-live --backup <dir>` with an older backup",
        )
    })?;
    if !confirmed
        && !ctx.confirm.confirm(
            &format!("Roll back to the pre-upgrade backup {} and the previous version {}. Everything written since is lost.", record.backup_name.clone().unwrap_or_default(), record.from_version),
            ROLLBACK_PHRASE,
        )
    {
        return Err(CliError::refused("the rollback was not confirmed; nothing was changed", format!("run again and type the phrase, or pass --confirm {ROLLBACK_PHRASE}")));
    }
    // The previous version's settings and overlay come back first, so `up` starts the old images.
    // What is there now is kept in memory: if the restore does not change the database, it goes back.
    let current_env = std::fs::read(ctx.layout.env_file())?;
    let current_overlay = std::fs::read(ctx.layout.overlay_file()).ok();
    let put_current_back = |ctx: &Ctx<'_>| -> Result<()> {
        write_private(&ctx.layout.env_file(), &current_env)?;
        match &current_overlay {
            Some(bytes) => write_private(&ctx.layout.overlay_file(), bytes)?,
            None => {
                let _ = std::fs::remove_file(ctx.layout.overlay_file());
            }
        }
        Ok(())
    };
    if let Some(saved) = &record.saved_state_dir {
        restore_saved(ctx, Path::new(saved))?;
    }
    let mut report = match restore::restore_live(
        ctx,
        &LiveOptions {
            backup: Some(backup_dir),
            config_from,
            no_safety_backup: false,
            keep_previous: false,
            confirmed: true,
        },
    ) {
        Ok(report) => report,
        Err(e) => {
            // The restore refused or failed before it changed anything: the settings go back too.
            return Err(match put_current_back(ctx) {
                Ok(()) => e,
                Err(back) => CliError::failed(
                    format!(
                        "{}; the previous settings could NOT be put back: {}",
                        e.what, back.what
                    ),
                    format!(
                        "{} Your settings before the rollback began are in {}",
                        e.todo,
                        record.saved_state_dir.clone().unwrap_or_default()
                    ),
                ),
            });
        }
    };
    if !report.ok && !report.live_changed {
        // The old data is still the live data (nothing was replaced, or the swap was undone), so the
        // newer version's settings must come back as well, and the services must follow them.
        match put_current_back(ctx) {
            Ok(()) => {
                let up = Compose::new(ctx)
                    .shielded()
                    .run(&["up", "-d", "--wait", "--wait-timeout", "300"]);
                let started = up.as_ref().map(|o| o.ok()).unwrap_or(false);
                report.notes.push(format!(
                    "the rollback did not change the database, so the settings from before it were put back{}",
                    if started { " and the services restarted with them" } else { "; the services could NOT be restarted, run `docker compose up -d --wait`" }
                ));
            }
            Err(e) => report.notes.push(format!(
                "the rollback did not change the database, but the settings from before it could NOT be put back: {}",
                e.what
            )),
        }
    }
    let ok = report.ok;
    Ok(RollbackReport {
        restore: report,
        ok,
    })
}

/// `dynodoctl rollback`.
pub fn rollback(ctx: &Ctx<'_>, record_arg: Option<&str>) -> Result<RollbackReport> {
    ctx.require_installed()?;
    crate::recover::require_no_journal(ctx)?;
    let (mut record, path) = latest_record(ctx, record_arg)?;
    let report = rollback_with(ctx, &record, false)?;
    record.result = if report.ok {
        "rolled-back".into()
    } else {
        record.result
    };
    let mut text = serde_json::to_string_pretty(&record)?;
    text.push('\n');
    write_private(&path, text.as_bytes())?;
    Ok(report)
}

#[cfg(test)]
#[path = "tests/upgrade.rs"]
mod tests;
