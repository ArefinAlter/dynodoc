# Decision 014: no browser-authored project branches, no follow graph

Date: 13 October 2026. Owner: Verne (delegated judgement, recorded here).

## Browser-authored project-wide branches: not built

Project-wide commits are signed by an enrolled device key (Ed25519) and published through
project-scoped device sessions. The web workspace keeps working per file: private drafts,
change requests, review and merge, restore/revert/cherry-pick, and a read-only Branches tab
for history that was published from the command line.

Why the browser does not author project-wide branches yet:

1. **No signing identity in the page.** A branch commit must be signed; a browser session
   token is not a signature. A non-extractable WebCrypto Ed25519 key enrolled as a device key
   is technically possible, but it needs the second item below to be useful.
2. **Commit construction lives in the engine.** Canonical encoding, typed object graphs,
   content addresses, recovery provenance and constraints are implemented once, in Rust. A
   second implementation in the page (or a WebAssembly build of the repository crate) would
   have to match the engine bit for bit, and any divergence produces commits that the
   server rightly rejects. That is a project of its own with its own test vectors.
3. **Server-side signing is the wrong trade.** Having the service sign on a user's behalf
   would make the receipt authority and the user key indistinguishable and would remove the
   property that a published commit proves a person's device authored it.

Revisit when the engine has a stable WebAssembly build with the published test vectors, or
when a custody model for browser-held keys is agreed. Until then the supported paths are
per-file drafts in the web workspace and project-wide branches from the command line.

## Follows and profile followers: not built

Profiles are visible only to people who share a project. A follow graph would publish who
knows whom beyond project membership, and it would add notification volume without a
workflow that needs it. The product's collaboration unit is the project, so the equivalents
are project membership, watch settings, mentions in issues and review requests. Stars
are available and the profile lists starred projects only among those the viewer can see.
