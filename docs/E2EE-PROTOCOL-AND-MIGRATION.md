# End-to-end encryption: protocol, formats and migration

Status: design, not implemented. Date: 14 October 2026. Owner: Verne Ltd. Documentation only;
nothing here changes code, schema or API in the repository. All structures, tables and
endpoints are **proposed**. Migration numbers are written `00NN`: the
repository rule is that applied migrations are immutable and new work gets the next free
number when it is actually written (the working tree already uses 0041 for receipt-authority
epochs, so this work will be later).

Part of a set. Master design: [END-TO-END-ENCRYPTION.md](END-TO-END-ENCRYPTION.md).
Companions: [cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md),
[clients and key management](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md),
[threat model and assurance](E2EE-THREAT-MODEL-AND-ASSURANCE.md). Related and concurrent, not
authoritative here: [SELF-HOSTED-EDITION.md](SELF-HOSTED-EDITION.md),
[AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md).

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Purpose

Define, at field level, how the existing event, content, checkpoint, commit, receipt, review
and API formats change so that a project can be stored and exchanged as ciphertext, which
fields stay in the clear and why, how plaintext and encrypted projects coexist, how a project
moves in place between modes, how it rolls back, and what every server-side feature becomes.

### 1.2 Goals

| ID | Goal |
|---|---|
| P-G1 | Every change is additive and versioned. Existing plaintext projects, events, snapshots and commits keep working and keep verifying. |
| P-G2 | The service can still authorize, order, apply compare-and-swap, count approvals, enforce quotas, plan retention and erase, for an encrypted project, without any key. |
| P-G3 | A server that is wrong about plaintext cannot leak it: encrypted projects reject plaintext content at the storage boundary. |
| P-G4 | An encrypted project can be created, migrated in place, exported back and rolled back by owners without operator help or loss of provenance. |
| P-G5 | Each degraded feature has a stated client-side or reduced behaviour, not silent breakage. |

### 1.3 Non-goals

- Hiding structure (tree shape, `pos`, node types) from the service in Phase 1 (Phase 3 option).
- Server-side search, similarity detection, public rendering or conversion over encrypted
  content. They are removed or moved to clients, not emulated.
- Changing any existing hash, signature domain or applied migration.
- Real-time collaborative editing (the product has none today; decision 012 defers CRDT).

### 1.4 Personas and use cases

| Persona | Use case | What they need from the protocol |
|---|---|---|
| Project owner | Turn an existing project into an encrypted one, then add a collaborator at another institution | In-place migration; invite flow with pending grants |
| Editor | Edit and push; review a colleague's change | Same push, review and merge flow; diff computed locally |
| Reviewer | Approve a change request on protected `main` | Server still counts independent approvals without seeing content |
| Auditor (read-only member) | Verify that history was not altered | Chain, signature and receipt verification works over ciphertext with no key; content check with a read-only grant |
| Operator or administrator | Run backups, upgrade, erase on request | Operations that never need content; clear failure behaviour |

### 1.5 Success metrics

| Metric | Target |
|---|---|
| Plaintext-class fields persisted for an encrypted project (title, text, formatting, comments, names, messages) | 0, checked by an automated storage scan in CI |
| Server-side features that fail with a wrong or silent result on an encrypted project | 0 (each returns a typed `encrypted_project` error or documented reduced result) |
| Migration of a 1,000-node project, one file | Under 5 minutes on a typical laptop, resumable |
| Extra bytes per edit versus plaintext | Under 12 KiB for a one-paragraph edit including index objects (illustrative estimate in 7.8) |
| Backward compatibility | Old readers reject encrypted data fail-closed; plaintext readers unaffected |

## 2. Current state versus target

### 2.1 Where content and metadata are stored today

| Store | Table or object | Content-bearing columns | Evidence |
|---|---|---|---|
| Event log | `event` | `payload jsonb` (full semantic operation), `target_node_id`, `type` | `migrations/0002_events.sql`; `crates/engine-core/src/log.rs` |
| Materialized state | `node` | `current_fields jsonb` (text, formatting, expressions), `var_name`, GIN index `node_fields_gin` | `migrations/0001_core.sql` |
| Snapshots | `snapshot` | `state jsonb` (entire document state) | `migrations/0003_snapshots.sql` |
| Document | `document` | `title`, `settings jsonb` | `migrations/0001_core.sql` |
| Named versions | `document_version` | `name`, `note` | `migrations/0012_workspace.sql` |
| Drafts and change requests | `workspace_draft`, `draft_edit` | `name`, `base_state jsonb`, `submission_note`, `review_note`, `source jsonb`, `payload jsonb` | `migrations/0012_*`, `0020_*` |
| Review | `change_request_review` | `note` | `migrations/0021_*` |
| Original files | `document_upload` | `filename`, `content bytea` (up to 20 MiB by check), `report jsonb` | `migrations/0012_workspace.sql` |
| Shared objects | `checkpoint_object` | `bytes bytea` (typed JSON, chunks) | `migrations/0025_shared_checkpoints.sql` |
| Project history | `project_staged_commit`, `project_published_commit` | `body bytea` (signed commit with paths and message) | `migrations/0035_*`, `0036_*` |
| Provenance | `provenance_bundle` | `body jsonb` up to 16 MiB | `migrations/0023_provenance_sync.sql` |
| Issues | `project_issue`, `project_issue_comment`, `project_label` | `title`, `body`, `body`, `name`, `description` | `migrations/0039_project_issues.sql` |
| Derived indexes | `document_fingerprint`, `document_fingerprint_band`, `document_block_hash`, `document_match` | MinHash signature, band keys, block hashes, titles | `migrations/0021_*`; `crates/engine-api/src/copies.rs` |
| Public links | `public_document_view` | `published_title`, frozen `snapshot_id` | `migrations/0015_*` |
| Notifications | `notification` | `payload jsonb` identifiers and scores only | `migrations/0021_*`; `notifications.rs` |
| Audit | `operation_audit` | `detail jsonb` (addresses, branches, ids) | `migrations/0015_*` |
| Backups | `pg_dump -Fc` files | Everything above | `infra/demo/backup.sh` |
| Converter | transient process memory and temp files | Whole `.doc` file | `infra/converter/server.py` |

### 2.2 Capability table

| Capability | State | Evidence |
|---|---|---|
| Typed, chunked, content-addressed shared state with structural records separate from payloads (`Node` record with `fields` reference) | EXISTS | `crates/engine-core/src/shared_checkpoint/typed.rs`, `content.rs`; [TYPED-CHECKPOINTS.md](TYPED-CHECKPOINTS.md), [CHUNKED-CONTENT.md](CHUNKED-CONTENT.md) |
| Signed multi-file project commits v1 to v3, branch CAS, protected main, review counting, receipts | EXISTS (receipt authority not installed in production) | `repository.rs`, `engine-api/src/project_history.rs`, `project_staging.rs` |
| Client-side merge and convergence over typed states (CLI) | EXISTS | `repository/convergence.rs`, `project_merge.rs`; `engine-cli` |
| Client-side import parsing for DOCX, XLSX, PPTX in the web workspace | EXISTS | `frontend/researcher/src/lib/workspace/docx-import.ts`, `xlsx-import.ts`, `pptx-import.ts` and their workers |
| TypeScript port of the fold | EXISTS | `frontend/researcher/src/lib/materializer.ts` |
| Server-side three-way merge and diff for per-file drafts | EXISTS (server) | `engine-core/src/workspace_merge.rs`, `engine-api/src/workspace.rs` (`sync_draft`), `reviews.rs` |
| Server-side integrity gate for `Deployed` | EXISTS (server) | `engine-core/src/integrity.rs`, `governance.rs` |
| Server-side literal search | EXISTS (server) | `engine-api/src/workspace.rs` `search` (`strpos(lower(n.current_fields::text), ...)`, 2 s timeout, 50 rows) |
| Server-side copy detection | EXISTS (server) | `engine-api/src/copies.rs`, `engine-core/src/similarity.rs` |
| Server-side legacy `.doc` conversion | EXISTS (server) | `infra/converter/server.py` (LibreOffice headless, 3 MB cap) |
| Public links to frozen plaintext copies | EXISTS (server) | `public_document_view`, route `view/[token]` |
| Sealed container, typed codec v3, commit v4, key tables, API | MISSING | This document |
| Encryption mode flag per project | MISSING | `project_settings` has no such column |
| Storage-boundary check that rejects plaintext for an encrypted project | MISSING | `validate_stored` only checks hash and canonical form (`shared_checkpoint.rs:308`) |
| Server-independent verification of content (`verify`) | PARTIAL | Chain verification works over any payload; snapshot-versus-replay comparison needs plaintext (`snapshot_verification.rs`) |

## 3. Field classification: what stays in the clear and what is sealed

### 3.1 Principles

1. **Clear if the server needs it to be correct.** Authorization, ordering, compare-and-swap,
   reachability, quotas, reviewer independence, retention and erasure need identifiers,
   addresses, parent links, counters and timestamps.
2. **Clear if it is already metadata the product exposes without content.** Event types drive
   activity labels (`project_activity.rs::EVENT_LABEL`); notifications already carry only
   identifiers and scores.
3. **Sealed if it is author-supplied natural language, formatting, values, names or notes.**
4. **Sealed if it reveals content by hashing.** Rich-text `before` and `after` fingerprints,
   `var_name`, field names such as `relevance` or `constraint`.
5. **Absent if it is derived from plaintext on the server.** Search indexes, similarity
   signatures and previews are not computed for encrypted projects.
6. **A choice is shown to the owner, never silent.** The "sealed project name" option below is
   the one structural exception and is a project setting.

### 3.2 Per structure

| Structure | Field | Class | Reason |
|---|---|---|---|
| `event` | `id`, `document_id`, `seq`, `type`, `actor_id`, `target_node_id`, `content_hash`, `prev_chain_hash`, `chain_hash`, `created_at` | Clear | Chain verification, FK, activity, authorization |
| `event` | `payload` | Split: structural keys clear, rest sealed (table 4.4) | Server structural validation versus content |
| `node` | `id`, `document_id`, `parent_id`, `type`, `pos`, `deleted`, timestamps | Clear | Tree integrity checks (`ops.rs` `NodeExists`, `ParentNotFound`, `IllegalContainment`, `MoveCreatesCycle`, `NodeHasChildren`) |
| `node` | `current_fields` | Replaced by `{"$sealed":"<address>"}`; plaintext never stored | Content |
| `node` | `var_name` | NULL; value inside sealed fields | Semantic content; uniqueness becomes a client check |
| `snapshot` | `state` | Not written for encrypted documents; sealed checkpoint root instead | Plaintext JSON |
| `snapshot` | `merkle_root`, `event_chain_hash`, `through_seq` | Clear (Merkle over sealed object addresses) | Commitments |
| `document` | `title` | Placeholder `"Encrypted file"`; real title sealed (kind 5) in `settings` | Content-bearing |
| `document` | `settings` | Sealed except `kind` and `encryption` | Free-form |
| `document_version` | `name`, `note` | Sealed (kind 5); `snapshot_id`, `created_by`, `created_at` clear | Content |
| `workspace_draft` | `name`, `submission_note`, `review_note`, `source` | Sealed; ids, `base_seq`, state flags, timestamps clear | Content |
| `workspace_draft` | `base_state` | Replaced by sealed state root reference | Plaintext JSON |
| `draft_edit` | `payload` | Sealed event-style envelope; `actor_id`, time clear | Content |
| `change_request_review` | `note` | Sealed; `verdict`, `node_id`, reviewer clear | Content |
| `document_upload` | `filename`, `report`, `content` | `filename` and `report` sealed; `content` stored as sealed asset chunks; byte size clear | Original files |
| `checkpoint_object` | `bytes` | Structural typed v3 JSON (clear) or `DYNOSEAL` containers | Format change |
| Project commit | `project`, `parents`, `author_key`, `authored_at_ms`, `version`, `recovery`, `same_file_resolutions`, state roots | Clear | Ancestry, signatures, receipts |
| Project commit | `message` | Sealed envelope inline | Content |
| Project commit | file path (map key) | Map key becomes the document UUID; path map sealed inline | Content |
| `project_ref_event`, `project_history_review` | all | Clear | Addresses and counters only |
| `project_issue` | `number`, `state`, `author_id`, `assignee_id`, times, `revision`, `comment_count` | Clear | Workflow and quotas |
| `project_issue` | `title`, `body`; `project_issue_comment.body`; `project_label.name`, `description` | Sealed with `K_meta`; `color` clear | Content |
| `notification` | `payload` | Unchanged (already identifiers only) | See 11.1 |
| `operation_audit` | `action`, `actor_id`, `resource_id`, `created_at`, `detail` | Clear; `detail` must not carry sealed-class text (existing rule in `project_staging.rs`: "No filenames or messages in the audit record") | Audit |
| `document_fingerprint*`, `document_block_hash`, `document_match` | all | Not computed | Derived from plaintext |
| `public_document_view` | all | Not creatable for encrypted documents | Needs server plaintext |
| `provenance_bundle` | `body` | Sealed (kind 7); header (ids, base, counts, host, mode) clear | Content |
| `access_space.name` (project or folder) | name | Clear by default, optional sealed project name | Invitations and navigation need it; owner can choose sealed |
| `identity.email`, `display_name` | | Clear (account metadata, outside this design) | Authentication |

### 3.3 Optional sealed project name

Organisation, team and folder names stay clear because invitation emails, breadcrumb and
permission screens are built server-side. A project owner can set `sealed_name = true`: the
server stores the placeholder `"Encrypted project"` in `access_space.name` and the real name as
a kind 5 object. Invitation emails then contain the inviter's display name and no project
name. This costs email usefulness and is therefore a choice.

## 4. Format changes

### 4.1 Sealed object (new)

Container, algorithms and AAD are specified in the
[cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md#8-sealing-authenticated-encryption-with-bound-context).
Storage integration:

- `shared_checkpoint::validate_stored` gains a branch on the magic bytes `DYNOSEAL`, next to
  the existing `DYNOCNT\0` (content) and `DYNOTYP\0\x02` (typed) branches. The new validator
  checks magic, version, suite, kind, flags, size bound, strictly ascending unique `refs`, and
  that the supplied address equals `SHA-256("dynodoc.sealed-object\0v1\0" || bytes)`. It never
  decrypts.
- `shared_checkpoint::references` returns the header `refs`, so retention planning
  (`repository/local/retention.rs`), transfer inventory and erasure keep working.
- The existing size bound `MAX_OBJECT_BYTES` (1 MiB) and table check
  `octet_length(bytes) between 1 and 1048576` apply unchanged.

### 4.2 Typed checkpoint codec v3

Same envelope as v2 (`DYNOTYP\0` + canonical JSON) with a different version byte, so v2 readers
reject it.

| Change | v2 | v3 |
|---|---|---|
| Magic | `DYNOTYP\0\x02` | `DYNOTYP\0\x03` |
| Hash domain | `dynodoc.typed-checkpoint\0v2\0` | `dynodoc.typed-checkpoint\0v3\0` |
| `node` kind | `id, parent_id, node_type, pos, var_name, deleted, fields` | Adds `rev` (u64, revision along a branch); `var_name` must be null; `fields` must reference a `DYNOSEAL` object of kind 1 |
| `comment` kind | `node_id`, `body: Text` | `body` is a reference to a sealed object (kind 3) |
| `suggestion` kind | `target_node_id, detail, accepted, rejected_reason` | `detail` and `rejected_reason` reference sealed objects (kinds 4, 3) |
| `scalar`, `text`, `array`, `dictionary` kinds | Plaintext JSON values and content trees | Not permitted below a sealed boundary; they only appear in plaintext projects |
| `asset` | Content root of plaintext bytes | Reference to a root whose chunks are sealed kind 6 |
| `state` | Four roots | Same four roots plus the sealed boundary flag `sealed: true` and `epoch_floor` (lowest epoch present) |
| Reader mode | Full reconstruction into `DocumentState` | Two modes: `read_structure` (server: verifies graph, hashes, structure; does not interpret sealed leaves) and `read_state` (client: with keys) |

The `nodes` map stays the v1 hash-routed radix map (leaf up to 32 entries,
`shared_checkpoint.rs` `LEAF_ENTRIES`), so an edit rewrites one leaf, its branches and the
state object. `var_name` moves inside sealed fields, so the unique-name index
`node_varname_idx` is empty for encrypted documents.

### 4.3 Commit v4

Canonical JSON, sorted keys, `deny_unknown_fields`, same envelope as v1 to v3 in
`crates/engine-core/src/repository.rs`. Differences:

| Field | v1 to v3 | v4 |
|---|---|---|
| `version` | 1, 2 or 3 | 4 |
| Address domain | `dynodoc.project-commit\0vN\0` | `dynodoc.project-commit\0v4\0` |
| Signature domain | `dynodoc.project-commit-signature\0vN\0` | `dynodoc.project-commit-signature\0v4\0` |
| `files` keys | Logical paths, validated by `valid_path` | Canonical lowercase hyphenated document UUID, must equal `FileEntry.document` |
| `message` | Plaintext, at most 4096 bytes | Must be the empty string; sealed message in `sealed.message` |
| `recovery`, `same_file_resolutions` | v2 and v3 exclusive matrix `(1,None,false)`, `(2,Some,false)`, `(3,None,true)` | Allowed together; invariants kept: recovery implies exactly one parent; resolutions imply exactly two parents |
| `sealed` (new, required) | | `{ "epoch": u32, "message": <base64url envelope kind 5>, "paths": <base64url envelope kind 5>, "membership_head": <64 hex> }` |
| Limits | 4096 files, 1 MiB canonical | At most 2048 files (path map is inline) and 1 MiB; raised when path maps become indexed objects (OQ-PR-3) |

Example (abridged; line breaks added):

```json
{
  "authored_at_ms": 1791043200000,
  "author_key": "ab12c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90",
  "files": {
    "3f2b8c1e-9d4a-4e7b-a2c6-5d1f0a9b7c33": {
      "document": "3f2b8c1e-9d4a-4e7b-a2c6-5d1f0a9b7c33",
      "state": "2b6c0f4d5e7a91c3d8e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0"
    }
  },
  "message": "",
  "parents": ["c9a1...e402"],
  "project": "7c1e5a52-0b7a-4c2e-9d0a-4e1b0f7b2a10",
  "sealed": {
    "epoch": 4,
    "membership_head": "9f21...c004",
    "message": "RFlOT1NFQUwBAQUAAAAABAhUKM2_Eg-J...",
    "paths": "RFlOT1NFQUwBAQUAAAAABAhUKM2_Eg-J..."
  },
  "version": 4
}
```

The server validates v4 structurally: the version matrix, key equals document UUID, empty
message, `sealed.epoch` not greater than the project's current epoch, `membership_head` is a
known entry of the project's log, envelopes parse as sealed containers of kind 5. It never
decrypts them.

### 4.4 Centralized event payload (per-file path)

For encrypted documents that still use the centralized per-document log (drafts, change
requests, add-in bundles), an event's `payload` is

```json
{ "v": 1, "clear": { ...structural keys... }, "sealed": "<base64url DYNOSEAL kind 8>" }
```

`event.type` and `event.target_node_id` columns stay as today. `content_hash` is computed by
the same function over the canonical JSON of this wrapper, so `verify_chain` is unchanged.

| Event type | `clear` | `sealed` (kind 8 plaintext) |
|---|---|---|
| `NodeCreated` | `node_id`, `node_type`, `parent_id`, `pos` | `fields`, `var_name` |
| `NodeMoved` | `node_id`, `new_parent_id`, `new_pos` | none |
| `NodeDeleted`, `NodeRestored` | `node_id` | none |
| `FieldEdited` | `node_id` | `field`, `value` |
| `RichTextPatched` | `node_id` | `patch` (including `before`, `after` fingerprints) |
| `ChoiceAdded` | `node_id`, `choice_id` | `fields` |
| `ChoiceRemoved` | `node_id`, `choice_id` | none |
| `CommentAdded` | `node_id` (anchor, optional) | `body` |
| `CommentReplied` | `thread_id` | `body` |
| `CommentResolved` | `thread_id`, `resolved` | none |
| `SuggestionProposed` | `suggestion_id`, `target_node_id` | `detail` |
| `SuggestionAccepted` | `suggestion_id` | none |
| `SuggestionRejected` | `suggestion_id` | `reason` |
| `Deployed` | `snapshot_id` | none |

Server validation that survives (structural, from `ops.rs::validate_op`): node exists or
absent, parent exists and live, containment by `node_type`, no move cycle, no deleting a node
with live children, restore requires a live parent, suggestion exists and is pending, thread
identifiers are well formed. Validation that moves to clients: rich-text base checks
(`richtext::apply_patch`), duplicate `var_name`, comment length and emptiness, wrapped-operation
validity inside suggestions, and the referential-integrity gate. Consequence: the server cannot
prevent a malformed sealed payload; clients treat an event they cannot apply as an error
event and surface it (threat model, "malicious member").

### 4.5 New records

The epoch record, key grant and membership log entry are defined in the
[cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md#6-key-epochs-and-rotation). The service
stores them verbatim, verifies signatures and authorization, and serves them in order.

### 4.6 Unchanged records

Ref receipts (`repository/attestation.rs`), review rows, device enrolment proofs, project
device sessions and per-project key grants are unchanged. Reviews and receipts refer only to
commit addresses. The `history:review` scope and the independent-approval rule continue to
work because they never needed content.

### 4.7 Versioning and capability negotiation

| Mechanism | Rule |
|---|---|
| Project mode | `project_settings.encryption_mode` in `none`, `migrating`, `sealed` (section 5) |
| Server capability | `GET /capabilities` (new, unauthenticated) returns `{"e2ee":{"formats":["seal-1","typed-3","commit-4"],"suites":[1]}}`; clients refuse to create or open encrypted projects on a server that does not list them |
| Client capability | Every API request that writes sealed data carries header `X-Dynodoc-Formats: seal-1,typed-3,commit-4`; the server rejects with `426` if it does not support them |
| Old readers | Reject `DYNOTYP\0\x03`, commit version 4 and `DYNOSEAL` with the existing "invalid" errors; they never display partial plaintext |
| Mode enforcement | In an encrypted project the server rejects any `checkpoint_object` that is not `DYNOSEAL` or typed v3 structural, any v1 to v3 commit, and any plaintext event payload; in a plaintext project it rejects `DYNOSEAL` and v4 (prevents mixed-mode confusion) |
| Downgrade | There is no automatic downgrade; section 10 describes explicit declassification |

## 5. Database changes (proposed SQL)

Sketch of one additive migration (placeholder name `00NN_e2ee_foundations.sql`). It follows
the conventions of `0033_device_signing_keys.sql`, `0036_shared_project_history.sql` and
`0040_device_key_project_grants.sql`: append-only tables use the existing `event_immutable()`
trigger, state changes that must happen exactly once are guarded by a trigger, the runtime
login gets an explicit allowlist (see [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md)), and
nothing existing is rewritten.

```sql
-- PROPOSED. NOT CREATED. Placeholder number; applied migrations are immutable.

-- 1. Mode and counters. project_settings is the existing mutable per-project row.
alter table project_settings
    add column encryption_mode text not null default 'none'
        check (encryption_mode in ('none', 'migrating', 'sealed')),
    add column current_epoch integer not null default 0
        check (current_epoch between 0 and 1000000),
    add column sealed_name boolean not null default false,
    add column escrow_required boolean not null default false,
    add column agent_delegation text not null default 'none'
        check (agent_delegation in ('none', 'local_only', 'export_allowed'));

-- Per-document mode (a project can be mid-migration with some files sealed).
alter table document
    add column encryption text not null default 'none'
        check (encryption in ('none', 'sealed'));

-- 2. Device encryption keys, bound to an enrolled signing key (schema 33 table).
create table device_encryption_key (
    id             uuid primary key default gen_random_uuid(),
    signing_key_id uuid not null references device_signing_key(id),
    identity_id    uuid not null references identity(id),
    public_key     text not null unique check (public_key ~ '^[0-9a-f]{64}$'),
    -- canonical binding message and Ed25519 signature (crypto design, section 5.2)
    binding        jsonb not null check (octet_length(binding::text) <= 2048),
    registered_at  timestamptz not null default now()
);
create index device_encryption_key_identity
    on device_encryption_key(identity_id, registered_at, id);
-- A key whose signing key appears in device_key_revocation is not offered as a recipient;
-- rows are retained so old grants stay explainable.

-- 3. Signed, hash-chained membership log (append-only).
create table project_e2ee_log (
    project_id    uuid not null references project_settings(space_id),
    index         bigint not null check (index >= 0),
    entry_hash    text not null check (entry_hash ~ '^[0-9a-f]{64}$'),
    previous_hash text check (previous_hash ~ '^[0-9a-f]{64}$'),
    entry_type    text not null check (entry_type in (
        'genesis', 'member_add', 'member_remove', 'role_change', 'device_add',
        'device_revoke', 'recovery_set', 'escrow_set', 'escrow_clear',
        'escrow_backfill', 'escrow_recover', 'epoch_open', 'policy_change',
        'agent_grant', 'agent_revoke', 'migration_begin', 'migration_complete',
        'reseal_live_state', 'owner_transfer')),
    body          bytea not null check (octet_length(body) between 1 and 16384),
    signer_key    text not null check (signer_key ~ '^[0-9a-f]{64}$'),
    created_at    timestamptz not null default now(),
    primary key (project_id, index),
    unique (project_id, entry_hash),
    check ((index = 0) = (previous_hash is null))
);

-- 4. Epoch records (append-only, one per epoch number).
create table project_key_epoch (
    project_id      uuid not null references project_settings(space_id),
    epoch           integer not null check (epoch >= 1),
    record_hash     text not null check (record_hash ~ '^[0-9a-f]{64}$'),
    record          bytea not null check (octet_length(record) <= 8192),
    signature       text not null check (signature ~ '^[0-9a-f]{128}$'),
    created_by_key  text not null check (created_by_key ~ '^[0-9a-f]{64}$'),
    log_index       bigint not null,
    created_at      timestamptz not null default now(),
    primary key (project_id, epoch),
    foreign key (project_id, log_index) references project_e2ee_log(project_id, index)
);

-- 5. Key grants. A row is written once; the only permitted change is superseded_at
--    (null to a time, once), the same discipline as device_key_project_grant.revoked_at.
create table project_key_grant (
    id               bigint generated always as identity primary key,
    project_id       uuid not null,
    epoch            integer not null,
    scope_document   uuid references document(id),      -- null means project scope
    recipient_kind   text not null check (recipient_kind in
                       ('device', 'recovery', 'escrow', 'agent', 'invitee')),
    recipient_key    text not null check (recipient_key ~ '^[0-9a-f]{64}$'),
    recipient_device uuid references device_encryption_key(id),
    grant_body       bytea not null check (octet_length(grant_body) between 1 and 4096),
    created_by_key   text not null check (created_by_key ~ '^[0-9a-f]{64}$'),
    created_at       timestamptz not null default now(),
    superseded_at    timestamptz,
    foreign key (project_id, epoch) references project_key_epoch(project_id, epoch)
);
create unique index project_key_grant_active
    on project_key_grant(project_id, epoch,
        coalesce(scope_document, '00000000-0000-0000-0000-000000000000'::uuid), recipient_key)
    where superseded_at is null;
create index project_key_grant_recipient
    on project_key_grant(recipient_key, project_id, epoch);

-- 6. Storage-boundary guard: a sealed document accepts only sealed or structural objects.
create function checkpoint_object_mode_guard() returns trigger
language plpgsql set search_path = pg_catalog, public as $$
begin
    if (select encryption from public.document where id = new.document_id) = 'sealed'
       and substring(new.bytes from 1 for 8) <> '\x44594e4f5345414c'::bytea        -- DYNOSEAL
       and substring(new.bytes from 1 for 9) <> '\x44594e4f5459500003'::bytea       -- DYNOTYP\0\x03
       and substring(new.bytes from 1 for 8) <> '\x44594e4f434e5400'::bytea       -- DYNOCNT\0 (sealed chunks)
    then
        raise exception 'sealed document accepts only sealed or structural objects'
            using errcode = '23514';
    end if;
    return new;
end $$;
create trigger checkpoint_object_mode before insert on checkpoint_object
    for each row execute function checkpoint_object_mode_guard();

-- 7. Immutability, as in 0033 and 0036.
do $$ declare t text; begin
    foreach t in array array['device_encryption_key','project_e2ee_log','project_key_epoch'] loop
        execute format('create trigger immutable_update before update or delete on %I
                        for each row execute function event_immutable()', t);
        execute format('create trigger immutable_truncate before truncate on %I
                        for each statement execute function event_immutable()', t);
    end loop;
end $$;

revoke all on device_encryption_key, project_e2ee_log, project_key_epoch,
    project_key_grant from public;
```

Notes that the migration author must resolve (they are not decided here):

- **The guard literal for the typed v3 magic** above is a placeholder; write it from the
  constant `typed::MAGIC` of the implemented codec, not by hand.
- **Chunk objects.** Sealed asset chunks are `DYNOSEAL` kind 6, so `DYNOCNT\0` need not be
  allowed in sealed documents. The third clause exists only if content trees are retained for
  structural reasons; drop it if not.
- **Runtime privileges.** The three append-only tables join the SELECT and INSERT class;
  `project_key_grant` joins the SELECT, INSERT and UPDATE class with a trigger permitting one
  `superseded_at` change. `engine-cli grant-runtime` and `check-runtime`
  (`crates/engine-core/src/database.rs`) must list and count them or startup fails closed.
- **Erasure.** The audited document-erasure entry point must also remove grants whose
  `scope_document` is the erased document, in the same transaction and under the same receipt
  rule as `checkpoint_object` (`document_erasure_child_guard`).
- **Budgets.** Add a small budget table like `project_history_storage` for log, epoch and
  grant bytes (proposed default 16 MiB per project) so a member cannot fill the database with
  grants.

## 6. API (proposed)

All routes require the existing authentication (browser session or project device session).
Device-session scopes: reads need `history:read`; writes need `history:write`; opening an
epoch or changing membership additionally requires an owner or manager role, enforced
server-side and re-verified by clients from the signed log. New routes are listed in
`openapi.yaml` only when implemented.

| Method and path | Purpose | Scope and role | Notes |
|---|---|---|---|
| `GET /capabilities` | Server formats and suites | None | Used by clients to refuse unsupported servers |
| `POST /devices/{key_id}/encryption-key` | Register a device encryption key bound to an enrolled signing key | Browser session, own key | Verifies binding signature and that the signing key is active |
| `GET /projects/{id}/e2ee` | Mode, current epoch, policy flags, log head | Member | No secrets |
| `GET /projects/{id}/e2ee/directory` | Members, their active device encryption keys and fingerprints | Member | Clients verify against the signed log |
| `POST /projects/{id}/e2ee/begin` | Start enabling encryption (mode `migrating`) | Owner | Requires recovery acknowledgement; creates the genesis log entry |
| `GET /projects/{id}/e2ee/log?after={index}` | Membership log entries in order | Member | Paged, at most 256 |
| `POST /projects/{id}/e2ee/log` | Append a signed log entry | Role per entry type | Server checks signer is an authorized active device key, chain link and entry-type rules |
| `POST /projects/{id}/e2ee/epochs` | Open epoch `e+1` with its record and grants in one request | Owner or manager | Atomic; `409 epoch_conflict` unless `epoch = current + 1` |
| `GET /projects/{id}/e2ee/epochs?after={epoch}` | Epoch records | Member | |
| `GET /projects/{id}/e2ee/grants?recipient_key={hex}&epoch={e}` | Grants addressed to a key | Member (own keys only) | Returns sealed grants; server cannot read them |
| `POST /projects/{id}/e2ee/grants` | Add grants for later-added devices, invitees, agents or recovery | Member with grant authority | For existing epochs |
| `POST /projects/{id}/e2ee/migrations/{mid}/complete` | Finish an in-place migration | Owner | Switches to `sealed`, schedules plaintext erasure |
| `POST /projects/{id}/e2ee/declassify` | Begin the reverse path (section 10) | Owner | Audited; needs explicit confirmation field |

Existing routes change as follows: `POST /projects/{id}/staging/objects` and
`/history/commits` accept the new formats according to project mode;
`/documents/{id}/ops/*`, `/documents/{id}/batch`, `/documents/{id}/drafts/*/sync`, `/search`,
`/similarity/check` and
`/documents/{id}/versions` return `422 encrypted_project` (with a machine-readable `feature`
and `alternative` field) for sealed documents where the operation needs plaintext.

### 6.1 Examples

Register an encryption key:

```json
POST /devices/9c6b1d62-3a1b-4f7e-8d2a-0b3d6f1e7a55/encryption-key
{
  "public_key": "cf8e6a0b2d4c7f19e3a5b8d0c2f4a6b8d0e2f4a6c8e0a2b4d6f8a0c2e4a619a0",
  "binding": {
    "message": { "version": 1, "authority": "5d7f...", "identity": "2e8a...",
                 "signing_key": "ab12...", "encryption_key": "cf8e...", "created_at_ms": 1791043200000 },
    "signature": "6e02...c9b1"
  }
}
-> 201 { "id": "1d4e8f3a-6b2c-4a4d-8c10-3f7b9a2e5d11", "fingerprint": "40312 55987 ... (60 digits)" }
```

Open an epoch:

```json
POST /projects/7c1e5a52-.../e2ee/epochs
{
  "record":   { "version": 1, "project": "7c1e...", "epoch": 4, "reason": "member_removed",
                "suite": 1, "key_id": "085428cdbf120f89", "recipients_digest": "5b1c...",
                "membership_head": "9f21...", "history_policy": "full",
                "previous_epoch_record": "e0f3...", "created_by_key": "ab12...",
                "created_at_ms": 1791043200000 },
  "signature": "6e02...",
  "log_entry": { "type": "epoch_open", "body": { ... }, "signature": "..." },
  "grants": [ { "recipient_kind": "device", "recipient_device": "1d4e...", "scope": {"kind":"project"},
                "enc": "04d1...", "ciphertext": "9a77...", "signature": "..." } ]
}
-> 201 { "epoch": 4, "grants": 1, "log_index": 18 }
-> 409 { "error": "epoch_conflict", "current": 4 }
-> 422 { "error": "recipient_set_mismatch", "detail": "digest differs from supplied grants" }
```

Typed error for a feature that needs plaintext:

```json
422 { "error": "encrypted_project", "feature": "search", "alternative": "client_side_search" }
```

### 6.2 Server checks on `POST .../epochs`

1. Authenticate; take the account lock as in `devices::account_lock`; project role is owner
   or manager; the signing key is active and bound to the account (`devices::current_key`).
2. `record.epoch = current_epoch + 1`; `previous_epoch_record` matches the stored last hash;
   `membership_head` is a stored log entry hash.
3. Signature valid over the record domain; log entry chain link valid; signer authorized by
   the log (the server replays the same authorization rules clients will).
4. `recipients_digest` equals the digest of the supplied grants; every active device key of
   every current member (the server's view) has a grant, or the request states it as a
   pending invitee; no grant targets a revoked key or a non-member.
5. Insert record, log entry and grants in one transaction; update `current_epoch`; audit row
   `e2ee.epoch_opened` with epoch number and reason only.

The server is not the trust root for step 4: it can only reject incomplete sets it can see.
Clients verify the record against the signed log (a server could drop a member's grant
silently; the member's client then reports "no grant for this epoch").

## 7. Worked example: one edit, encrypted end to end

All values are synthetic. The sealed object is the vector of
[appendix A of the cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md#appendix-a-illustrative-sealed-object-vector-non-normative),
computed with WebCrypto on 14 October 2026. Other sizes are computed from the codec rules in
this document and are illustrative until implemented.

### 7.1 Scenario

Alice edits one paragraph in a file of an encrypted project and pushes it for review.

| Item | Value |
|---|---|
| Project | `a0a1a2a3-a4a5-a6a7-a8a9-aaabacadaeaf` |
| Document (file) | `b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf` |
| Node | `01J9ZK3Q8M2V7N4X5C6D7E8F9G`, a `paragraph`, parent `01J9ZK3Q8M2V7N4X5C6D7E8F9F`, `pos` `a2` |
| Epoch | 3; Alice's device holds the grant for epochs 1 to 3 |
| Node revision | 4 before, 5 after |
| Edit | Text "Revenue grew 4%." becomes "Revenue grew 5%." |
| What an editor emits today (plaintext, local only) | `RichTextPatched` with edits `retain 13, delete 1, insert "5", retain 2` and `before`/`after` SHA-256 fingerprints |

### 7.2 Steps and artefacts

1. **Client computes the new fields.** Canonical JSON of `current_fields` (113 bytes):
   `{"content":{"content":[{"text":"Revenue grew 5%.","type":"text"}],"type":"paragraph"},"label":"Revenue grew 5%."}`.
2. **Keyed identifier.** `cid = HMAC-SHA-256(K_cid_3, "dynodoc.cid.v1\0" || 0x01 || fields)` =
   `4113aa16...0fec` (32 bytes); placed inside the plaintext.
3. **Plaintext body.** `{"cid":"4113...","fields":{...},"v":1}` is 203 bytes; framed with a
   4-byte length and padded with zeros to the 256-byte bucket.
4. **Key block.** `K_doc_{3,d}` is derived from the epoch root; a fresh 32-byte salt feeds
   HKDF to produce a 32-byte AEAD key, 12-byte nonce and 32-byte commitment key.
5. **Seal.** AES-256-GCM over the 256-byte frame with the 175-byte AAD (project, document, node,
   kind 1, epoch 3, revision 5, suite 1, `refs_digest`). Output: 256 bytes ciphertext + 16 bytes
   tag.
6. **Object `a1` (sealed fields).** 346 bytes, address `89e8e004...c250`.
7. **Node record `a2` (typed v3, clear).** 260 bytes, address `24b1211a...cc4d`.
8. **Index path `a3`, `a4`, `a5`.** The hash-routed radix map changes one leaf (about 0.4 KB for
   2 to 3 entries), its level-1 branch (about 1.2 KB, 16 children) and the root branch (about
   1.2 KB).
9. **State object `a6`.** Four map roots and the sealed flag, about 0.4 KB.
10. **Commit v4.** Canonical JSON about 1.0 KB (abridged example in 4.3) including the sealed
    message envelope (about 230 bytes before base64) and the unchanged sealed path map
    envelope reused from the parent commit; Alice signs the address with her device key.
11. **Upload.** `POST /projects/{id}/staging/objects` with `{document, objects:[a1..a6]}`
    (base64, one request of about 6 KB under the 64 objects and 1 MiB limits); then
    `POST .../staging/commits` with the signed commit; then `POST .../history/commits`
    (publish) and `POST .../history/refs` (compare-and-swap to a branch or a change request).
12. **Server work.** Verifies each object's hash and header (no decryption), the commit
    signature and active key, project and file roles, dependencies and reachability, quotas,
    mode (rejects any plaintext), `membership_head` and epoch bounds, then records audit
    `project.commit_staged` and `project.commit_published` with the address only.
13. **Reviewer's client** (Bob): fetch commit, verify signature, resolve parent, fetch the
    six objects, open `a1` with his `K_doc_{3,d}` after checking commitment and AAD, compare
    with the parent's node locally, render the diff, record `approve` through
    `POST .../history/reviews` (candidate address only).

### 7.3 Byte-level field list: sealed object `a1`

| Offset | Bytes | Field | Hex or value |
|---:|---:|---|---|
| 0 | 8 | magic | `44594e4f5345414c` (`DYNOSEAL`) |
| 8 | 1 | format version | `01` |
| 9 | 1 | suite | `01` |
| 10 | 1 | kind | `01` (node fields) |
| 11 | 1 | flags | `00` |
| 12 | 4 | epoch | `00000003` |
| 16 | 8 | key id | `085428cdbf120f89` |
| 24 | 32 | salt | `c0c1c2c3c4c5c6c7c8c9cacbcccdcecfc0c1c2c3c4c5c6c7c8c9cacbcccdcecf` |
| 56 | 16 | key commitment | `d9d3eefc6923e82b1015ea358c1e2c84` |
| 72 | 2 | ref count | `0000` |
| 74 | 0 | refs | none |
| 74 | 256 | ciphertext | begins `67883ffe13ce3904ac25e05d103765df...` |
| 330 | 16 | GCM tag | `d95345f60fdf7c6c496ca640f1e68add` |
| | 346 | total | address `89e8e004de5459434f86d553f6064f1b2da33d49a2be9b982d64381cfd4ac250` |

AAD (175 bytes, not stored; rebuilt by every reader), each field preceded by its 8-byte
big-endian length except the two fixed-width integers (epoch and revision):

| Order | Length | Content |
|---:|---:|---|
| 1 | 15 | `dynodoc.seal.v1` |
| 2 | 16 | project UUID bytes `a0a1a2a3a4a5a6a7a8a9aaabacadaeaf` |
| 3 | 16 | document UUID bytes `b0b1b2b3b4b5b6b7b8b9babbbcbdbebf` |
| 4 | 26 | node ID `01J9ZK3Q8M2V7N4X5C6D7E8F9G` |
| 5 | 1 | kind `01` |
| 6 | 4 | epoch `00000003` (no length prefix) |
| 7 | 8 | revision `0000000000000005` (no length prefix) |
| 8 | 1 | suite `01` |
| 9 | 32 | `refs_digest` = SHA-256 of the empty string, `e3b0c442...b855` |

### 7.4 Byte-level field list: node record `a2` (typed v3, clear)

| Part | Value |
|---|---|
| Magic | `DYNOTYP\0\x03` (9 bytes: `44594e4f545950` `00` `03`) |
| Canonical JSON (251 bytes) | `{"kind":"node","value":{"deleted":false,"fields":"89e8e004...c250","id":"01J9ZK3Q8M2V7N4X5C6D7E8F9G","node_type":"paragraph","parent_id":"01J9ZK3Q8M2V7N4X5C6D7E8F9F","pos":"a2","rev":5,"var_name":null}}` |
| Total | 260 bytes |
| Address | `SHA-256("dynodoc.typed-checkpoint\0v3\0" || bytes)` = `24b1211a98777cd7ef3ff841e7b5b8568c8288b2b6cfd4a635423ca4a317cc4d` |

What the service sees in this record: a paragraph exists, where it sits in the tree, that its
revision is 5, and which opaque object holds its content.

### 7.5 What the service learns from this edit

| Learned | Not learned |
|---|---|
| Alice's account and device published a commit at time T | Any word of the paragraph or its formatting |
| One node (`01J9...9G`) of file `b0b1...` changed to revision 5 | That the change was "4" to "5", or its length beyond a 128/256 byte bucket |
| The size class of the changed object (256-byte bucket) | The commit message text (sealed) |
| Number and sizes of new index objects | The file's name |
| The reviewer's approval of a candidate address | Whether the reviewer read it |

### 7.6 The same edit on the centralized per-file path

For a draft in the web workspace the editor sends one event. The service stores
`event.type = "RichTextPatched"`, `target_node_id = 01J9...9G`, and
`payload = {"v":1,"clear":{"node_id":"01J9...9G"},"sealed":"<base64url kind 8 envelope>"}`.
The sealed plaintext is the `patch` object (edits, styles, `before`, `after`). The
`content_hash` and `chain_hash` are computed by `log::append_in_tx` over the canonical JSON of
that wrapper, so `verify_chain` still passes. The AAD uses the client event ULID and the base
sequence the client saw (cryptographic design, section 8.3).

### 7.7 Verification by a party without keys

An auditor holding no key can: verify every object address from its bytes; verify the commit
signature and that the signing key was enrolled and not revoked at that time (device-key
tables and receipts); verify the parent chain, branch receipts and review records; verify the
membership log chain and that the signer of the epoch record was an owner or manager; and
confirm that no plaintext class appears in the storage scan. They cannot read content or check
that it is well-formed.

### 7.8 Size estimate

| Item | Bytes (illustrative) |
|---|---:|
| Sealed fields object | 346 |
| Node record | 260 |
| Map leaf, level-1 branch, root branch | about 2,800 |
| State object | about 400 |
| Commit v4 body (sealed message and a reused path map envelope of about 3 KB for 20 files) | about 4,000 |
| **Stored for one edit** | **about 7.8 KB** |

For comparison the existing typed v2 write of the same edit decomposes the rich-text JSON into
a dictionary, array and scalar objects plus the same index path; a measurement against a real
plaintext push belongs in acceptance (section 12), not in this estimate.

## 8. Mixed plaintext and encrypted projects

### 8.1 Rules

| Rule | Statement |
|---|---|
| Unit of mode | The **project**. `project_settings.encryption_mode` is `none`, `migrating` or `sealed`. Documents carry `document.encryption` (`none` or `sealed`) so a project can be partly migrated, but only while `migrating`. |
| No mixed steady state | A `sealed` project has only sealed documents. A `none` project has only plaintext documents. The server enforces this at the storage boundary (section 5, guard trigger) and in the API (`422 mode_mismatch`). |
| Cross-project operations | Copying a file between an encrypted and a plaintext project is a client-side export and import. Encrypted to plaintext is a **declassification** and needs an explicit confirmation naming the destination project. |
| Organisation scope | A project's mode does not affect other projects of the same organisation or team. Organisation policy (section 8.3) can require or forbid encrypted mode for new projects. |
| Legacy history | A migrated project may contain archived plaintext documents until the owner erases them. They are labelled `legacy` in every view and are not covered by any confidentiality claim. |
| Capability gating | Any client without the formats of section 4.7 sees an encrypted project as "requires an updated client" and never as an empty or broken project. |

### 8.2 Why not per-file mixed mode

A per-file toggle looks attractive (keep a public README in plaintext) but fails on the
server features that make the product work: review counting, activity, notifications and
search would then behave differently inside one project, and the "what can the service see"
answer would depend on which file was clicked. Per-project mode keeps one honest sentence.
Public material belongs in a separate plaintext project or in an explicit "publish a plaintext
copy" action (section 11, row 15).

### 8.3 Organisation policy (proposed)

```yaml
# proposed organisation setting, stored as an audited row, visible to members
e2ee_policy:
  new_projects: optional        # forbidden | optional | required
  escrow: none                  # none | required | optional   (Phase 3)
  agent_delegation: local_only  # none | local_only | export_allowed
  history_policy_default: full  # full | from_join
  legacy_plaintext_max_days: 30 # after migration, legacy documents must be erased by then
```

`agent_delegation` governs whether members may delegate decryption keys to a local agent.
It is distinct from the per-file `agent_access` of the connector design
([AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md), concurrent and not authoritative), which
governs the hosted gateway and is expected to default to `none` for encrypted files.

Policy is advisory for clients that honour it and enforced by the service for the actions the
service performs (creation, mode changes, grants to escrow recipients). It cannot force a
client to encrypt correctly; the storage-boundary guard is the enforcement for "no plaintext".

## 9. Migrating a project in place

### 9.1 Constraint that shapes the design

Plaintext cannot be scrubbed from the existing history without erasing it:

- `event` rows are append-only; `UPDATE` and `DELETE` are forbidden by triggers
  (`migrations/0002_events.sql`) and `DELETE` is allowed only inside audited whole-document
  erasure (`document_erasure_child_guard`, `migrations/0017_administrator_erasure.sql`).
- `snapshot`, `document_version`, `draft_edit`, `provenance_bundle`, `checkpoint_object` and
  `project_published_object` are immutable in the same way.
- Hash chains and signatures commit to the plaintext bytes; "sealing in place" by rewriting
  rows would break every commitment the product exists to provide.

Therefore migration uses **successor documents**: new document identities carrying sealed
state, with a recorded lineage, and the legacy documents retained read-only until the owner
erases them through the audited path. Node IDs are preserved (stable identity is a rule of
the engine); the document UUID changes.

### 9.2 Migration modes

| Mode | What is sealed | History kept | Provenance effect | Use when |
|---|---|---|---|---|
| **M1 Seal from now (default)** | Current state of every file, and all later work | Legacy history stays plaintext and readable until erased | No signature is replaced; new commits parent the legacy head | Most projects; owner accepts that old history was already visible to the service |
| **M2 Re-seal retained history** | Current state plus selected historical states (named versions, last N commits, or all) | Sealed copies with lineage records; legacy then erased | Original signatures are replaced by the migrator's signature; the lineage record keeps the original commit addresses and verification evidence | The service must no longer hold the content of earlier drafts |
| **M3 New project and import** | Whatever the owner exports | None kept | New provenance root | Small or short-lived projects; cleanest |

All modes use the same client tooling. M2 is Phase 3 for projects with long histories because
re-sealing every commit is O(history); M1 and M3 ship first.

### 9.3 State machine

```
none --begin--> migrating --complete--> sealed
                   |  ^                    |
              abort|  |resume              | declassify
                   v  |                    v
                  none                  migrating(out) --complete--> none
```

`migrating` blocks new plaintext writes to the project's documents (`409 migrating`) after a
short grace period while allowing reads. Every step below is idempotent and resumable by
re-running the client command; the server records progress in a `project_e2ee_migration` row
(id, direction, step, started_by, started_at, last_error_code).

### 9.4 Procedure (M1)

| Step | Actor | Action | Server check |
|---:|---|---|---|
| 0 | Owner client | Preflight: list members and devices; flag members without a device encryption key (they will be "pending invitees"); require a recovery path (cryptographic design, 11.5); list open drafts and change requests | None |
| 1 | Owner | Resolve open change requests (merge, decline or withdraw). They are plaintext and cannot be carried | `begin` refuses while submitted requests exist unless `force_withdraw` |
| 2 | Owner client | `POST /e2ee/begin` | Role owner; project not already migrating; org policy allows |
| 3 | Owner client | Generate `PER_1`; build recipient set; sign epoch record; `POST /e2ee/epochs` | Section 6.2 checks |
| 4 | Owner client | For each file: read current plaintext state through the existing authorized API; create a successor document; seal nodes, comments and suggestions under epoch 1; write typed v3 state; record sealed lineage object | Document create is a normal project operation; storage guard applies |
| 5 | Owner client | Build commit v4 with parent = legacy head and file entries pointing at the successor documents; sign; stage; publish | Normal staging and publication checks; `membership_head` known |
| 6 | Reviewers | If `main` is protected, independent approvals are required as usual; temporarily lowering the floor is an audited policy change, not a migration privilege | Existing review rules |
| 7 | Owner client | Advance `main` (CAS) | Existing `advance` rules and receipts |
| 8 | Owner client | Verification: decrypt successor states and compare to the legacy states node by node; show a report | None (client) |
| 9 | Owner | `POST /e2ee/migrations/{id}/complete`: project becomes `sealed`; legacy documents become `archived` and read-only, flagged `legacy` | Owner role; all files have a published sealed successor |
| 10 | Owner | Erase legacy documents through the audited erasure path when satisfied, within `legacy_plaintext_max_days` if policy sets one | Existing erasure controls (see 9.6) |

### 9.5 Failure handling

| Failure | Effect | Recovery |
|---|---|---|
| Client crashes after step 3 | Epoch exists, no sealed content | Re-run; step 3 is a no-op because `epoch = current` |
| Network failure during step 4 | Some successor documents exist unpublished | Re-run; client reuses existing successors by lineage record or deletes unpublished ones (they are private staging) |
| A member's client cannot open the sealed state | The member sees "access pending"; others unaffected | Grant to a device they enrol; or revert the member's role |
| Owner abandons migration | Project stays `migrating` with writes blocked | `abort`: allowed before `complete`; legacy documents are untouched, successors are discarded |
| Server restored from an older backup mid-migration | Epoch or grants missing on the server | Devices that hold the epoch root re-publish the signed record and grants (idempotent, hash-addressed) |
| Verification (step 8) differs | Report lists nodes with different content | Do not complete; investigate; abort is safe |

### 9.6 Legacy erasure ownership (decision needed)

Today whole-document erasure is an administrator control (`/admin/erase`, `/admin/erase-batch`
in `crates/engine-api/src/admin_controls.rs`) with receipts, with reasons `owner_request`,
`duplicate`, `policy`, `other`. Migration needs the **owner** to trigger erasure of the legacy
documents of their own project after verification. Whether to expose an owner-initiated path,
a request that an administrator confirms, or leave it to the operator is a product and policy
decision (OQ-PR-4). Until it is made, the migration tool can only archive legacy documents,
and the claims policy forbids saying the service no longer holds the old content.

### 9.7 Invitees who have no device key yet

A member without a registered device encryption key cannot receive a grant. The owner client
creates `invitee` grants addressed to a one-time **invitation key** that the owner sends to
the member out of band (a short code or link fragment, never through the service). When the
member enrols a device, their client uses the invitation key to open the pending grant and
then issues itself a normal grant through a `device_add` entry. Until then they appear in the
member list as "pending key".

## 10. Rollback, declassification and downgrade

### 10.1 Three different rollbacks

| Kind | Meaning | How |
|---|---|---|
| Abort migration | Never completed | `abort` before step 9; legacy untouched |
| Declassify | Return an encrypted project to plaintext | Owner client decrypts the live state (and optionally named versions), uploads plaintext as new documents under mode `migrating(out)`, then `complete`; sealed documents are archived, keys stay with members. Requires an explicit confirmation naming the project; audit row `e2ee.declassified`; org policy can forbid |
| Software rollback | Deploy an older server build | Not by database downgrade. Keep a build that knows the schema and has `E2EE_ENABLED=false`: encrypted projects become **read-only ciphertext**, mode changes and sealed writes are refused. A build that does not know the new formats refuses to start against the schema (`check_schema` fails closed, as for earlier schema bumps) |

### 10.2 What rollback cannot do

- It cannot recall plaintext copies that left the service (exports, downloads, conversions).
- It cannot restore keys that no member holds.
- After legacy erasure, declassification works only from content that members can still
  decrypt.

### 10.3 Downgrade attacks

A malicious or buggy server could try to convince a client that an encrypted project is plain:
clients fetch `GET /projects/{id}/e2ee` and **pin** the mode in their local project record the
first time they see `sealed`; a later response of `none` is treated as a security alert, not
as a state change. The signed membership log (`migration_complete`, `owner_transfer`,
declassification entries) is the authority; a mode change without a matching signed entry is
rejected by clients.

## 11. Feature-by-feature matrix

"Today" cites the code. "Encrypted project" is the proposed behaviour. "Where" names the
runtime. Phases are those of the [master design](END-TO-END-ENCRYPTION.md): P1 encrypted
projects with command-line and basic web clients, P2 client-side engine in WebAssembly, P3
enterprise key management.

| # | Feature | Today (server-side plaintext?) | Encrypted project: behaviour | Where | Phase |
|---:|---|---|---|---|---|
| 1 | Edit a block in the web editor | Browser editor posts ops; server validates against materialized state (`ops/apply.rs`, `ops.rs`) | Browser decrypts, applies, seals ops (kind 8); server runs structural validation only | Client | P1 (basic), P2 (full) |
| 2 | Per-file drafts and autosave | `workspace_draft.base_state`, `draft_edit.payload` plaintext | Sealed base-state reference and sealed edit envelopes | Client | P1 |
| 3 | Merge a draft into the team version | `workspace_merge::merge` in `workspace.rs` `sync_draft` | `422 encrypted_project`; client merges and appends sealed events or a merge commit | Client (CLI P1, web P2) | P1 CLI, P2 web |
| 4 | Compare and diff views | Server builds structure and diff (`structure`, `reviews.rs`) | Client computes from decrypted states | Client | P1 CLI, P2 web |
| 5 | Review rendering of a change request | Server-rendered diff data | Client fetches both states and renders locally | Client | P2 |
| 6 | Required approvals, protected `main`, publisher-cannot-approve | `project_history.rs` `advance`, `review` | Unchanged; works on addresses and identities | Server | P1 |
| 7 | Branch CAS, receipts, pins | `project_history.rs`, `attestation.rs` | Unchanged | Server and client | P1 |
| 8 | Restore, revert, cherry-pick | `workspace.rs` `restore_version`; `repository/recovery.rs` | Client computes the new state; server validates file-map plan on addresses | Client | P1 CLI, P2 web |
| 9 | Named versions | `document_version` plaintext name and note | Sealed name and note; snapshot reference is a sealed checkpoint root | Client | P1 |
| 10 | Download as DOCX, XLSX, PPTX or original upload | Client export libs (`docx-export.ts` etc.) plus server `download_upload` | Client assembles from decrypted state; original upload fetched as sealed chunks | Client | P1 |
| 11 | Import DOCX, XLSX, PPTX | Client parse (`*-import.ts` workers), server stores original and `report` | Same parse; original and report uploaded sealed | Client | P1 |
| 12 | Legacy `.doc` conversion | LibreOffice converter service sees whole file (`infra/converter/server.py`) | Refused by default. Options: convert locally (CLI with local LibreOffice), convert in Word first, or an explicit per-file consent "send this file to the converter" that is recorded and downgrades that file's confidentiality; organisation policy can forbid | Client or consented server | P1 |
| 13 | Search | `/search` literal scan over `node.current_fields` (`workspace.rs`) | Disabled server-side. Client-side index over opened files, kept encrypted at rest on the device | Client | P2 |
| 14 | Copy and similarity detection | `copies.rs` fingerprints plaintext state | Not computed. Optional client-side check against locally unlocked files using the engine's `similarity` module | Client | P2 optional |
| 15 | Public links and `view/{token}` | `public_document_view` renders a frozen plaintext copy | Not available. Alternatives: (a) explicit "publish a plaintext copy" that creates a new plaintext public document, audited; (b) link with key in the fragment, viewer decrypts in the browser (Phase 3) | Client | P1 (a), P3 (b) |
| 16 | In-app notifications | Identifiers and scores only (`notifications.rs`) | Unchanged; titles resolved client-side after unlock | Server and client | P1 |
| 17 | Email notifications and invitations | Server-built messages with names | Generic text without titles or project names when `sealed_name` is set | Server | P1 |
| 18 | Admin overview, records, impact | Counts and metadata (`admin.rs`, `admin_controls.rs`) | Unchanged; operators cannot view content | Server | P1 |
| 19 | Audited erasure | Deletes rows and objects with receipts | Unchanged for ciphertext; plus deletion of grants; copies on member devices are outside it | Server | P1 |
| 20 | Comments and threads | `CommentAdded` body in event payload | Sealed bodies; anchor and thread identifiers clear | Client | P1 |
| 21 | Suggestions and accepting them | Server applies wrapped op and appends two events atomically (`governance.rs`) | Accepting client builds the canonical wrapped event and `SuggestionAccepted`, submits both in one batch | Client | P1 |
| 22 | Project issues | `project_issue` plaintext title and body | Sealed text; state, assignee, numbers clear; title filtering disabled | Client | P1 |
| 23 | Activity feed and contributors | Event-type labels, actor counts, document titles (`project_activity.rs`) | Labels unchanged (from event type); titles resolved client-side | Server and client | P1 |
| 24 | Verification (chain, snapshots, Merkle proof) | `audit.rs` `verify`, `snapshot_verification.rs` | Chain and object-graph verification server-side; semantic (state versus replay) verification in client with keys | Server and client | P1 |
| 25 | `Deployed` integrity gate (referential integrity) | `governance::authorize` runs `validate_integrity` | Server cannot run it; clients run it and refuse to produce `Deployed` events that fail; server cannot enforce this for a malicious member | Client | P1 (advisory), P2 |
| 26 | Spreadsheet formulas | Browser calculator (`workbook-calculator.ts`) | Unchanged (already client-side) | Client | P1 |
| 27 | Live updates (SSE) | `stream.rs` pushes event rows | Pushes ciphertext events; clients decrypt | Server and client | P1 |
| 28 | Office and Google add-ins (capture, preview, apply) | Server returns checkpoint text, accepts bundles (`provenance.rs`) | Add-in must hold keys; Office panes via WebCrypto after unlock (P2); Google sidebar (P2 or later); until then add-ins refuse encrypted projects with a clear message | Client | P2 |
| 29 | Public API, local MCP, agents | API reads plaintext | Only local, key-holding agents; cloud connectors cannot read (see [clients document](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md#8-local-mcp-and-ai-agents)) | Client | P1 CLI, P2 |
| 30 | Backups and restore | Plain `pg_dump` | Contain ciphertext; Phase 0 adds encrypted backups | Server | P0, P1 |
| 31 | Quotas, workload admission, rate limits | By bytes and counts | Unchanged; bytes are ciphertext sizes including padding | Server | P1 |
| 32 | Telemetry and operations status | No content | Unchanged | Server | P1 |
| 33 | Project policy (approvals, floors) and metadata | `project_settings`, `document_policy` | Unchanged; description sealed optionally | Server | P1 |
| 34 | Profile activity, stars, watch settings | Metadata only | Unchanged | Server | P1 |

## 12. Operations, compatibility and testing

### 12.1 Configuration (proposed)

| Setting | Values | Meaning |
|---|---|---|
| `E2EE_ENABLED` | `true`, `false` | `false`: encrypted projects are read-only ciphertext; creation and mode changes refused |
| `E2EE_ALLOW_CREATE` | `true`, `false` | Allow new encrypted projects (staged rollout) |
| `E2EE_MAX_FILES_PER_COMMIT` | integer, default 2048 | Limit for v4 commits (OQ-PR-3) |
| `E2EE_GRANT_BYTES_PER_PROJECT` | bytes, default 16 MiB | Budget for log, epochs and grants |
| `E2EE_REQUIRE_RECOVERY` | `true`, `false` | Require a recovery path before enabling |

### 12.2 Observability

Counts only, never content or keys: epochs opened and reasons; grants created, superseded and
unclaimed (devices that have not fetched a grant within N days); sealed-object validation
failures by reason; `422 encrypted_project` by feature (to find clients still calling removed
paths); storage-boundary guard rejections (a spike means a client or a server bug writing
plaintext); migrations by state and duration.

### 12.3 Compatibility rules

1. Old plaintext clients keep working against plaintext projects on a new server.
2. A new server serves old and new formats side by side; mode is per project.
3. Old clients never receive sealed bytes presented as plaintext; the API returns a typed
   error and the capability header makes it explicit.
4. No existing hash, signature domain, fixture or applied migration changes.

### 12.4 Test strategy

| Layer | Tests |
|---|---|
| Format | Golden vectors for typed v3 and commit v4 (independent producer); rejection tests: v2 reader on v3, v3 reader on tampered sealed leaf, unsorted refs, nonzero padding |
| Storage boundary | Property test: for a sealed document, any non-sealed object insert fails; scan job proves no plaintext-class column populated for an encrypted project |
| Authorization | Cross-member and cross-project grants; removed member cannot fetch grants for later epochs; device revoked mid-flow; account-lock races (reuse the schema 33 and 40 test patterns) |
| Adversarial server | A test harness server that reorders, drops, replays and substitutes objects, grants and log entries; clients must detect each |
| Migration | Resume at every step; abort; verification mismatch; backup restore mid-migration; large project (10,000 nodes) timing |
| Compatibility | Upgrade rehearsal on a production copy, as in the schema 38 to 40 release records |
| Feature degradation | Every row of section 11 has a test asserting either a correct client path or the typed error |

### 12.5 Acceptance criteria for Phase 1 protocol work

1. The storage scan reports zero plaintext-class values for a project created encrypted and
   exercised by the full end-to-end suite.
2. The adversarial-server harness passes with no undetected tampering case.
3. An upgrade rehearsal on a production copy leaves event and snapshot digests unchanged and
   plaintext projects unaffected.
4. Rollback to `E2EE_ENABLED=false` serves ciphertext read-only and loses no data.
5. Independent review of sections 3 to 6 has no open critical or high finding.

## 13. Delivery plan (protocol and server workstream)

| # | Task | Output | Person-weeks | Depends on |
|---|---|---|---:|---|
| S1 | Typed v3, sealed validator, commit v4 in `engine-core`, vectors | Engine crate changes behind a feature, tests | 8 | Crypto C1 to C3 |
| S2 | Migration SQL, triggers, privilege allowlist, erasure integration | `00NN` and rehearsal | 5 | S1 |
| S3 | API routes and checks with restricted-role tests; OpenAPI | Endpoints of section 6 | 8 | S2 |
| S4 | Mode enforcement and degradation matrix (422 paths, flags) | Section 11 behaviours | 6 | S3 |
| S5 | Migration tooling (M1, M3), abort, declassify, lineage records | CLI and API flows | 8 | S3, clients CLI |
| S6 | Capability negotiation, configuration, metrics, runbooks | Section 12 | 3 | S3 |
| S7 | Test suites: storage scan, adversarial server, upgrade and rollback rehearsal | CI jobs, rehearsal record | 6 | S1 to S5 |
| | **Total** | | **44** | |

Assumptions: two Rust engineers familiar with `engine-core` and `engine-api`, one reviewer from
the security engineer's time, existing CI with disposable PostgreSQL. Confidence: medium
(range 36 to 60 person-weeks). Excludes: cryptographic module (36 person-weeks in the
[cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md#16-delivery-plan-cryptography-workstream)),
all clients and UI ([clients document](E2EE-CLIENTS-AND-KEY-MANAGEMENT.md)), M2 history
re-sealing (about 10 additional person-weeks), external review, Office and Google add-ins,
performance tuning above 100 MB documents.

## 14. Risks and open questions

### 14.1 Risks

| # | Risk | Mitigation |
|---|---|---|
| PR1 | Plaintext leaks through a forgotten column or derived table | CI storage scan; guard trigger; checklist per migration |
| PR2 | Migration leaves legacy plaintext indefinitely and users believe they are protected | Claims policy; legacy labels; deadline setting; erase workflow |
| PR3 | Server-side validation gaps let a malicious member write malformed sealed events that break every client's fold | Clients isolate bad events (error event), fold continues; structural validation retained; review gates |
| PR4 | Commit size and file cap (inline path map) frustrate large projects | 2048-file limit, then indexed path map object (OQ-PR-3) |
| PR5 | Divergence between Rust and TypeScript codecs | Shared vectors; WASM build as the single implementation in Phase 2 |
| PR6 | Operators or support staff try to "fix" a stuck encrypted project by editing rows | Immutable triggers, runbook "never edit sealed tables", restricted runtime role |
| PR7 | Feature expectations (search, public links) not met | Matrix shown in sales and onboarding; typed errors with alternatives |

### 14.2 Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| OQ-PR-1 | Successor documents (new UUID) versus keeping the document UUID? | Successor documents, forced by immutable history (9.1) | Engineering lead |
| OQ-PR-2 | Make M1 the only Phase 1 migration mode? | Yes; M2 later | Product owner |
| OQ-PR-3 | Path map inline with a 2048-file cap, or indexed objects now? | Inline now; indexed with the commit-index work | Engineering lead |
| OQ-PR-4 | Who triggers erasure of legacy documents after migration? | Owner requests, administrator confirms, audited | Owner of Verne with counsel |
| OQ-PR-5 | Project names: sealed by default? | Clear by default, sealed option | Product owner |
| OQ-PR-6 | Keep `event.type` in the clear for activity labels? | Yes | Product owner |
| OQ-PR-7 | Enforce `Deployed` gate server-side via a client attestation? | No in Phase 1; advisory | Engineering lead |
| OQ-PR-8 | Should the server hold a copy of signed membership log heads in receipts? | Yes, embed log head in ref receipts when the authority is installed | Engineering lead |

## Appendix A: glossary

| Term | Meaning |
|---|---|
| Sealed object | Binary container with authenticated ciphertext and clear header (`DYNOSEAL`) |
| Structural object | Clear typed record or index node that the service can validate |
| Successor document | New document identity created by migration, with a sealed lineage record |
| Legacy document | Archived plaintext predecessor kept until erased |
| Declassify | Return an encrypted project to plaintext, deliberately and audited |
| Storage boundary | The place objects enter `checkpoint_object`; the guard trigger and validators sit here |

## Appendix B: references

| Reference | Location |
|---|---|
| Typed checkpoints, chunked content, portable history | [TYPED-CHECKPOINTS.md](TYPED-CHECKPOINTS.md), [CHUNKED-CONTENT.md](CHUNKED-CONTENT.md), [PORTABLE-PROJECT-HISTORY.md](PORTABLE-PROJECT-HISTORY.md) |
| Shared history, private transfers, receipts | [SHARED-PROJECT-HISTORY.md](SHARED-PROJECT-HISTORY.md), [PRIVATE-PROJECT-TRANSFERS.md](PRIVATE-PROJECT-TRANSFERS.md), [REF-RECEIPTS.md](REF-RECEIPTS.md) |
| Device keys, sessions, grants | [DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md), [PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md), [DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md) |
| Database privileges | [DATABASE-PRIVILEGES.md](DATABASE-PRIVILEGES.md) |
| Verified snapshots | [VERIFIED-SNAPSHOTS.md](VERIFIED-SNAPSHOTS.md) |
| External facts | Listed with dates in the [cryptographic design](E2EE-CRYPTOGRAPHIC-DESIGN.md#appendix-c-references) |
