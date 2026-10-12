# Portable project history

Schema 35 adds [authenticated private project transfers](PRIVATE-PROJECT-TRANSFERS.md)
around these local primitives. Shared publication/protected refs and native
adoption remain separate work; the dated local implementation below still applies.

12 October 2026. Source implementation in `engine_core::repository` introduces a
local SQLite repository alongside centralized document history. It does not yet
replace web drafts, turn connector queues into offline repositories, or authorize
HTTP project ref updates. Project commits are a separate versioned contract;
legacy event sequences remain intact.

[Project commit recovery](PROJECT-COMMIT-RECOVERY.md) adds local revert/cherry-pick
with signed v2 source references and explicit conflict resolution. Ordinary v1
commits remain unchanged; upgraded readers accept both versions. Source references
are transfer dependencies, distinct from ancestry parents.

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

`LocalRepository::open(path, project)` creates/reopens SQLite schema 2 with WAL,
FULL synchronization and one-project binding. Object and commit rows are immutable.
Schema 1 upgrades transactionally; older schema-1-only clients reject schema 2.
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
requires a previously retained head or independent witness. No garbage collection,
key vault, native background capture, credential storage or marketplace publication
is introduced by this module.

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
remain explicit. Same-file rich-text resolution, cross-file semantic validators,
criss-cross virtual bases and native application remain additional work. The
planner never writes to a branch or native document on its own.
