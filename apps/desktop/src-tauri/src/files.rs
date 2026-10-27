//! The files the person chooses, and the files the app writes for them.
//!
//! The page has no access to the file system. It asks the app to show the system's own open or
//! save dialog; the app remembers each chosen path under a random token and gives the page only the
//! token, the file's name and its size. Reading a file, binding a folder and opening an exported
//! copy all go through such a token, so the page can never name a path of its own.
//!
//! What the app writes: an exported copy of a document in a folder the person can see (the bound
//! folder of the project, or `Documents/Dynodoc/<project>/`), with the name Dynodoc's web
//! workspace uses for downloads (`Title (v12).docx`, which also tells a later upload which revision
//! the copy started from).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The largest file the app reads for the page: the engine's own upload limit, with room for the
/// base64 the page sends back.
pub const MAX_READ_BYTES: u64 = 3_000_000;
/// Backups are whole workspaces; they are read by the app itself, never passed to the page.
const MAX_TOKENS: usize = 400;

#[derive(Default)]
pub struct Picks {
    inner: Mutex<HashMap<String, PathBuf>>,
    order: Mutex<Vec<String>>,
}

impl Picks {
    pub fn remember(&self, path: PathBuf) -> Option<String> {
        let token = crate::local::secrets::random_hex(12).ok()?.to_string();
        let mut map = self.inner.lock().ok()?;
        let mut order = self.order.lock().ok()?;
        while order.len() >= MAX_TOKENS {
            let old = order.remove(0);
            map.remove(&old);
        }
        map.insert(token.clone(), path);
        order.push(token.clone());
        Some(token)
    }

    pub fn path(&self, token: &str) -> Option<PathBuf> {
        self.inner.lock().ok()?.get(token).cloned()
    }
}

/// What a name may contain once it is written to disk: no folder parts, no characters Windows
/// refuses, no trailing dots or spaces, at most 150 characters, never a reserved device name.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if "\\/:*?\"<>|".contains(c) { ' ' } else { c })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    let mut out: String = trimmed.chars().take(150).collect();
    if out.is_empty() {
        out = "Document".to_string();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    out
}

/// A free name in `dir`: `name.ext`, then `name (2).ext`, and so on.
pub fn free_name(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let next = dir.join(format!("{stem} ({n}).{ext}"));
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!("{stem} (copy).{ext}"))
}

/// `Documents/Dynodoc/<project>` for exported copies when a project has no folder of its own.
pub fn default_export_dir(documents: &Path, project_name: &str) -> PathBuf {
    documents.join("Dynodoc").join(safe_file_name(project_name))
}

/// Office extensions the app opens, exports and watches.
pub const OFFICE_EXTENSIONS: [&str; 3] = ["docx", "xlsx", "pptx"];

pub fn extension_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

/// Whether a file name in a bound folder is one of the person's documents (not an editor's lock
/// file, temp file or hidden file).
pub fn is_watched_name(name: &str) -> bool {
    !name.starts_with("~$")
        && !name.starts_with('.')
        && !name.starts_with("~WRL")
        && !name.ends_with(".tmp")
        && OFFICE_EXTENSIONS.contains(&extension_of(name).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_written_to_disk_are_safe() {
        assert_eq!(
            safe_file_name("Project Charter (v3)"),
            "Project Charter (v3)"
        );
        assert_eq!(safe_file_name("../../etc/passwd"), "etc passwd");
        assert_eq!(safe_file_name("a:b*c?.docx"), "a b c .docx");
        assert_eq!(safe_file_name("   ..  "), "Document");
        assert_eq!(safe_file_name("CON"), "_CON");
        assert_eq!(safe_file_name("nul.txt"), "_nul.txt");
        assert!(safe_file_name(&"x".repeat(400)).chars().count() <= 150);
        assert!(!safe_file_name("tab\tand\nnewline").contains(['\t', '\n']));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = std::env::temp_dir().join(format!("dynodoc-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "Report", "docx"), dir.join("Report.docx"));
        std::fs::write(dir.join("Report.docx"), b"x").unwrap();
        assert_eq!(
            free_name(&dir, "Report", "docx"),
            dir.join("Report (2).docx")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_office_files_are_watched_and_editor_lock_files_are_not() {
        assert!(is_watched_name("Budget 2027.xlsx"));
        assert!(is_watched_name("Charter.DOCX"));
        assert!(!is_watched_name("~$Charter.docx"));
        assert!(!is_watched_name(".hidden.docx"));
        assert!(!is_watched_name("notes.txt"));
        assert!(!is_watched_name("Budget.xlsx.tmp"));
        assert!(!is_watched_name("~WRL0001.tmp"));
    }

    #[test]
    fn a_token_names_a_path_and_old_tokens_are_forgotten() {
        let picks = Picks::default();
        let first = picks.remember(PathBuf::from("a.docx")).unwrap();
        assert_eq!(picks.path(&first), Some(PathBuf::from("a.docx")));
        for i in 0..MAX_TOKENS + 5 {
            picks.remember(PathBuf::from(format!("{i}.docx"))).unwrap();
        }
        assert_eq!(picks.path(&first), None);
        assert_eq!(picks.path("nonsense"), None);
    }
}
