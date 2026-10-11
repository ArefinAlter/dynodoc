# Roadmap acceptance ledger

12 October 2026. Source capabilities below are distinct from the deployed schema-29
pilot. This ledger does not certify the full roadmap. Exact source revisions and
test outcomes belong in [the handoff](ENGINE-EVOLUTION-HANDOFF.md).

| Area | Implemented source | Remaining acceptance or implementation |
|---|---|---|
| Shared storage | Document-scoped immutable objects; batched v1 maps; streaming byte trees; typed v2 checkpoints; opt-in shared draft/rebase/recovery bases and named/deployed/public snapshots. | Physical packing/compression; changed-node publication; realistic growth/latency measurements; rollout defaults. |
| Portable project history | SQLite durable local state; signed multi-file commits and parents; branch CAS with generations; bounded object inventory/import; all best merge bases; conservative project comparison; commit-addressed revert/cherry-pick with signed provenance and CLI. | Authenticated remote project transport, account/device key binding, commit discovery/exchange policy, protected project branches and review, existing bundle migration, UI and connector adoption. |
| Offline convergence | Local commits survive reopen; reordered/idempotent object exchange; one accepted project merge can reach matching replica heads; independent file planning checks shared path/identity constraints. | Same-file offline semantic resolution, adversarial partition/restart corpus across native editors, complete read/write/validation footprints and criss-cross virtual bases. |
| Audit | Immutable/verified event and snapshot boundaries; fixed event/content/commit vectors; signed project commits; portable signed ref receipts and independent signature fixtures; separate durable client pins with proof continuity, dual-signed authority rotation and local revocation. | Account/device mapping and revocation; atomic server receipt issuance with protected refs; trusted authority distribution, external witnesses/gossip and remote/connector adoption. Local tools alone do not protect the deployed server. |
| Algorithm/resources | Bounded diff fallback and workloads; chunk and semantic budgets; iterative cached structural depth ordering, including cyclic/deep inputs. | Persistent affected-node state instead of full clones/scans; chunk-boundary insertion amplification; incremental native parsing; end-to-end memory/latency acceptance. |
| Native adapters | Existing Word/Docs, Excel/Sheets and PowerPoint/Slides test packages, pairing, capture/review/apply and local acceptance worksheet. | Installed-host results supplied by the owner; lossless supported native structures/assets; background/offline capture and local-repository adoption; RTF; publisher setup and listing approval. |
| Service scale | Restricted runtime roles, request admission, deadlines, bounded history pages and SSE checks. | Representative 100/250/500 MB native workloads, ten collaborators, many independent teams, quotas/workers/backpressure, recovery/load-shedding and capacity/cost evidence. |
| Repository workspace | Existing Projects frontend, member-only profiles, multiple files, per-file drafts/reviews/history/recovery; 3 MB pilot upload cap. | Project-wide branch/commit/review interfaces backed by portable refs, integration of portable recovery commands, compare/activity/releases and native checkout across supported formats. |

Continue across these dependencies without requiring another “continue” for each
source checkpoint. Storage adoption and authorized remote exchange are the next
integration priorities. Keep the pilot cap and opt-in writer boundaries until
their acceptance evidence exists. The owner explicitly chose to run installed
Office/Google acceptance locally; do not report synthetic/local repository tests
as installed-editor or marketplace acceptance.
