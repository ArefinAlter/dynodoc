# Connect Claude to Dynodoc (read-only): setup guide

Written for the person who looks after a team's Dynodoc projects, not for a developer.
It covers Claude Code and Claude Desktop. 20 October 2026.

> **Status.** The part that runs on your computer is built: the `engine-cli mcp` program
> and the Claude Code plugin ([integrations/claude-code/dynodoc](../integrations/claude-code/dynodoc)).
> The Dynodoc server side is built separately and is switched off until your operator turns
> the agent connector on. Two screens in the Dynodoc web app that this guide mentions
> ("Use for an AI agent" and the read-access tick box) are **not built yet**; each step says
> so where it matters. Nothing here has been tried with a real Claude client yet. Vendor
> details that could not be checked offline are marked **unverified: check at build time**.
> Design background: [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md).

## What you get

Claude can look at the files in **one** Dynodoc project: list them, show a file's outline,
read the parts it needs, search, and compare two versions. That is all. It reads with your
own access, through a key you create, give to one project and can withdraw at any time.

## What Claude can and cannot do

| Claude can | Claude cannot |
|---|---|
| List the files of the one project you connected | Open any other project |
| Read the text of documents, the cells of spreadsheets, the text of slides | Change, add, delete or move anything |
| Search inside a file and compare two versions | Suggest changes into Dynodoc, send anything for review, approve or merge |
| Show what the connection itself is allowed to see | See other people's private drafts, the member list or project settings |
| Read only what *you* could open yourself, at the moment it asks | Read a file you have lost access to (it is checked on every request) |

Also good to know:

- Pictures in files are not sent, only that a picture exists and how large it is.
- Spreadsheet formulas are shown as written. They are not calculated.
- Each answer is limited in size. A long file arrives in pieces that Claude asks for in turn.

**What Dynodoc cannot control.** Whatever Claude reads is sent to the company that runs
the AI (Anthropic, for Claude), under your agreement with them. Dynodoc does not see your
conversation and cannot see what the AI company does with it. Connect only projects whose
content you are allowed to share with that AI service. A document can also contain text
that tries to give Claude orders. Dynodoc marks all document text as data, and this
connection cannot change anything, so the worst such text can do is mislead an answer.
Check anything important against the file itself.

## Before you start

You need:

1. A Dynodoc account that can open the project.
2. The `engine-cli` program on your computer, on the PATH (so that typing `engine-cli`
   in a terminal works). Ready-made downloads are not published yet; today it is built from
   the source with `cargo build --release -p engine-cli`. **unverified: check at build time**
   (release packaging).
3. Your operator has switched the agent connector on for your Dynodoc installation (the
   server setting `AGENT_GATEWAY=on`). Ask them; it is off by default.
4. Claude Code, or Claude Desktop, installed.

## Step 1. Make a key for the agent

An agent key is a small file. Whoever holds it can act as the agent, so treat it like a
password. Make a **new** key for this purpose; do not reuse the key you use for syncing.

```text
engine-cli device keygen --out agent.key --encrypt --keystore
```

This locks the file with a secret that your computer's own account storage holds (Windows,
macOS keychain, or Linux secret service), so no password prompt is needed when Claude starts
the program, and the file is useless on another computer or account. The command prints the
key's **public key** (64 letters and digits). Keep it for step 2.

If you prefer a password instead of the computer's storage, leave out `--keystore`: you
are asked for a password twice, and later you must give the program a file that contains it
(see "Troubleshooting").

## Step 2. Enrol the key as an agent key

In Dynodoc, open your **Profile**, then **Signing keys**, and follow the usual steps: paste
the public key, download the challenge, run the `engine-cli device prove ...` command that
the page shows, and upload the proof file. The page is described in
[DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md).

When you enrol, the key must be marked **"Use for an AI agent"**. *This choice is not on the
screen yet.* The server already accepts it (the key's class is `agent`), but the screen to
tick it is a later piece of work. Until then, ask whoever runs your installation to enrol
the key with the class `agent`. A key that is not marked this way cannot read files for
agents, and an agent key cannot be used for anything else.

## Step 3. Give the key one project, read only

Still under **Signing keys**, add the project you want Claude to read to this key, with
read access to its files only (the scope is named `content:read`). *The tick box for this
scope is not built yet either*; the same person who enrolled the key can set it. An agent
key can hold nothing but this one scope, and it works for the project you add and no other.

Because you choose the project here, not in the key file, a copy of the file is not a way
into your other projects.

## Step 4. Prepare the connection file

`engine-cli` needs to know where Dynodoc is and which installation to trust. Create a small
text file, for example `remote.json`:

```json
{
  "version": 1,
  "url": "https://dynodoc.example.org/api/",
  "authority": "the installation id (a UUID)",
  "public_key": "64 letters and digits"
}
```

Your operator gives you the three values. They are not secrets, but get them from someone
you trust and not from the server's own replies: the program checks every answer against
them so that a look-alike server cannot pose as yours. It is the same file `engine-cli sync`
uses; reuse one you already have. The web address must start with `https://` and end with `/`.

You also need the project's id (the long code in its address in Dynodoc).

## Step 5a. Claude Code

Claude Code reads three environment variables. Set them in the terminal you start it from
(replace the example paths and the id).

Windows PowerShell:

```text
$env:DYNODOC_REMOTE = "C:\Users\you\dynodoc\remote.json"
$env:DYNODOC_PROJECT = "11111111-2222-4333-8444-555555555555"
$env:DYNODOC_KEY = "C:\Users\you\dynodoc\agent.key"
```

macOS or Linux:

```text
export DYNODOC_REMOTE=$HOME/dynodoc/remote.json
export DYNODOC_PROJECT=11111111-2222-4333-8444-555555555555
export DYNODOC_KEY=$HOME/dynodoc/agent.key
```

Then start Claude Code with the plugin from this repository:

```text
claude --plugin-dir integrations/claude-code/dynodoc
```

**unverified: check at build time** (the `--plugin-dir` option, the plugin layout, `${VAR}`
substitution inside the plugin's `.mcp.json`, and whether the started program receives the
variables). If it does not work, use the plain command instead, which needs no plugin:

```text
claude mcp add dynodoc -- engine-cli mcp --remote C:\Users\you\dynodoc\remote.json --project 11111111-2222-4333-8444-555555555555 --device-key C:\Users\you\dynodoc\agent.key
```

Inside Claude Code, type `/mcp` to see whether `dynodoc` is connected. The plugin also adds
a short guide ("skill") that teaches Claude how to read Dynodoc's compact answers; it costs
nothing until Claude needs it.

## Step 5b. Claude Desktop

Open Claude Desktop, then **Settings**, **Developer**, **Edit Config**. This opens
`claude_desktop_config.json` (on Windows in `%APPDATA%\Claude\`, on macOS in
`~/Library/Application Support/Claude/`). Add the block below inside the file's outer braces
and keep any entries that are already there:

```json
{
  "mcpServers": {
    "dynodoc": {
      "command": "engine-cli",
      "args": [
        "mcp",
        "--remote", "C:/Users/you/dynodoc/remote.json",
        "--project", "11111111-2222-4333-8444-555555555555",
        "--device-key", "C:/Users/you/dynodoc/agent.key"
      ]
    }
  }
}
```

Save the file and **quit and reopen** Claude Desktop (a restart is required). If Claude
Desktop cannot find `engine-cli`, replace `"engine-cli"` with its full path, for example
`"C:/Users/you/engine-cli.exe"`; in this file write `/` or `\\` between folders, never a
single `\`. A newer Desktop package format (`.mcpb`) is not offered. Facts here come from
the vendor's own documentation read on 14 October 2026. **unverified: check at build time**
(Claude Desktop on Linux: the vendor's pages disagree; and whether a Desktop update
changes the file location).

## Step 6. Try it

Ask Claude, in plain words:

- "List the files in my Dynodoc project."
- "Show me the outline of the Q3 Policy document."
- "Find every place the word 'clause' appears in Q3 Policy."
- "What changed in Q3 Policy between revision 410 and today?"

You should see Claude call tools named `files`, `outline`, `find`, `diff`, `read` and
`status`. If Claude asks permission before using them, the safe choice is to allow them for
the session: they only read.

## Taking access away

Do any of these; the first two act at once, on Claude's very next request.

1. **Remove the project from the key**, or **revoke the key**, under Profile, Signing keys.
   Dynodoc checks the key and its project on every request, so Claude stops seeing the
   project immediately, even though a short-lived pass (one hour) may already have been
   issued.
2. **Your operator can switch the connector off** for everyone (`AGENT_GATEWAY=off`).
3. **Remove the `dynodoc` entry** from Claude Desktop's file, or run `claude mcp remove
   dynodoc` (**unverified: check at build time**), or disable the plugin. This only stops
   *your* Claude from asking.
4. **Delete the key file.** If the file may have been copied, also do step 1.

If you suspect the key file was copied, do step 1 first: it works whatever happens to the
copies.

## If something does not work

Claude shows the message from the server in a line starting `dd/1 error`. What they mean:

| Message | What it means and what to do |
|---|---|
| `code=unauthorized` | The key was refused: it was revoked, is not enrolled as an agent key, or has no access to this project. Check steps 2 and 3. |
| `code=forbidden` or `code=not_found` | The file or project is not available to this connection. The two look the same on purpose. Check the file id, the project and your own access. If *every* call says `not_found`, including `status`, the connector is probably switched off on the server (see "Before you start"). |
| `code=rate_limited` (with `retry_after`) | Too many requests. Wait that many seconds. |
| `code=busy`, `code=deadline_exceeded`, `code=internal` | Dynodoc is busy or unreachable. Check the web address in `remote.json`, then try again. |
| `code=bad_request field=...` | Claude gave a wrong value for that field. It usually corrects itself. |
| Claude does not list Dynodoc tools | The program did not start. Run the same command in a terminal; any problem (wrong path, a key it cannot open, a web address that is not `https://`) is printed there, because the program writes its messages to the terminal's error output and never to Claude. |
| The program asks for a password or says there is no passphrase source | The key is password-protected. Set the variable `DYNODOC_KEY_PASSPHRASE_FILE` to a file that contains the password (the file must be readable only by you), or make the key with `--keystore`. Whether Claude passes this variable to the program is **unverified: check at build time**. |

Do not paste the key file or the password into a chat. Nothing in this setup needs it.

## What was checked, and what was not

The program and the plugin files are covered by automated tests (the protocol, every tool
definition against the written specification, the requests it sends, error handling, that
nothing but protocol messages ever leaves the program's standard output, and that the
key's token never appears in messages or logs). The tests use a stand-in Dynodoc server.

**Not done:** running Claude Code or Claude Desktop against it, running against the real
Dynodoc server routes, and any measurement of how the AI behaves with these tools. The vendor
facts above marked "unverified" must be re-checked when this is first installed.
