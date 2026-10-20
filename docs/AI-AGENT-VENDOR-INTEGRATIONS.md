# AI agent connectors: vendor integrations

> **Status: design, not implemented.** 14 October 2026. For each AI client Dynodoc
> may connect to: transport, authentication flow, installation, administrator
> controls, listing and review requirements, limits, what works on an intranet or
> self-hosted install, and where it sits in the order (Claude first, then ChatGPT,
> then the others). Vendor facts were researched on 14 October 2026 from primary
> sources; each is tagged. **Vendor products change quickly: every row must be
> re-verified at build time.**
>
> Companions: [master design](AI-AGENT-CONNECTORS.md),
> [protocol specification](AI-AGENT-PROTOCOL-SPEC.md),
> [token efficiency](AI-AGENT-TOKEN-EFFICIENCY.md),
> [security](AI-AGENT-SECURITY.md).

## 0. How to read the facts

| Tag | Meaning |
|---|---|
| [V] | Read directly in a primary source (vendor documentation, vendor repository, specification) on 14 October 2026 |
| [V-arch] | Read in an archive snapshot of a primary page because the live page returned 403 from the research machine; snapshot date given |
| [S] | Secondary source only (forum, third-party blog, search summary); do not rely on it |
| [U] | Could not be verified; **check at build time** |

Where two primary sources disagree, both are listed. Quotes are short and verbatim from
the page or its markdown twin. No vendor behaviour is inferred from memory.

## 1. Purpose, goals, non-goals, personas, metrics

### 1.1 Purpose

The owner decided on 14 October 2026: Claude first, then ChatGPT (apps, connectors,
actions), then the others. This document turns that order into concrete integration
requirements and shows what each vendor needs from Dynodoc.

### 1.2 Goals

| # | Goal |
|---|---|
| G1 | One server contract works across vendors; vendor-specific work is configuration, packaging and small profiles. |
| G2 | A buyer can see, per vendor, what works on their network and plan. |
| G3 | Listing and review requirements are known before they become blockers. |
| G4 | The integration set survives vendor churn: facts are dated and sourced. |

### 1.3 Non-goals

- No vendor-specific forks of the tool contract.
- No dependence on vendor-hosted features (their own memory, retrieval, UI widgets) in
  v1; MCP Apps and ChatGPT widgets are out of scope for the first releases.
- No claim that any vendor product is "supported" until the matching acceptance test
  passes.

### 1.4 Personas and metrics

| Persona | Question |
|---|---|
| Enterprise admin | "Can I allow Claude but not ChatGPT, and where does it run?" |
| Individual user | "How do I connect my assistant in five minutes?" |
| Self-hosting operator | "Which assistants can reach my server inside my network?" |
| Product owner | "What do we have to build for each listing, and what will block us?" |

Metrics: time to first successful connection per client (target under 10 minutes for
Claude Desktop and Claude Code); number of clients passing the acceptance script;
listing status per vendor; vendor-change items resolved within 30 days.

## 2. Verified MCP baseline

All rows [V], read 14 October 2026.

| Topic | Fact | Source |
|---|---|---|
| Current revision | 2026-07-28 is "Current"; previous 2025-11-25; revisions also 2025-06-18, 2025-03-26, 2024-11-05 | https://modelcontextprotocol.io/specification/versioning ; repo `docs/specification` |
| Stateless | `Mcp-Session-Id` and protocol sessions removed; no `initialize` handshake; requests carry protocol version and client capabilities in `_meta`; servers must implement `server/discover` | 2026-07-28 changelog |
| Deprecated, not removed | Roots, Sampling, Logging; Dynamic Client Registration (in favour of Client ID Metadata Documents); earliest removal on or after 2027-07-28 | https://modelcontextprotocol.io/specification/2026-07-28/deprecated |
| Tasks | Moved to an official extension `io.modelcontextprotocol/tasks` | changelog |
| Transports | stdio and Streamable HTTP are the standard bindings; HTTP+SSE deprecated ("SHOULD NOT" for new implementations) | transports pages |
| Authorization | Optional; for HTTP transports: OAuth 2.1 draft-13, RFC 9728 protected resource metadata (servers MUST), RFC 8414, RFC 8707 `resource` (clients MUST send), PKCE `S256`, CIMD SHOULD, DCR MAY and deprecated, RFC 9207 `iss`; stdio SHOULD NOT use it and takes credentials from the environment | authorization pages |
| Token rules | Bearer in header only, never query string; server validates audience; MUST NOT accept or forward other tokens | same |
| Tool annotations | Hints only; "clients MUST consider tool annotations to be untrusted unless they come from trusted servers"; defaults `readOnlyHint` false, `destructiveHint` true, `idempotentHint` false, `openWorldHint` true | tools page; `schema.ts` |
| Tool results | `content` blocks plus optional `structuredContent` (any JSON value); `isError` for execution errors; no spec limit on result size found | tools page |
| `tools/list` | Deterministic order recommended (prompt-cache hit rates); `ttlMs` and `cacheScope` required in 2026-07-28; may vary by authorization | tools page; changelog |
| Names | Tool names SHOULD be 1 to 128 characters of `A-Za-z0-9_-.` | tools page |
| SDKs | Tier 1: TypeScript, Python, C#, Go, Rust (`rmcp`), Ruby; Java Tier 2. `rmcp` 3.5.1 published 5 October 2026 and serves 2026-07-28 statelessly | https://modelcontextprotocol.io/community/sdk-tiers ; crates.io |
| Registry | "currently in preview"; metadata only; no private servers; host apps should not consume it directly | https://modelcontextprotocol.io/registry/about |
| Extensions | MCP Apps (`io.modelcontextprotocol/ui`), Tasks, Skills over MCP, client credentials, enterprise-managed authorisation; always opt-in | https://modelcontextprotocol.io/extensions/overview |
| Security best practices | Confused deputy; token passthrough; SSRF; state-handle hijacking; local server compromise; authorisation URL validation; mix-up attacks; localhost redirect impersonation; CIMD trust policies; scope minimisation | https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices |

Implication: build one server on `rmcp`, support both the 2026-07-28 and 2025-11-25
handshakes, ship stdio first and Streamable HTTP second, offer CIMD and DCR, keep
tool lists deterministic, and never rely on annotations for authorisation.

## 3. Cross-client matrix

Cells: yes, no, n/a, or the verified value. [U] means not verified.

| Client | stdio | Streamable HTTP | SSE | Remote auth | Static header | Documented tool cap | Where calls originate |
|---|---|---|---|---|---|---|---|
| Claude Desktop | yes (config file, extensions) | yes (connectors UI) | legacy accepted | OAuth (DCR, CIMD, own client) | beta, up to 4 headers | not stated [U] | Local for stdio; Anthropic ranges for remote connectors |
| Claude Code | yes | yes | deprecated | OAuth (`/mcp`, loopback) | yes | none; tool search at 10 percent of context | User machine |
| claude.ai | no | yes | legacy | OAuth | beta | not stated [U] | Anthropic network, `160.79.104.0/21` |
| Claude Agent SDK | yes | yes | yes | caller passes token | yes | n/a | Your process |
| Messages API MCP connector | no | yes | yes | caller passes token | n/a | n/a | Anthropic API |
| ChatGPT (plugins, custom MCP) | local apps on Desktop only | yes (required for publication) | accepted in custom-server docs | OAuth 2.1, CIMD preferred; no keys | not offered | not stated [U] | OpenAI network (mTLS cert; published IPs) |
| ChatGPT Responses API MCP tool | no | yes | yes | caller passes `authorization` | `headers` | n/a | OpenAI API |
| Codex (CLI, IDE, desktop) | yes | yes | not listed | OAuth, CIMD, DCR; bearer | yes | none stated | User machine |
| Gemini CLI | yes | yes (`httpUrl`) | yes (`url`) | OAuth with DCR | yes | none; names cut at 63 characters | User machine (architecture-implied) |
| Gemini API (Interactions) | no | yes (only) | no | headers | yes | guidance 10 to 20 tools | Google network |
| Gemini Enterprise custom MCP | n/a | yes (only) | no | OAuth 2.0 or none | not read | 100 enabled actions | Google network |
| Copilot Studio | n/a | yes (only) | no (since Aug 2025) | DCR discovery, DCR, manual OAuth | API key header or query | 128 per agent (25 to 30 advised) | Microsoft network (Power Platform connectors) |
| Microsoft 365 Copilot (plugin 2.4) | n/a | not stated [U] | not stated [U] | OAuth static, DCR, Entra SSO | API key not supported for MCP | not stated [U] | Microsoft network |
| Foundry Agent Service | n/a | stated for Functions hosting; otherwise [U] | [U] | OAuth passthrough, keys, Entra | via connection | not stated [U]; 100 s timeout | Microsoft network |
| VS Code (GitHub Copilot) | yes | yes | legacy fallback | DCR then client credentials; `oauth.clientId` | yes | 128 tools per request | User machine |
| GitHub Copilot cloud agent | yes (`local`) | yes | yes | OAuth remote servers not supported | yes | not stated | GitHub runner |
| Cursor | yes | yes | yes | OAuth; fixed redirect; static `auth` | yes | 40 [S] | User machine (desktop) |
| Devin Desktop (ex-Windsurf) | yes | yes | yes | OAuth; `oauthClientId` | yes | 100 (legacy Cascade) | User machine |
| Cline | yes | yes | legacy default if `type` omitted | browser auth in CLI | yes | not stated | User machine |
| Continue | yes | yes (`streamable-http`) | yes | not stated [U] | not stated [U] | not stated | User machine |
| Zed | yes | not named [U] | not named [U] | MCP OAuth prompt | yes | not stated | User machine |
| JetBrains AI Assistant | yes | yes | legacy | not stated [U] | not stated [U] | not stated | User machine |

Sources for each row are in sections 5 to 9 and in the research record.

## 4. Compatibility lint rules for tool schemas

Derived from verified quirks; the contract tests apply them to the tool definitions.

| Rule | Reason | Source |
|---|---|---|
| Tool names at most 63 characters; letters, digits, `_` only in the core set | Gemini CLI truncates names over 63; Anthropic directory 64; M365 functions `^[A-Za-z0-9_]+$` | Gemini CLI `mcp-server.md`; Anthropic review criteria; plugin manifest 2.4 |
| Server name `dynodoc`: no hyphen, no underscore | Gemini API asks `snake_case` without `-`; Gemini CLI policy parser splits on the first underscore after `mcp_` | Gemini function-calling docs; Gemini CLI docs |
| Avoid `exclusiveMinimum` integers, type arrays and `oneOf` in input schemas | Copilot Studio known issues (`FormatException`, truncated multi-type schemas) | Copilot Studio troubleshooting page |
| Avoid reference-type inputs (`$ref`) | Copilot Studio filters tools with reference-type inputs | same |
| Avoid `default` inside `anyOf`; do not depend on `additionalProperties` being honoured | Gemini CLI strips `$schema` and `additionalProperties` and defaults inside `anyOf` | Gemini CLI docs |
| Keep schemas shallow | Gemini API "may reject very large or deeply nested schemas" | Gemini function-calling docs |
| Explicit boolean annotations on every tool | OpenAI plugin guidelines; Anthropic directory | plugin guidelines; review criteria |
| `title` on every tool | Anthropic directory requirement | submission page |
| Enum-valued parameters have string values or integers consistently | Copilot Studio interprets enum inputs as string | Copilot Studio troubleshooting |
| One tool does either reads or writes, never both | Anthropic review criteria reject catch-all read and write tools; OpenAI asks to split by risk | review criteria; plugin guidelines |
| Server `instructions` at most 500 characters, key content first | Claude Code truncates at 4,096; ChatGPT: "Keep the most important details in the first 512 characters" | Claude Code MCP docs; OpenAI MCP server guide |
| No sentences addressed to other tools or the model beyond the tool's own use | Anthropic review criteria prohibit prompt-injection patterns in descriptions | review criteria |

The current draft `tools.json` (protocol spec 7) removes a type-less `nth` property
for this reason: the quirk list above shows type-less or union-typed properties are
the likeliest to break a vendor's schema import.

## 5. Anthropic: Claude (priority 1)

### 5.1 Surfaces and what Dynodoc ships for each

| Surface | Transport and auth | Install path | Phase |
|---|---|---|---|
| Claude Code | stdio with device-key session (Phase 0 to 1); Streamable HTTP with OAuth (Phase 2) | `claude mcp add`, `.mcp.json`, or the Dynodoc plugin | 0 |
| Claude Desktop | stdio via `claude_desktop_config.json` (Phase 0 to 1); remote via Connectors UI (Phase 2) | JSON edit; optional `.mcpb` bundle | 0 |
| Claude Agent SDK | stdio, HTTP or SSE; caller supplies tokens | `mcpServers` option | 1 |
| claude.ai (web, mobile) and Desktop connectors | Streamable HTTP + OAuth | Settings, "Add custom connector" | 2 |
| Messages API MCP connector | Streamable HTTP or SSE, tools only, caller supplies token | `mcp_servers` + `mcp_toolset` | 2 (documentation only) |
| Connectors directory | Remote server + review | claude.ai/directory/manage | 2 |

### 5.2 Facts by surface

#### Claude Code [V]

- Add: `claude mcp add --transport http <name> <url>` (recommended remote);
  `--transport sse` deprecated; stdio `claude mcp add <name> -- <command> [args]`.
  Scopes: local (default), project (`.mcp.json`, approval prompt), user.
  Source: https://code.claude.com/docs/en/mcp.md
- OAuth: `/mcp` or `claude mcp login <name>`; `--callback-port`, `--client-id`;
  environment expansion `${VAR}` in `command`, `args`, `env`, `url`, `headers`.
- Output: "displays a warning when MCP tool output exceeds 10,000 tokens and limits
  output to 25,000 tokens by default" (`MAX_MCP_OUTPUT_TOKENS`); text over 50,000
  characters is saved to a file; per-tool `_meta["anthropic/maxResultSizeChars"]` up
  to 500,000.
- Tool search on by default; "Only tool names and server instructions load at session
  start"; `auto` defers when definitions reach 10 percent of context; server
  instructions and upfront descriptions cut at 4,096 characters.
- v2 runtime negotiates 2026-07-28 and falls back to the earlier handshake;
  `MCP_PROTOCOL_NEGOTIATION` overrides.
- Managed policy: `managed-mcp.json` (exclusive control), `allowedMcpServers` and
  `deniedMcpServers` by `serverUrl` (wildcards), `serverCommand`, `serverName` (not a
  security control), `allowManagedMcpServersOnly`.
- Plugins: `.claude-plugin/plugin.json`; can bundle skills, commands, agents, hooks,
  `.mcp.json`, MCP servers; tool names `mcp__plugin_<plugin>_<server>__<tool>`;
  marketplaces by `.claude-plugin/marketplace.json`. Skills follow the Agent Skills
  open standard (`SKILL.md`; description plus `when_to_use` cut at 1,536 characters in
  listings). https://code.claude.com/docs/en/plugins-reference.md

**Dynodoc plan.** Ship a plugin `dynodoc` in the engine repository (marketplace file
included): `skills/dynodoc/SKILL.md` (DCT legend, op grammar, error recovery, review
etiquette; the "usage guide at zero fixed cost"), `.mcp.json` (stdio server with
`${DYNODOC_*}` variables), optional `/dynodoc:workflow` command.

#### Claude Desktop [V]

- Local servers: Settings > Developer > Edit Config opens `claude_desktop_config.json`
  (macOS `~/Library/Application Support/Claude/`, Windows `%APPDATA%\Claude\`); `{ "mcpServers": { name: { command, args, env } } }`; restart required.
  https://modelcontextprotocol.io/docs/develop/connect-local-servers
- Desktop Extensions `.mcpb` (renamed from `.dxt`): zip of a local server plus
  `manifest.json`; install by double click or Settings > Extensions. **The directory
  no longer accepts MCPB submissions; to distribute through the directory, include the
  server in a plugin.** https://claude.com/docs/connectors/building/mcpb.md
- Admin (Team and Enterprise, Owners only): enable or disable extensions; allowlist
  (off by default; turning it on removes existing installs); needs Desktop 0.13.91 or
  later. https://support.claude.com/en/articles/12592343-enabling-and-using-the-desktop-extension-allowlist
- Linux availability: MCP docs say yes, Anthropic's MCPB page says macOS and Windows
  only (**unresolved**).

**Dynodoc plan.** Document the JSON snippet (section 12.1). Offer an `.mcpb` for
sideloading only if a customer asks, because it is not a listing path.

#### claude.ai custom connectors (remote MCP) [V]

Source: https://claude.com/docs/connectors/custom/add-unlisted.md and sibling pages.

| Item | Fact |
|---|---|
| Plans | Free (one custom connector), Pro, Max, Team, Enterprise |
| Who adds | Team: an Owner adds for the organisation, members connect with their own account; Enterprise: members with a custom role that can manage libraries |
| Transport | Streamable HTTP; legacy HTTP+SSE "being deprecated"; URL ending `/sse` selects SSE |
| Auth modes | "Sign in now", "Sign in when needed", none; OAuth client: Claude's published identity (CIMD, default), automatic registration (DCR), or your own client; up to four static request headers (beta) |
| OAuth rules | Needs a `401`; uses the first `authorization_servers` entry; CIMD only if the metadata advertises `client_id_metadata_document_supported: true` and `"none"` in `token_endpoint_auth_methods_supported`, otherwise DCR; PKCE `S256` always; appends `offline_access` when advertised; no `client_credentials` |
| Callback | `https://claude.ai/api/mcp/auth_callback` (web, Desktop, mobile) |
| Specs followed | "2025-03-26, 2025-06-18, and 2025-11-25 authorization specifications" |
| Features | Tools, prompts, resources; text and image results; not resource subscriptions, sampling or draft capabilities; MCP Apps supported |
| Limits | About 150,000 characters per tool result; 240 seconds per call; discovery and token endpoints must answer within 10 seconds; tool-count cap not stated [U] |
| Egress | Outbound traffic from `160.79.104.0/21` |
| Private servers | Allowlist that range, or MCP tunnels (research preview, Enterprise by request; outbound-only `cloudflared`) |
| Tool permissions | Per-user and per-organisation per-tool controls; `readOnlyHint: true` tools "can run without per-call confirmation, and destructive tools always prompt" |
| Unknown | Whether claude.ai itself negotiates 2026-07-28 [U]; elicitation support [U]; organisation-level URL allow or deny list [U] |

**Dynodoc requirements derived.** Streamable HTTP at one URL; `401` with
`resource_metadata`; authorisation-server metadata advertising CIMD support and
`none`; DCR endpoint as fallback; exact callback registered where DCR or manual client
is used; token endpoint accepts `application/x-www-form-urlencoded`; answers within
10 seconds; refresh tokens with `offline_access`; no redirect to another host on
token endpoints (the `Authorization` header is dropped on cross-host redirects).

#### Claude Agent SDK [V]

`options.mcpServers` (TypeScript) or `mcp_servers` (Python): stdio, `http`, `sse` or
in-process. Tool names `mcp__<server>__<tool>`; `allowedTools` accepts wildcards after
the literal prefix. "doesn't open a browser or run an interactive OAuth flow": pass a
token in `headers`. Prefer `allowedTools` over permission modes; `bypassPermissions`
auto-approves MCP tools; annotations are "metadata, not enforcement".
https://code.claude.com/docs/en/agent-sdk/mcp.md

**Dynodoc plan.** Document how to pass an agent API key (Path C) or an OAuth token.

#### Messages API MCP connector [V]

Beta (`mcp-client-2025-11-20`); `mcp_servers: [{type:"url", url, name,
authorization_token}]` plus `tools: [{type:"mcp_toolset", mcp_server_name}]`; URL must
be `https://`; tools only (no resources or prompts); stdio servers cannot be connected;
the caller obtains and refreshes OAuth tokens; "zdr: not-eligible". Token counting
rejects requests that use it. https://platform.claude.com/docs/en/agents-and-tools/mcp-connector.md

#### Connectors directory [V]

Sources: https://claude.com/docs/connectors/building/submission.md ,
https://claude.com/docs/connectors/building/review-criteria.md ,
https://claude.com/docs/directory/publish.md

| Requirement | Dynodoc status |
|---|---|
| Submit at claude.ai/directory/manage; Pro, Max, Team or Enterprise account; Owner on Team and Enterprise | Needs an Anthropic paid account held by the operator |
| Every tool has `title` and the applicable `readOnlyHint` or `destructiveHint` | Designed (protocol spec 7.1) |
| OAuth 2.0 for authenticated services | Phase 2 |
| Privacy policy URL, documentation URL, support contact, icon | **Privacy policy is a draft "not yet effective"** (dependency) |
| Fully populated test account | To prepare (synthetic project) |
| Tool names at most 64 characters; no catch-all read and write tool | Designed |
| No prompt-injection patterns in descriptions; responses "reasonably sized" | Designed; budgets |
| Remote servers: recognised CA certificates; Streamable HTTP (SSE "temporarily acceptable") | Phase 2 |
| At least three working example prompts | To write |
| Own first-party APIs only; server domain should match the service | Satisfied (operator-owned domain) |
| Prohibited: money transfer, AI image, video or audio generation, ads | Not applicable |
| Review: automated scan; listed as Community connector by default; some get human functional review; timing "isn't fixed" | Plan buffer |
| Auth modes for directory: DCR, CIMD, Anthropic-held credentials; "prefer CIMD or `oauth_anthropic_creds` over DCR" for high traffic | CIMD primary |

## 6. OpenAI: ChatGPT, Codex, API (priority 2)

Terminology note [V]: since late 2025 OpenAI's docs call connectors "apps" (renamed 18
December 2025 per a Help Center snapshot, [V-arch]) and the Apps SDK documentation
now lives under "plugins" (`developers.openai.com/plugins`); a plugin bundles skills,
MCP servers and optional UI. Older material uses the older names.

### 6.1 Surfaces

| Surface | Transport and auth | Install | Phase |
|---|---|---|---|
| ChatGPT web, custom MCP server in a plugin | Streamable HTTP, OAuth 2.1 (CIMD preferred; DCR supported; PKCE `S256`; `resource`); no API keys or client credentials | chatgpt.com/plugins > "Add custom MCP server" | 3 |
| ChatGPT public directory | Same + review | Platform dashboard submission | 3 |
| Deep research and company knowledge | Same server; `search` and `fetch` read-only tools | Same | 3 |
| Custom GPT Actions | OpenAPI + none, API key or OAuth | GPT editor | not planned (retiring) |
| Responses API MCP tool | `server_url`, `authorization`, `allowed_tools`, `require_approval` | API call | 3 (documentation) |
| Agents SDK | Hosted, Streamable HTTP, stdio, SSE | code | 3 (documentation) |
| Codex CLI, IDE, desktop | stdio or Streamable HTTP; OAuth | `~/.codex/config.toml` | 0 (snippet) |

### 6.2 ChatGPT custom MCP server and plugins [V unless stated]

Sources: https://developers.openai.com/plugins/build/mcp-server ,
https://developers.openai.com/plugins/build/auth ,
https://developers.openai.com/api/docs/guides/custom-mcp-server

- **Transport.** "Support the MCP streamable HTTP transport"; stable URL typically
  ending `/mcp`. The custom-server guide also says "Supported MCP protocols: SSE and
  streaming HTTP" (conflict noted; build for Streamable HTTP).
- **Spec revision.** None single; 2025-06-18 and 2025-11-25 features are cited, and
  2026-07-28 for authorisation-response validation and MCP Events.
- **Auth.** OAuth 2.1 conforming to the MCP authorisation spec; RFC 9728 metadata;
  `code_challenge_methods_supported` must include `S256` or the server is
  unsupported; CIMD preferred when the authorisation server supports it (client id
  `https://chatgpt.com/oauth/client.json`), DCR supported, predefined clients
  supported; `resource` sent to authorisation and token endpoints; redirect URI
  `https://chatgpt.com/connector_platform_oauth_redirect` (or a per-connection
  callback). "ChatGPT does **not** support machine-to-machine OAuth grants such as
  client credentials, service accounts, or JWT bearer assertions, nor can it present
  custom API keys or customer-provided mTLS certificates."
- **Identification.** ChatGPT presents an OpenAI-managed mTLS client certificate
  (SAN `mtls.prod.connectors.openai.com`) and publishes egress IPs at
  https://openai.com/chatgpt-connectors.json; "An IP allowlist does not replace request
  authentication."
- **Install.** Web only: plus button, "Add custom MCP server", enter URL (with `/mcp`)
  or choose a tunnel, configure auth, accept the risk warning, "Create as a plugin".
  Elevated-risk label shown. Mobile: "No - web only" [V-arch]. Local MCP apps run only
  in ChatGPT Desktop [V-arch].
- **Plans.** [V-arch] Business and Enterprise/Edu: apps, full MCP including write
  actions, developer mode (beta); Pro: read and fetch permissions only; Plus and
  Free: not documented [U]. The developers page says ChatGPT "supports both read and
  write tools" without a plan split (conflict recorded).
- **Writes.** "Write actions by default require confirmation"; tools without
  `readOnlyHint` are treated as write actions; "ChatGPT currently requires manual
  confirmation in any conversation before write actions can be taken" and writes may
  occur even if a server tagged a tool read-only (OpenAI's own warning).
- **Admin.** [V] Layers: availability, MCP server access by role, action control
  (read-only or custom set, treatment of new actions), app permissions. [V-arch]
  Enterprise/Edu: RBAC, plugins disabled by default; Business: workspace-wide;
  "Only Admins and Owners can publish apps"; published apps use a frozen snapshot of
  tools, "New actions are disabled by default, and changes to existing actions are
  shown as a diff"; logs available through the Compliance API depending on
  configuration.
- **Limits.** No tool count, result size or timeout documented for a public endpoint
  [U]. Documented: server `instructions` first 512 characters; tunnel defaults 10 MiB
  per request or response and 120 s deadline (tunnel traffic only).
- **Intranet.** Needs a public HTTPS endpoint or OpenAI's Secure MCP Tunnel (not
  accepted for public submission) [V].
- **Warnings.** OpenAI documents prompt-injection risk, "Trusting a MCP's developer
  does not make this safe", and "Even 'read-only' MCPs can embed prompt-injection
  payloads in search results" (deep research guide).

### 6.3 Deep research and company knowledge

- Server should implement read-only `search` (`{query}` returning `results[]` of `id`,
  `title`, `url`) and `fetch` (`{id}` returning `id`, `title`, `text`, `url`,
  `metadata`), returned as `structuredContent` and the same JSON as text for
  compatibility; citation needs a non-empty `url`.
- [V-arch] ChatGPT custom apps "no longer" need these for ordinary use; company
  knowledge needs at least one app with search and fetch; write actions are not
  available through company knowledge; apps with interactive UI unsupported there.
- API deep research models "don't support tool calls or MCP servers that don't
  implement this interface"; `require_approval` must be `never` (conflict: the API
  guide says required, the ChatGPT help article says no longer required; likely
  different surfaces).

**Dynodoc plan.** `chatgpt` profile adds `search` and `fetch` mapped to `find` and
`read` (protocol spec 13.5). Doc URLs returned are review links within the app, which
require sign-in: citation URLs must be openable by the user.

### 6.4 Public directory (plugin submission) [V]

Source: https://developers.openai.com/plugins/deploy/app-review ,
https://developers.openai.com/plugins/deploy/submission ,
https://developers.openai.com/plugins/plugin-guidelines

| Requirement | Dynodoc status |
|---|---|
| Verified organisation (individual or business) in the Platform dashboard | Business verification for Verne Systems needed |
| `api.apps.write` permission; global data-residency project ("projects with EU data residency cannot submit plugins with MCP servers") | Operational |
| Publicly accessible HTTPS server; no local or test endpoint | Yes (Phase 2 host) |
| Domain verification file `/.well-known/openai-apps-challenge` | Easy |
| Website, support, privacy policy and terms URLs, HTTPS, at most 1,024 characters | **Policies are drafts** (dependency) |
| Demo account that works without MFA, magic links or private network | Synthetic account; magic-link sign-in is the current method, so a **password-less test login path is needed** (open issue) |
| 5 positive and 3 negative test cases; video walkthrough; release notes | To write |
| Explicit booleans for `readOnlyHint`, `destructiveHint`, `openWorldHint` | Designed |
| Single MCP server per plugin; no non-empty server headers; auth none, OAuth or mixed | Fine |
| Daily rescans; origin cannot change; changed tools wait for checks | Operational |
| No collection of credentials, PHI, full chat log; privacy policy lists categories, purposes, recipients, retention | Policy work |
| Prohibited: ads, digital commerce, money or crypto transfers, trade execution, etc. | Not applicable |

The sign-in finding matters: Dynodoc's login is an emailed link, and OpenAI's reviewers
need a test account "without MFA approval, email or SMS codes, magic links". A
dedicated reviewer account with a long-lived credential path (for example a
password-protected synthetic tenant or an API-key based consent) must be designed
before submission.

### 6.5 Custom GPT Actions [V / V-arch]

- Auth none, API key (basic, bearer, custom header) or OAuth; limits: 300 characters
  per endpoint description, 700 per parameter, request and response under 100,000
  characters, 45 seconds, no custom headers, text only; no stated OpenAPI version or
  size cap [U] (the official example uses `openapi: 3.1.0`).
- **Status.** [V-arch] "We're planning to retire custom GPTs ... For affected
  Enterprise workspaces, retirement is planned for Dec 11, 2026"; "Custom actions do
  not transfer through the migration workflow" (learn.chatgpt.com migration page [V]).
  Actions remain in API docs without a banner.
- **Decision.** Do not build an Actions-specific integration. The OpenAPI document
  generated for the HTTP binding (protocol spec 12) suffices for anyone who still uses
  Actions: use API-key auth (Path C), mind the 100,000-character cap (our default
  `read` is 16,000 bytes), and set `x-openai-isConsequential: true` on `propose`.

### 6.6 Responses API, Agents SDK, Codex [V]

- Responses API tool `{"type":"mcp","server_label","server_url" | "connector_id" |
  "tunnel_id","allowed_tools","require_approval","authorization","headers",
  "defer_loading"}`; approval requested before data is shared by default;
  `connector_id` deprecated for models released after 1 September 2026.
- Agents SDK: hosted, Streamable HTTP, stdio; SSE class exists "deprecated by the MCP
  project".
- Codex: `~/.codex/config.toml` or `.codex/config.toml` (trusted projects),
  `[mcp_servers.<name>]`; stdio keys `command`, `args`, `env`; HTTP keys `url`, `auth`
  (`oauth` default), `bearer_token_env_var`, `http_headers`; `startup_timeout_sec`
  default 10, `tool_timeout_sec` default 60; `enabled_tools`, `disabled_tools`,
  `default_tools_approval_mode` (`auto`, `prompt`, `writes`, `approve`; `writes`
  prompts for tools not marked read-only); `output_token_limit` per tool; server
  `instructions` first 512 characters self-contained. `codex mcp add`, `codex mcp
  login`. Source: https://learn.chatgpt.com/docs/extend/mcp

### 6.7 Data handling statements to reflect in consent and policy [V / V-arch]

- Business, Enterprise and Edu: no training on information accessed through apps by
  default; for Free, Plus, Go, Pro it may be used if "Improve the model for everyone"
  is on. Data sent to a third-party MCP server follows that server's terms.
  Data residency "does not mean that every processing step ... or third-party app
  remains in the selected region."
- Developers are told: "Assume prompt injection and malicious inputs will reach your
  server. Check every input and keep audit logs."

## 7. Google: Gemini (priority 3)

| Surface | Facts | Source |
|---|---|---|
| Gemini CLI | Servers in `mcpServers` of `settings.json` (`~/.gemini/settings.json` or `.gemini/settings.json`); stdio (`command`), SSE (`url`), Streamable HTTP (`httpUrl`); `includeTools`, `excludeTools`, `trust`, `timeout` (default 600,000 ms); OAuth with DCR and `iss` validation; tool names `mcp_{server}_{tool}` truncated at 63 characters; `$schema` and `additionalProperties` removed. Extensions: `gemini-extension.json` with `mcpServers`, `contextFileName`, `settings` (sensitive values to keychain); installed from a GitHub URL. Enterprise: system settings `mcp.allowed`, `mcp.excluded`, `admin.mcp.enabled`, `admin.mcp.config`, `admin.mcp.requiredConfig`. Gallery: GitHub topic `gemini-cli-extension`, crawled daily, validation unspecified. [V] | github.com/google-gemini/gemini-cli `docs/tools/mcp-server.md`, `docs/extensions/*`, `docs/cli/enterprise.md` |
| Gemini API (Interactions) | "Remote MCP only works with Streamable HTTP servers. SSE ... not supported"; tool type `mcp_server` with `name`, `url`, `headers`, `allowed_tools`; guidance 10 to 20 tools; managed agents call from Google's sandbox. [V] | ai.google.dev function-calling page; Google blog 7 July 2026 |
| Gemini Enterprise custom MCP | StreamableHTTP only; at most 100 enabled actions; OAuth 2.0 or none; redirect `https://vertexaisearch.cloud.google.com/oauth-redirect`; self-signed certificates unsupported; egress FQDN allowlisting; admin must override a default organisation-policy block and hold `roles/discoveryengine.editor`; GA per release note 7 August 2026 (an earlier note said Public Preview for the unauthenticated variant). [V] | docs.cloud.google.com Gemini Enterprise connectors pages and release notes |
| Google-hosted MCP servers | Google Cloud's own remote servers; not a route for hosting ours. [V] | docs.cloud.google.com/mcp/overview |
| ADK and Agent Platform | `McpToolset` over stdio, SSE, Streamable HTTP; deploy servers as separate Streamable HTTP services; reaching private MCP servers from the managed runtime [U] | adk.dev |

**Dynodoc plan.** Gemini CLI: stdio snippet and an extension manifest in the engine
repository (Phase 1; cost near zero). Gemini Enterprise: Phase 4, public HTTPS
required; keep the enabled-actions count under 100 (eight). CIMD support in Gemini CLI
is [S]; rely on DCR there.

## 8. Microsoft and GitHub (priority 3, enterprise)

### 8.1 Copilot Studio [V]

Sources: https://learn.microsoft.com/en-us/microsoft-copilot-studio/mcp-add-existing-server-to-agent ,
`.../add-tools-custom-agent`, `.../admin-data-loss-prevention`,
`.../requirements-quotas`, `.../mcp-troubleshooting`.

| Item | Fact |
|---|---|
| Transport | "Currently, Copilot Studio supports the Streamable transport type"; SSE no longer supported after August 2025 |
| Features | MCP tools and resources (not prompts); generative orchestration required |
| Onboarding | Wizard (Tools > Add a tool > Model Context Protocol: name, description, URL) or custom connector from an OpenAPI (swagger 2.0) file with `x-ms-agentic-protocol: mcp-streamable-1.0`; or Agents 365 CLI registration |
| Auth | None; API key (header or query); OAuth 2.0 "Dynamic discovery", "Dynamic" (DCR), or "Manual". CIMD not documented [U] |
| Admin | Power Platform data policies apply to MCP servers as connectors; blocking the connector blocks its tools; tenant policies; tools can be disabled by policy |
| Limits | 128 tools per agent (25 to 30 recommended); connector payload 5 MB (public cloud; applicability to MCP results [U]); tool-call timeout [U] |
| Known issues | Integer `exclusiveMinimum` throws `FormatException`; multi-type arrays truncated; reference-type inputs filtered out; enums read as strings |
| Listing | Optional publication of the connector across tenants via custom connector certification [requirements not read, U] |
| Intranet | Calls go through Power Platform connectors from Microsoft's cloud. VNet support lets connectors reach on-premises or Azure resources, with a well-known CA chain (no custom root CA); MCP over VNet not stated [U]. Community answers advise a public HTTPS front end [S] |

### 8.2 Microsoft 365 Copilot (declarative agents, plugins) [V]

Plugin manifest 2.4 adds `RemoteMCPServer` runtime (`url`, `mcp_tool_description`);
auth: Entra SSO, DCR (MCP only), OAuth authorization code, none; **API key not
supported for MCP plugins**; OAuth redirect
`https://teams.microsoft.com/api/platform/v1.0/oAuthRedirect`; function names
`^[A-Za-z0-9_]+$`; descriptions beyond 2,048 characters may be ignored; dynamic tool
discovery screens each new or changed tool with RAI and XPIA classifiers. Agent
connectors need "a secure public endpoint" ("The endpoint must be publicly
accessible"). Transport named explicitly, tool count and result size: not stated [U].
Listing: publisher attestation and Microsoft 365 certification for ISV apps.
https://learn.microsoft.com/en-us/microsoft-365/copilot/extensibility/plugin-manifest-2.4

### 8.3 Microsoft Foundry Agent Service [V]

Remote MCP endpoints only; `require_approval` default `always`; auth by key, Entra
agent identity or project managed identity, OAuth passthrough (same tenant only),
unauthenticated; non-streaming MCP calls time out at 100 seconds; private MCP only
with Standard Agent Setup and BYO VNet. OpenAPI 3.0 and 3.1 tools are also supported
directly. https://learn.microsoft.com/en-us/azure/foundry/agents/how-to/tools/model-context-protocol

### 8.4 GitHub Copilot in VS Code and on GitHub.com [V]

- VS Code: `.vscode/mcp.json` (top-level key to confirm at build time [U]),
  `type` `stdio`, `http`, `sse` ("VS Code tries HTTP first and falls back to SSE");
  install link `vscode:mcp/install?{json}`; 128 tools per request; OAuth by DCR with
  client-credentials fallback and `oauth.clientId` for pre-registered clients;
  settings `chat.mcp.access`.
- Organisation policy "MCP servers in Copilot" disabled by default; allowlist GA via
  `managed-settings.json` (`allowedMcpServers`, `deniedMcpServers`, deny wins);
  "Registry only" mode is public preview and "can be bypassed by editing
  configuration files".
- GitHub.com cloud agent: `http`, `sse`, `local`; "do not currently support remote
  MCP servers that leverage OAuth"; secrets via `COPILOT_MCP_` variables.
- GitHub MCP Registry (VS Code `@mcp` gallery): publish to the MCP Registry, then
  request inclusion (October 2025 process; current process [U]).

## 9. Local coding agents and IDEs (priority 3; near-zero cost)

| Client | Config | Facts [V] | Plan |
|---|---|---|---|
| Cursor | `.cursor/mcp.json` or `~/.cursor/mcp.json`; deeplink `cursor://anysphere.cursor-deeplink/mcp/install?name=…&config=<base64>` | Transports stdio, SSE, Streamable HTTP; OAuth with fixed redirect URLs and static `auth` (`CLIENT_ID`, `CLIENT_SECRET`); enterprise allowlist (Enterprise only) by command or URL pattern; 40-tool warning is [S] | Snippet and deeplink in docs (Phase 1) |
| Codex | `~/.codex/config.toml` | Section 6.6 | Snippet (Phase 0) |
| Devin Desktop (ex-Windsurf) | `~/.config/devin/mcp_config.json` (macOS, Linux), `%APPDATA%\devin\mcp_config.json` | Windsurf renamed 2 June 2026; "Cascade has been removed" 8 September 2026; legacy Cascade limit 100 tools; allowlist by regex in team settings; `oauthClientId` for pre-registered clients | Snippet; verify the Devin Local file path [U] |
| VS Code | Section 8.4 | | Snippet |
| Cline | `cline_mcp_settings.json` | Streamable HTTP recommended; omitting `type` defaults to legacy SSE; marketplace by GitHub issue with 400x400 logo and README | Optional marketplace entry |
| Continue | `.continue/mcpServers/*.yaml` | `type`: `sse`, `stdio`, `streamable-http`; agent mode only | Snippet |
| Zed | `context_servers` in settings | Prompts for MCP OAuth when no Authorization header; tools and prompts only | Snippet |
| JetBrains AI Assistant | Settings > Tools > AI Assistant > MCP | STDIO, Streamable HTTP, SSE (legacy); import from Claude | Snippet |

Local clients call from the user's machine, so stdio and intranet HTTP both work
wherever that machine can reach the Dynodoc API ([architecture-implied]; no vendor
sentence states it for most of them).

## 10. Generic REST and OpenAPI

| Consumer | OpenAPI handling [V] | Plan |
|---|---|---|
| Copilot Studio REST tool | Upload OpenAPI; must be v2 JSON; v3 auto-translated | Provide generated file |
| M365 declarative agent plugin | Runtime `OpenApi` with `spec.url`; functions from `operationId` | Same |
| Foundry | OpenAPI 3.0 and 3.1 tools; key, managed identity or anonymous auth | Same |
| Google ADK | `OpenAPIToolset`, v3.x; names from `operationId` snake_cased, max 60 | Same |
| Custom GPT Actions | Section 6.5 | Same; legacy |
| Others in this document | No direct OpenAPI import documented [U] | Use MCP |

The generated document is `GET /agent/openapi.json` (proposed), derived from the same
schema source as the MCP tools. Requirements: typed bodies (the existing
`openapi.yaml` has 176 untyped bodies), `operationId` equal to tool names, short
summaries (300-character cap in Actions), API-key and OAuth security schemes, and an
`x-openai-isConsequential` marker on write operations.

## 11. Priority and sequencing

The owner's order is Claude, then ChatGPT, then the others. Cost is dominated by two
pieces of shared work (the read-only stdio server and the OAuth authorisation server);
once those exist, each additional client is configuration and a listing.

| Order | Client | Why this position | Dependency | Phase | Effort beyond shared work |
|---|---|---|---|---|---|
| 1 | Claude Code (plugin) | Skills carry usage guidance at zero fixed cost; local stdio; fastest feedback; plugin marketplace is self-published | Phase 0 server | 0 | 0.5 week |
| 1 | Claude Desktop (stdio) | Largest likely audience for document tasks; config file | Phase 0 | 0 | 0.25 week |
| 1 | Claude Agent SDK, Messages API | Integrators | Phase 1 tokens | 1 | 0.25 week (docs) |
| 1 | claude.ai custom connector | Remote OAuth; Team owners can add; needs public endpoint | Phase 2 | 2 | 1 week (matrix testing) |
| 1 | Anthropic connectors directory | Discovery and trust | Legal pages adopted; test account | 2 | 1 to 2 weeks plus review time |
| 2 | ChatGPT custom MCP server | OpenAI's remote path; OAuth 2.1 with CIMD | Phase 2 server; `chatgpt` profile | 3 | 1.5 weeks |
| 2 | ChatGPT directory | Discovery; verified organisation; reviewer account without magic link | Business verification; reviewer login path | 3 | 2 to 3 weeks plus review time |
| 2 | Responses API and Agents SDK | Integrators | Phase 2 | 3 | 0.25 week (docs) |
| 2 | Codex | Local stdio; config snippet | Phase 0 | 0 | 0.1 week |
| 3 | Gemini CLI, Cursor, VS Code, Devin Desktop, Cline, Continue, Zed, JetBrains | Local stdio snippets | Phase 0 | 1 | 0.1 week each |
| 3 | Copilot Studio, Microsoft 365 Copilot, Gemini Enterprise, Foundry | Enterprise buyers; public HTTPS; admin gates | Phase 2; tenant model | 4 | 1 to 1.5 weeks each |
| 3 | OpenAPI-based clients | Integrators | Phase 2 | 2 | 0.5 week (typed spec) |
| n/a | Custom GPT Actions | Retiring | none | none | 0 |

Rules: a client is "supported" only when its acceptance script (section 14) passes in
CI against a release candidate; "documented" means a snippet exists without a test.

## 12. Artifacts per client

### 12.1 Configuration snippets (proposed; verify keys at build time)

Claude Code, `.mcp.json` (project) with environment references:

```json
{
  "mcpServers": {
    "dynodoc": {
      "command": "dynodoc-mcp",
      "args": ["--remote", "${DYNODOC_REMOTE}", "--project", "${DYNODOC_PROJECT}",
               "--device-key", "${DYNODOC_KEY}"]
    }
  }
}
```

Claude Code, remote (Phase 2): `claude mcp add --transport http dynodoc https://mcp.example.com/mcp`
then `/mcp` to sign in.

Claude Desktop, `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dynodoc": {
      "command": "dynodoc-mcp",
      "args": ["--remote", "/path/remote.json", "--project", "<uuid>", "--device-key", "/path/agent.der"]
    }
  }
}
```

Codex, `~/.codex/config.toml`:

```toml
[mcp_servers.dynodoc]
command = "dynodoc-mcp"
args = ["--remote", "/path/remote.json", "--project", "<uuid>", "--device-key", "/path/agent.der"]
tool_timeout_sec = 60
default_tools_approval_mode = "writes"
```

Gemini CLI, `settings.json`:

```json
{
  "mcpServers": {
    "dynodoc": {
      "command": "dynodoc-mcp",
      "args": ["--remote", "/path/remote.json", "--project", "<uuid>", "--device-key", "/path/agent.der"],
      "trust": false
    }
  }
}
```

Cursor (`mcpServers` assumed), VS Code (`servers` assumed): same command and arguments;
confirm top-level keys against current vendor docs before publishing.

The key file path is a secret-adjacent value; the guide recommends a per-user
location with owner-only permissions and, where a client supports it, an environment
variable instead of a literal path.

### 12.2 Plugin layout (Claude Code)

```
dynodoc-plugin/
  .claude-plugin/plugin.json          name, version, description, userConfig for remote URL and project
  .mcp.json                           stdio server definition (above)
  skills/dynodoc/SKILL.md             DCT legend, op grammar, error recovery, review etiquette
  commands/dynodoc-workflow.md        optional slash command with the recommended sequence
  README.md                           install, grants, revocation
```

Plugin skill budget: description plus `when_to_use` under 1,536 characters; body
as needed (loads on demand).

### 12.3 Acceptance scripts

One scripted scenario per client, run in CI where automatable and by hand per release
where not:

1. Connect (local or remote) and list tools; assert tool count and annotations.
2. `files`, `outline`, `read` on a fixture project; assert byte counts within bounds.
3. `propose` a one-block change; assert a private draft owned by the agent identity
   and no new team event.
4. Revoke the grant; assert the next call fails.
5. Remote only: complete OAuth with the client's real consent flow; assert PKCE,
   `resource`, refresh rotation.

## 13. Intranet and self-hosted summary

| Scenario | Works | Does not work without extra setup |
|---|---|---|
| Local stdio client (Claude Desktop and Code, Codex, Cursor, Gemini CLI, VS Code, others) with the Dynodoc API reachable from the machine | Yes, any network | n/a |
| Local client with remote OAuth to a private HTTPS server | Yes if the machine reaches it | Certificate must be trusted by the client's runtime |
| claude.ai connectors to a private server | Needs public HTTPS reachable from Anthropic's `160.79.104.0/21` (allowlist), or MCP tunnels (Enterprise research preview) | Direct private addresses |
| ChatGPT web | Public HTTPS (mTLS cert and published IPs for allowlisting), or Secure MCP Tunnel (not accepted for public submission) | Private addresses; mobile |
| Copilot Studio, M365 Copilot | Public endpoint with publicly trusted certificate; VNet-integrated connectors for Azure or on-premises resources in some configurations (MCP over VNet [U]) | Custom root CAs |
| Gemini Enterprise | Public HTTPS from a trusted CA; FQDN egress allowlist | Self-signed certificates |
| Foundry | Standard Agent Setup with private networking (BYO VNet) | Basic agent setup |
| Self-hosted Dynodoc, air-gapped | Local stdio with a locally run model client only | Any cloud-hosted client |

Self-hosted design notes (the self-hosted edition is documented separately):
the MCP server and OAuth endpoints must be configurable with the installation's own
host names; CIMD and DCR fetches need an operator-controlled egress path; the
vendor-published IP ranges change and must not be hard-coded.

## 14. Listing and review checklist (master)

| Requirement | Anthropic | OpenAI | Status |
|---|---|---|---|
| Privacy policy URL | Required | Required (HTTPS, categories, purposes, recipients, retention) | Draft; owner adoption needed |
| Terms URL | Not listed in what was read [U] | Required | Draft |
| Support contact | Required | Required URL | Operator email exists |
| Test account | Fully populated | No MFA, magic link, email codes | Needs a reviewer login path |
| Documentation with examples | At least 3 prompts | 5 positive, 3 negative cases, video | To write |
| OAuth | 2.0, CIMD or DCR | 2.1, PKCE `S256`, CIMD preferred | Phase 2 |
| Annotations | `title` + hint | Explicit three booleans | Designed |
| Domain proof | Domain matches service | Challenge file | Easy |
| Public certificate and HTTPS | Yes | Yes | Phase 2 |
| Organisation verification | Paid account | Individual or business verification | Owner action |
| EU residency | n/a | Projects with EU data residency cannot submit MCP plugins | Use a global project |
| Ongoing | Version scans | Daily rescans; origin cannot change | Operational |

## 15. Operations

| Topic | Design |
|---|---|
| Vendor-change monitoring | Quarterly review plus alerts on the MCP specification repository, Anthropic and OpenAI changelogs; a table of "facts last verified" in this document with owners |
| Version matrix | Which client versions the acceptance scripts were last run against |
| Re-verification list | Every [U] item in Appendix B is a build-time task |
| Support | A per-client troubleshooting page: logs locations (`%APPDATA%\Claude\logs`, `~/Library/Logs/Claude`), `/mcp` status in Claude Code, ChatGPT "Refresh" after tool changes |
| Tool definition changes | Bump `serverInfo.version`; ChatGPT admins see changes as a diff and new actions are disabled by default, so unannounced renames break customers |
| Backward compatibility | Within protocol major, tool names and parameters are stable (protocol spec 15) |

## 16. Delivery plan

| Item | Phase | Weeks | Confidence |
|---|---|---|---|
| Claude Code plugin, Desktop snippet, install guide, acceptance script | 0 | 1 | High |
| Codex, Gemini CLI, Cursor, VS Code, others: snippets and a smoke test | 1 | 1 | High |
| claude.ai custom connector end-to-end, matrix testing, troubleshooting page | 2 | 1 | Medium |
| Anthropic directory submission pack | 2 | 1 to 2 | Medium (review time external) |
| `chatgpt` profile and developer-mode test on a Business workspace | 3 | 1.5 | Medium |
| ChatGPT directory pack incl. reviewer login path | 3 | 2 to 3 | Low to medium |
| Copilot Studio and Gemini Enterprise adapters (typed OpenAPI, quirk workarounds) | 4 | 2 to 3 | Low |
| Quarterly vendor review | ongoing | 0.5 per quarter | High |

Excludes the shared server and OAuth work (master plan), legal adoption, vendor review
waiting, and paid-plan licence costs for testing.

## 17. Risks

| Risk | Mitigation |
|---|---|
| Vendor spec or policy changes break a client | Official SDK, acceptance scripts, quarterly review |
| Directory rejection over privacy policy or reviewer login | Treat as explicit dependencies; plan reviewer path early |
| A vendor drops a transport we depend on | HTTP binding stays authoritative; stdio and Streamable HTTP are the two standard transports |
| ChatGPT write-confirmation friction reduces usefulness | Document; `propose` is non-destructive; read-heavy flows work without prompts |
| Claude and ChatGPT diverge on `structuredContent` and `_meta` handling | Profile mechanism |
| Plan limits (Free single connector, Pro read-only MCP) frustrate users | State in install guides |
| Vendor tunnels are preview and change | Treat as optional; recommend allowlisting or public endpoint |

## 18. Open questions

| # | Question | Default | Decider |
|---|---|---|---|
| 1 | Ship the `.mcpb` bundle at all, given the directory no longer accepts it? | No; plugin and config only | Product |
| 2 | Distribute the Claude Code plugin through a Dynodoc-owned marketplace file or the public directory? | Own marketplace file first | Product |
| 3 | Is a reviewer login path acceptable (long-lived synthetic-tenant credential)? | Yes, isolated tenant, no real data | Owner and security reviewer |
| 4 | Support SSE for older clients? | No | Engine lead |
| 5 | Pre-register with Anthropic's held-credentials option instead of relying on CIMD? | CIMD first | Engine lead |
| 6 | Which Microsoft route first: Copilot Studio wizard or plugin 2.4? | Copilot Studio (documented, GA wizard) | Product |
| 7 | Publish to the MCP Registry? | After GA of the registry or when a customer needs it; it hosts metadata only and rejects private servers | Product |

## Appendix A. Glossary

| Term | Meaning |
|---|---|
| App, connector, plugin | OpenAI's names for an MCP-based integration at different times; see section 6 |
| CIMD | Client ID Metadata Document |
| DCR | Dynamic Client Registration |
| MCPB | MCP Bundle (`.mcpb`), a zip of a local MCP server with a manifest |
| Streamable HTTP | MCP's HTTP transport (single endpoint, POST) |
| Tunnel | Vendor outbound-connection product letting a cloud client reach a private server |

## Appendix B. References and unverified items

All accessed 14 October 2026. Where a Help Center page returned 403, an archive
snapshot was read; the snapshot date is in the research record and is marked [V-arch].

### B.1 Specification and registry

- https://modelcontextprotocol.io/specification/versioning
- https://modelcontextprotocol.io/specification/2026-07-28/changelog
- https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/index
- https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/index
- https://modelcontextprotocol.io/specification/2026-07-28/server/tools
- https://modelcontextprotocol.io/specification/2026-07-28/deprecated
- https://modelcontextprotocol.io/community/sdk-tiers
- https://modelcontextprotocol.io/registry/about
- https://modelcontextprotocol.io/extensions/overview

### B.2 Anthropic

- https://claude.com/docs/connectors/custom/add-unlisted.md
- https://claude.com/docs/connectors/building/submission.md
- https://claude.com/docs/connectors/building/review-criteria.md
- https://claude.com/docs/connectors/building/authentication.md
- https://claude.com/docs/connectors/building/mcpb.md
- https://claude.com/docs/connectors/mcp-tunnels/overview.md
- https://claude.com/docs/directory/publish.md
- https://code.claude.com/docs/en/mcp.md , `/managed-mcp.md` , `/plugins-reference.md`
- https://code.claude.com/docs/en/agent-sdk/mcp.md
- https://platform.claude.com/docs/en/agents-and-tools/mcp-connector.md
- https://support.claude.com/en/articles/12592343-enabling-and-using-the-desktop-extension-allowlist
- https://www.anthropic.com/engineering/writing-tools-for-agents

### B.3 OpenAI

- https://developers.openai.com/plugins/build/mcp-server , `/plugins/build/auth` ,
  `/plugins/deploy/app-review` , `/plugins/deploy/submission` , `/plugins/plugin-guidelines` ,
  `/plugins/reference` , `/plugins/guides/security-privacy`
- https://developers.openai.com/api/docs/guides/custom-mcp-server ,
  `/api/docs/guides/tools-connectors-mcp` , `/api/docs/mcp` , `/api/docs/guides/deep-research`
- https://developers.openai.com/api/docs/actions/production
- https://learn.chatgpt.com/docs/enterprise/apps-and-connectors ,
  `/docs/migrate-custom-gpts` , `/docs/extend/mcp`
- Help Center articles 12584461, 11509118, 11487775, 20001256, 8798878 via archive
  snapshots dated 3 to 10 October 2026

### B.4 Google, Microsoft, GitHub, others

- https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/tools/mcp-server.md
- https://ai.google.dev/gemini-api/docs/function-calling
- https://docs.cloud.google.com/gemini/enterprise/docs/connectors/custom-mcp-server/set-up-custom-mcp-server
- https://learn.microsoft.com/en-us/microsoft-copilot-studio/mcp-add-existing-server-to-agent
- https://learn.microsoft.com/en-us/microsoft-365/copilot/extensibility/plugin-manifest-2.4
- https://learn.microsoft.com/en-us/azure/foundry/agents/how-to/tools/model-context-protocol
- https://code.visualstudio.com/docs/agents/reference/mcp-configuration
- https://docs.github.com/en/copilot/how-tos/administer-copilot/manage-mcp-usage/configure-enterprise-allowlist
- https://cursor.com/docs/context/mcp
- https://docs.devin.ai/desktop/cascade/mcp
- https://docs.cline.bot/mcp/configuring-mcp-servers
- https://docs.continue.dev/customize/deep-dives/mcp
- https://zed.dev/docs/ai/mcp
- https://www.jetbrains.com/help/ai-assistant/mcp.html

### B.5 Unverified items to re-check at build time

| Item | Vendor |
|---|---|
| Whether claude.ai negotiates MCP 2026-07-28; elicitation on claude.ai; organisation URL allow or deny list; tool-count cap | Anthropic |
| Claude Desktop on Linux | Anthropic (conflicting pages) |
| Claude Code plugin `.mcp.json`: whether the `${VAR:-default}` form expands (the plugin's write switch relies on it; text left unexpanded is not a clear yes, so the switch stays off, but an error instead would stop the plugin from starting), whether `env` values expand, and whether the started program receives them | Anthropic |
| Whether Plus and Free can add custom MCP servers; tool count, result size and timeout limits for custom MCP servers | OpenAI |
| OpenAPI version and size cap for Actions; domain ownership verification | OpenAI |
| Cursor tool cap and current OAuth registration behaviour; VS Code CIMD; top-level keys in `mcp.json` files | Cursor, GitHub, Microsoft |
| Copilot Studio MCP timeout and result size; MCP over VNet; connector certification requirements | Microsoft |
| M365 plugin transport, tool cap, result size; Foundry tool cap and GA label | Microsoft |
| Private MCP reachability from Google Agent Runtime; CIMD in Gemini CLI | Google |
| Devin Desktop "Devin Local" config path and tool cap | Cognition |
| MCP Registry GA date | MCP project |
