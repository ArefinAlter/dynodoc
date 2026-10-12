# Selected-history synchronization

12 October 2026. `engine-cli sync` connects the local SQLite repository to the
schema-36 project history API. PostgreSQL stays at schema 36; the local repository
stays at schema 2. No event, commit, object or receipt hash format changes.
These sources are not deployed to the schema-29 pilot.

## Behavior

- **Plan** reads the selected candidate and discovers its unpublished dependency
  closure. The saved JSON contains the original signed manifests, including paths,
  author keys, messages, parents and recovery sources. Already published history
  ends traversal. This is a read-only remote preview.
- **Push** checks the explicit remote/project/branch and rechecks the plan against
  current publication and branch state. Every newly public dependency must occur
  in the reviewed plan. It publishes in dependency order. Unrelated local branches,
  unattached objects and other private staged history are never enumerated/uploaded.
- **Fetch** starts at a signature-verified remote branch head, retrieves dependency
  manifests and follows the typed codec's reachable object edges. It does not trust
  an arbitrary server object inventory. Objects, signatures, project identity,
  recovery dependencies and reconstructed states are checked before commit import.
- **Tracking** reads the separate durable receipt journal offline. The tracked
  identity is `(authority UUID, project UUID, branch)`, not a mutable URL alias.
  Fetch and push do not move local working branches or apply files in an editor.
- **Pull** fetches and then fast-forwards one explicitly selected local branch.
  The expected local address and generation protect concurrent local work; omitted
  expectations mean the branch must not exist. Divergence is retained for explicit
  comparison/resolution. This changes a repository ref, never an open/native file.

For upload, missing-object negotiation uses the current account's private staging.
Bytes reachable from an already published immediate dependency can be acknowledged
under that authorized file root. Only the remaining objects are uploaded, in batches
of at most 64 objects / 1 MiB decoded bytes. Exact retries reuse private acknowledgements,
published commits and locally imported objects. Older non-immediate roots may still
require retransmission, although server physical storage deduplicates them.

Default push requests the exact branch address AND generation from the plan.
`--publish-only` publishes candidates for review without moving a shared branch.
Protected-main policies/approvals remain server enforced; successful publication
does not mean a protected ref update was approved. A rejected/racing update can
leave an authorized published candidate available for review. Publication of a
multi-commit plan is deliberately resumable, not one all-or-nothing transaction.

## Operator procedure

This first client uses a current ordinary **account access token**. A signing key
must already be enrolled in that account for newly published commits. File connector
grants do not become project tokens. Project-scoped device sign-in, secure native
credential storage and automatic refresh are still required for end-user adoption.

Prepare `remote.json` yourself from a trusted installation/operator channel:

```json
{
  "version": 1,
  "url": "https://your-api.example/api/",
  "authority": "11111111-1111-4111-8111-111111111111",
  "public_key": "REPLACE_WITH_THE_INDEPENDENTLY_VERIFIED_64_CHARACTER_PUBLIC_KEY"
}
```

The URL is the actual API base, must end in `/`, and may include a reverse-proxy
prefix. The example is not a working installation/key. Do not derive first trust
solely from the server's `/history/authority` response or from an incoming plan.
The client compares that response with the independently prepared configuration.

Keep the access token alone in a private `account-token.txt`, outside the repository
and public source archives. The command takes its path, never the token value.
It neither reads browser cookie stores nor persists the token in a plan/journal.
Restrict the file using the operating system's permissions and remove it after use.
An expired token stops transfer; obtain a fresh account token and retry.

```sh
engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main plan COMMIT --out push-plan.json

# Inspect every signed manifest in push-plan.json before publishing.
engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main push push-plan.json

# Publish a candidate for protected-main review without changing main.
engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main plan COMMIT --out review-plan.json --publish-only
engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main push review-plan.json

# On another authorized machine, retrieve verified history without moving local work.
engine-cli sync other.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal other-pins.sqlite --branch main fetch
engine-cli sync other.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal other-pins.sqlite --branch main tracking

# Create an unborn local branch, or provide its exact previous address/generation.
engine-cli sync other.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal other-pins.sqlite --branch main pull local-main
engine-cli sync other.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal other-pins.sqlite --branch main pull local-main --expected LOCAL_COMMIT --expected-generation LOCAL_GENERATION
```

Replace PROJECT with the project UUID and COMMIT with the selected signed commit
address. Plans are created exclusively: choose a new filename for a revised plan.
Keep them private because they disclose unpublished names, messages and hashes.
After review approval, create a new ordinary push plan for that candidate and the
current branch generation. The initial CLI has no review-submission command; use
the existing history review API until Projects adopts this contract.

Tracking still takes the shared CLI arguments, but it does not read the token file
or contact the server. `pins.sqlite` must be separate from the project database;
the journal detects and refuses a project database, including path aliases.
Keep it outside downloaded/restored project bundles and retain its backups.

Use `repository show`, `read-state`, `compare`, `merge-bases` and `recovery-plan`
to inspect fetched work. `pull` accepts only parent-ancestry fast-forwards and uses
local CAS; `repository set-head` remains available for deliberate local ref changes.
Same-file merge/application and native document checkout remain separate work. Local branches and the
receipt journal have separate storage and lifecycles.

## Integrity and failure semantics

An existing pin requires a contiguous, signature-checked chain to the selected
head. Lower generations, conflicting receipts, gaps, a different authority/key or
local revocation fail closed. On first fetch/push the independently configured key
trusts the selected checkpoint, not unseen earlier receipts. No external witness
or secure hardware is implied. The filesystem owner can still alter/delete local
data or trust files. Server key compromise can create signed forks; client pins
detect contradictions relative to previously retained history, not all equivocation.

Fetch imports complete verified commits before updating the receipt journal.
Failure can leave verified objects/commits for retry, but cannot change a working
branch. Receipt proof is checked in full before journal advancement. Long proofs
are then saved in contiguous atomic pages; a crash may retain a verified prefix.
Concurrent pin advancement/revocation is rechecked by journal transactions. A
revoked or erased/inaccessible remote head is not silently reset or replaced.

If a ref response was lost, rerunning the original plan recognizes only the exact
candidate at the next expected generation and verifies its receipt. A different
head requires fetch/compare/new planning. If the server advanced further after the
uncertain write, explicit fetch is required. Never assume a network error rolled
back publication or a ref update.

The HTTP client requires HTTPS with certificate validation. Synthetic loopback
tests can opt in to HTTP on a literal loopback IP only. Redirects, ambient proxy
discovery and automatic decompression are disabled. Credentials are marked
sensitive; response bodies are not copied into HTTP error messages. Read errors
and JSON parsing failures can report syntax/validation details, not whole documents.
Transport settings follow the [reqwest client contract](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html).

## Bounds and limitations

- One run: 4,096 visited commits, at most 64 MiB downloaded manifest encoding,
  32 MiB raw plan manifests / 64 MiB formatted plan, 4,096 receipt catch-up entries,
  100,000 HTTP requests, 512 MiB combined encoded request/response JSON and a
  30-minute admission deadline. Each request has a 10-second connection timeout
  and a 30-second total timeout. These are ceilings, not throughput promises.
- Graph validation shares a 65,536-read / 64 MiB encoded read budget across all
  files in a commit. The server retains its own graph and current-access limits.
  Bodies/pages/batches also have explicit caps; all retained objects count against
  existing durable local/server quotas.
- Graph validation currently reconstructs one complete file state at a time, and
  some scans repeat during import/staging/publication. Complete manifests and
  pending dependency metadata remain resident. This is not a streaming large-file
  memory bound or a changed-node-only algorithm.
- Existing verified objects make retries inexpensive in bytes, but metadata and
  graph scans repeat. Extremely long ancestry or receipt catch-up needs explicit
  smaller/manual transfers using the existing packet/receipt tools. There is no
  durable arbitrary job queue or automatic backoff/refresh in this initial CLI.
- No automatic local deletion/GC, native writes, complete offline convergence,
  web/connector project branch UI or marketplace/installed-host acceptance.

Regression evidence covers real HTTP through the restricted runtime, two authors,
private-history exclusion, explicit private ancestry disclosure, shared-root reuse,
publish-only review candidates, stale plan rejection, lost-response retry, durable
pins, corruption/retry, fast-forward/local-CAS/divergence checks,
fork/rollback/gap/key/revocation checks, and URL/body bounds.
See the handoff for actual commands, results and source revisions.
