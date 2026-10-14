# Portable project history

Schema 35 adds [authenticated private project transfers](PRIVATE-PROJECT-TRANSFERS.md)
around these local primitives. Schema 36 adds [shared publication, protected refs
and server receipts](SHARED-PROJECT-HISTORY.md). [Selected-history CLI push/fetch, fast-forward pull and remote tracking](SELECTED-HISTORY-SYNC.md)
now connect these contracts. Native/UI adoption remains separate work; the dated local implementation below
still applies.

12 October 2026. Source implementation in `engine_core::repository` introduces a
local SQLite repository alongside centralized document history. It does not yet
replace web drafts, turn connector queues into offline repositories, or by itself authorize
HTTP project ref updates. The separate schema-36 service provides that boundary. Project commits are a separate versioned contract;
legacy event sequences remain intact.

[Project commit recovery](PROJECT-COMMIT-RECOVERY.md) adds local revert/cherry-pick
with signed v2 source references and explicit conflict resolution. Ordinary v1
commits remain unchanged; upgraded readers accept both versions. Source references
are transfer dependencies, distinct from ancestry parents.
[Applying clean same-file merges](#applying-clean-same-file-merges) adds an opt-in
two-parent merge and commit format v3, with the same compatibility rule.

## Contract

A commit v1 binds a project UUID, zero to two ordered parent addresses, the full
path-to-document/state-root map, Ed25519 author public key, declared timestamp
in milliseconds and message. Stable document UUIDs persist across path changes.
File state roots use typed checkpoint v2. Parents preserve the first-parent
distinction. Paths are bounded relative logical names; duplicate document IDs and
file/directory collisions are rejected. This format currently caps projects at
4096 files and a canonical commit at 1 MiB. The file map is fully encoded per
commit; persistent file indexes are still an optimization to implement.

Commit addresses hash `dynodoc.project-commit\0v1\0` followed by sorted-key canonical
JSON. Signatures cover `dynodoc.project-commit-signature\0v1\0` followed by the
32 raw address bytes. All fields, including parent order, actor and timestamp,
are therefore bound. A valid signature proves possession of a key. Account
binding and permanent device revocation are now available through the separate
[enrollment protocol](DEVICE-SIGNING-KEYS.md). Remote publication must check that
binding and current project permissions atomically. Trustworthy time, issued server
receipts and external witnesses remain separate; a self-asserted key alone is not
a verified user identity. No existing database event hash is changed.

## Durable local operations

`LocalRepository::open(path, project)` creates/reopens SQLite schema 3 with WAL,
FULL synchronization and one-project binding. Object and commit rows are immutable
to ordinary writes; only the explicit collector removes unreferenced object rows,
and only `repository ... compact` changes how a row is stored (never its address or
bytes). Schemas 1 and 2 upgrade transactionally; a client that predates a schema
rejects it (a schema-2 client rejects 3, as schema-1-only clients rejected 2).
Object rows are compressed at rest from schema 3; hashes, addresses, transfer
packets and the quota counters are about the canonical encoded bytes and do not
change.
[Persistent local capacity](LOCAL-STORAGE-BUDGETS.md) bounds retained object/commit
bytes and object/commit/branch counts, including staged objects. Exact retries and
existing branch updates remain possible at capacity; no history is auto-pruned.
Writes and transfer batches are transactional. `write_state` verifies exact typed
reconstruction before committing objects. `import_commit` verifies its signature,
parent existence/scope, recovery dependencies/results and all reachable file state graphs under a shared object/
byte budget before storing it. It never changes a branch implicitly.

`compare_and_swap(branch, expected_head, new_commit)` publishes all project files
through one transaction. A head contains a generation as well as an address, so
an A → B → A update cannot make a stale comparison succeed. Historical commits
remain available after branch changes. Local refs may intentionally move to an old
commit; remote protected-branch policy is not implemented by this local method.

`merge_bases` walks bounded parent history and returns **all** best common
ancestors. Multiple bases and disconnected histories are explicit results. It
does not choose an arbitrary base or claim to resolve all concurrent rich-text
and structural edits. State reconstruction still uses bounded resident state.

## Exchange and trust boundaries

Inventory is keyset-paged (maximum 256 addresses), missing-object negotiation is
bounded, and object packets are individually hashed and document-scoped. Imports
accept at most 64 objects / 1 MiB atomically, tolerate reordering and are idempotent.
An interrupted sender can restart negotiation without replaying existing content.
Commit import waits for parents and all referenced content; refs publish last.
These are transport primitives, not authenticated remote routes. Inventory includes
staged/unreachable objects within the local project. A server must authorize each
document/project, enforce quotas and avoid exposing cross-tenant existence.

SQLite guarantees depend on the filesystem and hardware honoring synchronization.
Reopen, transaction rollback and retry tests are not an OS power-loss experiment.
Owner-level edits to the SQLite file are outside local trigger protection; hashes
and signatures are checked on reads, but detecting removal of the newest history
requires a previously retained head or independent witness. No key vault, native
background capture, credential storage or marketplace publication is introduced by
this module. The only deletion is the explicit, hash-bound
[staged-object collector](LOCAL-RETENTION-PLAN.md), which removes stored objects no
commit references; commits, signatures and refs are never pruned.

See [the evolution handoff](ENGINE-EVOLUTION-HANDOFF.md) for validation and the
remaining integration work. Installed-editor testing remains the owner's local
worksheet in the application repository.

## Local acceptance and CLI

Run `cargo run --locked -p engine-core --example local_repository_demo -- NEW_DIR`
with a new directory under an existing writable parent. The synthetic example
creates Alice and Bob databases, commits two project files, closes and reopens
both replicas, exchanges only missing objects, compares independent edits in both
orders, signs one accepted merge and publishes matching project heads. It also
recovers the initial report and verifies a repeated transfer sends zero objects.
The example's publicly known keys are test fixtures, never production identities.
The retained SQLite files allow inspection with the CLI.

`engine-cli repository DATABASE PROJECT_UUID --help` exposes initialization,
state storage/reconstruction, signed commit import/show, branch reads/CAS, merge
bases and project comparison. `commit MANIFEST --help` describes the PKCS#8 key
input (currently a positional path). Unencrypted DER Ed25519 PKCS#8 v1 and v2
are accepted, including standard OpenSSL v1 output; v2 component consistency
remains checked. Private keys are operator-supplied; the CLI
does not generate/store credentials or establish a trusted account/device binding.
Use an appropriately protected key source. State JSON input is capped at 16 MiB
for these local commands; this does not change the product's 3 MB native-upload cap.

Project comparison uses roots to combine unchanged/equal/independent file changes.
Competing edits, delete/edit, shared-document rename and path namespace conflicts
remain explicit. Same-file node/field convergence is an opt-in merge (below);
cross-file semantic validators, criss-cross virtual bases and native application
remain additional work. The planner never writes to a branch or native document on
its own.

## Same-file convergence report

`compare` (and `LocalRepository::same_file_convergence`) now adds a dry-run report
for conflicting paths that still name the same stable document UUID on the base,
left and right commits. It reads the three typed states and runs the existing
node/field three-way merge used for change requests, including rich-text run
merging. For each such file it reports whether the merge is clean, the typed-state
address the merged state **would** have, and node-level conflicts with stable node
ids, the field name and the base, left and right values. The path-level `files` and
`conflicts` output is unchanged; the extra `same_file_convergence` key appears only
when at least one conflict qualifies, and a comparison without one prints exactly
what it printed before.

Nothing is stored or signed and no branch moves. The address is computed without
writing; storing the merged state later gives the same root. Left and right can be
swapped: the merge runs in both orientations and is reported clean only when both
are, the address comes from one canonical orientation (the side with the smaller
state address is treated as the team side), and conflicts are the same list with
left and right values exchanged. Two replicas merging each other's work therefore
agree on the address, but where a tie-break exists (for example two different marks
added to the same words) the order of those marks is the canonical choice, not
either user's.

Limits. The report itself never writes or resolves anything; the opt-in merge
below applies exactly this computation. It is not
constraint-aware across documents (references between files, shared identity or
namespace rules) and does not understand native structures the typed state does not
represent. Nodes are compared by stable id; a node missing from one side's map is a
removal that wins only over an unchanged copy, otherwise a delete/edit conflict.
Comments and suggestions are compared as whole collections. Only documents whose
id is identical on all three commits are examined, at most 64 files per comparison
(the rest are counted as `not_examined`), each bounded by the ordinary state read
budgets. Criss-cross and disconnected histories are still refused before any file
is read; virtual merge bases remain future work.

## Applying clean same-file merges

`engine-cli repository DATABASE PROJECT_UUID merge LEFT RIGHT --key KEY.der
--message TEXT --authored-at-ms MS [--resolutions FILE] [--same-file-merge clean]`
(library `LocalRepository::prepare_merge`) signs and stores a two-parent merge
commit: `LEFT` is the first parent, the base is the unique best common ancestor, and
criss-cross or disconnected histories are refused exactly as for `compare`. It does
not move a branch; publish with `set-head` and the expected address and generation.
Before this command a merge was composed by hand as a manifest for `commit`; the
command computes the same file map, so without the flag the signed commit is
byte-identical to that manual manifest (version 1, parents `[LEFT, RIGHT]`).

Every path-level conflict needs an entry in `--resolutions`, a `FileEntry` or `null`
to delete, as in `recover`. Unknown paths are refused and no content is written
while a conflict is open (the error lists the paths).

With `--same-file-merge clean`, a conflicting path whose document UUID is identical
on base, left and right is read as three typed states and merged by the
convergence algorithm above (node/field, rich-text runs, comments and suggestions).
The path is merged only when that merge is **clean in both orientations**. The
merged state is stored through the ordinary budgeted `write_state` (before the
commit, staged exactly like `store-state` output if a later step fails), becomes the
file's entry, and is recorded in the commit. Swapping `LEFT` and `RIGHT` yields the
same merged state address (the commit addresses differ because parent order is
signed). Paths with any node or field conflict, paths whose document identity
changed, delete/edit and rename conflicts, and the paths beyond 64 per merge stay
conflicts that need an explicit resolution. An explicit resolution for a path always
wins over the automatic merge and leaves no record. The final file map is validated
as for any commit, so shared document identity and path namespace constraints still
apply. A merge with no automatic resolution is an ordinary v1 commit.

### Commit format v3

A merge with at least one automatic resolution is a version 3 commit: exactly two
parents, no `recovery`, and a non-empty `same_file_resolutions` array. Each record
is `{path, document, base, left, right, merged, strategy}`: the state roots at the
base and the first and second parent, the stored merged root (equal to the file's
entry) and the strategy id `dynodoc.same-file.node-field.v1`. Addresses are SHA-256
of `dynodoc.project-commit\0v3\0` followed by sorted-key canonical JSON, and
signatures cover `dynodoc.project-commit-signature\0v3\0` followed by the 32 raw
address bytes. Version 1 and 2 serializations never contain the new field, so no
existing address, signature or vector changes. `project-merge-v3.json` is an
independent Python canonicalization/hashing and OpenSSL Ed25519 vector (public
synthetic seed 7, placeholder dependencies); never regenerate it to accommodate a
failed check. Readers that predate v3 reject these commits (unknown version and
field): upgrade every participating reader and server before exchanging them.

Import checks that each record names the commit's real parents, the unique merge
base and the same document under that path, and rejects unknown strategy ids.
It does **not** re-run the merge, so importing never depends on the algorithm
staying unchanged. `verify-merge COMMIT` (`verify_same_file_resolutions`) reads the
recorded inputs and requires the named strategy to reproduce the recorded merged
address. A signature proves the author's key, not that the merge was automatic.

### Limits

No cross-file semantic validation, no virtual merge base, no three-or-more-parent
merge, and no native-file application. A record covers whole typed states: it does
not describe which individual nodes changed. Comments and suggestions are compared
as whole collections, and where two different marks are added to the same words the
mark order is the canonical choice, not either author's. Servers older than this
format cannot accept v3 commits; the PostgreSQL service schema is unchanged and
SQLite local schema 2 is unchanged.
