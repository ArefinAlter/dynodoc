# Dynodoc 0.1.0-alpha.1 acceptance

Validated 29 October 2026. Implementation commit: `01d5931f60965619274a07b146a4ef1651f25e03`. Release documentation may follow this
commit without changing the packaged application sources.

The desktop remains the workflow, history and synchronization manager for Office add-ins and
Google extensions. The full web platform opens in the system browser. This release packages the
responsive panel fixes from PR #4, durable per-link node identities and interrupted-copy recovery
([decision 023](decisions/023-linked-node-identities.md)), and official installed-agent connections.
Grok Build uses a separate official sign-in with runtime configuration isolated from authentication;
see the [setup guide](acceptance/grok-subscription.md).

## Validation

- 86 client tests passed; TypeScript, generated bundle audit and bundle freshness checks passed.
- Native formatting and Clippy with warnings denied passed. 255 native tests passed. The three
  ignored entries are a subprocess fixture helper, live Claude inference and a standalone managed
  workspace test; managed workspace behavior was exercised through the packaged app below.
- Grok fixtures passed authentication, completed text, API-key-only rejection, capability refusal,
  malformed/truncated output, response limits and authentication/configuration separation checks.
- The actual Windows portable binary initialized its bundled PostgreSQL and engine in a fresh
  task-owned profile. One file synchronized into two projects on that same engine, using distinct
  node IDs. Local and remote edits synchronized both ways; all three history chains verified.
- A full app restart preserved the node mappings, synchronized content and configured Grok preset.
  Acceptance waits for the UI and its asynchronous preference save before closing.
- Native windows resized from 1280×720 to 640×560 and back without root overflow or a collapsed
  workspace. Compact provider settings fit; agent details closed. No page errors occurred.
- The compiled NSIS source manifest matched 709 runtime files in the portable package, including
  the app, engine programs and PostgreSQL. The installer output hash matched the compiler output.
  The installer was not executed or unpacked into the owner's Windows profile.

The package acceptance scripts and receipts are retained at
`D:/verne-build/0.1/evidence/dynodoc`, including `native-verified`, `installer`, native build/test logs
and public-download verification. Earlier harness attempts checked before UI initialization or
before the preference debounce completed; an early cleanup also stopped the app before the database checkpoint finished. The final runs wait for UI readiness, saved preferences and natural shutdown.

## Components and limits

The unchanged engine component remains version 0.1.0, schema 47. Its crates, migrations and root
Cargo manifests exactly match the pinned 0.0.1b component archive (SHA-256
`f89b29c89eaea3f7b2d993ecddff8c7274775c1e65c3a05d9574f9f78ab2dcee`). PostgreSQL and the committed
Office codec retain their verified component versions. No database migration or API deployment
is required for these desktop changes. The app licence remains proprietary; engine and bundled
third-party notices keep their respective licences.

These Windows x64 packages are unsigned. No live provider credential or owner workspace was used
in this pass. Live Grok entitlement/inference, external Office/Google applications, Linux/macOS
runtime behavior and Android/iOS builds were not tested by this release pass.

## Windows download hashes

```text
c5e1453d81bc45567f1a37e155559e2c066586bb88c95d5f6a7cc5c61be70846  Dynodoc_0.1.0-alpha.1_windows-x64-portable.zip
7a4a8d8a3798765ea934c55cd61c6d726b6c7fd7de2749c017a6b6a9c60438da  Dynodoc_0.1.0-alpha.1_windows-x64-setup.exe
f4d947cc70c1df1d7884bfe41993c6b139bc1709a2473bd5c999b0aa7a66ec43  engine-cli_0.1.0-alpha.1_windows-x64.exe
```
