//! Project-level three-way planning. Equal/unchanged roots merge without reading
//! document bodies; competing changes are explicit conflicts. No guessed identity
//! matching, silent file deletion, or automatic merge of shared constraints.

use super::{Commit, Error, FileEntry};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct FileConflict {
    pub path: String,
    pub base: Option<FileEntry>,
    pub left: Option<FileEntry>,
    pub right: Option<FileEntry>,
}

#[derive(Debug, Serialize)]
pub struct ProjectPlan {
    /// Resolved paths only. A caller must resolve conflicts before signing a
    /// commit, and validate the resulting complete file map again.
    pub files: BTreeMap<String, FileEntry>,
    pub conflicts: Vec<FileConflict>,
}

pub fn plan(base: &Commit, left: &Commit, right: &Commit) -> Result<ProjectPlan, super::Error> {
    for commit in [base, left, right] {
        commit.validate()?;
    }
    if base.project != left.project || base.project != right.project {
        return Err(Error::Invalid("merge project scope"));
    }
    let paths: BTreeSet<_> = base
        .files
        .keys()
        .chain(left.files.keys())
        .chain(right.files.keys())
        .collect();
    let mut result = ProjectPlan {
        files: BTreeMap::new(),
        conflicts: Vec::new(),
    };
    for path in paths {
        let b = base.files.get(path);
        let l = left.files.get(path);
        let r = right.files.get(path);
        let chosen = if l == r || r == b {
            Some(l)
        } else if l == b {
            Some(r)
        } else {
            None
        };
        match chosen {
            Some(Some(file)) => {
                result.files.insert(path.clone(), file.clone());
            }
            Some(None) => {}
            None => result.conflicts.push(FileConflict {
                path: path.clone(),
                base: b.cloned(),
                left: l.cloned(),
                right: r.cloned(),
            }),
        }
    }
    // Independent paths may violate shared identity/namespace constraints, such
    // as two concurrent renames. Surface every participant, not an arbitrary winner.
    let mut invalid = BTreeSet::new();
    let mut ids = BTreeMap::new();
    for (path, file) in &result.files {
        if let Some(other) = ids.insert(file.document.0, path) {
            invalid.insert(other.clone());
            invalid.insert(path.clone());
        }
        for (index, _) in path.match_indices('/') {
            if result.files.contains_key(&path[..index]) {
                invalid.insert(path[..index].into());
                invalid.insert(path.clone());
            }
        }
    }
    for path in invalid {
        result.files.remove(&path);
        result.conflicts.push(FileConflict {
            base: base.files.get(&path).cloned(),
            left: left.files.get(&path).cloned(),
            right: right.files.get(&path).cloned(),
            path,
        });
    }
    result.conflicts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_checkpoint::Address;
    use engine_shared::DocumentId;

    fn entry(id: u128, byte: &str) -> FileEntry {
        FileEntry {
            document: DocumentId(uuid::Uuid::from_u128(id)),
            state: Address::try_from(byte.repeat(64)).unwrap(),
        }
    }
    fn commit(files: BTreeMap<String, FileEntry>) -> Commit {
        super::super::tests::signed(uuid::Uuid::from_u128(1), vec![], files, "test").commit
    }
    #[test]
    fn independent_files_converge_in_either_order_and_conflicting_edits_remain_explicit() {
        let base = commit(BTreeMap::from([
            ("a".into(), entry(1, "a")),
            ("b".into(), entry(2, "b")),
        ]));
        let mut left = base.clone();
        left.files.get_mut("a").unwrap().state = entry(1, "c").state;
        let mut right = base.clone();
        right.files.get_mut("b").unwrap().state = entry(2, "d").state;
        let forward = plan(&base, &left, &right).unwrap();
        let reverse = plan(&base, &right, &left).unwrap();
        assert!(forward.conflicts.is_empty());
        assert_eq!(forward.files, reverse.files);
        right.files.remove("a");
        let conflict = plan(&base, &left, &right).unwrap();
        assert_eq!(conflict.conflicts.len(), 1);
        assert_eq!(conflict.conflicts[0].path, "a");
    }
    #[test]
    fn different_paths_are_not_independent_when_they_share_identity_or_namespace() {
        let base = commit(BTreeMap::from([("old".into(), entry(1, "a"))]));
        let left = commit(BTreeMap::from([("left".into(), entry(1, "a"))]));
        let right = commit(BTreeMap::from([("right".into(), entry(1, "a"))]));
        let result = plan(&base, &left, &right).unwrap();
        assert_eq!(result.conflicts.len(), 2);
        assert!(result.files.is_empty());
        let base = commit(BTreeMap::new());
        let left = commit(BTreeMap::from([("dir".into(), entry(1, "a"))]));
        let right = commit(BTreeMap::from([("dir/file".into(), entry(2, "b"))]));
        let result = plan(&base, &left, &right).unwrap();
        assert_eq!(result.conflicts.len(), 2);
        assert!(result.files.is_empty());
    }
}
