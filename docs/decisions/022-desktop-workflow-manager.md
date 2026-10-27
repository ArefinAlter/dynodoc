# 022: Desktop workflow manager and existing agent sign-ins

Owner direction: 27 October 2026. Implemented for the next development increment.

Dynodoc desktop manages repositories, connected files, staged changes, commits,
branches, comparisons, reviews, history, conflict choices, accounts and sync.
Editing happens through the extensions/add-ins in existing applications. The web
platform opens in the system browser. This changes the desktop workspace-opening
behavior of decision 021; the local manager window and its origin restrictions
remain. The existing isolated sign-in view is retained for authentication only; platform navigation is sent to the system browser. Embedded local history, the engine and PostgreSQL continue to support
standalone/offline operation.

Model setup supports installed Codex, Claude Code, Cursor and Antigravity agents
alongside API-key providers. An installed agent owns sign-in and credential
refresh. The desktop keeps consent, offline mode, caps and the closed tool table;
no delegated request may merge, approve or decide a review. Cloud processing is
still cloud processing when its client program runs locally. See
[installed agents](../INSTALLED-AGENTS.md) for supported CLIs, billing limits and
the separately verified Grok limitation. Native Office/Google host acceptance is
independent of this desktop change.

OS deep links retain page-originated handoff limits: they do not spend a native menu/sign-in allowance or automatically launch a loopback destination. Open a local server's workspace through the manager's own button/menu when needed.
