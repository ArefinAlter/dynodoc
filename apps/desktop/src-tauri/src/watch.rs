//! Folder watch: a project can be bound to a folder on this computer; a Word, Excel or PowerPoint
//! file saved in it by the editor is noticed here and reported to the page, which turns it into
//! changes (the web workspace's own push planner, in the Office codec).
//!
//! The folder is read by looking, every two seconds, at the names, sizes and modification times of
//! the files in it: no operating-system hooks, so it works the same on a network drive, a synced
//! folder and every platform. A file is reported once it has stopped changing (two looks in a
//! row), because an editor writes a file in several steps. Nothing in the folder is ever written
//! or deleted by this module.

use crate::files::{is_watched_name, Picks};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// What one look at a file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub mtime_ms: i64,
    pub size: u64,
}

/// What the app knows about a file of a bound folder from earlier: the document it belongs to, the
/// revision of that document it was last in step with, and its modification time then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub document_id: String,
    pub base_seq: i32,
    pub seen_mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Change {
    pub project_id: String,
    pub rel_path: String,
    pub name: String,
    /// `new` (not in the project yet) or `changed` (saved after the project last saw it).
    pub kind: &'static str,
    pub document_id: Option<String>,
    pub base_seq: Option<i32>,
    pub size: u64,
    pub mtime_ms: i64,
    /// For reading it (`file_read`); never a path.
    pub token: String,
}

#[derive(Default)]
pub struct Watcher {
    /// (project, relative path) -> the last look and how many looks in a row it was the same.
    looks: Mutex<HashMap<(String, String), (Seen, u32)>>,
    pending: Mutex<Vec<Change>>,
    /// The look (file time) each file was last reported at, so a file is reported once per save.
    reported: Mutex<HashMap<(String, String), i64>>,
}

/// Looks at the watched files of a folder (top level and one level of sub-folders).
pub fn list_folder(root: &Path) -> Vec<(String, PathBuf, Seen)> {
    let mut out = Vec::new();
    let visit = |dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf, Seen)>| -> Vec<PathBuf> {
        let mut subs = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return subs;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            if meta.is_dir() && !name.starts_with('.') && !name.starts_with('~') && prefix.is_empty() {
                subs.push(entry.path());
            } else if meta.is_file() && is_watched_name(&name) {
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                out.push((
                    format!("{prefix}{name}"),
                    entry.path(),
                    Seen {
                        mtime_ms,
                        size: meta.len(),
                    },
                ));
            }
        }
        subs
    };
    let subs = visit(root, "", &mut out);
    for sub in subs.into_iter().take(50) {
        let name = sub.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        visit(&sub, &format!("{name}/"), &mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.truncate(500);
    out
}

impl Watcher {
    /// Takes one look at a project's folder and queues what is new or changed and has settled.
    pub fn scan(
        &self,
        project_id: &str,
        files: Vec<(String, PathBuf, Seen)>,
        known: &HashMap<String, Known>,
        picks: &Picks,
    ) {
        let (Ok(mut looks), Ok(mut pending), Ok(mut reported)) =
            (self.looks.lock(), self.pending.lock(), self.reported.lock())
        else {
            return;
        };
        let present: std::collections::HashSet<&String> = files.iter().map(|f| &f.0).collect();
        looks.retain(|(p, rel), _| p != project_id || present.contains(rel));
        for (rel, path, seen) in files {
            let key = (project_id.to_string(), rel.clone());
            let steady = match looks.get_mut(&key) {
                Some((last, n)) if *last == seen => {
                    *n += 1;
                    *n
                }
                _ => {
                    looks.insert(key.clone(), (seen.clone(), 1));
                    1
                }
            };
            if steady < 2 {
                continue;
            }
            let kind = match known.get(&rel) {
                None => "new",
                Some(k) if k.seen_mtime_ms != seen.mtime_ms => "changed",
                Some(_) => continue,
            };
            if reported.get(&key) == Some(&seen.mtime_ms) {
                continue;
            }
            reported.insert(key.clone(), seen.mtime_ms);
            // A newer look of the same file replaces an older, unread one.
            pending.retain(|c| !(c.project_id == project_id && c.rel_path == rel));
            let Some(token) = picks.remember(path.clone()) else { continue };
            pending.push(Change {
                project_id: project_id.to_string(),
                name: rel.rsplit('/').next().unwrap_or(&rel).to_string(),
                rel_path: rel.clone(),
                kind,
                document_id: known.get(&rel).map(|k| k.document_id.clone()),
                base_seq: known.get(&rel).map(|k| k.base_seq),
                size: seen.size,
                mtime_ms: seen.mtime_ms,
                token,
            });
        }
    }

    /// The changes found since the last call (each is reported once).
    pub fn drain(&self) -> Vec<Change> {
        self.pending.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default()
    }

    /// Forgets what was looked at for a project (its folder changed or was unbound).
    pub fn forget(&self, project_id: &str) {
        if let Ok(mut looks) = self.looks.lock() {
            looks.retain(|(p, _), _| p != project_id);
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.retain(|c| c.project_id != project_id);
        }
        if let Ok(mut reported) = self.reported.lock() {
            reported.retain(|(p, _), _| p != project_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, mtime: i64, size: u64) -> (String, PathBuf, Seen) {
        (rel.to_string(), PathBuf::from(rel), Seen { mtime_ms: mtime, size })
    }

    #[test]
    fn a_file_is_reported_when_it_has_settled_and_only_once() {
        let w = Watcher::default();
        let picks = Picks::default();
        let known = HashMap::new();
        w.scan("p1", vec![file("a.docx", 100, 10)], &known, &picks);
        assert!(w.drain().is_empty(), "one look is not enough: an editor may still be writing");
        w.scan("p1", vec![file("a.docx", 100, 10)], &known, &picks);
        let found = w.drain();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "new");
        assert_eq!(found[0].name, "a.docx");
        w.scan("p1", vec![file("a.docx", 100, 10)], &known, &picks);
        assert!(w.drain().is_empty());
    }

    #[test]
    fn a_file_that_keeps_changing_waits_and_a_known_file_reports_a_change() {
        let w = Watcher::default();
        let picks = Picks::default();
        let mut known = HashMap::new();
        known.insert(
            "b.xlsx".to_string(),
            Known { document_id: "d1".into(), base_seq: 7, seen_mtime_ms: 100 },
        );
        w.scan("p1", vec![file("b.xlsx", 100, 10)], &known, &picks);
        w.scan("p1", vec![file("b.xlsx", 100, 10)], &known, &picks);
        assert!(w.drain().is_empty(), "unchanged since the project last saw it");
        w.scan("p1", vec![file("b.xlsx", 200, 12)], &known, &picks);
        w.scan("p1", vec![file("b.xlsx", 250, 14)], &known, &picks);
        assert!(w.drain().is_empty(), "still being written");
        w.scan("p1", vec![file("b.xlsx", 250, 14)], &known, &picks);
        let found = w.drain();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, "changed");
        assert_eq!(found[0].document_id.as_deref(), Some("d1"));
        assert_eq!(found[0].base_seq, Some(7));
    }

    #[test]
    fn a_folder_is_listed_without_lock_files_and_forgetting_clears_the_queue() {
        let dir = std::env::temp_dir().join(format!("dynodoc-watch-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["a.docx", "~$a.docx", "notes.txt", "sub/b.pptx", ".x.docx"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let names: Vec<String> = list_folder(&dir).into_iter().map(|f| f.0).collect();
        assert_eq!(names, vec!["a.docx".to_string(), "sub/b.pptx".to_string()]);
        let w = Watcher::default();
        let picks = Picks::default();
        w.scan("p", vec![file("a.docx", 1, 1)], &HashMap::new(), &picks);
        w.scan("p", vec![file("a.docx", 1, 1)], &HashMap::new(), &picks);
        w.forget("p");
        assert!(w.drain().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
