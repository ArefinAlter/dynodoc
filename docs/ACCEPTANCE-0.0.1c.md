# 0.0.1c acceptance

28 October 2026. Public release `v0.0.1c`, internal desktop/runtime version `0.0.1-beta.1`.

Desktop manages document repositories, changes, history, branches, reviews, conflicts, connected accounts and sync. Editing stays in Office/Google applications through add-ins/extensions; the full platform opens in the system browser. The manager supports installed subscription CLIs alongside API keys. The bundled engine remains the verified unchanged 0.1.0 component, with schema 47 and its separate licence notices.

## Verification

- Frontend type checks, tests and production builds pass. Native Rust Clippy (warnings denied) and the relevant workspace tests pass.
- Windows x64 NSIS installer and portable payload built from the release branch. Native portable-window acceptance uses fresh app/WebView profiles and no existing user data. The bundled engine answers; the page has no uncaught errors. No global install/uninstall of the owner's applications was performed.
- Linux x64 AppImage and Debian packages are built in the retained private Ubuntu 22.04 build distributions. Linux transport regression tests include descendant-process cancellation. Each AppImage opened as an ordinary user on its own private Xvfb display; Debian package metadata and contents were inspected. These are not real-hardware Linux acceptance.
- Evidence: `D:/verne-build/0.0.1c/evidence/full-c/`, including native test logs, package logs, `portable-gui/results-*.json`, screenshots and per-platform checksums.

The managed PostgreSQL/engine integration fixture passes profile/project creation, client-state persistence, shutdown and restart. The final portable UI also completes the actual first-run form and shows Changes/History. Engine source, migrations and root lockfile match the component shipped in 0.0.1b; it is reused with verified hashes. No schema migration is introduced. No new Word/Excel/PowerPoint or Google host acceptance was run.

## Provider coverage and remaining limits

Installed Claude Code, Codex, Cursor and Antigravity CLI adapters use the owning tool's sign-in. The apps do not read browser/IDE credentials. Cloud consent, offline settings, budgets, request previews and cancellation remain. Each request uses stdin, a fresh private scratch directory, deadlines, output bounds and process-tree cleanup (Windows jobs; Unix process groups). Claude tools/hooks/MCP are disabled, Codex runs read-only with shell/web tools disabled, Cursor Ask gets an app-created deny policy, and Antigravity requires its own strict tool-permission setting. These controls do not claim OS isolation of the installed CLI.

Claude was tested live through each native provider path, plus an Ogun workflow. Codex is installed but its subscription sign-in did not pass on this machine. Cursor/Antigravity have protocol and policy coverage; live account acceptance remains. A consumer Grok subscription has no verified direct adapter. Cursor may expose a Grok model when that account is entitled. Subscription quotas and extra-usage billing remain the provider's responsibility; API price estimates are not subscription pricing.

Windows packages are unsigned. Asten updater assets are signed with its existing updater key; this is distinct from OS code signing. macOS builds and real-hardware Linux checks remain planned. Existing release-specific limitations from 0.0.1b that are not changed above still apply.
