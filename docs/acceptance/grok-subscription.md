# Grok Build subscription adapter

Source audit: `xai-org/grok-build` commit `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8`, 29 October 2026. The adapter uses the official CLI's ACP process interface. It never reads, copies or parses a provider credential file, browser session, or token. Availability means an executable was found, not that the account is entitled to use a model.

Grok requires a **separate official sign-in** in a dedicated profile. This differs from the existing Claude Code, Codex, Cursor and Antigravity connectors, which use those programs' existing authenticated profiles.

Windows PowerShell setup:

```powershell
$env:GROK_HOME = "$env:USERPROFILE/.verne/local-agents/grok"
grok login
Remove-Item Env:GROK_HOME
```

Linux setup:

```sh
GROK_HOME="$HOME/.verne/local-agents/grok" grok login
```

Use the Grok Build provider preset and `default` for the CLI's selected model, or enter a model supported by your subscription. The user completes the official browser sign-in; Verne does not automate it. Provider plan limits apply. Grok's official CLI may retain its own session history and perform its own diagnostics; this adapter does not claim an ephemeral provider session. No live Grok entitlement or production inference has been tested for this release.

The process receives a fresh temporary working directory and HOME/USERPROFILE/AppData/XDG paths. GROK_HOME also points inside that scratch directory. The official GROK_AUTH_PATH setting points only to the dedicated login profile’s auth.json; the CLI itself reads, refreshes and locks that file. API-key environment variables, parent-agent configuration and inherited provider override variables are removed. Persistent hooks, settings caches, custom configuration, plugin registries and MCP configuration are never loaded, because their paths resolve in the empty scratch home. Credential presence is a metadata check only; linked/reparse ancestors and nonregular auth paths fail closed. Keep the login profile dedicated to Verne.

The adapter starts `grok --no-auto-update agent --no-leader --agent-profile <generated profile> [--model MODEL] stdio`. It exchanges newline-delimited JSON-RPC: `initialize` (ACP 1, no filesystem or terminal capabilities), `authenticate` (`cached_token`, headless), `session/new` (no MCP servers, no automatic approvals), and `session/prompt`. Prompts travel through stdin. Only text from the current session's `agent_message_chunk` updates is returned, and only after an `end_turn` completion. The process tree is closed after completion, timeout, error or caller cancellation. Responses have a cumulative 2 MiB limit. Usage totals are omitted because this adapter has not verified a stable ACP usage schema.

The generated agent definition has `injectDefaultTools: false`, `discoverSkills: false`, `inheritSkills: false`, `agentsMd: false`, `mcpInheritance: none` and `maxTurns: 1`. Upstream rejects an initially empty curated toolset, so the profile seeds `GrokBuild:read_file` and removes that exact identifier with its final deny filter, together with the MCP meta-tools. `GROK_AGENT` prevents model-specific strict harness selection from taking precedence over the generated profile. Every client permission request is cancelled; filesystem and terminal helper requests receive an unsupported-method error and stop the request. Provider-native tool-call notifications also stop the request. App operations can only be requested as JSON from the existing closed product tool table, with the existing product approvals.

The headless `--tools` allowlist was deliberately not used: unknown names restore the full toolset, empty lists inherit tools, and MCP `search_tool`/`use_tool` remain unless expressly denied. Headless prompts also require argv or a real file; they do not consume stdin.

Primary evidence:

- [Agent protocol and flags](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/15-agent-mode.md)
- [Agent definition fields](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-agent/src/config.rs)
- [Tool assembly and final deny filter](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-agent/src/builder.rs)
- [Allowlist behavior](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-agent/src/tool_list.rs)
- [Official separate authentication-file path](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/storage.rs)
- [Cached subscription authentication](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs)
- [Profile precedence](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs)

Protocol fixtures cover successful authentication/text completion, API-key-only rejection, permission cancellation, filesystem helper refusal, provider tool refusal, malformed JSON, incomplete turns, response limits and authentication/configuration separation. Native acceptance results are recorded separately in `ACCEPTANCE-0.1.0-alpha.1.md`.
