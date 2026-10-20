# Dynodoc plugin for Claude Code

Lets Claude Code read Dynodoc documents, spreadsheets and presentations through a
read-only connection. Nothing in this plugin can change a file.

What is in it, all plain text (no hooks, no scripts):

| File | Purpose |
|---|---|
| `.claude-plugin/plugin.json` | The plugin's name and description |
| `.mcp.json` | Starts `engine-cli mcp` over standard input and output |
| `skills/dynodoc/SKILL.md` | How to read Dynodoc answers; loads only when needed |

It needs the `engine-cli` program on your PATH and three environment variables:
`DYNODOC_REMOTE` (the connection file), `DYNODOC_PROJECT` (the project id) and
`DYNODOC_KEY` (the agent key file). Creating the key, giving it access to one project,
revoking it, and the Claude Desktop setup are in
[docs/AI-AGENT-CLAUDE-SETUP.md](../../../docs/AI-AGENT-CLAUDE-SETUP.md).

To try it from a checkout, start Claude Code with `claude --plugin-dir <this folder>`
(unverified: check at build time). The plugin marketplace entry is not written yet.
