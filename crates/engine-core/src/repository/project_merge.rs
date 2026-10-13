//! Two-parent project merges with an explicit, opt-in same-file convergence.
//!
//! The path-level planner (`compare::plan`) is unchanged: a path edited on both
//! sides is a whole-file conflict that needs an explicit `FileEntry` (or `None`
//! to delete). With [`SameFile::Clean`] a conflicting path that keeps one stable
//! document identity on the base, left and right commits is first merged node by
//! node and field by field (`convergence::converge`, clean in both orientations).
//! The merged state is stored, becomes the file's entry and is recorded in the
//! commit as a signed same-file resolution (commit format v3). A path with any
//! node/field conflict, a path whose identity changed, and every path beyond the
//! per-merge bound stay conflicts that need an explicit choice. An explicit
//! resolution for a path always wins over the automatic merge and leaves no record.
//!
//! This module builds an unsigned commit; signing and publication stay with the
//! caller (`SignedCommit::sign`, `import_commit`, `compare_and_swap`).

use super::{
    compare,
    convergence::{self, converge, same_document_conflicts, state_address, Resolution},
    local::LocalRepository,
    Commit, Error, FileEntry,
};
use crate::shared_checkpoint::Address;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SameFile {
    /// Every path-level conflict needs an explicit resolution (the default).
    Off,
    /// Merge same-document conflicts that are clean in both orientations.
    Clean,
}

#[derive(Debug)]
pub struct Merge {
    /// The unique best common ancestor both parents were merged against.
    pub base: Address,
    /// The unsigned two-parent commit (`author_key` is a placeholder until
    /// signing). `None` while `unresolved` is not empty; nothing was stored then.
    pub commit: Option<Commit>,
    /// Automatic same-file resolutions recorded in the commit (empty means v1).
    pub applied: Vec<Resolution>,
    /// Conflict paths settled by the caller's resolutions.
    pub explicit: Vec<String>,
    /// Conflict paths that still need an explicit resolution.
    pub unresolved: Vec<String>,
}

impl LocalRepository {
    /// Prepare a two-parent merge of `left` (first parent) and `right`.
    ///
    /// Requires one unique best common ancestor; criss-cross and disconnected
    /// histories are refused exactly as for `compare`. Writes (merged same-file
    /// states, through the ordinary budgeted `write_state`) happen only when every
    /// conflict is resolved, before the commit would be imported.
    pub async fn prepare_merge(
        &self,
        left: &Address,
        right: &Address,
        same_file: SameFile,
        resolutions: &BTreeMap<String, Option<FileEntry>>,
        authored_at_ms: i64,
        message: String,
    ) -> Result<Merge, Error> {
        if left == right {
            return Err(Error::Invalid("merge parents must differ"));
        }
        let left_commit = self.commit(left).await?;
        let right_commit = self.commit(right).await?;
        let bases = self.merge_bases(left, right).await?;
        let [base] = bases.as_slice() else {
            return Err(Error::Invalid(
                "merge requires the unique best common ancestor; resolve disconnected or criss-cross history explicitly",
            ));
        };
        let base_commit = self.commit(base).await?;
        let plan = compare::plan(
            &base_commit.commit,
            &left_commit.commit,
            &right_commit.commit,
        )?;
        let conflicts: BTreeSet<&String> = plan.conflicts.iter().map(|c| &c.path).collect();
        if resolutions.keys().any(|path| !conflicts.contains(path)) {
            return Err(Error::Invalid(
                "resolution names a path that is not a conflict",
            ));
        }

        // Pure computation first: nothing is stored unless every path resolves.
        let mut merged_states = Vec::new();
        if same_file == SameFile::Clean {
            let candidates = same_document_conflicts(&plan);
            for (path, base_file, left_file, right_file) in candidates
                .iter()
                .filter(|(path, ..)| !resolutions.contains_key(*path))
                .take(convergence::MAX_REPORTED_FILES)
            {
                let document = base_file.document;
                let base_state = self.read_state(document, &base_file.state).await?;
                let left_state = self.read_state(document, &left_file.state).await?;
                let right_state = self.read_state(document, &right_file.state).await?;
                let result = converge(
                    &base_state,
                    &left_state,
                    &right_state,
                    &left_file.state,
                    &right_file.state,
                );
                if let Some(state) = result.merged {
                    merged_states.push((
                        (*path).clone(),
                        document,
                        [
                            base_file.state.clone(),
                            left_file.state.clone(),
                            right_file.state.clone(),
                        ],
                        state,
                    ));
                }
            }
        }
        let automatic: BTreeSet<&String> = merged_states.iter().map(|m| &m.0).collect();
        let unresolved: Vec<String> = conflicts
            .iter()
            .filter(|path| !automatic.contains(**path) && !resolutions.contains_key(**path))
            .map(|path| (*path).clone())
            .collect();
        let explicit: Vec<String> = resolutions.keys().cloned().collect();
        if !unresolved.is_empty() {
            return Ok(Merge {
                base: base.clone(),
                commit: None,
                applied: Vec::new(),
                explicit,
                unresolved,
            });
        }

        let mut files = plan.files.clone();
        for (path, choice) in resolutions {
            if let Some(file) = choice {
                files.insert(path.clone(), file.clone());
            }
        }
        let mut applied = Vec::new();
        for (path, document, [base_state, left_state, right_state], state) in merged_states {
            let predicted = state_address(document, &state).await?;
            let (merged, _) = self.write_state(document, &state).await?;
            if merged != predicted {
                return Err(Error::Invalid("merged state address changed while storing"));
            }
            files.insert(
                path.clone(),
                FileEntry {
                    document,
                    state: merged.clone(),
                },
            );
            applied.push(Resolution {
                path,
                document,
                base: base_state,
                left: left_state,
                right: right_state,
                merged,
                strategy: convergence::STRATEGY.into(),
            });
        }
        let commit = Commit {
            version: if applied.is_empty() { 1 } else { 3 },
            project: self.project(),
            parents: vec![left.clone(), right.clone()],
            files,
            author_key: "00".repeat(32), // Filled by SignedCommit::sign.
            authored_at_ms,
            message,
            recovery: None,
            same_file_resolutions: applied.clone(),
        };
        commit.bytes()?; // Shared identity, namespace and size validation.
        Ok(Merge {
            base: base.clone(),
            commit: Some(commit),
            applied,
            explicit,
            unresolved: Vec::new(),
        })
    }

    /// Re-run the recorded strategy for every same-file resolution of a stored
    /// commit and require its recorded merged address. Returns the number of
    /// records verified (0 for commits without any).
    ///
    /// Import checks that a record describes the commit's real parents and base;
    /// this additionally proves the merged state is what the named strategy
    /// produces from those inputs. It reads each record's three states once.
    pub async fn verify_same_file_resolutions(&self, commit: &Address) -> Result<usize, Error> {
        let signed = self.commit(commit).await?;
        for record in &signed.commit.same_file_resolutions {
            let document = record.document;
            let base = self.read_state(document, &record.base).await?;
            let left = self.read_state(document, &record.left).await?;
            let right = self.read_state(document, &record.right).await?;
            let result = converge(&base, &left, &right, &record.left, &record.right);
            let reproduced = match &result.merged {
                Some(state) => Some(state_address(document, state).await?),
                None => None,
            };
            if reproduced.as_ref() != Some(&record.merged) {
                return Err(Error::Invalid(
                    "same-file resolution does not reproduce its recorded state",
                ));
            }
        }
        Ok(signed.commit.same_file_resolutions.len())
    }
}
