# Build stage → doc file map

README stages (used in `SETUP.md` and conversation) map to `docs/*-build-*.md` files. Internal headings in each doc use a 00–20 index (offset −11).

| README stage | Doc file | Internal # | PoC scope |
|-------------:|----------|:----------:|:---------:|
| 11 | `docs/11-build-index-and-stack.md` | 00 | reference |
| 12 | `docs/12-build-repo-and-environment.md` | 01 | done |
| 13 | `docs/13-build-database-schema.md` | 02 | yes |
| 14 | `docs/14-build-event-log-core.md` | 03 | yes |
| 15 | `docs/15-build-snapshot-engine.md` | 04 | yes |
| 16 | `docs/16-build-semantic-ops-and-governance.md` | 05 | yes |
| 17 | `docs/17-build-auth-and-identity.md` | 06 | yes (magic link only) |
| 18 | `docs/18-build-api-surface.md` | 07 | yes |
| 19 | `docs/19-build-dynodoc-frontend.md` | 08 | yes |
| 20 | `docs/20-build-crdt-convergence.md` | 09 | **skip** |
| 21 | `docs/21-build-integrity-and-merge.md` | 10 | yes |
| 22 | `docs/22-build-public-consultation-surface.md` | 11 | no |
| 23 | `docs/23-build-citizen-participation.md` | 12 | no |
| 24 | `docs/24-build-moderation-automated.md` | 13 | no |
| 25 | `docs/25-build-moderation-human.md` | 14 | no |
| 26 | `docs/26-build-deliberation-layer.md` | 15 | no |
| 27 | `docs/27-build-ai-summary.md` | 16 | no |
| 28 | `docs/28-build-t3-civic-identity.md` | 17 | no |
| 29 | `docs/29-build-closeout-reports.md` | 18 | no |
| 30 | `docs/30-build-transparency-and-audit.md` | 19 | no |
| 31 | `docs/31-build-deployment.md` | 20 | no |

PoC schema note: stage 13 uses migrations `0000`–`0004` and `0009_views`; `0005`–`0008` are reserved for consultation tables (post-PoC). Stage 17 adds `0010_auth` (PoC auth: `document_access`, `refresh_token`) after that reserved block.
