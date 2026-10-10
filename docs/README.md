# Dynodoc documentation

Updated 11 October 2026. Dynodoc is document version control and provenance around
existing editors. Start with the current contract, not the original questionnaire
or public-consultation stage prompts.

## Current product and implementation

| Need | Read |
|---|---|
| User journey, identity, storage and scale requirements | [PRODUCT-SPEC](PRODUCT-SPEC.md) |
| Accepted direction and scope precedence | [Decision 012](decisions/012-local-first-document-version-control.md) |
| Ordered engineering work and acceptance gates | [ENGINE-EVOLUTION-PLAN](ENGINE-EVOLUTION-PLAN.md) |
| Current increment, tests, commits and next task | [ENGINE-EVOLUTION-HANDOFF](ENGINE-EVOLUTION-HANDOFF.md) |
| Implemented capabilities and production record | [PROJECT-STATUS](PROJECT-STATUS.md), [HANDOFF](HANDOFF.md) |
| Security findings, verified fixes, trust limits and next gates | [SECURITY-HARDENING](SECURITY-HARDENING.md) |
| Legacy snapshot receipts, replay verification and upgrade | [VERIFIED-SNAPSHOTS](VERIFIED-SNAPSHOTS.md) |
| Separate migrations, restricted API login and upgrade | [DATABASE-PRIVILEGES](DATABASE-PRIVILEGES.md) |
| Shared checkpoint codec, storage limits and measured spike | [SHARED-CHECKPOINTS](SHARED-CHECKPOINTS.md) |
| Bounded serialization contract and allocation measurement | [BOUNDED-CHECKPOINT-ENCODING](BOUNDED-CHECKPOINT-ENCODING.md) |
| Combined protocol guide | [04-versioning-combined](04-versioning-combined.md) |
| Algorithm assumptions and corrected mathematics | [03-versioning-mathematics-vol2](03-versioning-mathematics-vol2.md) |
| Implementation versus theoretical claims | [VERSIONING-CONFORMANCE](VERSIONING-CONFORMANCE.md) |
| Rich-text algorithm and benchmark boundaries | [RICH-TEXT-VERSIONING-RESEARCH](RICH-TEXT-VERSIONING-RESEARCH.md) |
| Current connector contract and native acceptance | [PROVENANCE-SYNC](PROVENANCE-SYNC.md), [NATIVE-CONNECTOR-ACCEPTANCE](NATIVE-CONNECTOR-ACCEPTANCE.md) |
| Desktop preflight, host tests and marketplace requirements | [ADDIN-PUBLICATION-AND-ACCEPTANCE](ADDIN-PUBLICATION-AND-ACCEPTANCE.md) |
| Standalone MIT engine synchronization | [ENGINE-EXTRACTION](ENGINE-EXTRACTION.md) |

## Historical design and build references

Files 00-03 record the original feasibility, UX, questionnaire-oriented architecture
and mathematics. Mathematical sections remain useful with their explicit assumptions;
they are not a formal verification of the executable product. File 04 is now the
current combined reading guide rather than a duplicate of two long volumes.

Files 05-10 describe a public-consultation extension outside the current roadmap.
Files 11-31 describe the original staged build, including unimplemented civic/CRDT/ML
alternatives. Their numbering is not the current build order. Read the evolution
plan for new work, and consult an old stage when maintaining its existing subsystem.

The original PRD and decision 000 retain historical context. Decisions 010-012
supersede research-instrument and web-editor-parity priorities. Dated release and
operations records describe what was observed at the time, not newly verified state.
Do not relabel a design, prototype, test run or publication as production acceptance.
