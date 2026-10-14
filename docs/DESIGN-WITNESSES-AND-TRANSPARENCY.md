# Design: independent witnesses and a transparency log for signed heads

> **Status: design, not implemented.** 14 October 2026. Nothing here exists in
> source. It adds no migration, key, endpoint or file format to the current
> repository. Part of the [remaining-engine-designs index](ENGINE-REMAINING-DESIGNS.md).
> Item: `E-WITNESS`. Needs the receipt authority to be installed for its main leaf
> type ([decision](RECEIPT-AUTHORITY-DECISION.md)); event-chain anchoring
> (section 4.3) does not.

## 0. Summary

Dynodoc already produces signed branch-head receipts and lets clients pin them
([REF-RECEIPTS.md](REF-RECEIPTS.md), `crates/engine-core/src/repository/attestation.rs`,
`receipt_journal.rs`). Those receipts prove continuity of one branch *as seen by one
client*. They do not stop an operator who holds the signing key from showing two
clients two different histories, and they say nothing about legacy per-document
event chains, which have no external commitment at all
([VERIFIED-SNAPSHOTS.md](VERIFIED-SNAPSHOTS.md); [03-versioning-mathematics-vol2.md](03-versioning-mathematics-vol2.md)
E.6). The status ledger lists "external witnesses/gossip" as open
([ROADMAP-ACCEPTANCE.md](ROADMAP-ACCEPTANCE.md)).

This design adds:

1. an **append-only transparency log** per installation: a Merkle tree (RFC 6962
   construction) whose leaves are opaque commitments to receipts, authority
   rotations, history horizons and document chain heads;
2. **signed checkpoints** in the C2SP signed-note / `tlog-checkpoint` format, so
   existing witness software and libraries are usable;
3. **independent witnesses** that cosign a checkpoint only after verifying a
   consistency proof from the one they last signed (`tlog-witness` protocol);
4. a **client trust policy** (log key + witness keys + threshold) and a verifier
   that can state, for any receipt, which of four assurance levels it reaches;
5. **blinded leaf tags** so third-party witnesses learn nothing about projects,
   branches or documents, while project members (or auditors they authorize) can
   monitor completeness and detect equivocation;
6. scenarios for **self-hosted customers**: witnessing the hosted log, being
   witnessed by third parties, mutual witnessing between institutions, and
   air-gapped checkpoint exchange.

The value is conditional detection, not prevention. A log operator that collaborates
with a threshold of witnesses can still lie. The design makes that collusion
necessary and visible to monitors that look.

### 0.1 Order and dependencies

| Needs | Why |
|---|---|
| Receipt authority installed, or event-chain anchoring only | `ref-receipt` leaves need receipts; event-chain heads do not. |
| Nothing else mandatory | Pure addition: new tables, new endpoints, journal v2. |
| Benefits from | [commit pruning](DESIGN-COMMIT-PRUNING.md) (horizon leaves); [large-file acceptance](DESIGN-LARGE-FILE-ACCEPTANCE.md) is unrelated except for load. |

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Problem statement

1. A server with the receipt signing key can sign a fork. The current mitigation
   is the client's own pin; a client that never saw the other branch cannot know.
   REF-RECEIPTS.md says so: "A server with its signing key can still equivocate;
   isolated clients cannot discover a fork they have never observed."
2. A database administrator who rewrites an entire event chain and replaces its
   head cannot be detected without an independently retained head
   ([README](../README.md) "Tamper-evidence").
3. There is no way for a customer, auditor or partner institution to obtain
   evidence about a Dynodoc installation that does not depend on that
   installation's honesty.

### 1.2 Goals

| ID | Goal |
|---|---|
| G1 | Equivocation (two different histories for one scope) requires collusion of the log with a threshold of independent witnesses. |
| G2 | Rewrites and truncations of anchored history are detectable by anyone holding a previously cosigned checkpoint. |
| G3 | Third parties learn no project, document, branch, person or content information from published data. |
| G4 | Interoperate with the C2SP log ecosystem formats rather than invent new ones. |
| G5 | Work for the hosted service, self-hosted installations and air-gapped sites. |
| G6 | Degrade by policy when witnesses are unavailable: never block writes; verifiers choose fail-closed or warn. |
| G7 | Cover both receipts and legacy event-chain heads. |

### 1.3 Non-goals

- Preventing a privileged actor from deleting data. Witnesses make it visible.
- Trusted timestamping. Cosignature times are witness clocks, not a TSA.
- Proving authorship or that capture observed every edit.
- A public ledger or blockchain. No consensus, no tokens, no global ordering.
- Making content available or verifiable (data availability is separate).
- Replacing receipts. Receipts remain the per-scope continuity proof.

### 1.4 Personas

| Persona | Use case |
|---|---|
| Hosted-service customer admin | Wants evidence that Verne cannot rewrite their project history unnoticed. |
| Self-hosted university IT | Wants an off-site witness so a rogue DBA cannot rewrite; wants to witness a partner. |
| Independent witness operator (academic partner, auditor) | Runs a tiny cosigning service; wants low operating cost and no access to content. |
| Project member | Wants her client to say "this head is in a log cosigned by 2 of 3 witnesses". |
| Auditor | Wants to verify an exported project history against a checkpoint from last year. |

### 1.5 Success metrics

| Metric | Target (hypothesis) |
|---|---|
| Time from ref update to inclusion in a cosigned checkpoint | p95 under 60 s with three witnesses |
| Client verification cost per receipt (cached tiles) | under 5 ms CPU, under 2 KB transfer |
| Witness state per origin | under 1 KB (one checkpoint) |
| False alarms per 10^6 verifications in absence of attack | 0 |
| Detection in fault injection (fork, rollback, truncation) | 100 % of injected events |

## 2. Current state against target

| Capability | Tag | Evidence / path |
|---|---|---|
| Versioned Ed25519 branch receipts with `previous` chain | EXISTS | `crates/engine-core/src/repository/attestation.rs`; fixtures `ref-receipt-v1.json`, `ref-receipt-signatures-v1.json` |
| Client pins, contiguous proofs, dual-signed authority rotation, local revocation | EXISTS | `attestation.rs` (`Pin`, `SignedRotation`), `receipt_journal.rs` |
| Atomic server receipt issuance with protected refs | EXISTS | `crates/engine-api/src/project_history.rs`, `migrations/0036_shared_project_history.sql` (`project_ref_event`) |
| Operator-installed authority key, immutable binding | EXISTS | `crates/engine-core/src/repository/server_authority.rs`, `project_ref_authority` |
| Production authority installed | MISSING | [RECEIPT-AUTHORITY-DECISION.md](RECEIPT-AUTHORITY-DECISION.md) |
| Server-side authority rotation and compromise recovery | MISSING | same document, "Why not now" 2 |
| Selected-history sync verifying receipt chains and pinning remote heads | EXISTS | [SELECTED-HISTORY-SYNC.md](SELECTED-HISTORY-SYNC.md) |
| Per-document event hash chain and Merkle snapshot roots | EXISTS | `crates/engine-core/src/log.rs`, `snapshot.rs` |
| Any external anchoring of event-chain heads | MISSING | none |
| Merkle log over receipts, checkpoints, consistency proofs | MISSING | none |
| Witness protocol, witness service, monitor | MISSING | none |
| Client trust policy with quorum | MISSING | pin has one `trusted_key` |
| Gossip between clients or services | MISSING | none |

## 3. Architecture

### 3.1 Components

```
 +-------------------------------- installation (hosted or self-hosted) --------------------------------+
 |                                                                                                       |
 |  API (ref update, rotation, horizon, ...)                                                            |
 |      |   same DB transaction                                                                          |
 |      v                                                                                                |
 |  transparency_outbox  --(single leader worker)-->  Log: Merkle tree + tile store                     |
 |                                                      |                                                |
 |                                                      v   sign checkpoint with LOG KEY (separate)     |
 |                                              checkpoint (signed note)                                 |
 +------------------------------------------------------|------------------------------------------------+
                                                         | POST add-checkpoint (old size + consistency proof)
                         +-------------------------------+--------------------------+
                         v                               v                          v
                  Witness W1 (Verne,           Witness W2 (partner         Witness W3 (customer
                  separate infra)              institution)                 self-hosted)
                  persists last size+root      persists last size+root     persists last size+root
                         |  cosignature                  |                          |
                         +-------------------+-----------+--------------------------+
                                             v
                                  checkpoint + cosignatures (published at /checkpoint)
                                             |
        +------------------------------------+---------------------------------+
        v                                    v                                 v
  client verifier (CLI, connector)     member monitor (has k_project)    auditor / archive
  policy: log key + k-of-n witnesses   checks completeness, duplicates  keeps checkpoints
```

| Component | Responsibility | Holds secrets |
|---|---|---|
| Outbox | Durable queue of leaves written in the same transaction as the event that causes them | none |
| Log builder | Single-writer leader (PostgreSQL advisory lock); assigns indexes, builds tiles, produces checkpoints | log signing key (mounted separately) |
| Witness | Verifies consistency, stores one checkpoint per origin atomically, signs | witness key |
| Monitor | Reads leaves, checks that the scopes it can decode are complete and unforked | `k_project` (blinding key) for its projects |
| Client verifier | Applies the trust policy to a receipt + inclusion + checkpoint; remembers the latest verified checkpoint | none |

### 3.2 Trust boundaries

| Boundary | Trusted for |
|---|---|
| Log operator | availability and ordering; **not** for consistency |
| Log key holder | signing checkpoints; compromise is bounded by witnesses |
| Witness operator | refusing to cosign inconsistent checkpoints; persistence of its last state |
| Quorum of witnesses | at least one honest member intersects any two quorums if threshold is above half |
| Client filesystem | storing the latest verified checkpoint (existing limit for the journal) |
| Project members | holding `k_project`; leakage of `k_project` reveals only linkability inside the project's own leaves |

### 3.3 Why a separate log key

The receipt authority key signs branch receipts and is installed immutably. The log
key signs checkpoints at a different cadence and is exposed to a different worker.
Sharing keys would let a receipt-signing compromise also forge checkpoints, and a
checkpoint-signer bug would risk the receipt key. Distinct keys, distinct custody
(open question WT-Q3).

## 4. Specification (proposed)

### 4.1 Leaves

A leaf's data is canonical JSON (sorted keys, no whitespace, UTF-8, at most 256
bytes). The Merkle leaf hash is the RFC 6962 leaf hash
`SHA-256(0x00 || "dynodoc.transparency-leaf\0v1\0" || data)`; interior nodes use
prefix `0x01`.

| Type | Data | Blinding | Producer |
|---|---|---|---|
| `ref-receipt` | `{"t":"ref","tag":T,"receipt":ADDR}` | `T = HMAC-SHA256(k_project, "ref\0" || branch || "\0" || generation_u64be)` | ref update transaction |
| `authority-rotation` | `{"t":"rot","tag":T,"rotation":ADDR}` | `T = HMAC(k_installation, "rot")` | rotation endpoint (future) |
| `history-horizon` | `{"t":"hzn","tag":T,"horizon":ADDR,"seq":N}` | `T = HMAC(k_project, "hzn\0" || seq)` | pruning design |
| `event-heads` | `{"t":"evh","root":R,"count":N,"interval":I}` | none (a root) | anchor job (4.3) |
| `named-version` | `{"t":"ver","tag":T,"version":ADDR}` | `T = HMAC(k_project, "ver\0" || doc_uuid)` | named snapshot (optional) |
| `log-successor` | `{"t":"suc","prev_origin":O,"size":N,"root":R}` | none | log key rotation |

Properties:

- `receipt` is the SHA-256 address already defined for receipts
  (`dynodoc.ref-receipt\0v1\0` domain); clients that hold a receipt compute it.
- The tag makes two leaves for the same `(project, branch, generation)`
  recognizable **only** to holders of `k_project`. Two *different* receipts with the
  same tag are proof of equivocation.
- No leaf contains a project UUID, branch name, actor, commit address or path.

### 4.2 Tree, checkpoints and cosignatures

| Item | Specification |
|---|---|
| Tree | RFC 6962 Merkle tree hash over leaves in index order (RFC 9162 describes the same hash construction; it is Experimental, so only the construction is relied on) |
| Origin | `<host>/dynodoc/v1/transparency` (schema-less URL, recommended by the signed-note format); one per installation |
| Checkpoint body | three lines: origin; tree size in ASCII decimal; base64 root hash; extension lines are **not used** (the checkpoint format advises against them) |
| Log signature | Ed25519 signed note, key name = origin, key ID per signed-note (first 4 bytes of `SHA-256(name || 0x0A || 0x01 || pubkey)`) |
| Cosignature | `cosignature/v1` Ed25519 over `time`, origin, size, root (includes an 8-byte timestamp); each witness has a unique name and key |
| Serving | `GET /transparency/v1/{sha256_hex(origin)}/checkpoint`, tiles at `/tile/{L}/{N}[.p/{W}]`, entry bundles at `/tile/entries/{N}[.p/{W}]` (C2SP tlog-tiles layout: 256 hashes per tile) |
| Cadence | a checkpoint when 1,000 leaves are pending or 10 s elapsed, whichever first, never more often than 1 per second; an empty interval produces no checkpoint |

Witness protocol (C2SP `tlog-witness`, editor's copy read 14 October 2026): the log
sends `POST add-checkpoint` with `old <size>`, up to 63 consistency-proof hashes, a
blank line and the new checkpoint; the witness answers 200 with cosignature lines,
409 with its latest size on mismatch, 422 on a bad proof or root mismatch, 403 if no
trusted signature verifies. The witness must check and persist atomically.

### 4.3 Event-chain anchoring (works without receipts)

For each document with new events in the interval, the anchor job computes
`doc_tag = SHA-256("dynodoc.doc-tag\0v1\0" || installation_uuid || document_uuid)`
and the tuple `(doc_tag, seq, chain_hash)`. Tuples are sorted and Merkle-hashed to
`R`; one `event-heads` leaf commits to `R`. The tuple list is retained locally
(an operator can prove any document head against `R` with a two-level path).

| Property | Value |
|---|---|
| Cost | one leaf per interval regardless of document count; tuples stored in a PostgreSQL table |
| Interval | default 1 hour (open question WT-Q5) |
| Disclosure | `R` reveals nothing about documents |
| Verification | the operator exports `(document, seq, chain_hash, path)` for an auditor, who checks `chain_hash` against the chain via `engine-cli verify` and the path against a checkpoint |

This closes the "no independently retained trusted hash" gap noted in the README
for centralized documents.

### 4.4 Inclusion bundle

```json
{
  "version": 1,
  "leaf": { "t": "ref", "tag": "<hex>", "receipt": "<64 hex>" },
  "index": 48211,
  "size": 50000,
  "path": ["<hex>", "..."],
  "checkpoint": "<signed note text, with cosignatures>"
}
```

Verification: recompute the leaf hash, fold the path to the root for `(index,
size)`, compare to the checkpoint root, verify signatures per policy.

### 4.5 Trust policy

```json
{
  "version": 1,
  "log": { "origin": "app.dynodoc.online/dynodoc/v1/transparency",
           "vkey": "app.dynodoc.online/dynodoc/v1/transparency+1a2b3c4d+AQ..." },
  "witnesses": [
    { "name": "witness.verne.example/w1", "vkey": "...", "operator": "verne" },
    { "name": "witness.uni-a.example/w", "vkey": "...", "operator": "uni-a" },
    { "name": "witness.customer.example/w", "vkey": "...", "operator": "customer" }
  ],
  "quorum": { "threshold": 2, "distinct_operators": true },
  "freshness": { "max_age_seconds": 86400 },
  "on_failure": "warn"
}
```

`distinct_operators: true` counts at most one cosignature per `operator`, so one
organization with three keys is one vote. `on_failure` is `warn` or `reject` per
consumer (CLI scripts may choose `reject`; the editor connectors `warn`).

### 4.6 Assurance levels reported by the verifier

| Level | Meaning |
|---|---|
| L0 | receipt signature and chain valid against the pinned authority (today) |
| L1 | L0 + inclusion in a checkpoint signed by the log key |
| L2 | L1 + quorum of witness cosignatures within `max_age` |
| L3 | L2 + the checkpoint is consistent with the latest checkpoint this client previously verified (consistency proof) |

The verifier never says "verified" without a level. A client that has never seen a
checkpoint is at most L2 (bootstrap, as pins "trust a checkpoint, not unseen
history").

### 4.7 Storage (proposed)

PostgreSQL (migration number at implementation):

```sql
create table transparency_outbox (
    seq bigint generated always as identity primary key,
    leaf_type text not null,
    leaf bytea not null check (octet_length(leaf) between 1 and 256),
    created_at timestamptz not null default now(),
    included_at_size bigint
);
create table transparency_checkpoint (
    size bigint primary key, root bytea not null check (octet_length(root) = 32),
    note bytea not null, created_at timestamptz not null default now()
);
create table transparency_event_heads (
    interval_id bigint not null, doc_tag bytea not null, seq bigint not null,
    chain_hash bytea not null, primary key (interval_id, doc_tag)
);
-- tiles are derived data in object storage or a table; recomputable from leaves.
```

Rows are immutable (the existing `event_immutable()` trigger pattern). The runtime
login has INSERT on the outbox only; the log worker uses a separate role.

Client journal (SQLite `receipt_journal_meta.version` 1 to 2, additive):

```sql
create table checkpoints (origin text, size integer, root blob, note blob,
                          primary key (origin, size));         -- immutable
create table inclusions (receipt text primary key, origin text, size integer,
                         bundle blob);
create table witness_policies (scope text primary key, body blob);
```

### 4.8 Wire and CLI surface

```text
GET  /transparency/v1/{origin_hash}/checkpoint
GET  /transparency/v1/{origin_hash}/tile/{L}/{N}[.p/{W}]
GET  /transparency/v1/{origin_hash}/tile/entries/{N}[.p/{W}]
GET  /projects/{id}/history/receipts/{generation}/inclusion?branch=...   # bundle, history:read
GET  /projects/{id}/history/transparency-key                                 # k_project, history:read
POST {witness}/add-checkpoint                                               # C2SP tlog-witness
GET  {witness}/{sha256_hex(origin)}/checkpoint                              # witness monitoring
```

```text
engine-cli transparency policy init|show
engine-cli transparency verify-receipt RECEIPT.json --policy P.json --journal J.sqlite [--level L2]
engine-cli transparency fetch-checkpoint --policy P.json --journal J.sqlite
engine-cli transparency witness serve --state witness.sqlite --listen ADDR --log ORIGIN=VKEY ...
engine-cli transparency witness add-log ORIGIN VKEY
engine-cli transparency monitor run --project UUID --branch main --journal J.sqlite --policy P.json
engine-cli transparency export-checkpoint --out FILE      # for air-gapped exchange
engine-cli transparency import-checkpoint FILE --journal J.sqlite
```

`sync fetch` and `sync push` gain an optional `--transparency-policy` that adds the
inclusion fetch and level reporting; without it they behave exactly as today.

### 4.9 Algorithms

**Log builder (single leader).**

```
loop every tick:
  acquire advisory lock (leader); if not leader: sleep
  pending = select from outbox where included_at_size is null order by seq limit 1000
  if pending is empty and (now - last_checkpoint) < refresh_interval: continue
  tx begin
    for leaf in pending: append leaf hash at index = size; size += 1
    update outbox set included_at_size = size where seq in pending
    new_root = merkle_root(size)           # incremental: O(log n) new nodes + tile updates
    note = sign_checkpoint(origin, size, new_root, LOG_KEY)
    insert into transparency_checkpoint(size, root, note)
  tx commit
  publish tiles and checkpoint (object store or table)
  for each witness w in config (concurrently, per-witness timeout):
      submit(w, old = w.last_size, proof = consistency_proof(w.last_size, size), note)
      on 200: store cosignature; on 409: set w.last_size from body, retry once
      on 422/403: raise alarm; stop submitting to w
  assemble note with log signature + collected cosignatures; make it the served checkpoint
```

The transaction around outbox and checkpoint insertion makes leaf inclusion
atomic with the size it implies; a crash between commit and publication is
repaired on restart by republishing the latest `transparency_checkpoint` row.

**Witness (per request).**

```
on add-checkpoint(origin, old, proof[], note):
  require origin is configured; else 404
  verify at least one trusted log signature on note; else 403
  parse size, root from note body
  begin immediate (single-row lock on witness_state where origin = $origin)
    cur = select size, root from witness_state
    if old != cur.size:              respond 409 with cur.size
    if size < old:                   respond 400
    if size == old:  require root == cur.root; else 422
    else:            require verify_consistency(old, cur.root, size, root, proof); else 422
    update witness_state set size = $size, root = $root
  commit
  respond 200 with cosignature line (timestamp = now, Ed25519 over cosignature/v1 message)
```

The check and the update are one transaction (the protocol requires atomicity;
otherwise a race can roll back a newer checkpoint). The state per origin is two
fields; losing it is a security event (section 9, T5).

**Client verification (`verify-receipt`).**

```
input: receipt R, policy P, journal J, optional inclusion bundle B
1  verify R signature and chain per existing Pin rules                      -> level L0
2  if B absent: fetch bundle (index, size, path, checkpoint) for R's leaf
3  leaf = canonical leaf for R (tag from k_project if known, else from bundle)
   fold path; compare with checkpoint root                                -> else FAIL
4  verify log signature on checkpoint with P.log.vkey                       -> level L1
5  count distinct-operator valid cosignatures within P.freshness            -> level L2 if >= threshold
6  cp_prev = J.latest_checkpoint(origin)
   if cp_prev exists and cp_prev.size <= size: verify consistency(cp_prev, checkpoint)
        (fetch proof from log or witness)                                   -> level L3
   if cp_prev.size > size: verify consistency(checkpoint, cp_prev) (rollback check)
7  persist checkpoint (immutable) and inclusion bundle in J
output: level and reasons; non-zero exit when level < --level
```

**Member monitor (`monitor run`).**

```
state: last_size, per (branch) last_generation, k_project
each interval:
  cp = fetch latest quorum checkpoint; verify consistency with last (else ALERT equivocation/rewrite)
  for new leaves in [last_size, cp.size):
     if leaf.type == ref: index by tag
  for each branch of interest and each generation g > last_generation[branch]:
     tag = HMAC(k_project, "ref\0" || branch || "\0" || g)
     found = leaves with tag
     if found is empty:       warn "missing" (may be pending; escalate after 2 intervals)
     if |found| > 1 and receipts differ:   ALERT equivocation (publish proof: both receipts + leaf indexes)
     if found == 1:           fetch receipt from API; check address equals leaf.receipt; check previous chain
  last_size = cp.size
```

A fork proof is two signed receipts with equal `(scope, generation)` and different
addresses, each with an inclusion proof under checkpoints that are mutually
consistent. It is self-contained and verifiable by anyone with the receipts.

### 4.10 Changes to the existing sync flows

| Flow | Change |
|---|---|
| `sync fetch` | after verifying the receipt chain (unchanged), if `--transparency-policy` is set, fetch inclusion for the head receipt and report the level; with `--require-level L2` refuse to update the tracked pin below that level |
| `sync push` | after the server returns a signed receipt, request its inclusion bundle (may be pending: retry later, never block the push result) and store it in the journal |
| `receipt` CLI | unchanged offline verbs; new `transparency` verb group works from the journal |
| Receipt API | additive `inclusion` field or the separate inclusion endpoint; old clients ignore it |

### 4.11 Journal v2 migration

The receipt journal refuses non-journal databases and has `version = 1`. Version 2
adds tables only (`checkpoints`, `inclusions`, `witness_policies`); existing rows
and triggers are unchanged. A v2 journal is rejected by v1 binaries by the
existing version check (`receipt journal version`), exactly as local repository
schema bumps behave. No journal data is rewritten.

## 5. Gossip and consistency

### 5.1 Why witnesses prevent split views

A witness stores one checkpoint per origin and refuses any new checkpoint that is
not consistent with it. A log therefore cannot make one witness sign two inconsistent
trees. If clients require cosignatures from `k` of `n` witnesses with `2k > n`, any
two accepted checkpoints were cosigned by quorums that share at least one witness; an
honest shared witness cannot have signed both unless one extends the other. Thus a
fork shown to two clients that both require quorum is impossible without a
**dishonest or compromised witness in the intersection**.

Limits: the argument assumes honest witnesses persist state atomically; assumes
clients are not eclipsed (see 5.3); and says nothing about *stale* views, which are
handled by freshness (5.2).

### 5.2 Freshness

Cosignatures carry the witness's timestamp. A verifier rejects (or warns) when the
newest quorum checkpoint it can obtain is older than `max_age_seconds`. Without this
a log could freeze a client on an old, valid checkpoint (a rollback by omission).

### 5.3 Client-carried gossip

Clients are the gossip channel:

1. Every API response that relates to history includes an `X-Dynodoc-Checkpoint`
   header carrying the server's latest checkpoint size and root (small, cacheable).
2. A client that has a *newer* verified checkpoint from another source (a witness or
   another installation component) sends it with requests as
   `X-Dynodoc-Seen-Checkpoint`; the server replies 409 with the consistency proof
   between the two if it can, or an `inconsistent` signal that the client logs and
   surfaces.
3. Monitors and the CLI periodically ask witnesses directly (monitoring endpoint)
   and compare with the log's checkpoint.

This yields "gossip by use" without an always-on peer network.

### 5.4 Eclipse resistance

A client whose every network path is controlled by the attacker can be shown a
consistent but false world. Mitigations: policy lists witnesses on distinct
operators and networks; the client fetches witness checkpoints directly (not via the
log); optional out-of-band exchange (QR/file) of checkpoints between members;
documented as a residual risk.

## 6. Self-hosted and cross-organization scenarios

| ID | Scenario | Configuration |
|---|---|---|
| S1 | Self-hosted customer's own log witnessed by independent parties | the installation's log submits to a Verne-run witness (service), a partner witness and a customer-run witness on a different host; clients use quorum 2-of-3 |
| S2 | Self-hosted customer **witnesses the hosted service's log** (against a third party) | the customer runs `witness serve` with `--log app.dynodoc.online/...=VKEY`; Verne adds the customer's witness URL and key to its published witness list; the customer's clients include that witness in policy; their project owners run a monitor with `k_project` |
| S3 | Mutual witnessing between institutions | each institution runs one witness for each peer's log; policy lists three peers; no content ever leaves an institution |
| S4 | Air-gapped installation | log + one local witness (different host/operator within the institution); checkpoints exported on removable media to an off-site witness that cosigns offline; imported cosignatures extend assurance |
| S5 | Customer wants to verify the hosted service never rewrote their project | monitor with `k_project`: confirms every receipt it downloaded is a leaf, generations are gapless, no duplicate tag with a different receipt, checkpoints extend |

For S2 the customer's witness learns: log size and root over time, nothing else.
It cannot read leaves' meaning (tags are blinded for everyone without
`k_project`). For S5 the customer's monitor holds `k_project`; the key is
delivered over the authenticated API to a `history:read` principal and may be
shared with an external auditor by the project owner.

### 6.1 Independence guidance for witness operators

| Property | Guidance |
|---|---|
| Operators | at least two organizations other than the log operator in any quorum used for claims |
| Infrastructure | different cloud account and region; no shared database with the log |
| Keys | Ed25519, stored outside the application database; backup in escrow |
| State | transactional single-row store; restoring an old backup of witness state is an incident (it could cosign a fork) |
| Software | the reference implementation is small and auditable; interoperating third-party witness implementations are encouraged |

## 7. Privacy of what is published

| Datum | Published | Who can read | Leak |
|---|---|---|---|
| Checkpoint (origin, size, root) | yes | anyone with the URL (or only witnesses, for private logs) | total activity count over time |
| Leaf bytes | in entry bundles | same | tags and receipt hashes only |
| Witness keys, cosignature times | yes | anyone | witness clocks |
| Project UUIDs, branch names, document UUIDs, actors, commit addresses, paths | **no** | n/a | none |
| `k_project` | to authenticated project readers via API | members, auditors they choose | enables linking that project's leaves |

Mitigations: optional dummy leaves and checkpoint batching to blur activity timing
(off by default; costs storage); private logs served only to witnesses and members
(mTLS or bearer) for deployments that cannot publish even sizes.

**Erasure.** Leaves cannot be removed. They hold no personal data: a keyed-hash tag
and a receipt address. After an audited project erasure the operator may destroy
`k_project`; the tags then cannot be linked to any scope by anyone, including the
operator. Whether that satisfies a particular erasure obligation is a legal
question (open question WT-Q7); the design provides the mechanism and says what
remains (opaque hashes and counts).

## 8. Relation to receipts, rotation and horizons

| Topic | Interaction |
|---|---|
| Receipts | A receipt stays the unit of per-scope continuity. The log adds *global* consistency and a place where equivocation becomes evidence. Receipt format is unchanged. |
| Rotation | A dual-signed authority rotation (existing `SignedRotation`) is published as a leaf; verifiers that see a rotation also need its inclusion. Log-key rotation uses a `log-successor` leaf and a new origin version; witnesses are updated by an explicit operator act, because C2SP witnesses bind origin and key. |
| Revocation distribution | A `revocation-notice` leaf type (reserved) lets monitors learn about key revocation without polling the API. The bootstrap of trust still needs an independent channel. |
| Horizons (pruning) | Each horizon record is a leaf; truncation outside a recorded horizon is detectable relative to a pinned checkpoint. See [DESIGN-COMMIT-PRUNING.md](DESIGN-COMMIT-PRUNING.md). |
| Decisions | Without an installed authority, only `event-heads` leaves can be produced: the log can run before receipts do. |

## 9. Threat model

| # | Threat | Control | Residual |
|---|---|---|---|
| T1 | Log operator rewrites or truncates history | witnesses refuse inconsistent checkpoints (409/422); clients keep latest checkpoint; monitors compare | collusion with a quorum |
| T2 | Equivocation: different histories to different clients | quorum intersection (5.1); duplicate tags with different receipts are public evidence to `k_project` holders | colluding witness in the intersection; eclipse |
| T3 | Log key compromise | attacker can sign checkpoints but cannot get witness cosignatures for an inconsistent tree; rotate via successor | attacker extends the log consistently with fake leaves: detected only by monitors that notice unknown leaves (member monitors see receipts they never received) |
| T4 | Witness compromise (one) | threshold; distinct operators | none up to `n - k` witnesses |
| T5 | Witness state rollback (backup restore) | witness operators monitor own size; persisted atomically; documented incident; clients' own latest checkpoint exposes lower sizes | operator error |
| T6 | Freeze/rollback by omission | freshness policy | clients with `warn` |
| T7 | Information leakage via leaves | blinded tags; keyed hash; no identifiers | timing/volume |
| T8 | Guessing tags | HMAC-SHA256 with a 256-bit project key | key disclosure |
| T9 | Cross-protocol signature confusion | distinct key; distinct domain strings (`dynodoc.ref-receipt-signature\0v1\0` for receipts; signed-note text for checkpoints) | none |
| T10 | DoS on witnesses | log never blocks on witnesses; rate-limited submission; witnesses accept only known origins | availability of cosignatures |
| T11 | Leader worker bug duplicates or drops leaves | outbox sequence + `included_at_size`; reconciliation job; consistency check before signing | |
| T12 | Replay of old checkpoint to a new client | freshness; consistency to any prior checkpoint | first-contact client |

### 9.1 Not claimed

That anything is prevented; that timestamps are trustworthy; that witnesses are
independent unless the operator proves it; that monitors exist for any given
project; that availability of the underlying data is guaranteed; that the log
proves human authorship.

## 10. Operations

| Topic | Design |
|---|---|
| Keys | log key and witness keys are Ed25519 PKCS#8 files mounted read-only, distinct from the receipt authority key; two offline copies with a named custodian (as the authority checklist requires) |
| Deployment | outbox + leader worker inside the engine API deployment (separate role); tile store on the same host initially, object storage later; witness service as a separate small binary |
| Config | `DYNODOC_TRANSPARENCY_LOG_KEY_FILE`, `DYNODOC_TRANSPARENCY_ORIGIN`, `DYNODOC_TRANSPARENCY_WITNESSES` (list of URL + vkey), cadence parameters |
| Upgrade | additive tables; journals upgrade v1 to v2; clients without the feature ignore headers |
| Rollback | disable the worker; outbox accumulates; no effect on writes; checkpoint publication pauses (freshness warnings appear) |
| Observability | outbox lag, checkpoint age, witness response codes and latency, consistency failures (must be 0), cosignatures per checkpoint, monitor findings |
| Runbooks | witness added/removed; log key rotation; witness state loss; suspected equivocation (freeze, preserve checkpoints, notify) |

### 10.1 Failure modes

| Failure | Behaviour |
|---|---|
| Witness unreachable | checkpoint published with fewer cosignatures; clients at L1 or warn |
| All witnesses down | log continues; verifiers use `on_failure` policy |
| Witness returns 409 | log fetches its size and retries with a proof from that size |
| Witness returns 422 | alarm; stop submitting to it; investigate (log bug or attack) |
| Log restart | outbox resumes from last checkpoint size |
| Clock skew on witness | cosignature times outside `max_age` window; operator alert |
| Journal lost on a client | bootstrap again; earlier consistency evidence is lost |

### 10.2 Test strategy

| Layer | Tests |
|---|---|
| Vectors | fixed leaves, tree sizes 1..64, expected roots, inclusion paths, consistency proofs; a signed-note checkpoint with a log signature and two cosignatures; produced by an independent implementation (not the Rust code under test) and never regenerated to satisfy a failing check |
| Property tests | for random sizes `n <= 10^6`: inclusion proof verifies for every index; consistency proof verifies for every `(m, n)`; mutation of any hash, index or size fails; tile reconstruction equals incremental computation |
| Protocol | witness state machine: 200, 400, 403, 404, 409, 422 paths; concurrent submissions (exactly one wins; no rollback of a newer checkpoint); restart persistence |
| Client | assurance levels L0-L3 with each failure injected; clock skew on cosignature times; policy with duplicate operators counted once |
| Monitor | equivocation (two receipts, one tag) detected; missing leaf escalation; no false alarm when leaves are merely pending |
| Fault injection | log rewrites a leaf; log truncates; log serves two roots to two clients; witness restored from backup; witness clock stops; leader crash between outbox commit and checkpoint publication; two leaders (advisory lock failure) |
| Interoperability | at least one independent witness implementation and one independent verifier library cosign/verify a Dynodoc checkpoint (if available at build time) |
| Privacy | scan published artifacts (leaves, checkpoints, logs) for project/document UUIDs, branch names and commit addresses: zero matches over a synthetic workload |
| Soak | 72 hours at 10 updates/s with three witnesses; memory and outbox lag stable |

## 11. Delivery plan

### 11.1 Phases

| Phase | Deliverable | Depends on | Effort (person-weeks) | Confidence |
|---|---|---|---|---|
| W0 | Spec: leaf encodings, tag construction, checkpoint and cosignature formats, vectors cross-checked with an independent implementation | none | 1.5 - 2 | high |
| W1 | Log core in `engine-core` (RFC 6962 tree, tiles, inclusion/consistency), PostgreSQL outbox and worker, checkpoint signing, HTTP endpoints | W0 | 4 - 5 | medium |
| W2 | Client verifier, trust policy, journal v2, CLI verbs, `sync` integration at levels L0-L3 | W0 | 3 - 4 | medium |
| W3 | Reference witness service and log-side submission client; interoperability test with a third-party witness | W1 | 2 - 3 | medium |
| W4 | Member monitor and `k_project` distribution; duplicate-tag detection | W1, W2 | 2 - 3 | medium |
| W5 | Event-chain anchoring job and export for auditors | W1 | 2 - 3 | medium |
| W6 | Operations: key custody, deployment, runbooks, dashboards, soak | W1-W3 | 2 - 3 | medium |
| W7 | Fault injection: fork, rollback, truncation, witness restore, clock skew; documentation | W1-W5 | 3 | medium |
| Total | | | 19.5 - 26 | |

**Assumptions**: one engineer; reuse of well-tested Merkle code paths and the
signed-note format; the receipt authority decision resolved (or event-chain only
first). **Excludes**: HSM/KMS integration; running third-party witnesses (people and
contracts); legal review of publication and erasure; a public status page; UI
beyond CLI and a read-only assurance badge in Projects; formal protocol
verification; multi-region log replication.

### 11.2 Acceptance

| Phase | Acceptance |
|---|---|
| W0 | Independent implementation reproduces leaf hashes, root, checkpoint text and cosignature verification for fixed vectors |
| W1 | Property tests: inclusion and consistency proofs verify for random sizes up to 10^6; restart safety; leader failover without gaps |
| W2 | A forked receipt without inclusion is rejected at L1; rollback detected at L3 in tests |
| W3 | Interoperates with at least one non-Dynodoc witness implementation (if available at build time); 409 and 422 paths exercised |
| W4 | Injected duplicate-tag equivocation is reported by a monitor within one checkpoint interval |
| W5 | Auditor workflow: export tuple + path; offline verification against an old cosigned checkpoint |
| W7 | Detection rate 100 % across the fault matrix |

### 11.3 Rollout

1. Event-chain anchoring and log with **one** Verne witness on separate
   infrastructure (assurance L2 with a threshold of 1, labelled as such).
2. Publish policy and vkeys on the website; add a second independent witness
   (partner) before making any quorum claim.
3. Enable receipt leaves when the authority is installed.
4. Offer customer-run witness (S2) and self-hosted configurations.
5. Adopt in connectors as read-only badge; no write-path dependency.

### 11.4 Migration

None for existing data. Anchoring starts from the first checkpoint; earlier history
is covered only from then on ("trust a checkpoint, not unseen history", as pins).

## 12. Risks and open questions

### 12.1 Risks

| Risk | Mitigation |
|---|---|
| Complexity out of proportion to pilot-scale usage | phase W0-W2 first; hosted single witness; reuse standard formats |
| False sense of security with one witness | policy requires distinct operators for claims; UI shows L-level |
| Witness operators drop out | at least three, documented replacement procedure |
| Metadata leakage deters customers | private log option; dummy leaves |
| Standards churn (C2SP revisions, cosignature algorithms) | pin versions in vectors; algorithm agility field in policy |

### 12.2 Open questions

| # | Question | Recommended default | Decided by |
|---|---|---|---|
| WT-Q1 | Public log or private-to-witnesses by default? | private tiles for hosted service, public checkpoint | Product owner |
| WT-Q2 | Ed25519 cosignatures only, or also ML-DSA-44 (the witness spec recommends it for new deployments)? | Ed25519 first; track the spec | Security lead |
| WT-Q3 | Log key custody: same offline custodian as receipt authority? | separate key, same custodian process | Security lead |
| WT-Q4 | Do we offer a hosted witness as a paid service? | one free Verne witness, others partner-operated | Product owner |
| WT-Q5 | Event-head anchoring interval | 1 hour | Engineering lead |
| WT-Q6 | Distribute `k_project` over the API or derive from a project secret in the future E2EE design | API for now | Security lead |
| WT-Q7 | Is destroying `k_project` an acceptable erasure measure for leaves? | present mechanism; legal decides | Product owner with counsel |
| WT-Q8 | Should the editors' connectors enforce `reject`? | no; `warn` only | Product owner |
| WT-Q9 | Include named versions and public snapshots as leaves in v1? | no; receipts, rotations, horizons, event-heads only | Engineering lead |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| Checkpoint | signed statement of a log's size and root |
| Cosignature | a witness's signature over a checkpoint it verified is consistent |
| Witness | party that cosigns only consistent checkpoints |
| Monitor | party that reads leaves and checks completeness of scopes it can decode |
| Origin | unique identifier of a log, first line of the checkpoint |
| Tag | keyed hash that lets scope holders recognize leaves |
| Equivocation | presenting inconsistent histories to different parties |

## Appendix B. References

| Source | Used for | Checked |
|---|---|---|
| C2SP signed-note, https://c2sp.org/signed-note | note format, key ID derivation, algorithms | 14 October 2026 |
| C2SP tlog-checkpoint, https://c2sp.org/tlog-checkpoint | checkpoint body (origin, size, root) | 14 October 2026 |
| C2SP tlog-cosignature, https://c2sp.org/tlog-cosignature | `cosignature/v1` message with timestamp | 14 October 2026 |
| C2SP tlog-witness (editor's copy), https://github.com/C2SP/C2SP/blob/main/tlog-witness.md | add-checkpoint protocol, 409/422 semantics; "editor's copy, not the stable reference": re-check c2sp.org/tlog-witness at build time | 14 October 2026 |
| C2SP tlog-tiles, https://c2sp.org/tlog-tiles | tile layout (256 hashes per tile), entry bundles | 14 October 2026 |
| RFC 9162 Certificate Transparency Version 2.0 (Experimental), https://www.rfc-editor.org/rfc/rfc9162.html; RFC 6962 | Merkle tree hash, consistency proofs, STH, monitors | 14 October 2026 |
| Russ Cox, "Transparent Logs for Skeptical Clients", 1 March 2019, https://research.swtch.com/tlog | client-verified tile logs; design rationale | 14 October 2026 |
| Sigsum design documents, https://git.sigsum.org/sigsum/tree/doc/design.md | witness cosigning as gossip; trust policy with quorum (the design document is marked work in progress) | 14 October 2026 (search result only; details unverified, check at build time) |

The Go module checksum database's use of a transparency log is widely described but
was not re-verified here: unverified, check at build time.
