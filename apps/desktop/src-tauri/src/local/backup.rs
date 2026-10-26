//! Backup and restore of the whole local workspace as one archive.
//!
//! The database is stopped for a moment (a cold copy is consistent without any tooling), its data
//! folder and the person's profile are written to one zip with a manifest, and everything starts
//! again. The archive holds no secret of the app: a restore makes new passwords and keys for the
//! restored data (the database is started once with its login rules relaxed to loopback "trust"
//! for the one statement that sets the new passwords, and put back at once). It does hold the
//! documents, unencrypted, so it is as private as the files themselves.
//!
//! A backup opens only with the same PostgreSQL major version and the same kind of operating
//! system, which the manifest records and a restore checks.

use super::layout::Layout;
use super::{appdb, pg, secrets, Local, SecretBundle, SECRETS_ENTRY, SESSION_ENTRY};
use secrets::Store;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;
use zeroize::Zeroizing;

const FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";
const SAVED: &str = "workspace.json";
const DATA_PREFIX: &str = "pgdata/";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format: u32,
    pub app_version: String,
    pub pg_major: String,
    pub os: String,
    pub created_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub path: String,
    pub bytes: u64,
    pub files: usize,
}

pub fn current_os() -> &'static str {
    std::env::consts::OS
}

fn pg_major(layout: &Layout) -> String {
    std::fs::read_to_string(layout.pgdata.join("PG_VERSION"))
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

/// Whether a zip entry name may be extracted: under `pgdata/` (or one of the two known files), no
/// parent folders, no drive letters, no absolute path.
pub fn entry_ok(name: &str) -> bool {
    if name == MANIFEST || name == SAVED {
        return true;
    }
    let Some(rest) = name.strip_prefix(DATA_PREFIX) else {
        return false;
    };
    !rest.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && Path::new(rest).components().all(|c| matches!(c, Component::Normal(_)))
}

/// What a restore says about an archive it will not open.
pub fn check_manifest(m: &Manifest, layout_pg: &str) -> Result<(), String> {
    if m.format != FORMAT {
        return Err("This backup was made by a different version of the format and cannot be restored here.".into());
    }
    if m.os != current_os() {
        return Err(format!(
            "This backup was made on {} and holds a database that only the same kind of operating system can open (this is {}).",
            m.os,
            current_os()
        ));
    }
    if !layout_pg.is_empty() && m.pg_major != layout_pg {
        return Err(format!(
            "This backup holds PostgreSQL {} data; this app runs {}.",
            m.pg_major, layout_pg
        ));
    }
    Ok(())
}

fn zip_folder(
    writer: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    dir: &Path,
    count: &mut usize,
) -> std::io::Result<()> {
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata()?;
        if meta.is_dir() {
            zip_folder(writer, root, &path, count)?;
        } else if meta.is_file() {
            // A lock and a command line from the run that was just stopped are not data.
            if name == "postmaster.pid" || name == "postmaster.opts" {
                continue;
            }
            writer.start_file(format!("{DATA_PREFIX}{rel}"), options).map_err(std::io::Error::other)?;
            let mut file = std::fs::File::open(&path)?;
            std::io::copy(&mut file, writer)?;
            *count += 1;
        }
    }
    Ok(())
}

/// Writes the archive of a stopped cluster. Pure file work (no database, no process).
pub fn write_archive(layout: &Layout, dest: &Path, app_version: &str) -> Result<BackupInfo, String> {
    let partial = dest.with_extension("zip.partial");
    let result = (|| -> std::io::Result<usize> {
        let file = std::fs::File::create(&partial)?;
        let mut writer = zip::ZipWriter::new(file);
        let plain = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let manifest = Manifest {
            format: FORMAT,
            app_version: app_version.to_string(),
            pg_major: pg_major(layout),
            os: current_os().to_string(),
            created_ms: appdb::now_ms(),
        };
        writer.start_file(MANIFEST, plain).map_err(std::io::Error::other)?;
        writer.write_all(serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes())?;
        if let Ok(saved) = std::fs::read(&layout.state_file) {
            writer.start_file(SAVED, plain).map_err(std::io::Error::other)?;
            writer.write_all(&saved)?;
        }
        let mut count = 0;
        zip_folder(&mut writer, &layout.pgdata, &layout.pgdata, &mut count)?;
        writer.finish().map_err(std::io::Error::other)?;
        Ok(count)
    })();
    match result {
        Ok(files) => {
            std::fs::rename(&partial, dest).map_err(|_| "The backup could not be saved there.".to_string())?;
            let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            Ok(BackupInfo {
                path: dest.display().to_string(),
                bytes,
                files,
            })
        }
        Err(_) => {
            let _ = std::fs::remove_file(&partial);
            Err("The backup could not be written. Is there room on that drive?".to_string())
        }
    }
}

/// Reads an archive into `into` (a new folder), refusing anything outside the known names.
pub fn read_archive(src: &Path, into: &Path, layout_pg: &str) -> Result<Manifest, String> {
    let file = std::fs::File::open(src).map_err(|_| "The backup could not be opened.".to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "That file is not a Dynodoc backup.".to_string())?;
    let manifest: Manifest = {
        let mut entry = archive
            .by_name(MANIFEST)
            .map_err(|_| "That file is not a Dynodoc backup (it has no manifest).".to_string())?;
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|_| "The backup's manifest could not be read.".to_string())?;
        serde_json::from_str(&text).map_err(|_| "The backup's manifest is not valid.".to_string())?
    };
    check_manifest(&manifest, layout_pg)?;
    std::fs::create_dir_all(into).map_err(|_| "The restore folder could not be made.".to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| "The backup is damaged.".to_string())?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        if !entry_ok(&name) {
            return Err("The backup holds a file it should not; it was not opened.".to_string());
        }
        let target = into.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| "The restore folder could not be made.".to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|_| "The restore folder could not be written.".to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|_| "The backup is damaged.".to_string())?;
    }
    if !into.join("pgdata").join("PG_VERSION").is_file() {
        return Err("The backup holds no database.".to_string());
    }
    Ok(manifest)
}

impl Local {
    /// Stops the engine and the database (the workspace reads as "starting" meanwhile).
    async fn begin_maintenance(&self, what: &str) -> Result<super::layout::Programs, String> {
        if !self.is_ready() {
            return Err("The workspace is not ready yet.".to_string());
        }
        let programs = self.programs()?;
        self.set("starting", what);
        let _ = self.inner.shutdown.send(true);
        tokio::time::sleep(Duration::from_millis(500)).await;
        *self.inner.app.lock().await = None;
        *self.inner.session.lock().await = None;
        self.inner.base.set(None);
        let database = pg::Pg::new(programs.clone(), self.inner.layout.clone());
        if let Err(e) = database.stop().await {
            self.end_maintenance().await;
            return Err(e.to_string());
        }
        Ok(programs)
    }

    /// Starts the workspace again and waits until it is ready (or has said why it is not).
    async fn end_maintenance(&self) {
        let _ = self.inner.shutdown.send(false);
        self.inner.started.store(false, Ordering::SeqCst);
        self.start();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
        while tokio::time::Instant::now() < deadline {
            let s = self.status();
            if s.phase == "ready" || s.phase == "failed" || s.phase == "profile" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    pub async fn backup(&self, dest: PathBuf) -> Result<BackupInfo, String> {
        let layout = self.inner.layout.clone();
        let version = env!("CARGO_PKG_VERSION").to_string();
        self.begin_maintenance("Closing the database for a moment to make the backup").await?;
        let out = tauri::async_runtime::spawn_blocking(move || write_archive(&layout, &dest, &version))
            .await
            .map_err(|_| "The backup stopped unexpectedly.".to_string())
            .and_then(|r| r);
        self.end_maintenance().await;
        if out.is_ok() {
            applog_note("backup written");
        }
        out
    }

    pub async fn restore(&self, src: PathBuf) -> Result<Manifest, String> {
        let layout = self.inner.layout.clone();
        let major = pg_major(&layout);
        // Read and check the archive before anything is stopped.
        let staging = layout.root.join("restore-staging");
        let _ = std::fs::remove_dir_all(&staging);
        let staging_read = staging.clone();
        let manifest = tauri::async_runtime::spawn_blocking(move || read_archive(&src, &staging_read, &major))
            .await
            .map_err(|_| "The backup could not be read.".to_string())??;
        // New passwords and keys for the restored data.
        let fresh = SecretBundle {
            owner: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
            runtime: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
            paseto: secrets::random_hex(32).map_err(|e| e.to_string())?.to_string(),
            service: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
        };
        let (owner_pw, runtime_pw) = (fresh.owner.clone(), fresh.runtime.clone());
        let programs = self.begin_maintenance("Closing the database for a moment to restore the backup").await?;
        let work_layout = layout.clone();
        let staging_move = staging.clone();
        let swapped = tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, String> {
            let kept = work_layout.root.join(format!("pgdata.before-restore-{}", appdb::now_ms()));
            std::fs::rename(&work_layout.pgdata, &kept).map_err(|_| "The current data could not be set aside.".to_string())?;
            if std::fs::rename(staging_move.join("pgdata"), &work_layout.pgdata).is_err() {
                let _ = std::fs::rename(&kept, &work_layout.pgdata);
                return Err("The restored data could not be put in place; nothing was changed.".to_string());
            }
            if let Err(e) = reset_passwords(&programs, &work_layout, &owner_pw, &runtime_pw) {
                // The restore did not work: the earlier data goes back.
                let _ = std::fs::remove_dir_all(&work_layout.pgdata);
                let _ = std::fs::rename(&kept, &work_layout.pgdata);
                return Err(e);
            }
            if let Ok(saved) = std::fs::read(staging_move.join(SAVED)) {
                let _ = std::fs::write(&work_layout.state_file, saved);
            }
            let _ = std::fs::remove_dir_all(&staging_move);
            Ok(kept)
        })
        .await
        .map_err(|_| "The restore stopped unexpectedly.".to_string())
        .and_then(|r| r);
        if let Ok(kept) = &swapped {
            // The restored data opens with the new keys, so they are stored before it starts.
            let text = Zeroizing::new(serde_json::to_string(&fresh).map_err(|_| "New keys could not be made.".to_string())?);
            self.inner.secrets.set(SECRETS_ENTRY, &text).map_err(|e| e.to_string())?;
            let _ = self.inner.secrets.delete(SESSION_ENTRY);
            applog_note(&format!("restored; the earlier data is kept in {}", kept.display()));
        }
        let _ = std::fs::remove_dir_all(&staging);
        self.end_maintenance().await;
        swapped.map(|_| manifest)
    }
}

fn applog_note(text: &str) {
    super::applog::note(text);
}

/// Sets the owner's and the runtime login's passwords on a restored cluster: the cluster is started
/// once with loopback `trust` login, the two statements run, and its login rules are put back.
fn reset_passwords(
    programs: &super::layout::Programs,
    layout: &Layout,
    owner: &str,
    runtime: &str,
) -> Result<(), String> {
    let hba = layout.pgdata.join("pg_hba.conf");
    let kept = layout.pgdata.join("pg_hba.conf.scram");
    std::fs::copy(&hba, &kept).map_err(|_| "The restored data has no login rules.".to_string())?;
    let result = tauri::async_runtime::block_on(async {
        std::fs::write(&hba, "host all all 127.0.0.1/32 trust\nhost all all ::1/128 trust\n")
            .map_err(|_| "The login rules could not be changed.".to_string())?;
        let database = pg::Pg::new(programs.clone(), layout.clone());
        let port = pg::free_port().map_err(|_| "No local port is free.".to_string())?;
        database.start(port).await.map_err(|e| e.to_string())?;
        let done = async {
            let client = pg::Pg::connect(port, pg::OWNER, "", "postgres").await.map_err(|e| e.to_string())?;
            for sql in [
                format!("alter role {} password '{owner}'", pg::OWNER),
                format!("alter role {} login nosuperuser nocreatedb nocreaterole noinherit password '{runtime}'", pg::RUNTIME),
            ] {
                client.batch_execute(&sql).await.map_err(|_| "The new passwords could not be set.".to_string())?;
            }
            Ok::<(), String>(())
        }
        .await;
        let _ = database.stop().await;
        done
    });
    // The login rules go back whatever happened.
    let restored = std::fs::copy(&kept, &hba).and_then(|_| std::fs::remove_file(&kept));
    result?;
    restored.map_err(|_| "The login rules could not be put back.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(os: &str, pg: &str, format: u32) -> Manifest {
        Manifest {
            format,
            app_version: "0.0.1-beta".into(),
            pg_major: pg.into(),
            os: os.into(),
            created_ms: 0,
        }
    }

    #[test]
    fn only_known_names_are_extracted_and_never_outside_the_folder() {
        assert!(entry_ok("manifest.json"));
        assert!(entry_ok("workspace.json"));
        assert!(entry_ok("pgdata/PG_VERSION"));
        assert!(entry_ok("pgdata/base/1/1259"));
        for bad in ["pgdata/", "../pgdata/x", "pgdata/../x", "/etc/passwd", "C:/x", "pgdata/a:b", "pgdata\\x", "other/x", "secrets.json"] {
            assert!(!entry_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_backup_from_elsewhere_is_refused_in_plain_words() {
        let ok = manifest(current_os(), "16", FORMAT);
        assert!(check_manifest(&ok, "16").is_ok());
        assert!(check_manifest(&manifest("plan9", "16", FORMAT), "16").unwrap_err().contains("operating system"));
        assert!(check_manifest(&manifest(current_os(), "15", FORMAT), "16").unwrap_err().contains("PostgreSQL 15"));
        assert!(check_manifest(&manifest(current_os(), "16", 2), "16").is_err());
    }

    #[test]
    fn an_archive_round_trips_and_a_foreign_one_is_refused() {
        let root = std::env::temp_dir().join(format!("dynodoc-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let layout = Layout::new(&root.join("data"));
        std::fs::create_dir_all(layout.pgdata.join("base").join("1")).unwrap();
        std::fs::write(layout.pgdata.join("PG_VERSION"), "16\n").unwrap();
        std::fs::write(layout.pgdata.join("base").join("1").join("1259"), vec![7u8; 5000]).unwrap();
        std::fs::write(layout.pgdata.join("postmaster.pid"), "123").unwrap();
        std::fs::write(&layout.state_file, "{\"schema\":1}").unwrap();
        let dest = root.join("backup.zip");
        let info = write_archive(&layout, &dest, "0.0.1-beta").unwrap();
        assert_eq!(info.files, 2, "the lock file is not data");
        let out = root.join("out");
        let m = read_archive(&dest, &out, "16").unwrap();
        assert_eq!(m.pg_major, "16");
        assert_eq!(std::fs::read(out.join("pgdata").join("base").join("1").join("1259")).unwrap().len(), 5000);
        assert!(!out.join("pgdata").join("postmaster.pid").exists());
        assert_eq!(std::fs::read_to_string(out.join("workspace.json")).unwrap(), "{\"schema\":1}");
        assert!(read_archive(&dest, &root.join("out2"), "17").is_err());
        std::fs::write(root.join("junk.zip"), b"not a zip").unwrap();
        assert!(read_archive(&root.join("junk.zip"), &root.join("out3"), "16").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
