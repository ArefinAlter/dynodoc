# Durable shared-history budgets

12 October 2026. Source schema 34; production remains schema 29 with shared writers off.

Encoded shared content now has persistent per-document and per-creator account
budgets, independent of the existing per-request graph/chunk limits. All retained
objects count, including objects for old revisions and staged/unreachable roots.
Deduplicated reuse and exact retries do not spend the budget again. PostgreSQL
checks the budget inside the same transaction as object insertion and publication.
A rejected graph cannot leave partial objects or counters if its writer follows
the existing transaction contract.

Defaults are **1 GiB / 250,000 objects per document** and **4 GiB / 1,000,000 objects
per creator account**. These are encoded object-byte and object-count limits, not
physical disk limits. Uploads, events, legacy JSON snapshots, indexes, WAL, backups
and local SQLite repositories are outside this accounting. The pilot native-upload
limit remains **3,000,000 bytes**. Packing, incremental publication, total-service
capacity and local-store retention remain unfinished.
Local repositories now have [separate persistent admission limits](LOCAL-STORAGE-BUDGETS.md);
those counters are not included in the server profile's usage.

The profile shows the signed-in account's usage via `GET /profile/storage`.
Quota failures reach the web client as HTTP 507 with `code: storage_limit` and an
actionable message. They do not fall back to a legacy snapshot to evade the quota.
The periodic checkpoint job can log a failed checkpoint without deleting existing
history; its previously committed event is retained. Monitor those job failures.

## Accounting and isolation

Migration 0034 backfills existing shared objects. If a pre-existing store exceeds
the default, its initial limit is raised to its exact retained usage; data is
preserved and new content needs an operator increase. A document is charged to its
creator at first retained-object insertion. Moves and access grants do not shift
that charge. Account erasure retains pseudonymous authorship and accounting for
shared documents.

Statement-level transition-table triggers count only rows actually inserted,
including correct `ON CONFLICT DO NOTHING` behavior. They update account rows in
UUID order, then document rows in UUID order for each batch. Independent accounts
have separate counters. Multiple files of the same account serialize on that
account's counter; this is a pilot correctness boundary, not a high-scale throughput
claim. Future multi-account project transactions must acquire all affected account
locks in a common order before writing their documents.

Counters and budgets are read-only for the runtime login. Restricted trigger
functions update them using an explicit safe search path; the runtime cannot
execute those functions directly. Existing append-only/erasure rules still apply.
Normal deletes cannot free quota. Audited whole-document erasure releases its
charge atomically and removes its document counter. No garbage collector or
automatic pruning was added. Do not recommend erasing a document just to free space.

## Operator adjustments

Use the separate migration/operator credential, during an operational maintenance
window if a large update is needed. Increase the account or document budget using
exact UUIDs, for example (replace the example UUID before running):

```sql
begin;
update checkpoint_storage_account
set byte_limit = 8589934592, object_limit = 2000000
where identity_id = '00000000-0000-0000-0000-000000000001';
commit;
```

Check the affected row count. Rows are created lazily on first shared object use;
there is no row to update for an unused account. A limit cannot be reduced below
current usage. Do not alter usage counters directly. A database owner remains able
to alter/disable these mechanisms; this does not replace independent audit anchors
or disk monitoring.

## Acceptance

Restricted-role integration tests exercise exact byte/object accounting, quota
immutability, reuse at a full budget, failed-write rollback, concurrent spending
across two documents, readable error translation, and audited-erasure reclamation.
The handoff records actual test outcomes. The full suite must include existing
snapshot, draft, public-pin, runtime-privilege and erasure regressions after this
migration. Keep the opt-in writer flags off in production until rollout acceptance.
