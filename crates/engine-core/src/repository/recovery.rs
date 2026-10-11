//! Commit-addressed project recovery. Apply a source delta (or its inverse) to
//! an explicit current commit. Recovery sources never become ancestry implicitly.
//! Same-file edits and shared path/identity constraints require explicit choices.
use super::{compare, Commit, Error, FileEntry};
use crate::shared_checkpoint::Address;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    CherryPick,
    Revert,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub kind: Kind,
    pub commit: Address,
    /// None only for a root commit. A merge requires an explicit chosen parent.
    pub mainline: Option<Address>,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub project: uuid::Uuid,
    pub onto: Address,
    pub source: Source,
    pub changes: compare::ProjectPlan,
}

/// The caller loads and verifies these commits. Mainline must be an actual
/// parent; only a root can use the empty-tree baseline. No ancestry search or
/// arbitrary first-parent choice is performed here.
pub fn plan(
    kind: Kind,
    source: &Commit,
    onto: &Commit,
    mainline: Option<&Commit>,
) -> Result<Plan, Error> {
    source.validate()?;
    onto.validate()?;
    if source.project != onto.project {
        return Err(Error::Invalid("recovery project scope"));
    }
    let mut empty = source.clone();
    empty.files.clear();
    let (baseline, parent) = match mainline {
        None if source.parents.is_empty() => (&empty, None),
        Some(parent) => {
            let address = parent.address()?;
            if parent.project != source.project || !source.parents.contains(&address) {
                return Err(Error::Invalid("recovery mainline is not a source parent"));
            }
            (parent, Some(address))
        }
        None => return Err(Error::Invalid("recovery source requires a mainline parent")),
    };
    let (before, after) = match kind {
        Kind::CherryPick => (baseline, source),
        Kind::Revert => (source, baseline),
    };
    Ok(Plan {
        project: source.project,
        onto: onto.address()?,
        source: Source {
            kind,
            commit: source.address()?,
            mainline: parent,
        },
        changes: compare::plan(before, onto, after)?,
    })
}

impl Plan {
    /// Only conflicting paths can differ from the deterministic plan. Importers
    /// recheck this against the verified source and current commits. A signed
    /// manifest cannot label unrelated edits as recovery of a different commit.
    pub fn validate_files(&self, files: &BTreeMap<String, FileEntry>) -> Result<(), Error> {
        let conflicts: BTreeSet<_> = self.changes.conflicts.iter().map(|c| &c.path).collect();
        for path in files.keys().chain(self.changes.files.keys()) {
            if !conflicts.contains(path) && files.get(path) != self.changes.files.get(path) {
                return Err(Error::Invalid("recovery changes an unconflicted path"));
            }
        }
        Ok(())
    }

    /// Each conflict needs an explicit value or null (delete). Unknown choices
    /// are rejected. Signing and branch publication remain separate operations.
    pub fn resolve(
        &self,
        resolutions: &BTreeMap<String, Option<FileEntry>>,
        authored_at_ms: i64,
        message: String,
    ) -> Result<Commit, Error> {
        if resolutions.len() != self.changes.conflicts.len()
            || self
                .changes
                .conflicts
                .iter()
                .any(|c| !resolutions.contains_key(&c.path))
        {
            return Err(Error::Invalid("resolve every conflict path exactly once"));
        }
        let mut files = self.changes.files.clone();
        for (path, choice) in resolutions {
            if let Some(file) = choice {
                files.insert(path.clone(), file.clone());
            }
        }
        self.validate_files(&files)?;
        let result = Commit {
            version: 2,
            project: self.project,
            parents: vec![self.onto.clone()],
            files,
            author_key: "00".repeat(32), // Filled by SignedCommit::sign.
            authored_at_ms,
            message,
            recovery: Some(self.source.clone()),
        };
        result.bytes()?; // Shared identity, namespace and size validation.
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{tests::signed, SignedCommit};
    use engine_shared::DocumentId;
    use ring::signature::Ed25519KeyPair;

    fn entry(id: u128, byte: &str) -> FileEntry {
        FileEntry {
            document: DocumentId(uuid::Uuid::from_u128(id)),
            state: Address::try_from(byte.repeat(64)).unwrap(),
        }
    }
    fn commit(parents: Vec<Address>, files: BTreeMap<String, FileEntry>) -> Commit {
        signed(uuid::Uuid::from_u128(1), parents, files, "test").commit
    }

    #[test]
    fn cherry_pick_and_revert_preserve_unrelated_later_work_and_only_add_current_parent() {
        let base = commit(
            vec![],
            BTreeMap::from([
                ("Report.docx".into(), entry(1, "a")),
                ("Budget.xlsx".into(), entry(2, "b")),
            ]),
        );
        let mut source = base.clone();
        source.parents = vec![base.address().unwrap()];
        source.files.get_mut("Report.docx").unwrap().state = entry(1, "c").state;
        let mut onto = base.clone();
        onto.parents = vec![base.address().unwrap()];
        onto.files.get_mut("Budget.xlsx").unwrap().state = entry(2, "d").state;
        let pick = plan(Kind::CherryPick, &source, &onto, Some(&base)).unwrap();
        let picked = pick
            .resolve(&BTreeMap::new(), 2, "Recover report edit".into())
            .unwrap();
        assert_eq!(picked.files["Report.docx"], source.files["Report.docx"]);
        assert_eq!(picked.files["Budget.xlsx"], onto.files["Budget.xlsx"]);
        assert_eq!(picked.parents, vec![onto.address().unwrap()]);
        assert_eq!(
            picked.dependencies(),
            vec![
                onto.address().unwrap(),
                source.address().unwrap(),
                base.address().unwrap()
            ]
        );
        let undo = plan(Kind::Revert, &source, &picked, Some(&base)).unwrap();
        let undone = undo
            .resolve(&BTreeMap::new(), 3, "Undo only the report edit".into())
            .unwrap();
        assert_eq!(undone.files, onto.files);
        // Repeating an already-applied delta is a content no-op, never a reset.
        let repeated = plan(Kind::CherryPick, &source, &picked, Some(&base)).unwrap();
        assert_eq!(
            repeated
                .resolve(&BTreeMap::new(), 4, "Already present".into())
                .unwrap()
                .files,
            picked.files
        );
    }

    #[test]
    fn competing_edit_or_delete_requires_complete_explicit_resolution() {
        let base = commit(
            vec![],
            BTreeMap::from([("a".into(), entry(1, "a")), ("b".into(), entry(2, "b"))]),
        );
        let mut source = base.clone();
        source.parents = vec![base.address().unwrap()];
        source.files.get_mut("a").unwrap().state = entry(1, "c").state;
        let mut onto = base.clone();
        onto.files.remove("a");
        let plan = plan(Kind::CherryPick, &source, &onto, Some(&base)).unwrap();
        assert_eq!(plan.changes.conflicts.len(), 1);
        assert!(plan
            .resolve(&BTreeMap::new(), 2, "Missing choice".into())
            .is_err());
        assert!(plan
            .resolve(
                &BTreeMap::from([("b".into(), None)]),
                2,
                "Wrong choice".into()
            )
            .is_err());
        let removed = plan
            .resolve(
                &BTreeMap::from([("a".into(), None)]),
                2,
                "Keep deletion".into(),
            )
            .unwrap();
        assert_eq!(removed.files, onto.files);
        let kept = plan
            .resolve(
                &BTreeMap::from([("a".into(), Some(entry(1, "c")))]),
                2,
                "Take recovered edit".into(),
            )
            .unwrap();
        assert_eq!(kept.files["a"], source.files["a"]);
        let mut forged = kept.files.clone();
        forged.remove("b");
        assert!(plan.validate_files(&forged).is_err());
        forged = kept.files;
        forged.insert("extra".into(), entry(3, "e"));
        assert!(plan.validate_files(&forged).is_err());
    }

    #[test]
    fn recovery_checks_shared_identity_and_path_constraints_after_resolution() {
        let base = commit(vec![], BTreeMap::from([("old".into(), entry(1, "a"))]));
        let source = commit(
            vec![base.address().unwrap()],
            BTreeMap::from([("renamed".into(), entry(1, "a"))]),
        );
        let onto = commit(
            vec![base.address().unwrap()],
            BTreeMap::from([("elsewhere".into(), entry(1, "a"))]),
        );
        let plan = plan(Kind::CherryPick, &source, &onto, Some(&base)).unwrap();
        assert_eq!(plan.changes.conflicts.len(), 2);
        assert!(plan
            .resolve(
                &BTreeMap::from([
                    ("renamed".into(), Some(entry(1, "a"))),
                    ("elsewhere".into(), Some(entry(1, "a"))),
                ]),
                2,
                "Invalid duplicate identity".into()
            )
            .is_err());
        let resolved = plan
            .resolve(
                &BTreeMap::from([
                    ("renamed".into(), Some(entry(1, "a"))),
                    ("elsewhere".into(), None),
                ]),
                2,
                "Choose rename".into(),
            )
            .unwrap();
        assert_eq!(resolved.files, source.files);
    }

    #[test]
    fn root_uses_empty_baseline_and_merge_requires_an_actual_selected_parent() {
        let empty = commit(vec![], BTreeMap::new());
        let root = commit(vec![], BTreeMap::from([("a".into(), entry(1, "a"))]));
        let added = plan(Kind::CherryPick, &root, &empty, None).unwrap();
        assert_eq!(
            added
                .resolve(&BTreeMap::new(), 2, "Initial recovery".into())
                .unwrap()
                .files,
            root.files
        );
        let removed = plan(Kind::Revert, &root, &root, None).unwrap();
        assert!(removed
            .resolve(&BTreeMap::new(), 3, "Undo initial".into())
            .unwrap()
            .files
            .is_empty());
        assert!(plan(Kind::Revert, &root, &root, Some(&empty)).is_err());
        let merge = commit(
            vec![root.address().unwrap(), empty.address().unwrap()],
            root.files.clone(),
        );
        assert!(plan(Kind::Revert, &merge, &merge, None).is_err());
        let against_root = plan(Kind::Revert, &merge, &merge, Some(&root)).unwrap();
        assert_eq!(against_root.changes.files, root.files);
        let against_empty = plan(Kind::Revert, &merge, &merge, Some(&empty)).unwrap();
        assert!(against_empty.changes.files.is_empty());
        let mut foreign = empty.clone();
        foreign.project = uuid::Uuid::from_u128(2);
        assert!(plan(Kind::Revert, &root, &foreign, None).is_err());
    }

    #[test]
    fn independent_v2_vector_binds_recovery_without_changing_v1() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/project-recovery-v2.json"
        ))
        .unwrap();
        let signed: SignedCommit = serde_json::from_value(fixture["signed"].clone()).unwrap();
        assert_eq!(
            signed.commit.bytes().unwrap(),
            fixture["canonical"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            signed.verify().unwrap().as_str(),
            fixture["address"].as_str().unwrap()
        );
        assert_eq!(
            SignedCommit::sign(
                signed.commit.clone(),
                &Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap()
            )
            .unwrap()
            .signature,
            signed.signature
        );
        let mut forged = signed.clone();
        forged.commit.recovery.as_mut().unwrap().kind = Kind::Revert;
        assert!(forged.verify().is_err());
        forged = signed.clone();
        forged.commit.recovery.as_mut().unwrap().mainline = None;
        assert!(forged.verify().is_err());
        forged = signed.clone();
        forged.commit.recovery.as_mut().unwrap().commit =
            Address::try_from("44".repeat(32)).unwrap();
        assert!(forged.verify().is_err());
        let mut bad = signed.commit.clone();
        bad.version = 1;
        assert!(bad.validate().is_err());
        bad = signed.commit.clone();
        bad.recovery = None;
        assert!(bad.validate().is_err());
        bad = signed.commit;
        bad.parents.clear();
        assert!(bad.validate().is_err());
    }
}
