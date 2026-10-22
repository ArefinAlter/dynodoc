//! `dynodoctl backup`: a consistent `pg_dump -Fc` of the database, a copy of the settings and
//! secrets, a manifest with checksums, and retention.
//!
//! Decisions (docs/SELF-HOSTED-OPERATIONS.md, "Backup"): logical `pg_dump` rather than
//! `pg_basebackup`, because it is one consistent snapshot, restores into a newer PostgreSQL 16
//! patch release, can be listed and checked with `pg_restore --list` and needs no replication
//! privilege. The copy of the secrets is NOT encrypted: dynodoctl does not invent cryptography and
//! the repository has no passphrase-sealed format for arbitrary files (the key-file format seals
//! one signing key). It is written with mode 0600 and the report says so loudly; `--secrets-to`
//! sends it to a different place than the dump.

use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::db::{self, LIVE_DB};
use crate::error::{CliError, Result};
use crate::manifest::{self, ConfigEntry, DumpEntry, FileEntry, Manifest};
use crate::report::{render_steps, Recorder, Step};
use crate::restore::{self, RestoreReport};
use crate::util::{
    create_private_dir, free_bytes, human_bytes, human_duration, iso, set_private, stamp,
    write_private,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Options {
    /// Directory that receives the backup (default: backups/ of the bundle).
    pub to: Option<PathBuf>,
    /// Send the copy of the settings and secrets to this directory instead of putting it in the backup.
    pub secrets_to: Option<PathBuf>,
    pub label: Option<String>,
    /// Routine backups to keep (older ones are removed after this one succeeded).
    pub keep: usize,
    pub kind: String,
    /// Restore the new backup into a throwaway database and verify it.
    pub verify: bool,
    /// Do not copy the settings and secrets at all.
    pub no_config: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            to: None,
            secrets_to: None,
            label: None,
            keep: 7,
            kind: manifest::KIND_ROUTINE.into(),
            verify: false,
            no_config: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BackupReport {
    pub path: PathBuf,
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
    pub toc_entries: u64,
    pub events: i64,
    pub documents: i64,
    pub schema_level: i64,
    pub config_location: String,
    pub config_files: usize,
    pub steps: Vec<Step>,
    pub removed: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub verification: Option<RestoreReport>,
    pub duration_ms: u64,
    pub ok: bool,
}

impl BackupReport {
    pub fn render(&self) -> String {
        let mut out = format!("Backup {}\n", if self.ok { "written" } else { "FAILED" });
        out.push_str(&format!("  directory         {}\n", self.path.display()));
        out.push_str(&format!("  kind              {}\n", self.kind));
        out.push_str(&format!(
            "  database dump     {} (sha256 {}), {} entries\n",
            human_bytes(self.bytes),
            &self.sha256[..self.sha256.len().min(16)],
            self.toc_entries
        ));
        out.push_str(&format!(
            "  content           {} events, {} documents, schema level {}\n",
            self.events, self.documents, self.schema_level
        ));
        out.push_str(&format!(
            "  settings/secrets  {} ({} files)\n",
            self.config_location, self.config_files
        ));
        out.push_str(&render_steps(&self.steps));
        if !self.removed.is_empty() {
            out.push_str(&format!(
                "  retention         removed {} older backup(s)\n",
                self.removed.len()
            ));
            for r in &self.removed {
                out.push_str(&format!("                    {}\n", r.display()));
            }
        }
        if let Some(v) = &self.verification {
            out.push_str("\nVerification of this backup\n");
            out.push_str(&v.render());
        }
        out.push_str(&format!(
            "  duration          {}\n",
            human_duration(self.duration_ms as u128)
        ));
        for w in &self.warnings {
            out.push_str(&format!("\nWARNING: {w}\n"));
        }
        out
    }
}

fn clean_label(label: &str) -> Result<String> {
    let ok = !label.is_empty()
        && label.len() <= 40
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(label.to_string())
    } else {
        Err(CliError::usage(
            format!("`{label}` is not a usable label"),
            "use 1 to 40 characters: a-z, 0-9 and -",
        ))
    }
}

/// The settings and secrets worth keeping, as paths relative to the bundle directory.
pub fn config_files(ctx: &Ctx<'_>) -> Result<Vec<String>> {
    let layout = &ctx.layout;
    let mut found = vec![".env".to_string()];
    for (dir, rel) in [
        (layout.secrets_dir(), "secrets"),
        (layout.tls_dir(), "tls"),
        (layout.config_dir(), "config"),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "README.md" && !n.starts_with('.'))
            .collect();
        names.sort();
        found.extend(names.into_iter().map(|n| format!("{rel}/{n}")));
    }
    if layout.overlay_file().is_file() {
        found.push(crate::ctx::OVERLAY_NAME.to_string());
    }
    Ok(found)
}

/// Copy the settings and secrets under `root` (mode 0600) and describe them: a checksum per
/// non-secret file, and one salted digest over the secret files together.
fn copy_config(ctx: &Ctx<'_>, root: &Path) -> Result<ConfigCopy> {
    create_private_dir(root)?;
    let mut entries = Vec::new();
    for rel in config_files(ctx)? {
        let source = ctx.layout.dir.join(&rel);
        let target = root.join(&rel);
        if let Some(parent) = target.parent() {
            create_private_dir(parent)?;
        }
        let bytes = std::fs::read(&source)?;
        write_private(&target, &bytes)?;
        entries.push(manifest::file_entry(root, &rel)?);
    }
    let (salt, digest) = if entries.iter().any(|e| manifest::is_secret_path(&e.path)) {
        let salt = crate::util::random_hex(16);
        let digest = manifest::secrets_digest(root, &entries, &salt)?;
        (Some(salt), Some(digest))
    } else {
        (None, None)
    };
    Ok(ConfigCopy {
        entries,
        salt,
        digest,
    })
}

struct ConfigCopy {
    entries: Vec<FileEntry>,
    salt: Option<String>,
    digest: Option<String>,
}

/// Marker file in a backup directory while it is being written. A directory that still has it was
/// never finished (a crash, a kill) and is removed at the next backup.
pub const INCOMPLETE_MARKER: &str = ".dynodoctl-incomplete";

/// Remove backup directories that were never finished.
pub fn clean_unfinished(backups_dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(backups_dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = entry
            .file_name()
            .to_string_lossy()
            .starts_with(manifest::DIR_PREFIX);
        let marked = std::fs::symlink_metadata(path.join(INCOMPLETE_MARKER)).is_ok();
        if ours
            && marked
            && !std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(true)
            && std::fs::remove_dir_all(&path).is_ok()
        {
            removed.push(path);
        }
    }
    removed
}

/// Is `inner` inside `outer`? (Textual, after making both absolute where they exist.)
fn is_inside(inner: &Path, outer: &Path) -> bool {
    let a = std::fs::canonicalize(inner).unwrap_or_else(|_| inner.to_path_buf());
    let b = std::fs::canonicalize(outer).unwrap_or_else(|_| outer.to_path_buf());
    a.starts_with(&b)
}

pub fn run(ctx: &Ctx<'_>, options: &Options) -> Result<BackupReport> {
    ctx.require_installed()?;
    let started = Instant::now();
    let label = options.label.as_deref().map(clean_label).transpose()?;
    let c = Compose::new(ctx);
    let services = c.ps()?;
    if !services.iter().any(|s| s.service == "postgres" && s.up()) {
        return Err(CliError::refused(
            "PostgreSQL is not running, so a consistent backup cannot be taken",
            "start the installation (`docker compose up -d --wait`) and run the backup again",
        ));
    }
    let backups_dir = options
        .to
        .clone()
        .unwrap_or_else(|| ctx.layout.backups_dir());
    create_private_dir(&backups_dir)?;
    let now = (ctx.now)();
    let name = match &label {
        Some(l) => format!("{}{}-{l}", manifest::DIR_PREFIX, stamp(now)),
        None => format!("{}{}", manifest::DIR_PREFIX, stamp(now)),
    };
    let path = backups_dir.join(&name);
    if path.exists() {
        return Err(CliError::refused(
            format!("{} already exists", path.display()),
            "wait a second and run again, or choose another --label; backups are never overwritten",
        ));
    }
    let mut rec = Recorder::for_ctx(ctx);
    let mut warnings = Vec::new();
    let cleaned = clean_unfinished(&backups_dir);
    if !cleaned.is_empty() {
        warnings.push(format!(
            "removed {} backup director{} that was never finished (a crash or an interrupted run)",
            cleaned.len(),
            if cleaned.len() == 1 { "y" } else { "ies" }
        ));
    }

    let size = db::size_bytes(&c, LIVE_DB)?;
    if let Some(free) = free_bytes(&backups_dir) {
        if free < size {
            return Err(CliError::refused(
                format!(
                    "only {} is free at {} and the database is {}",
                    human_bytes(free),
                    backups_dir.display(),
                    human_bytes(size)
                ),
                "free disk space or choose another disk with --to; nothing was written",
            ));
        }
        if free < size.saturating_mul(2) {
            warnings.push(format!(
                "only {} is free at {}; the next backup may not fit",
                human_bytes(free),
                backups_dir.display()
            ));
        }
    }
    create_private_dir(&path)?;
    write_private(&path.join(INCOMPLETE_MARKER), b"being written\n")?;
    // The settings copy may go to a directory of its own; if this run fails it must not be left behind.
    let external_root = options
        .secrets_to
        .as_ref()
        .filter(|_| !options.no_config)
        .map(|d| d.join(format!("{}-config", name)));
    let external_existed = external_root.as_ref().is_some_and(|p| p.exists());
    let work = (|| -> Result<(Manifest, Vec<FileEntry>)> {
        let env = ctx.env()?;
        let before = rec.step("read the database", || {
            let f = db::read_facts(&c, LIVE_DB)?;
            Ok((
                f.clone(),
                format!(
                    "{} events, {} documents, schema level {}",
                    f.events, f.documents, f.latest_migration
                ),
            ))
        })?;
        let dump_path = path.join(manifest::DUMP_NAME);
        rec.step("pg_dump -Fc (consistent snapshot)", || {
            let out = c.pg_sh_io(
                &format!("pg_dump {} -Fc {LIVE_DB}", crate::compose::PG_CONNECT),
                None,
                Some(dump_path.clone()),
            )?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!("pg_dump failed: {}", short(&out.text())),
                    "check `docker compose logs postgres`; no backup was kept",
                ));
            }
            let head = std::fs::read(&dump_path)
                .map(|b| b.starts_with(b"PGDMP"))
                .unwrap_or(false);
            if !head {
                return Err(CliError::failed(
                    "pg_dump wrote something that is not a custom-format dump",
                    "no backup was kept; check `docker compose logs postgres`",
                ));
            }
            Ok((
                (),
                human_bytes(std::fs::metadata(&dump_path).map(|m| m.len()).unwrap_or(0)),
            ))
        })?;
        let toc = rec.step("pg_restore --list (the dump is readable)", || {
            let n = db::toc_entries(&c, &dump_path)?;
            if n == 0 {
                return Err(CliError::failed(
                    "the dump has no entries",
                    "no backup was kept",
                ));
            }
            Ok((n, format!("{n} entries")))
        })?;
        let after = db::read_facts(&c, LIVE_DB)?;
        let (sha, bytes) = crate::util::sha256_file(&dump_path)?;
        set_private(&dump_path)?;

        let mut config = ConfigEntry::none();
        if !options.no_config {
            let (root, location, dir_name) = match &options.secrets_to {
                Some(dir) => {
                    let n = format!("{}-config", name);
                    (dir.join(&n), "external", Some(n))
                }
                None => (path.join("config"), "inside", None),
            };
            let copy = rec.step("copy settings and secrets (owner only)", || {
                let copy = copy_config(ctx, &root)?;
                let detail = format!("{} files to {}", copy.entries.len(), root.display());
                Ok((copy, detail))
            })?;
            config = ConfigEntry {
                location: location.into(),
                dir_name,
                files: copy.entries,
                secrets_salt: copy.salt,
                secrets_digest: copy.digest,
            };
        }

        let mut images = BTreeMap::new();
        for s in &services {
            if !s.image.is_empty() {
                images.insert(s.service.clone(), s.image.clone());
            }
        }
        let manifest = Manifest {
            format: manifest::FORMAT,
            tool: format!("dynodoctl {}", crate::VERSION),
            created_utc: iso(now),
            kind: options.kind.clone(),
            label: label.clone(),
            project: ctx.project_name(),
            origin: env.get("ORIGIN").unwrap_or("").to_string(),
            tls_mode: env.get("DYNODOC_TLS_MODE").unwrap_or("").to_string(),
            bundle_version: env.get("DYNODOC_VERSION").unwrap_or("dev").to_string(),
            images,
            database: DumpEntry {
                file: manifest::DUMP_NAME.into(),
                bytes,
                sha256: sha,
                toc_entries: toc,
            },
            facts: before,
            facts_after: after,
            config,
            duration_ms: started.elapsed().as_millis() as u64,
        };
        rec.step("write manifest.json and SHA256SUMS", || {
            manifest::write_manifest(&path, &manifest)?;
            let n = manifest::write_sums(&path, &manifest)?;
            // The last thing: from here on the directory is a complete backup.
            let _ = std::fs::remove_file(path.join(INCOMPLETE_MARKER));
            Ok((
                (),
                format!("{n} checksums (secret files have none of their own)"),
            ))
        })?;
        Ok((manifest.clone(), manifest.config.files.clone()))
    })();

    let (manifest, config_entries) = match work {
        Ok(v) => v,
        Err(e) => {
            // A half-written backup is worse than none: it looks like one.
            let _ = std::fs::remove_dir_all(&path);
            if let Some(external) = &external_root {
                if !external_existed {
                    let _ = std::fs::remove_dir_all(external);
                }
            }
            return Err(e);
        }
    };

    if options.secrets_to.is_none() && !options.no_config {
        warnings.push(format!(
            "{} holds the database AND the secrets. Anyone who can read it can read every document and run the installation. It is not encrypted. Use --secrets-to to keep the secrets apart, and copy the backup to another machine: a backup on this disk does not protect against losing this machine.",
            path.display()
        ));
    } else if options.no_config {
        warnings.push("the settings and secrets were NOT copied (--no-config). Without them the dump alone cannot start an installation; keep them somewhere safe.".to_string());
    } else if let Some(dir) = &options.secrets_to {
        warnings.push(format!("the secrets copy in {} is not encrypted; keep that directory private and away from the dump.", dir.display()));
    }
    if is_inside(&backups_dir, &ctx.layout.dir) {
        warnings.push("this backup is on the same disk as the installation. Copy it to another machine or a removable disk; same-host backups are not disaster recovery.".to_string());
    }

    let mut ok = true;
    let verification = if options.verify {
        let info = manifest::BackupInfo {
            path: path.clone(),
            manifest: manifest.clone(),
        };
        let config_root = options
            .secrets_to
            .as_ref()
            .map(|d| d.join(format!("{}-config", name)));
        let report = restore::restore_verify(
            ctx,
            Some(&info.path.display().to_string()),
            config_root.as_deref(),
        )?;
        ok = report.ok;
        Some(report)
    } else {
        None
    };

    // Retention runs only after this backup (and its verification, when asked) succeeded.
    let removed = if ok {
        manifest::apply_retention(
            &backups_dir,
            &options.kind,
            options.keep,
            &path,
            &manifest.project,
            &manifest.origin,
        )?
    } else {
        Vec::new()
    };
    if !ok {
        warnings.push("the verification failed; the backup was kept for inspection but should not be relied on, and older backups were not removed".to_string());
    }
    Ok(BackupReport {
        path,
        name,
        kind: manifest.kind.clone(),
        bytes: manifest.database.bytes,
        sha256: manifest.database.sha256.clone(),
        toc_entries: manifest.database.toc_entries,
        events: manifest.facts_after.events,
        documents: manifest.facts_after.documents,
        schema_level: manifest.facts_after.latest_migration,
        config_location: manifest.config.location.clone(),
        config_files: config_entries.len(),
        steps: rec.steps,
        removed,
        warnings,
        verification,
        duration_ms: started.elapsed().as_millis() as u64,
        ok,
    })
}

/// Age in whole days of the newest backup, if any.
pub fn newest_age_days(ctx: &Ctx<'_>, dir: &Path) -> Option<(String, i64)> {
    let newest = manifest::list_backups(dir).into_iter().last()?;
    let created = chrono::DateTime::parse_from_rfc3339(&newest.manifest.created_utc).ok()?;
    Some((
        newest.manifest.created_utc.clone(),
        ((ctx.now)() - created.with_timezone(&chrono::Utc)).num_days(),
    ))
}

#[cfg(test)]
#[path = "tests/backup.rs"]
mod tests;
