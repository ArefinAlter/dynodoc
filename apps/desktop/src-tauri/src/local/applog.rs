//! The app's own log: one small file in the workspace's log folder. Lines are plain sentences.
//! Callers pass text that holds no secret, no URL query, no document text and no file name of a
//! person's document; `procs::scrub` exists for program output that might quote a connection.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Sets the log file (once). Later calls are ignored.
pub fn open(path: PathBuf) {
    super::procs::rotate(&path, 2 * 1024 * 1024);
    let _ = PATH.set(path);
}

pub fn note(line: &str) {
    let Some(path) = PATH.get() else {
        return;
    };
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let clean: String = line.chars().filter(|c| !c.is_control()).take(500).collect();
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{when} {clean}");
    }
}
