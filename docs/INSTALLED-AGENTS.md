# Installed agents and existing subscriptions

Development version 0.0.1c. Choose an existing agent sign-in in Providers, sign in using that agent’s own CLI, select `default` or a model allowed by your account, allow the provider to receive the request, then enable it. API-key providers remain available separately. The desktop starts the selected program for a bounded request and stops it on cancellation or timeout. This connection is for the desktop computer, not the hosted visitor sandbox.

| Connection | Installed program | Authentication owner |
| --- | --- | --- |
| Codex | `codex` | Codex / ChatGPT sign-in |
| Claude Code | `claude` | Claude Code sign-in |
| Cursor | `agent` | Cursor CLI sign-in |
| Antigravity | `agy` | Antigravity CLI sign-in |

No credential database, browser profile, IDE token file or keychain item belonging to another program is read by this application. The installed agent owns its credentials and refresh. No prompt is interpolated into shell source. No permission-bypass flag is passed; Claude’s built-in tools are disabled, Codex uses read-only mode with shell tools disabled, Cursor uses Ask with a generated workspace policy denying file, shell, fetch and MCP operations. Its `--trust` flag trusts only that fresh, app-created workspace configuration and does not override deny rules. Antigravity uses Plan with its terminal sandbox, disables slash commands and is refused unless its documented CLI `toolPermission` setting is `strict` (every tool is denied when no interactive approval can be obtained). The app checks that policy setting without changing it; the temporary read buffer is zeroized. The working directory is a fresh temporary folder. These modes are provider controls, not a new OS isolation guarantee for the installed program. Product tool requests return through the existing closed tool table and human approvals.

A local agent can call a cloud model. Existing consent and offline switches still apply. Provider quotas, model access, extra-usage settings and billing remain controlled by that provider. Token counts are recorded where the CLI supplies them; a zero API-price estimate is not a promise of free usage or a monetary cap on a subscription. Selecting a provider does not prove entitlement. A successful inference is required. A failed connection does not silently change to an API-key provider.

Grok’s consumer subscription is not treated as an xAI API key. There is no verified direct Grok desktop adapter in this increment. If the signed-in Cursor account exposes a Grok model, select that model in the Cursor connection; eligibility belongs to Cursor. Otherwise use a separately configured xAI-compatible API provider.

Sources checked 27 October 2026: [Codex](https://developers.openai.com/codex/app-server/), [Claude Code](https://code.claude.com/docs/en/cli-reference), [Cursor](https://cursor.com/docs/cli/headless), [Antigravity](https://antigravity.google/docs/cli/headless/), [xAI account billing](https://docs.x.ai/developers/faq/accounts). Atlas’s `crates/atlas-agent-servers` was inspected as a reference; no Atlas code was copied.

Antigravity setup: in its own CLI settings (`~/.gemini/antigravity-cli/settings.json`), set `"toolPermission": "strict"` before enabling this connection. The app reads only that policy field and discards other settings; it does not open the CLI’s authentication store. This requirement prevents a pre-existing permissive CLI setup from independently executing a model’s tools. Cursor’s generated deny rules use the documented [permission configuration](https://cursor.com/docs/cli/reference/permissions); Antigravity’s strict policy follows its [CLI configuration reference](https://antigravity.google/docs/cli/reference/).

## Reference harnesses (checked 28 October 2026)

T3 Code supports an installed CLI login and a separate **Connect with ChatGPT** consent flow. The installed-CLI route starts the provider's supported executable and exchanges structured events; the executable owns its authentication. The ChatGPT flow obtains authorization for T3 itself. Neither mechanism requires treating browser cookies as an API key. See T3's [Codex provider guide](https://github.com/pingdotgg/t3code/blob/main/docs/user/providers-codex.md), [Claude provider guide](https://github.com/pingdotgg/t3code/blob/main/docs/user/providers-claude.md), and OpenAI's [Codex app-server integration](https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server).

Atlas supports external agents as ACP JSON-RPC subprocesses. Its [architecture](https://github.com/pacifio/atlas/blob/main/docs/architecture.md) distinguishes that from its own in-process agent. Detection of an installed executable is an offer to connect, not evidence that every consumer subscription is available.

This release uses the installed-agent route described above. It does not yet implement an app-specific ChatGPT OAuth connection, Antigravity's separate native sign-in approach, or a verified Grok Build adapter. Provider eligibility, quotas and permission requirements still apply; using a supported harness cannot guarantee an account outcome.
