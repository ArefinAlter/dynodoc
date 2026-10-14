# Decision 015: customer profile, formula engine, planned products, copyright line

Date: 14 October 2026. Owner: Verne.

## Decisions

1. **Customer profile.** Teams of any size, in enterprise and academia, working on multiple
   documents. Planning documents cover three profiles: a small team or lab on one node,
   a department or university with SSO, and an enterprise or regulated site that is HA,
   intranet-only or air-gapped.
2. **Formula engine.** `hyperformula` (GPL-3.0-only or commercial) is replaced; no commercial
   licence is bought. The replacement must be permissively licensed, including its
   transitive dependencies. See `FORMULA-ENGINE-REPLACEMENT.md` for the evidence and result.
   This resolves the open point in decision 013.
3. **Planned products, documented but not built.**
   - Self-hosted edition: `SELF-HOSTED-EDITION.md` and its companion files.
   - AI-agent connectors, Claude first, then ChatGPT, then other clients:
     `AI-AGENT-CONNECTORS.md` and its companion files.
   - End-to-end encryption, with customer-managed at-rest encryption as Phase 0:
     `END-TO-END-ENCRYPTION.md` and its companion files.
   - Remaining engine items (commit pruning, cross-document merge, node-conflict
     resolution, witnesses, large-file and installed-host acceptance):
     `ENGINE-REMAINING-DESIGNS.md`.
4. **Copyright line.** The copyright holder shown on the site and in repository notices is
   Verne Ltd. The registered operator details supplied by the owner (Verne Systems) stay in
   the operator block of the legal pages, because they come from the company registration.
5. **Built in the same round.** Receipt-authority rotation, passphrase-encrypted key files
   with optional OS key storage, and transparent compression of local repository objects.

## Consequences

- Marketplace publication stays parked.
- Anything marketed as "end-to-end encrypted" or "zero knowledge" needs the independent
  review described in `E2EE-THREAT-MODEL-AND-ASSURANCE.md` first.
- Token-saving claims for agent connectors need the benchmark in
  `AI-AGENT-TOKEN-EFFICIENCY.md` first.
