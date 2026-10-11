# Portable project commit recovery

12 October 2026. Local repositories can preview, revert and cherry-pick retained
project commits. This is core/CLI functionality, separate from the deployed
[per-file revision recovery](HISTORY-RECOVERY.md). It does not publish remote
branches, bypass project review or apply a native file automatically.

## Behavior

Choose an exact source commit and an exact current (`onto`) commit in the same
project. Cherry-pick applies the difference from the selected source parent to
the source. Revert applies that difference in reverse. A root source uses an empty
file map. A source with one parent selects and records that parent; a merge source
requires an explicit `mainline` address matching one of its parents.

The planner compares file paths, stable document IDs and typed state roots.
Unchanged/equal changes and edits to independent files combine automatically.
Competing changes to the same file, delete/edit, concurrent renames and shared
file/directory or document-identity collisions require explicit resolution.
Conflict values name `base`, `left` (current) and `right` (desired historical end).
This planner does not automatically merge different paragraphs within one file.

Every conflict needs a supplied `FileEntry` (`document` UUID and typed `state`
address) or `null` for deletion. Resolution can select an existing root or an
explicitly constructed, stored result. Missing/extra resolution keys fail; final
file count, identity, namespace and size constraints are rechecked. Commit import
verifies every referenced state graph. The importer recomputes the recovery plan
and rejects changes to its unconflicted paths even when the signature is valid.

The resulting commit has exactly one ancestry parent: `onto`. Source and mainline
are separate signed provenance references. Reverting a merge therefore does not
erase its ancestry. Repeating an already-applied change may produce a content
no-op commit with a new message/provenance record. No object or ref is deleted.

## Compatibility and transfer

Ordinary v1 commits retain their exact canonical JSON, addresses and signature
domain. Recovery commits use version 2 and require a `recovery` object containing
`kind` (`cherry-pick` or `revert`), source `commit` and `mainline` (null only for a
root source). Version 1 cannot carry this metadata. Version 2 requires one parent.

V2 addresses are SHA-256 of `dynodoc.project-commit\0v2\0` followed by sorted-key
canonical JSON. Ed25519 signatures cover
`dynodoc.project-commit-signature\0v2\0` followed by the 32 raw address bytes.
The fixed `project-recovery-v2.json` vector comes from independent Python
canonicalization/hashing and OpenSSL signing with a public synthetic seed.
Never regenerate it to accommodate a failed check. Its dependency hashes are
placeholders; it tests hashing/signing, not a complete importable history.

`Commit::dependencies()` enumerates ancestry plus recovery dependencies for
transfer. Send these verified commits and their content before importing recovery.
Missing sources fail closed. `merge_bases` follows only ancestry parents. Old
readers reject v2 commits; upgrade every participating reader before exchange.
SQLite schema 1 and PostgreSQL schema 32 are unchanged by this increment.

## CLI workflow

The common prefix is `engine-cli repository DATABASE PROJECT_UUID`:

```text
recovery-plan SOURCE ONTO --kind revert
recovery-plan MERGE_SOURCE ONTO --kind cherry-pick --mainline SELECTED_PARENT
recover SOURCE ONTO --kind revert --key KEY.der --message "Recover earlier work" --authored-at-ms TIMESTAMP
recover SOURCE ONTO --kind cherry-pick --resolutions choices.json --key KEY.der --message "Recover reviewed change" --authored-at-ms TIMESTAMP
```

The preview creates no commit, content object or branch update. `recover` signs
and stores only the new commit; the
caller reviews it with `show` and publishes separately using `set-head` with the
expected address **and generation**. A stale branch cannot overwrite later work.
Signatures prove key control, not account permissions; hosted authorization and
project review remain separate integration work.

## Validation and limits

Regressions cover fifty-commit recovery, later independent work, restart/retry,
unchanged object inventory, branch CAS, root/merge sources, missing provenance,
forged source/mainline, unconflicted-path tampering and explicit conflict choices.
V1 known-answer tests continue to run alongside independent v2 vectors.
See [the handoff](ENGINE-EVOLUTION-HANDOFF.md) for exact suite and CLI outcomes.

Preview loads at most three indexed commit manifests (source, current and selected
parent), regardless of the source's distance from the head. Recovery compares
bounded file manifests and existing roots; no full native file is copied or parsed
by the planner. Import still validates the reachable content
under existing budgets, one resident file state at a time. Native exports, remote
current-permission checks, protected refs, project UI and connector recovery still
need integration. Offline same-file semantic convergence remains a separate
roadmap requirement.

Local roots represent the complete typed `DocumentState`, including any comments
and suggestions captured in it. Native/remote adoption must map content changes
through current permission and workflow rules; importing a local root must never
replace server-owned access, approval or audit records.
