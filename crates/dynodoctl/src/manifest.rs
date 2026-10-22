//! A backup is a directory: `database.dump`, `manifest.json`, `SHA256SUMS` and (unless it was
//! sent elsewhere with `--secrets-to`) a copy of the settings and secrets under `config/`.
//! This module reads and writes the manifest, checks the checksums and applies retention.

use crate::error::{CliError, Result};
use crate::util::{sha256_file, write_private};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FORMAT: u32 = 1;
pub const MANIFEST_NAME: &str = "manifest.json";
pub const SUMS_NAME: &str = "SHA256SUMS";
pub const DUMP_NAME: &str = "database.dump";
pub const DIR_PREFIX: &str = "dynodoc-";

pub const KIND_ROUTINE: &str = "routine";
pub const KIND_PRE_UPGRADE: &str = "pre-upgrade";
pub const KIND_PRE_RESTORE: &str = "pre-restore";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative to the backup directory (database, manifest) or to the config root (config files).
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpEntry {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    pub toc_entries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// `inside` (under config/), `external` (sent elsewhere with --secrets-to) or `none`.
    pub location: String,
    /// For `external`: the name of the directory that holds the copy.
    pub dir_name: Option<String>,
    /// The files copied. Secret files (`secrets/*`, `tls/privkey.pem`) carry an EMPTY `sha256`: an
    /// unsalted hash of a human-chosen password (the mail server's) could be guessed offline.
    pub files: Vec<FileEntry>,
    /// Random salt and one digest over all the secret files together (salt, then each path, length
    /// and content). The files include 32-byte random keys, so no single weak secret can be tried
    /// against it; it still detects a changed or damaged secrets copy.
    #[serde(default)]
    pub secrets_salt: Option<String>,
    #[serde(default)]
    pub secrets_digest: Option<String>,
}

impl ConfigEntry {
    pub fn none() -> Self {
        Self {
            location: "none".into(),
            dir_name: None,
            files: Vec::new(),
            secrets_salt: None,
            secrets_digest: None,
        }
    }
}

/// Secret files get no individual checksum anywhere (see [`ConfigEntry::files`]).
pub fn is_secret_path(rel: &str) -> bool {
    rel.starts_with("secrets/") || rel == "tls/privkey.pem"
}

/// A plain file name: one path component, nothing that escapes a directory.
pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0'])
}

/// A path a backup may carry for the settings and secrets copy, relative to the bundle directory:
/// `.env`, the release overlay, or one plain file directly under `secrets/`, `tls/` or `config/`.
/// Absolute paths, `..`, drive letters, UNC prefixes, backslashes and anything deeper are refused:
/// the manifest of a backup is not trusted to name where a file is written.
pub fn safe_config_path(rel: &str) -> bool {
    if rel == ".env" || rel == crate::ctx::OVERLAY_NAME {
        return true;
    }
    match rel.split_once('/') {
        Some((dir, name)) => {
            ["secrets", "tls", "config"].contains(&dir)
                && is_plain_name(name)
                && !name.starts_with('.')
        }
        None => false,
    }
}

fn validate(m: &Manifest) -> Result<()> {
    let refuse = |what: String| {
        CliError::refused(what, "do not use this backup: it is damaged or was not made by dynodoctl; nothing was changed")
    };
    if !is_plain_name(&m.database.file) {
        return Err(refuse(format!(
            "the manifest names the database file `{}`, which is outside the backup directory",
            m.database.file
        )));
    }
    if !["inside", "external", "none"].contains(&m.config.location.as_str()) {
        return Err(refuse(format!(
            "the manifest has an unknown settings location `{}`",
            m.config.location
        )));
    }
    if let Some(name) = &m.config.dir_name {
        if !is_plain_name(name) {
            return Err(refuse(format!(
                "the manifest names the settings directory `{name}`, which is not a plain name"
            )));
        }
    }
    for f in &m.config.files {
        if !safe_config_path(&f.path) {
            return Err(refuse(format!("the manifest lists the settings file `{}`, which is not an allowed path (allowed: .env, the release overlay, or one file under secrets/, tls/ or config/)", f.path)));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub tool: String,
    pub created_utc: String,
    pub kind: String,
    pub label: Option<String>,
    pub project: String,
    pub origin: String,
    pub tls_mode: String,
    pub bundle_version: String,
    pub images: BTreeMap<String, String>,
    pub database: DumpEntry,
    /// Counts read just before the dump. `pg_dump` is one consistent snapshot, but the counts are
    /// separate queries: a restored copy must lie between these and `facts_after`.
    pub facts: crate::db::DbFacts,
    pub facts_after: crate::db::DbFacts,
    pub config: ConfigEntry,
    pub duration_ms: u64,
}

#[derive(Clone, Debug)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub manifest: Manifest,
}

impl BackupInfo {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    pub fn dump_path(&self) -> PathBuf {
        self.path.join(&self.manifest.database.file)
    }
    /// Where the settings and secrets copy is, given an optional override.
    pub fn config_root(&self, override_dir: Option<&Path>) -> Option<PathBuf> {
        match (override_dir, self.manifest.config.location.as_str()) {
            (Some(dir), _) => Some(dir.to_path_buf()),
            (None, "inside") => Some(self.path.join("config")),
            _ => None,
        }
    }
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_NAME)).map_err(|e| {
        CliError::refused(
            format!("{} has no readable {MANIFEST_NAME} ({e})", dir.display()),
            "point --backup at a directory made by `dynodoctl backup`",
        )
    })?;
    let manifest: Manifest = serde_json::from_str(&text)?;
    if manifest.format != FORMAT {
        return Err(CliError::refused(
            format!(
                "backup format {} is not understood by this dynodoctl (expects {FORMAT})",
                manifest.format
            ),
            "use the dynodoctl version that took the backup",
        ));
    }
    validate(&manifest)?;
    Ok(manifest)
}

pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let mut text = serde_json::to_string_pretty(manifest)?;
    text.push('\n');
    write_private(&dir.join(MANIFEST_NAME), text.as_bytes())
}

/// `SHA256SUMS` in the format `sha256sum -c` reads (two spaces, relative path, LF).
pub fn sums_text(entries: &[FileEntry]) -> String {
    entries
        .iter()
        .map(|e| format!("{}  {}\n", e.sha256, e.path))
        .collect()
}

/// The entry for a copied settings file. A secret file gets no checksum of its own.
pub fn file_entry(root: &Path, relative: &str) -> Result<FileEntry> {
    if is_secret_path(relative) {
        let bytes = std::fs::metadata(root.join(relative))?.len();
        return Ok(FileEntry {
            path: relative.to_string(),
            bytes,
            sha256: String::new(),
        });
    }
    let (sha256, bytes) = sha256_file(&root.join(relative))?;
    Ok(FileEntry {
        path: relative.to_string(),
        bytes,
        sha256,
    })
}

/// One digest over the secret files of a copy (see [`ConfigEntry::secrets_digest`]).
pub fn secrets_digest(root: &Path, files: &[FileEntry], salt: &str) -> Result<String> {
    let mut paths: Vec<&str> = files
        .iter()
        .filter(|f| is_secret_path(&f.path))
        .map(|f| f.path.as_str())
        .collect();
    paths.sort_unstable();
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    context.update(salt.as_bytes());
    for p in paths {
        let target = root.join(p);
        if !std::fs::symlink_metadata(&target)?.file_type().is_file() {
            return Err(CliError::refused(
                format!("{p} in the settings copy is not a regular file"),
                "do not use this copy",
            ));
        }
        let bytes = std::fs::read(&target)?;
        context.update(p.as_bytes());
        context.update(&[0]);
        context.update(&(bytes.len() as u64).to_le_bytes());
        context.update(&bytes);
    }
    Ok(hex::encode(context.finish()))
}

/// `SHA256SUMS` as `sha256sum -c` reads it, parsed. Written by `write_sums`, read back by
/// `verify_files`: a second record of the same hashes, so editing the manifest alone is noticed.
pub fn read_sums(dir: &Path) -> std::result::Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(dir.join(SUMS_NAME))
        .map_err(|e| format!("{SUMS_NAME} is missing or unreadable ({e})"))?;
    let mut map = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| format!("{SUMS_NAME} has a line that is not `hash  path`"))?;
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("{SUMS_NAME} has a bad hash for {path}"));
        }
        map.insert(path.to_string(), hash.to_ascii_lowercase());
    }
    Ok(map)
}

/// Write `SHA256SUMS` for a backup directory: the dump, the manifest and the non-secret files of
/// an inside copy (`config/...`). Secret files are left out on purpose.
pub fn write_sums(dir: &Path, m: &Manifest) -> Result<usize> {
    let mut entries = vec![
        FileEntry {
            path: m.database.file.clone(),
            bytes: m.database.bytes,
            sha256: m.database.sha256.clone(),
        },
        file_entry(dir, MANIFEST_NAME)?,
    ];
    if m.config.location == "inside" {
        for f in m.config.files.iter().filter(|f| !is_secret_path(&f.path)) {
            entries.push(FileEntry {
                path: format!("config/{}", f.path),
                bytes: f.bytes,
                sha256: f.sha256.clone(),
            });
        }
    }
    write_private(&dir.join(SUMS_NAME), sums_text(&entries).as_bytes())?;
    Ok(entries.len())
}

/// Every file the manifest vouches for, checked three ways: the file against the manifest, the file
/// against `SHA256SUMS`, and the manifest's dump hash against `SHA256SUMS`. Only regular files are
/// read (a symlink in a backup is a refusal). The result is a list of plain-words problems.
///
/// This detects damage and edits that touch one record. It cannot authenticate a backup: whoever
/// can write the directory can rewrite the manifest and `SHA256SUMS` together. Restoring means
/// trusting the backup (see docs/SELF-HOSTED-OPERATIONS.md, "Trust").
pub fn verify_files(
    info: &BackupInfo,
    config_override: Option<&Path>,
) -> std::result::Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let m = &info.manifest;
    let sums = match read_sums(&info.path) {
        Ok(s) => s,
        Err(e) => {
            problems.push(e);
            BTreeMap::new()
        }
    };
    let regular = |p: &Path| {
        std::fs::symlink_metadata(p)
            .map(|m| m.file_type().is_file())
            .unwrap_or(false)
    };
    let check = |root: &Path, e: &FileEntry, sum_key: Option<&str>, problems: &mut Vec<String>| {
        let target = root.join(&e.path);
        if !regular(&target) {
            problems.push(format!("{} is missing or is not a regular file", e.path));
            return;
        }
        match sha256_file(&target) {
            Ok((hash, bytes)) => {
                if bytes != e.bytes {
                    problems.push(format!(
                        "{} has {bytes} bytes, the manifest says {}",
                        e.path, e.bytes
                    ));
                } else if !e.sha256.is_empty() && hash != e.sha256 {
                    problems.push(format!(
                        "{} does not match its checksum (the file changed or is damaged)",
                        e.path
                    ));
                }
                if let Some(key) = sum_key {
                    match sums.get(key) {
                        Some(listed) if *listed == hash => {}
                        Some(_) => problems.push(format!("{} does not match {SUMS_NAME} (the file or the checksum list was changed)", e.path)),
                        None => problems.push(format!("{} is not listed in {SUMS_NAME}", e.path)),
                    }
                }
            }
            Err(_) => problems.push(format!("{} is unreadable", e.path)),
        }
    };
    let dump = FileEntry {
        path: m.database.file.clone(),
        bytes: m.database.bytes,
        sha256: m.database.sha256.clone(),
    };
    check(&info.path, &dump, Some(&m.database.file), &mut problems);
    check(
        &info.path,
        &FileEntry {
            path: MANIFEST_NAME.into(),
            bytes: std::fs::metadata(info.path.join(MANIFEST_NAME))
                .map(|x| x.len())
                .unwrap_or(0),
            sha256: String::new(),
        },
        Some(MANIFEST_NAME),
        &mut problems,
    );
    // The settings and secrets copy is checked when it is at hand (inside the backup, or named
    // with --config-from). An external copy that is not named is simply not checked here: an
    // installation that still has its own .env and secrets/ does not need it, and a restore
    // that does need it refuses on its own (restore_live).
    if let Some(root) = info.config_root(config_override) {
        let inside = m.config.location == "inside";
        for e in &m.config.files {
            let key = format!("config/{}", e.path);
            let listed = (inside && !is_secret_path(&e.path)).then_some(key.as_str());
            check(&root, e, listed, &mut problems);
        }
        if m.config.files.iter().any(|f| is_secret_path(&f.path)) {
            match (&m.config.secrets_salt, &m.config.secrets_digest) {
                (Some(salt), Some(expected)) => match secrets_digest(&root, &m.config.files, salt) {
                    Ok(actual) if actual == *expected => {}
                    Ok(_) => problems.push("the secrets copy does not match its digest (a secret file changed or is damaged)".to_string()),
                    Err(e) => problems.push(e.what),
                },
                _ => problems.push("the manifest lists secret files but no digest for them".to_string()),
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Backups in `dir` that carry a valid manifest, oldest first. Anything else is ignored.
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !entry.file_name().to_string_lossy().starts_with(DIR_PREFIX) {
            continue;
        }
        if let Ok(manifest) = read_manifest(&path) {
            found.push(BackupInfo { path, manifest });
        }
    }
    found.sort_by(|a, b| {
        a.manifest
            .created_utc
            .cmp(&b.manifest.created_utc)
            .then(a.name().cmp(&b.name()))
    });
    found
}

/// Remove the oldest backups of `kind` beyond `keep`. Only directories with a valid manifest,
/// directly inside `dir`, are ever removed; `protect` (the backup just made) never is. Only
/// backups of THIS installation (same project name and origin) are counted or removed: a shared
/// backup directory such as an external disk may hold another installation's backups.
pub fn apply_retention(
    dir: &Path,
    kind: &str,
    keep: usize,
    protect: &Path,
    project: &str,
    origin: &str,
) -> Result<Vec<PathBuf>> {
    let mut of_kind: Vec<BackupInfo> = list_backups(dir)
        .into_iter()
        .filter(|b| {
            b.manifest.kind == kind && b.manifest.project == project && b.manifest.origin == origin
        })
        .collect();
    if of_kind.len() <= keep {
        return Ok(Vec::new());
    }
    let excess = of_kind.len() - keep;
    let mut removed = Vec::new();
    for old in of_kind.drain(..excess) {
        if old.path == protect || old.path.parent() != Some(dir) {
            continue;
        }
        std::fs::remove_dir_all(&old.path)?;
        removed.push(old.path);
    }
    Ok(removed)
}

#[cfg(test)]
pub fn sample(kind: &str, created: &str) -> Manifest {
    Manifest {
        format: FORMAT,
        tool: "dynodoctl test".into(),
        created_utc: created.into(),
        kind: kind.into(),
        label: None,
        project: "dynodoc-selfhosted".into(),
        origin: "https://docs.example.org".into(),
        tls_mode: "evaluation".into(),
        bundle_version: "dev".into(),
        images: BTreeMap::new(),
        database: DumpEntry {
            file: DUMP_NAME.into(),
            bytes: 0,
            sha256: String::new(),
            toc_entries: 0,
        },
        facts: crate::db::DbFacts::default(),
        facts_after: crate::db::DbFacts::default(),
        config: ConfigEntry::none(),
        duration_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(dir: &Path, name: &str, kind: &str, created: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).unwrap();
        let mut m = sample(kind, created);
        std::fs::write(path.join(DUMP_NAME), b"PGDMPdata").unwrap();
        m.database = DumpEntry {
            file: DUMP_NAME.into(),
            bytes: 9,
            sha256: crate::util::sha256_hex(b"PGDMPdata"),
            toc_entries: 3,
        };
        write_manifest(&path, &m).unwrap();
        write_sums(&path, &m).unwrap();
        path
    }

    const PROJECT: &str = "dynodoc-selfhosted";
    const ORIGIN: &str = "https://docs.example.org";

    /// A manifest text with one field replaced, for hostile-backup tests.
    fn with_manifest(path: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path.join(MANIFEST_NAME)).unwrap())
                .unwrap();
        edit(&mut value);
        std::fs::write(
            path.join(MANIFEST_NAME),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn config_paths_are_confined_to_the_known_places() {
        // H1: the manifest of a backup is not trusted to say where a file is written.
        for ok in [
            ".env",
            "compose.release.generated.yml",
            "secrets/session_secret",
            "tls/fullchain.pem",
            "tls/privkey.pem",
            "config/notice.txt",
        ] {
            assert!(safe_config_path(ok), "{ok}");
        }
        for bad in [
            "../escaped.txt",
            "secrets/../../escaped",
            "/etc/passwd",
            "\\\\server\\share\\x",
            "C:\\Windows\\x",
            "C:/x",
            "secrets\\x",
            "secrets/",
            "secrets",
            "secrets//x",
            "secrets/a/b",
            "other/x",
            ".ssh/authorized_keys",
            "secrets/.hidden",
            "tls/..",
            "./.env",
            ".env/",
            "",
            "config/x:stream",
            "secrets/x\0y",
            "~/x",
        ] {
            assert!(!safe_config_path(bad), "{bad:?} must be refused");
        }
        for ok in ["database.dump", "x.dump"] {
            assert!(is_plain_name(ok));
        }
        for bad in ["", ".", "..", "../x", "a/b", "a\\b", "C:x", "/abs"] {
            assert!(!is_plain_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_manifest_that_names_a_path_outside_the_bundle_is_refused_when_it_is_read() {
        let dir = tempfile::tempdir().unwrap();
        for (what, edit) in [
            (
                "a config path with ..",
                Box::new(|v: &mut serde_json::Value| {
                    v["config"]["files"] =
                        serde_json::json!([{"path": "../escaped.txt", "bytes": 1, "sha256": ""}])
                }) as Box<dyn Fn(&mut serde_json::Value)>,
            ),
            (
                "an absolute config path",
                Box::new(|v| {
                    v["config"]["files"] =
                        serde_json::json!([{"path": "/etc/cron.d/x", "bytes": 1, "sha256": ""}])
                }),
            ),
            (
                "a database file outside the backup",
                Box::new(|v| v["database"]["file"] = serde_json::json!("../../elsewhere.dump")),
            ),
            (
                "an absolute database file",
                Box::new(|v| v["database"]["file"] = serde_json::json!("/tmp/other.dump")),
            ),
            (
                "an unknown location",
                Box::new(|v| v["config"]["location"] = serde_json::json!("somewhere")),
            ),
            (
                "a settings directory with a separator",
                Box::new(|v| v["config"]["dir_name"] = serde_json::json!("../x")),
            ),
        ] {
            let path = make(
                dir.path(),
                "dynodoc-x",
                KIND_ROUTINE,
                "2026-10-22T00:00:00Z",
            );
            with_manifest(&path, |v| edit(v));
            let err = read_manifest(&path)
                .err()
                .unwrap_or_else(|| panic!("{what} was accepted"));
            assert_eq!(err.code(), crate::error::exit::REFUSED, "{what}");
            assert!(err.what.contains("manifest"), "{what}: {err}");
            std::fs::remove_dir_all(&path).unwrap();
        }
    }

    #[test]
    fn the_checksum_list_is_a_second_record_and_editing_the_manifest_alone_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(
            dir.path(),
            "dynodoc-a",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        let info = || BackupInfo {
            manifest: read_manifest(&path).unwrap(),
            path: path.clone(),
        };
        assert!(verify_files(&info(), None).is_ok());
        // Someone swaps the dump AND fixes its hash in the manifest, but not in SHA256SUMS.
        std::fs::write(path.join(DUMP_NAME), b"PGDMPevil!").unwrap();
        with_manifest(&path, |v| {
            v["database"]["bytes"] = serde_json::json!(10);
            v["database"]["sha256"] = serde_json::json!(crate::util::sha256_hex(b"PGDMPevil!"));
        });
        let problems = verify_files(&info(), None).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("SHA256SUMS")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_missing_or_malformed_checksum_list_fails_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(
            dir.path(),
            "dynodoc-a",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        let info = || BackupInfo {
            manifest: read_manifest(&path).unwrap(),
            path: path.clone(),
        };
        std::fs::write(path.join(SUMS_NAME), "not a checksum list\n").unwrap();
        assert!(verify_files(&info(), None)
            .unwrap_err()
            .iter()
            .any(|p| p.contains("not `hash  path`")));
        std::fs::remove_file(path.join(SUMS_NAME)).unwrap();
        assert!(verify_files(&info(), None)
            .unwrap_err()
            .iter()
            .any(|p| p.contains("SHA256SUMS is missing")));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_place_of_the_dump_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(
            dir.path(),
            "dynodoc-a",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        let outside = dir.path().join("outside.dump");
        std::fs::write(&outside, b"PGDMPdata").unwrap();
        std::fs::remove_file(path.join(DUMP_NAME)).unwrap();
        std::os::unix::fs::symlink(&outside, path.join(DUMP_NAME)).unwrap();
        let info = BackupInfo {
            manifest: read_manifest(&path).unwrap(),
            path: path.clone(),
        };
        assert!(verify_files(&info, None)
            .unwrap_err()
            .iter()
            .any(|p| p.contains("not a regular file")));
    }

    #[test]
    fn secret_files_get_no_individual_checksum_but_one_salted_digest_over_all_of_them() {
        // LOW: an unsalted SHA-256 of a human-chosen password (the mail server's) can be guessed.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config");
        std::fs::create_dir_all(root.join("secrets")).unwrap();
        std::fs::write(
            root.join("secrets").join("session_secret"),
            "a-random-32-byte-secret-value\n",
        )
        .unwrap();
        std::fs::write(root.join("secrets").join("smtp_password"), "Summer2026!\n").unwrap();
        std::fs::write(root.join(".env"), "ORIGIN=https://x\n").unwrap();
        let secret = file_entry(&root, "secrets/smtp_password").unwrap();
        assert_eq!((secret.sha256.as_str(), secret.bytes), ("", 12));
        let plain = file_entry(&root, ".env").unwrap();
        assert_eq!(plain.sha256, crate::util::sha256_hex(b"ORIGIN=https://x\n"));
        let files = vec![
            file_entry(&root, "secrets/session_secret").unwrap(),
            secret,
            plain,
        ];
        let a = secrets_digest(&root, &files, "salt-one").unwrap();
        assert_ne!(
            a,
            secrets_digest(&root, &files, "salt-two").unwrap(),
            "salted"
        );
        assert_ne!(
            a,
            crate::util::sha256_hex(b"Summer2026!\n"),
            "never the bare hash of one secret"
        );
        std::fs::write(root.join("secrets").join("smtp_password"), "Summer2027!\n").unwrap();
        assert_ne!(
            a,
            secrets_digest(&root, &files, "salt-one").unwrap(),
            "a changed secret changes the digest"
        );
    }

    #[test]
    fn manifest_round_trips_and_refuses_unknown_formats() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(
            dir.path(),
            "dynodoc-20261022T000000Z",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        assert_eq!(read_manifest(&path).unwrap().kind, "routine");
        let text = std::fs::read_to_string(path.join(MANIFEST_NAME))
            .unwrap()
            .replace("\"format\": 1", "\"format\": 99");
        std::fs::write(path.join(MANIFEST_NAME), text).unwrap();
        assert_eq!(
            read_manifest(&path).unwrap_err().code(),
            crate::error::exit::REFUSED
        );
        assert!(read_manifest(dir.path()).is_err());
    }

    #[test]
    fn checksums_detect_a_changed_or_missing_dump() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(
            dir.path(),
            "dynodoc-20261022T000000Z",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        let info = BackupInfo {
            manifest: read_manifest(&path).unwrap(),
            path: path.clone(),
        };
        assert!(verify_files(&info, None).is_ok());
        std::fs::write(path.join(DUMP_NAME), b"PGDMPdatX").unwrap();
        let problems = verify_files(&info, None).unwrap_err();
        assert!(problems[0].contains("checksum"), "{problems:?}");
        std::fs::remove_file(path.join(DUMP_NAME)).unwrap();
        assert!(verify_files(&info, None).unwrap_err()[0].contains("missing"));
    }

    #[test]
    fn sums_file_is_sha256sum_compatible() {
        let text = sums_text(&[FileEntry {
            path: "database.dump".into(),
            bytes: 1,
            sha256: "ab".repeat(32),
        }]);
        assert_eq!(text, format!("{}  database.dump\n", "ab".repeat(32)));
    }

    #[test]
    fn listing_is_oldest_first_and_ignores_directories_without_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        make(
            dir.path(),
            "dynodoc-b",
            KIND_ROUTINE,
            "2026-10-22T02:00:00Z",
        );
        make(
            dir.path(),
            "dynodoc-a",
            KIND_ROUTINE,
            "2026-10-22T01:00:00Z",
        );
        std::fs::create_dir_all(dir.path().join("dynodoc-junk")).unwrap();
        std::fs::create_dir_all(dir.path().join("other")).unwrap();
        let names: Vec<String> = list_backups(dir.path()).iter().map(|b| b.name()).collect();
        assert_eq!(names, vec!["dynodoc-a", "dynodoc-b"]);
    }

    #[test]
    fn retention_removes_the_oldest_and_never_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in ["dynodoc-1", "dynodoc-2", "dynodoc-3", "dynodoc-4"]
            .iter()
            .enumerate()
        {
            make(
                dir.path(),
                name,
                KIND_ROUTINE,
                &format!("2026-10-2{}T00:00:00Z", i + 1),
            );
        }
        let newest = dir.path().join("dynodoc-4");
        let removed =
            apply_retention(dir.path(), KIND_ROUTINE, 2, &newest, PROJECT, ORIGIN).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!dir.path().join("dynodoc-1").exists() && !dir.path().join("dynodoc-2").exists());
        assert!(dir.path().join("dynodoc-3").exists() && newest.exists());
    }

    #[test]
    fn retention_counts_only_its_own_kind_and_keeps_unknown_directories() {
        let dir = tempfile::tempdir().unwrap();
        make(
            dir.path(),
            "dynodoc-1",
            KIND_PRE_UPGRADE,
            "2026-10-21T00:00:00Z",
        );
        make(
            dir.path(),
            "dynodoc-2",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        std::fs::create_dir_all(dir.path().join("dynodoc-not-ours")).unwrap();
        std::fs::write(dir.path().join("dynodoc-not-ours").join("precious"), b"x").unwrap();
        let removed = apply_retention(
            dir.path(),
            KIND_ROUTINE,
            1,
            &dir.path().join("dynodoc-2"),
            PROJECT,
            ORIGIN,
        )
        .unwrap();
        assert!(removed.is_empty());
        assert!(dir.path().join("dynodoc-1").exists());
        assert!(dir
            .path()
            .join("dynodoc-not-ours")
            .join("precious")
            .exists());
    }

    #[test]
    fn retention_of_zero_still_protects_the_backup_just_made() {
        let dir = tempfile::tempdir().unwrap();
        make(
            dir.path(),
            "dynodoc-1",
            KIND_ROUTINE,
            "2026-10-21T00:00:00Z",
        );
        let newest = make(
            dir.path(),
            "dynodoc-2",
            KIND_ROUTINE,
            "2026-10-22T00:00:00Z",
        );
        apply_retention(dir.path(), KIND_ROUTINE, 0, &newest, PROJECT, ORIGIN).unwrap();
        assert!(newest.exists());
        assert!(!dir.path().join("dynodoc-1").exists());
    }

    #[test]
    fn retention_never_counts_or_removes_another_installations_backups() {
        // M6: a shared backup disk may hold backups of other installations (other project or origin).
        let dir = tempfile::tempdir().unwrap();
        for (name, created, project, origin) in [
            (
                "dynodoc-other-project-1",
                "2026-10-01T00:00:00Z",
                "someone-else",
                ORIGIN,
            ),
            (
                "dynodoc-other-origin-1",
                "2026-10-02T00:00:00Z",
                PROJECT,
                "https://other.example.org",
            ),
            ("dynodoc-mine-1", "2026-10-03T00:00:00Z", PROJECT, ORIGIN),
            ("dynodoc-mine-2", "2026-10-04T00:00:00Z", PROJECT, ORIGIN),
        ] {
            let path = make(dir.path(), name, KIND_ROUTINE, created);
            with_manifest(&path, |v| {
                v["project"] = serde_json::json!(project);
                v["origin"] = serde_json::json!(origin);
            });
        }
        let removed = apply_retention(
            dir.path(),
            KIND_ROUTINE,
            1,
            &dir.path().join("dynodoc-mine-2"),
            PROJECT,
            ORIGIN,
        )
        .unwrap();
        assert_eq!(
            removed,
            vec![dir.path().join("dynodoc-mine-1")],
            "only my oldest goes"
        );
        assert!(
            dir.path().join("dynodoc-other-project-1").exists()
                && dir.path().join("dynodoc-other-origin-1").exists()
        );
        let none = apply_retention(
            dir.path(),
            KIND_ROUTINE,
            0,
            &dir.path().join("dynodoc-mine-2"),
            "nobody",
            "https://nobody.example",
        )
        .unwrap();
        assert!(
            none.is_empty(),
            "an installation with no backups of its own removes nothing"
        );
    }
}
