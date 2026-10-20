# Dynodoc plugin for Claude Code

Lets Claude Code read Dynodoc documents, spreadsheets and presentations through a
connection you control. **By default it is read-only: nothing in this plugin can change a
file.** An opt-in switch (below) also lets Claude *propose* edits. A proposal is a draft that
a person reviews and merges in Dynodoc; Claude cannot approve or merge anything, and nothing
it proposes is live before a person does.

What is in it, all plain text (no hooks, no scripts):

| File | Purpose |
|---|---|
| `.claude-plugin/plugin.json` | The plugin's name and description |
| `.mcp.json` | Starts `engine-cli mcp` over standard input and output |
| `skills/dynodoc/SKILL.md` | How to read Dynodoc answers and how to propose well; loads only when needed |

It needs the `engine-cli` program on your PATH and three environment variables:
`DYNODOC_REMOTE` (the connection file), `DYNODOC_PROJECT` (the project id) and
`DYNODOC_KEY` (the agent key file). Creating the key, giving it access to one project,
revoking it, and the Claude Desktop setup are in
[docs/AI-AGENT-CLAUDE-SETUP.md](../../../docs/AI-AGENT-CLAUDE-SETUP.md).

## Letting Claude propose edits (off by default)

Set `DYNODOC_WRITE=on` in the environment you start Claude Code from. `.mcp.json` passes it
to the program as `${DYNODOC_WRITE:-off}`, so with the variable unset it is off, and any value
other than `on`, `yes`, `true` or `1` (including an expansion that did not happen) leaves it
off. The program then lists `propose` and `submit` besides the six reading tools and asks for
the write permissions when it signs in.

Three things must also be true, and none of them is decided by this plugin: the key was
given `content:propose` and `content:submit` in your profile (they cannot be given on the
Signing keys screen yet; the setup guide says how), the project's owner set `write_mode` to
something other than `none`, and your project role may draft. If any is missing the program
says so and keeps reading: write calls answer with a plain refusal.

To try it from a checkout, start Claude Code with `claude --plugin-dir <this folder>`
(unverified: check at build time; so is the `${VAR:-default}` form inside `.mcp.json`, which
the vendor facts recorded here do not cover). The plugin marketplace entry is not written yet.
