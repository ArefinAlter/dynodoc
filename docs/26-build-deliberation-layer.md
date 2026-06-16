# 15 · Deliberation Layer (Polis-style clustering)

### Claude Code prompt to start this stage

> Implement the deliberation layer that turns raw contributions and votes into bridge statements, cluster summaries, and the structure of opinion. Reference: `dynodoc_public_consultation_design.md` §7.1–§7.4. In `services/deliberation/`:
>
> 1. **Inputs and outputs**:
>    - Reads from the engine's event log (Postgres) and the deliberation cache tables.
>    - Writes to: `deliberation_cluster`, `deliberation_statement`, `deliberation_vote` (this last one is just a denormalized cache; the source of truth is the `VoteCast` events).
>    - Exposes:
>      - `POST /recompute/{document_id}` — recompute clustering for one document.
>      - `GET /document/{id}/node/{node_id}/clusters` — current cluster state for a clause.
>      - `GET /document/{id}/bridge-statements` — top bridge statements across the consultation.
>
> 2. **Statement embedding** (the deduplication side of §7.4):
>    - On each new `CommentAdded` event, compute a sentence embedding using a multilingual model: `paraphrase-multilingual-MiniLM-L12-v2` (good Bangla + English support, 384-dim).
>    - Store the embedding in a `statement_embedding` table with pgvector.
>    - Find near-duplicates by cosine similarity ≥ 0.85 within the same clause; group them into one `deliberation_statement` row with `member_count`.
>    - A moderator (or contributor) can manually split or merge clusters via the review UI.
>
> 3. **Per-clause clustering** (the §7.1 algorithm):
>    - For each clause with ≥ 50 contributing participants:
>      - Build the participant × statement vote matrix (rows: participants who voted on any statement for this clause; columns: statements; cells: -1/0/+1; unvoted = 0).
>      - Reduce dimensionality: PCA to 2 components for visualization; UMAP or just the PCA-2D representation for clustering.
>      - Cluster participants: HDBSCAN with min_cluster_size = max(20, sqrt(n_participants)). Allows variable number of clusters; tags noise as cluster -1.
>      - Store cluster assignments per participant in `deliberation_cluster_membership` table.
>
> 4. **Bridge statement detection**:
>    - For each statement, compute *group-informed consensus*: fraction agreeing within each cluster.
>    - A statement is a **bridge statement** if: agreement ≥ 0.7 in every non-noise cluster, with ≥ 50 total voters across clusters.
>    - A statement is **divisive** if: agreement ≥ 0.7 in at least one cluster AND ≤ 0.3 in another.
>    - Set `is_bridge` and `bridge_score` (the minimum across-cluster agreement) on `deliberation_statement`.
>
> 5. **Cross-clause meta-clustering** (the §7.3 second-order analysis):
>    - For participants who contributed to multiple clauses, compute their cluster vector across clauses.
>    - Run a second clustering on these vectors to identify *coherent constituencies* — citizens whose clause-level positions correlate consistently.
>    - Store as `deliberation_constituency` rows with a brief auto-generated description.
>    - These constituencies feed the sponsor dashboard but are intentionally NOT shown publicly per-citizen (privacy).
>
> 6. **Schedule**:
>    - Incremental recompute every 15 minutes during active consultation.
>    - On-demand recompute on any clause receiving >5 contributions per minute (burst handling).
>    - Full nightly recompute that recomputes everything from scratch (cleans up drift from incremental updates).
>
> 7. **Tests**:
>    - Synthetic test: generate 200 participants with 2 known opinion groups + 1 bridge position; verify the algorithm finds 2 clusters and identifies the bridge statement.
>    - Sybil robustness test: inject 50 fake participants all voting identically; verify they cluster tightly as one group (not amplifying a bridge statement); verify the bridge calculation correctly downweights them (they're high agreement in their own cluster, not cross-cluster).
>    - Performance: 5000 participants × 500 statements clustering runs in < 30 seconds.

### Why this is the most novel stage

The clustering is what makes the platform categorically different from a comment forum. Without it, 30,000 contributions are an unreadable stream. With it, the sponsor sees: "5 opinion groups; 12 bridge statements with cross-cluster agreement; 8 divisive issues; 3 coherent constituencies."

The Sybil robustness is a *consequence* of the algorithm, not a bolt-on. Coordinated fake votes tend to be highly correlated; the clustering pulls them into a single tight cluster; statements supported by *only that cluster* are not bridge statements; the consultation's headline output (bridge statements) is structurally hard to fake.

### The pipeline architecture

```
┌─────────────────────────────────────────────────────────┐
│  Event log (Postgres)                                    │
│  ↓ change-data-capture or polling                        │
└─────────────────────────────────────────────────────────┘
            │
            ▼  CommentAdded events
┌─────────────────────────────────────────────────────────┐
│  Statement embedding service                              │
│  - compute embedding                                      │
│  - check near-duplicates, merge or create statement row   │
└─────────────────────────────────────────────────────────┘
            │
            ▼  VoteCast events
┌─────────────────────────────────────────────────────────┐
│  Per-clause clustering job (15-min cadence)               │
│  - PCA + HDBSCAN on vote matrix                           │
│  - compute bridge_score for each statement                │
│  - flag is_bridge / is_divisive                           │
└─────────────────────────────────────────────────────────┘
            │
            ▼  cluster assignments
┌─────────────────────────────────────────────────────────┐
│  Cross-clause constituency job (nightly)                  │
│  - aggregate per-citizen cluster vectors                  │
│  - second-order clustering                                │
└─────────────────────────────────────────────────────────┘
            │
            ▼  derived state in deliberation_* tables
            ↓  fed to:  citizen frontend, sponsor dashboard, closeout report
```

### Watch out for

**Sparse vote matrices.** Most citizens vote on a small fraction of statements. PCA on a sparse matrix is fine but use TruncatedSVD or sparse-aware implementations; don't densify naively.

**Cluster count instability.** HDBSCAN's cluster count can vary slightly run-to-run if new votes arrive between runs. Smooth by retaining cluster IDs across runs when membership overlap is high (the matching-by-Jaccard trick); avoid renaming "Cluster 3" to "Cluster 5" between displays.

**Embedding model multilingual quality.** Verify on real Bangla + English mixed corpora before committing to the model. If quality on Bangla is weak, consider a Bangla-specific model + English-specific model with a language-id router.

**Don't surface individual cluster membership to the public.** A citizen seeing "you are in cluster 3" feels labeled. Surface cluster *positions* publicly; surface a citizen's own position privately ("here's where your view fits").

### Verification checklist

- [ ] Embedding deduplication groups near-paraphrases correctly.
- [ ] Clustering finds the right number of clusters in synthetic test.
- [ ] Bridge statements identified correctly; Sybil flood doesn't pollute them.
- [ ] Constituency analysis surfaces coherent groupings.
- [ ] Recompute cadence holds under load.
- [ ] Performance target met.
