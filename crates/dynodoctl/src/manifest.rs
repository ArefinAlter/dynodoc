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
    pub files: Vec<FileEntry>,
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
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
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
        CliError::refused(format!("{} has no readable {MANIFEST_NAME} ({e})", dir.display()), "point --backup at a directory made by `dynodoctl backup`")
    })?;
    let manifest: Manifest = serde_json::from_str(&text)?;
    if manifest.format != FORMAT {
        return Err(CliError::refused(
            format!("backup format {} is not understood by this dynodoctl (expects {FORMAT})", manifest.format),
            "use the dynodoctl version that took the backup",
        ));
    }
    Ok(manifest)
}

pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let mut text = serde_json::to_string_pretty(manifest)?;
    text.push('\n');
    write_private(&dir.join(MANIFEST_NAME), text.as_bytes())
}

/// `SHA256SUMS` in the format `sha256sum -c` reads (two spaces, relative path, LF).
pub fn sums_text(entries: &[FileEntry]) -> String {
    entries.iter().map(|e| format!("{}  {}\n", e.sha256, e.path)).collect()
}

pub fn file_entry(root: &Path, relative: &str) -> Result<FileEntry> {
    let (sha256, bytes) = sha256_file(&root.join(relative))?;
    Ok(FileEntry { path: relative.to_string(), bytes, sha256 })
}

/// Every file the manifest vouches for, as paths under the backup directory and the config root.
pub fn verify_files(info: &BackupInfo, config_override: Option<&Path>) -> std::result::Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let m = &info.manifest;
    let mut check = |root: &Path, e: &FileEntry| match sha256_file(&root.join(&e.path)) {
        Ok((hash, bytes)) => {
            if bytes != e.bytes {
                problems.push(format!("{} has {bytes} bytes, the manifest says {}", e.path, e.bytes));
            } else if hash != e.sha256 {
                problems.push(format!("{} does not match its checksum (the file changed or is damaged)", e.path));
            }
        }
        Err(_) => problems.push(format!("{} is missing or unreadable", e.path)),
    };
    check(&info.path, &FileEntry { path: m.database.file.clone(), bytes: m.database.bytes, sha256: m.database.sha256.clone() });
    if let Some(root) = info.config_root(config_override) {
        for e in &m.config.files {
            check(&root, e);
        }
    } else if m.config.location == "external" && !m.config.files.is_empty() {
        problems.push(format!(
            "the settings and secrets copy is outside this backup (directory `{}`); pass --config-from, or accept that it is not checked",
            m.config.dir_name.clone().unwrap_or_default()
        ));
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
    let Ok(entries) = std::fs::read_dir(dir) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !entry.file_name().to_string_lossy().starts_with(DIR_PREFIX) {
            continue;
        }
        if let Ok(manifest) = read_manifest(&path) {
            found.push(BackupInfo { path, manifest });
        }
    }
    found.sort_by(|a, b| a.manifest.created_utc.cmp(&b.manifest.created_utc).then(a.name().cmp(&b.name())));
    found
}

/// Remove the oldest backups of `kind` beyond `keep`. Only directories with a valid manifest,
/// directly inside `dir`, are ever removed; `protect` (the backup just made) never is.
pub fn apply_retention(dir: &Path, kind: &str, keep: usize, protect: &Path) -> Result<Vec<PathBuf>> {
    let mut of_kind: Vec<BackupInfo> = list_backups(dir).into_iter().filter(|b| b.manifest.kind == kind).collect();
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
        database: DumpEntry { file: DUMP_NAME.into(), bytes: 0, sha256: String::new(), toc_entries: 0 },
        facts: crate::db::DbFacts::default(),
        facts_after: crate::db::DbFacts::default(),
        config: ConfigEntry { location: "none".into(), dir_name: None, files: vec![] },
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
        m.database = DumpEntry { file: DUMP_NAME.into(), bytes: 9, sha256: crate::util::sha256_hex(b"PGDMPdata"), toc_entries: 3 };
        write_manifest(&path, &m).unwrap();
        path
    }

    #[test]
    fn manifest_round_trips_and_refuses_unknown_formats() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(dir.path(), "dynodoc-20261022T000000Z", KIND_ROUTINE, "2026-10-22T00:00:00Z");
        assert_eq!(read_manifest(&path).unwrap().kind, "routine");
        let text = std::fs::read_to_string(path.join(MANIFEST_NAME)).unwrap().replace("\"format\": 1", "\"format\": 99");
        std::fs::write(path.join(MANIFEST_NAME), text).unwrap();
        assert_eq!(read_manifest(&path).unwrap_err().code(), crate::error::exit::REFUSED);
        assert!(read_manifest(dir.path()).is_err());
    }

    #[test]
    fn checksums_detect_a_changed_or_missing_dump() {
        let dir = tempfile::tempdir().unwrap();
        let path = make(dir.path(), "dynodoc-20261022T000000Z", KIND_ROUTINE, "2026-10-22T00:00:00Z");
        let info = BackupInfo { manifest: read_manifest(&path).unwrap(), path: path.clone() };
        assert!(verify_files(&info, None).is_ok());
        std::fs::write(path.join(DUMP_NAME), b"PGDMPdatX").unwrap();
        let problems = verify_files(&info, None).unwrap_err();
        assert!(problems[0].contains("checksum"), "{problems:?}");
        std::fs::remove_file(path.join(DUMP_NAME)).unwrap();
        assert!(verify_files(&info, None).unwrap_err()[0].contains("missing"));
    }

    #[test]
    fn sums_file_is_sha256sum_compatible() {
        let text = sums_text(&[FileEntry { path: "database.dump".into(), bytes: 1, sha256: "ab".repeat(32) }]);
        assert_eq!(text, format!("{}  database.dump\n", "ab".repeat(32)));
    }

    #[test]
    fn listing_is_oldest_first_and_ignores_directories_without_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        make(dir.path(), "dynodoc-b", KIND_ROUTINE, "2026-10-22T02:00:00Z");
        make(dir.path(), "dynodoc-a", KIND_ROUTINE, "2026-10-22T01:00:00Z");
        std::fs::create_dir_all(dir.path().join("dynodoc-junk")).unwrap();
        std::fs::create_dir_all(dir.path().join("other")).unwrap();
        let names: Vec<String> = list_backups(dir.path()).iter().map(|b| b.name()).collect();
        assert_eq!(names, vec!["dynodoc-a", "dynodoc-b"]);
    }

    #[test]
    fn retention_removes_the_oldest_and_never_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in ["dynodoc-1", "dynodoc-2", "dynodoc-3", "dynodoc-4"].iter().enumerate() {
            make(dir.path(), name, KIND_ROUTINE, &format!("2026-10-2{}T00:00:00Z", i + 1));
        }
        let newest = dir.path().join("dynodoc-4");
        let removed = apply_retention(dir.path(), KIND_ROUTINE, 2, &newest).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!dir.path().join("dynodoc-1").exists() && !dir.path().join("dynodoc-2").exists());
        assert!(dir.path().join("dynodoc-3").exists() && newest.exists());
    }

    #[test]
    fn retention_counts_only_its_own_kind_and_keeps_unknown_directories() {
        let dir = tempfile::tempdir().unwrap();
        make(dir.path(), "dynodoc-1", KIND_PRE_UPGRADE, "2026-10-21T00:00:00Z");
        make(dir.path(), "dynodoc-2", KIND_ROUTINE, "2026-10-22T00:00:00Z");
        std::fs::create_dir_all(dir.path().join("dynodoc-not-ours")).unwrap();
        std::fs::write(dir.path().join("dynodoc-not-ours").join("precious"), b"x").unwrap();
        let removed = apply_retention(dir.path(), KIND_ROUTINE, 1, &dir.path().join("dynodoc-2")).unwrap();
        assert!(removed.is_empty());
        assert!(dir.path().join("dynodoc-1").exists());
        assert!(dir.path().join("dynodoc-not-ours").join("precious").exists());
    }

    #[test]
    fn retention_of_zero_still_protects_the_backup_just_made() {
        let dir = tempfile::tempdir().unwrap();
        make(dir.path(), "dynodoc-1", KIND_ROUTINE, "2026-10-21T00:00:00Z");
        let newest = make(dir.path(), "dynodoc-2", KIND_ROUTINE, "2026-10-22T00:00:00Z");
        apply_retention(dir.path(), KIND_ROUTINE, 0, &newest).unwrap();
        assert!(newest.exists());
        assert!(!dir.path().join("dynodoc-1").exists());
    }
}
