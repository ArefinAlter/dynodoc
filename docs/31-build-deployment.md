# 20 · Multi-Tenant Hardening and Deployment

### Claude Code prompt to start this stage

> Final stage: turn the working platform into something multiple institutions can deploy and operate. Reference: §11 (infrastructure), §12 (governance and operating model). This stage doesn't add features; it hardens, documents, and packages.
>
> 1. **Multi-tenancy model**:
>    - Choose: single-instance multi-tenant (one Postgres, one app server, multiple `tenant_id`s) versus deployment-per-tenant (one stack per institution). Recommend **deployment-per-tenant for sovereign institutions** (each government runs their own copy on their own infrastructure) and **single-instance multi-tenant for the hosted free tier** (a non-profit operator runs many small consultations on shared infra).
>    - For deployment-per-tenant: every stack ships from the same codebase via Helm charts; configuration is per-deployment.
>    - For single-instance multi-tenant: add a `tenant_id` column to every relevant table, row-level security via Postgres policies (`CREATE POLICY tenant_isolation ON event FOR ALL TO app_role USING (tenant_id = current_setting('app.tenant_id')::uuid)`).
>
> 2. **Helm chart**: in `infra/helm/dynodoc/`:
>    - Subcharts for: postgres (or external reference), redis, minio, the rust API, each python service, the frontends, vLLM (optional).
>    - Configurable values: tenant identity, sponsor branding, civic-ID provider, LLM provider, replicas per service, resource requests/limits, ingress host, TLS certs.
>    - Default values for a small national consultation (matching §11.5's sizing).
>    - `helm install dynodoc ./helm/dynodoc -f values-production.yaml` should be the complete deploy command.
>
> 3. **Observability hardening**:
>    - OpenTelemetry instrumentation in every service: traces, metrics, logs all unified.
>    - Grafana dashboards committed to the repo: "Platform Health," "Consultation Live View," "Moderation Queue Depth," "Sybil Signal Heat," "API Latency."
>    - Alertmanager rules: API P99 > 500ms, moderation queue depth > 1000 items, event-append failure rate > 0.1%, Postgres replication lag > 30s, MinIO disk > 80% full.
>    - Runbooks for each alert in `docs/runbooks/`.
>
> 4. **Backup and disaster recovery**:
>    - PITR (point-in-time recovery) for Postgres via `pgbackrest` to S3-compatible storage.
>    - Daily MinIO snapshots replicated to a separate region/zone.
>    - Documented RPO (recovery point objective): 15 minutes. RTO (recovery time objective): 1 hour.
>    - Quarterly DR drill: restore from backup to a staging environment, verify the audit chain.
>
> 5. **Security review**:
>    - Threat model document in `docs/security/threat-model.md`. Walk through STRIDE for each component.
>    - Penetration test by an external firm before any consultation involving >5000 participants. Repeat annually.
>    - Vulnerability scanning in CI (cargo-audit, pip-audit, npm-audit, trivy for containers).
>    - Secrets scanning: gitleaks pre-commit hook.
>    - Bug bounty (informal): a clear `SECURITY.md` with disclosure email, acknowledgment commitment, public credits for findings.
>
> 6. **Sovereignty packaging**:
>    - Air-gapped install option: tarball of all container images + Helm charts + the LLM model weights + the auditor CLI. Installable on a disconnected network.
>    - All third-party dependencies vendored (Cargo, pip, npm lockfiles, container base images) so a deployment doesn't depend on public registries at runtime.
>    - Documentation in Bangla and English for sovereign operators.
>
> 7. **Onboarding documentation**:
>    - "Deploying for the first time" guide.
>    - "Running your first consultation" guide for sponsor teams.
>    - "Moderator training" guide.
>    - "How to do a closeout report" template + examples.
>    - "Interpreting the deliberation layer" guide.
>    - All in `docs/handbook/`.
>
> 8. **License and governance commitments**:
>    - The codebase is AGPL v3.
>    - A `GOVERNANCE.md` documenting the maintainer process, the path for contributions, the schedule of releases.
>    - A `CODE_OF_CONDUCT.md`.
>    - A `CONTRIBUTING.md` with the development workflow.
>    - Public roadmap in GitHub Discussions or equivalent.
>
> Tests:
>    - Helm chart installs cleanly on a fresh k8s cluster (kind or k3s for testing).
>    - All observability metrics fire correctly under synthetic load.
>    - DR drill: backup → restore → audit chain verifies.
>    - Security scans pass with no critical findings.
>    - Onboarding guide: a colleague who has not worked on the project deploys successfully from the guide alone.

### Why this stage is not the climax

If stages 03 (event log) and 18 (closeout) are the *meaningful* milestones, stage 20 is the *responsible* one. It's what turns a working prototype into infrastructure other institutions can adopt. The work is unglamorous: Helm values, runbooks, threat models, backup drills. But: the difference between a project that has one production deployment versus one that has fifteen is almost entirely this work.

A few specific commitments matter:

**The codebase is AGPL.** This is non-negotiable for civic infrastructure. Any institution running a fork must publish their changes; this prevents the platform from being captured into a proprietary derivative that diverges from the public's interest.

**Sovereign-deployable end-to-end.** No mandatory dependencies on commercial cloud, commercial LLM providers, or commercial identity systems. Each can be substituted with a self-hosted alternative without code changes — just configuration.

**Penetration tested before public consultation.** A national consultation is a high-value target. The platform's security posture matters as much as its features. Budget for a real third-party audit, not just internal review.

### Watch out for

**Helm chart complexity creep.** Helm charts become unmaintainable if they try to cover every possible config. Ship a *minimal* chart that handles the common cases; document how to fork and customize for unusual deployments.

**Documentation drift.** The handbook gets stale. Run a "docs review week" each quarter where every doc is exercised by someone who didn't write it. Update or delete.

**Backup verification.** Backups that are never restored aren't backups. The quarterly DR drill is non-negotiable.

**Community management.** An open-source civic-tech project lives or dies on its community. Respond to issues, review contributions, hold a monthly contributor call. The platform's adoption depends as much on this as on its code quality.

### Verification checklist

- [ ] Fresh k8s cluster + `helm install` → working platform within 15 minutes.
- [ ] All observability dashboards populated under load.
- [ ] DR drill completes; audit chain verifies post-restore.
- [ ] Penetration test report on file with all critical findings addressed.
- [ ] Air-gapped install demonstrated.
- [ ] Onboarding guide validated by a fresh user.

### What comes after stage 20

You have a platform. It serves Dynodoc Phase-0 product, hosts public consultations at national scale, and is hardened for sovereign deployment. From here, the work is operational and political:

- Recruit the first sovereign deployment (a ministry, a city government, a regulator).
- Run a real consultation on a real policy. Document what breaks.
- Iterate. The architecture from this build reference should hold; the surface details (UX flows, moderation policy, language coverage) will need adjustment per real-world experience.
- Build community: workshops with civic-tech organizations, training for sponsor teams, partnerships with universities for the deliberation methodology research.
- Influence policy: the platform's existence creates the *option* for governments to consult differently; whether they take that option is political. The platform's job is to make it cheap and defensible to consult well.

The engine from Volume II survived this far without modification. That's the architectural payoff. The same stable-identity, semantically-merged, hash-chained log that defends a research instrument from accidental corruption defends a national policy consultation from adversarial corruption. The layers above it are extensive but they all sit on the same foundation, and that foundation was correct from the beginning.
