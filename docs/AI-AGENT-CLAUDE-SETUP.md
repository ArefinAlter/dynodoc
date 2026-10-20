# Connect Claude to Dynodoc: setup guide (read-only, with optional proposals)

Written for the person who looks after a team's Dynodoc projects, not for a developer.
It covers Claude Code and Claude Desktop. 20 October 2026.

> **Status.** The part that runs on your computer is built: the `engine-cli mcp` program
> and the Claude Code plugin ([integrations/claude-code/dynodoc](../integrations/claude-code/dynodoc)).
> It reads by default. An opt-in switch (`--write`, see "Letting the agent propose changes")
> adds two tools, `propose` and `submit`, with which Claude can stage a proposal for a person
> to review; it cannot merge or approve anything. Tried only against a stand-in server in
> automated tests, never with a real Claude client or the real Dynodoc server.
> The Dynodoc server side is built separately and is switched off until your operator turns
> the agent connector on, and then stays off for each project until that project's owner
> turns agents on (step 0). The three screens in the Dynodoc web app that this guide uses
> (the choice "This key is for an AI agent", the "Read documents" permission and the
> **AI agents** section of a project's settings) are built. They were tried only in automated
> browser tests on a test installation with the agent connector switched off, never with a real
> Claude client. Vendor details that could not be checked offline are marked **unverified:
> check at build time**.
> Design background: [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md).

## What you get

Claude can look at the files in **one** Dynodoc project: list them, show a file's outline,
read the parts it needs, search, and compare two versions. That is all, unless you also switch
proposals on (further down): then Claude can suggest edits, which a person must review and
merge before anything changes. It reads with your own access, through a key you create, give to
one project and can withdraw at any time.

## What Claude can and cannot do

The table is for the default, read-only connection. Proposals change one row; see below.

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
Check anything important against the file itself. With proposals switched on, the text of a
proposal is data too, and it is shown to a person before it can change anything.

## Before you start

You need:

1. A Dynodoc account that can open the project.
2. The `engine-cli` program on your computer, on the PATH (so that typing `engine-cli`
   in a terminal works). Ready-made downloads are not published yet; today it is built from
   the source with `cargo build --release -p engine-cli`. **unverified: check at build time**
   (release packaging).
3. Your operator has switched the agent connector on for your Dynodoc installation (the
   server setting `AGENT_GATEWAY=on`). Ask them; it is off by default.
4. An owner or manager of the project has turned agents on for it (step 0). That is a
   different person's decision if you are not an owner or manager yourself.
5. Claude Code, or Claude Desktop, installed.

## Step 0. A project owner turns agents on

Agents are **off in every project** until an owner or manager of that project turns them on.
Until then even a correctly made key reads nothing: every request is refused, with the same
plain "no access" answer you would get if you were not a member. Nobody else can make this
choice for the project: not a member, not the agent itself, not the person who made the key.
Switching the connector on for the installation (the operator's setting) does not turn on any
project.

How to do it: open the project, choose the **Settings** tab and scroll to the **AI agents**
section (below "People"). Only an owner or manager sees the controls; everyone else sees one
line saying so. At the top of the section Dynodoc says whether your installation serves AI
agents at all: "AI agents are turned on in this installation." or "AI agents are not available
on this installation yet." In the second case you can still prepare the settings, but they do
nothing until your operator turns the agent connector on.

What the owner or manager decides, in plain words, with the name of each control:

- **On or off: "Let AI agents read this project".** The master switch. New projects start off.
- **Who: "Lowest role that can use an AI agent".** The lowest project role whose assistants may
  read. The default is Viewer, so every member. Choosing, say, Editor means a viewer's key
  reads nothing.
- **Which kinds of files: "Kinds of file an AI agent may read".** Documents, Spreadsheets,
  Presentations, or any mix. At least one must stay ticked.
- **How much: "Reads per minute for each AI agent key" and "Data read per day for each AI
  agent key, in MB".** From 1 to 600 reads a minute (default 60) and from 0.01 MB to 1,000 MB a
  day (default 20 MB), for each key, so one runaway assistant cannot pull a whole project. Past
  the limit Claude is told to wait, and the wait ends by itself. A value outside these ranges is
  not sent; the page says why.

Press **Save AI agent settings**. The whole form is saved at once, so switching agents on or off
from this screen never changes the other settings behind your back. If someone else saved
first, you see "Someone else changed these settings while you were editing", the form shows
what is saved now, and you decide whether to make your change again. Every change is recorded
with who made it and what it changed.

**Single files.** A file can be hidden from assistants altogether (for example a contract)
while the rest of the project stays open. A hidden file looks, to the assistant, as if it did
not exist. The owner of the file, or a manager, can do this. Only a project owner or manager
can undo it again, so a file's creator who is only an editor, or no longer a member, can hide a
file but cannot reopen one a manager hid. **The screen only lists** the files that have such a
setting, under "Files with their own setting"; it cannot change them yet. Until it can, do it
through the Dynodoc API, signed in as the owner. `<token>` is the owner's sign-in token and
`<file>` a file's id:

```text
# Hide one file from assistants ("inherit" brings it back)
curl -X PUT -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"access":"none"}' https://dynodoc.example.org/api/documents/<file>/agent-policy
```

The same section shows what assistants did, under **Usage, last 7 days**: for each day (UTC),
how many calls were made, how much data was sent back and how many calls were refused. It holds
no file text. The clients of a day are added together on the screen; the API lists them
separately, by the name each assistant gave itself, which proves nothing. A burst of refused
calls from one key is recorded once a minute, not once per attempt, so the refusal count shows
that an assistant tried, not how often.

The full list of routes and values, for anyone who prefers the API, is in `openapi.yaml`, tag
"agent policy"; the design is in [AI-AGENT-CONNECTORS.md](AI-AGENT-CONNECTORS.md) section 5.

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

In Dynodoc, open your **Profile**, then **Signing keys**, and open **Connect a local signing
key**. Under "Who is this key for?" choose **This key is for an AI agent** (the default is
"This key is for me"; choose the agent option for this key). Give the key a **Name for this
key**, for example "Claude on my laptop" (1 to 80 characters; do not put a secret in it).
Then follow the usual steps: paste the public key, press **Create enrollment challenge**,
download the challenge, run the `engine-cli device prove ...` command that the page shows, and
select the proof file. The page is described in [DEVICE-SIGNING-KEYS.md](DEVICE-SIGNING-KEYS.md).

When it is connected the key appears in the list with an **AI agent** badge and its name. The
choice cannot be changed afterwards: a key that was not connected as an AI agent cannot read
files for agents, and an agent key cannot be used for anything else. If you chose wrongly,
revoke the key and make and connect a new one (a revoked key cannot be connected again).

## Step 3. Give the key one project, read only

Still under **Signing keys**, on the new key, press **Grant a project** (or **Add project**),
choose the project you want Claude to read and press **Save grant**. The only permission on
offer for an agent key is **Read documents** (the scope is named `content:read`): it lets the
agent list the project's files and read them, and it lets it change nothing. The screen says
so and does not show the permissions of a person's key. An agent key can hold nothing but agent permissions. This screen offers only **Read
documents**; the two write permissions described under "Letting the agent propose changes"
below can be given through the API and not on this screen yet, and you should leave them off
unless you want Claude to propose edits.
The key works for the project you add and no other. The screen has no "all
projects" choice for an agent key: each project must be granted.

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
a short guide ("skill") that teaches Claude how to read Dynodoc's compact answers and, when
proposals are on, how to propose well; it costs nothing until Claude needs it.

To let Claude propose edits as well, also set `DYNODOC_WRITE=on` before starting Claude Code
(PowerShell: `$env:DYNODOC_WRITE = "on"`; macOS or Linux: `export DYNODOC_WRITE=on`), after
reading "Letting the agent propose changes" below. The plugin passes the variable on to the
program (as `${DYNODOC_WRITE:-off}`); unset, it stays off. With the plain command, add
`--write` to it. Both are **unverified: check at build time**, like the other substitutions
above (and the `:-off` default form is not among the vendor facts recorded in this repository).

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

To allow proposals as well, add `"--write"` to the `args` list (after `"mcp"`), after reading
"Letting the agent propose changes" below. Leave it out to stay read-only.

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

With proposals switched on you can also ask, in plain words: "In Q3 Policy, change 'clause' to
'article' everywhere." Claude should first try the change as a dry run (it stores nothing),
tell you how many places it would touch, and then propose it. It should give you the
proposal's id and say that a person must review and merge it in Dynodoc. If it says the change
is made, saved or merged, it is wrong: nothing changes until a person merges the proposal.
Allow `propose` and `submit` for the session only if you are watching; every call is also
recorded by Dynodoc.

## Letting the agent propose changes

Off by default, in three places, and all three must agree. Dynodoc stores a proposal as a
draft that belongs to an assistant identity ("Claude for Olivia"), never in the team's
version, so nothing Claude does here changes what the team sees until a person merges it.

**1. The project allows it (an owner or manager).** Writing is separate from reading and
**off by default**. Set `write_mode` in the same agent settings as step 0
(`PUT .../projects/<project>/agent-policy`, with `"write_mode":"draft"` or
`"change_request"`; the AI agents screen does not have the control yet):

- `none` (the default): assistants only read. A `propose` is refused with `policy_denied`.
- `draft`: assistants stage drafts; a person submits them.
- `change_request`: an assistant may submit its own draft, unless `submit_requires_person` is
  true (it is by default), in which case a person must. Then `submit` changes nothing and
  answers "waiting for a person to submit".

The same settings carry the proposal limits: `proposals_per_day` (default 20) and
`open_proposals` (default 5) for each person in the project (all of their assistants
together, whatever the product), and
`ops_per_proposal` (default 500 stored operations in one proposal). A single file can stay read
only for assistants (`{"access":"read"}`) or be hidden (`none`). The stop button also sets
`write_mode` back to `none`, so turning agents on again does not turn writing on again.

**2. The key may write (the person whose key it is).** The Signing keys screen offers only
**Read documents**, so give the write permissions through the API, signed in as yourself
(`<token>` is your sign-in token, `<key>` the key's id from `GET /api/devices`, `<project>` the
project's id):

```text
curl -X PUT -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
  -d '{"scopes":["content:read","content:propose","content:submit"]}' \
  https://dynodoc.example.org/api/devices/<key>/projects/<project>
```

`content:propose` stages drafts and `content:submit` submits or withdraws them; always give
`content:read` with them (a preview of a proposal shows the text it changes, so Dynodoc
refuses a write scope without it). Only a project role that can write drafts (not a viewer)
is issued a write session, and the role is checked again on every call, so a person who loses
their role or is removed stops their assistants at once. When the person is disabled or
erased, their assistants' identities end with them.

**3. The program is started with writing on (you).** `--write` on the command, or
`DYNODOC_WRITE=on` in the environment (the Claude Code plugin reads that variable). Then the
program asks Dynodoc for the two write permissions as well when it signs in and lists
`propose` and `submit` beside the six reading tools. Without it, the program never asks for
them and the tools do not exist for Claude. If you start it with `--write` and the key was
not given them, it says so in the terminal's error output, keeps reading, and answers a
write call with `code=forbidden policy=write_scope`. If the project does not allow writing,
the answer is `code=policy_denied policy=write_mode`. Start it again after changing either.

**What Claude can do then.** Stage edits as a proposal (`propose`; ops such as `set_text`,
`replace`, `replace_all`, `insert`, `delete`, `move`, `set_type`, and `set_cells`, `clear`,
`set_shape`, `set_notes`; up to 200 per call), try them first without storing anything
(`dry_run`), add to its own proposal, send it to the reviewers or take it back (`submit` with
`action=submit` or `withdraw`, when the project lets it), and read where it stands (`status`:
the state, what reviewers said, how far behind the team version it is, where a merge would
overlap). People then look at the draft: Olivia sees it in her own drafts, can change it,
submit it or withdraw it, and once it is submitted every member of the file sees it as a
change request from "Claude for Olivia", with the person it acts for named. A person with
merge rights merges it under the file's usual review rules; only then does the team's version
change, and the merge record names the assistant as the author.

Every answer of these tools ends with a line `note "..."` that says what the state means
("proposal only ... nothing is live", "waiting for a person to submit; nothing is live"), and
the plugin's guide tells Claude to keep to it and to give you the proposal's id. **Ask Claude
for the id** if it does not: it is the handle for the proposal.

**What Claude cannot do.** Change the team's version; approve, decline, merge or comment on a
change request; edit anyone else's draft (not Olivia's own personal drafts either, not another
assistant's, not another project's); change a rule, a policy, a member or a key; sign in; or
receive mail. If the project says a person must submit, even its own submit call changes
nothing and answers "waiting for a person to submit". Olivia can never be the only approver of
her own assistant's request when the file needs approvals: her approval is refused, and an
approval of hers that got in some other way is not counted. Not offered yet: `fill`,
`add_slide`, `delete_slide`, macros, bold or italic in new text, a list of a file's proposals.

**Retries do not duplicate.** Each `propose` carries a fresh request id that the program
makes up, and the program sends a call once more, unchanged, if the connection broke. Dynodoc
stores a receipt under the id, so the same call twice stores nothing twice. If the answer
never arrives, the error names the id (`request_id=...`) and Claude repeats the call with it.

**What gets recorded.** Each proposal call leaves a receipt with its counts, the stale
references and the assistant's reasons (never the document text), an audit row, and a row in
the call ledger.

## Taking access away

Do any of these; the first three act at once, on Claude's very next request.

1. **Remove the project from the key** (**Revoke grant**), or **revoke the key** (**Revoke
   key**, then **Confirm revocation**), under Profile, Signing keys. Dynodoc checks the key and
   its project on every request, so Claude stops seeing the project immediately, even though a
   short-lived pass (one hour) may already have been issued.
2. **A project owner or manager can stop every assistant in the project at once**: in the
   project's **Settings**, section **AI agents**, either switch "Let AI agents read this
   project" off and press **Save AI agent settings**, or press **Stop all AI agents now**. The
   stop button asks you to confirm, and says what it does: it turns agents off, ends every AI
   agent key's access to this project and every open agent session, and leaves people's own
   keys untouched. Turning agents back on does not bring the old access back: each agent key
   must be given the project again (step 3) and Claude must start a new session. (The switch
   alone stops new requests by turning the project off, but does not end the keys' grants; the
   stop button does.)
3. **Your operator can switch the connector off** for everyone (`AGENT_GATEWAY=off`).
4. **Remove the `dynodoc` entry** from Claude Desktop's file, or run `claude mcp remove
   dynodoc` (**unverified: check at build time**), or disable the plugin. This only stops
   *your* Claude from asking.
5. **Delete the key file.** If the file may have been copied, also do step 1.

If you suspect the key file was copied, do step 1 first: it works whatever happens to the
copies.

## If something does not work

Claude shows the message from the server in a line starting `dd/1 error`. What they mean:

| Message | What it means and what to do |
|---|---|
| `code=unauthorized` | The key was refused: it was revoked, is not enrolled as an agent key, or has no access to this project. Check steps 2 and 3. |
| `code=forbidden` or `code=not_found` | The file or project is not available to this connection. The two look the same on purpose. Check the file id, the project and your own access. If `forbidden` comes for *every* call, including `status`, the likely reasons are that no owner has turned agents on for the project (step 0), your project role is below the owner's minimum, or the stop button was used. A file an owner has hidden from assistants is `not_found`. If *every* call says `not_found`, including `status`, the connector is probably switched off on the server (see "Before you start"). |
| `code=rate_limited` or `code=budget_exceeded` (with `retry_after`) | Too many requests, or this key has used its reads for the minute or the day (step 0). Wait that many seconds. |
| `code=busy`, `code=deadline_exceeded`, `code=internal` | Dynodoc is busy or unreachable. Check the web address in `remote.json`, then try again. After a `propose`, the line may carry `request_id=...`: the proposal may have been stored; Claude repeats the call with that id and Dynodoc answers `replayed=1` instead of storing it twice. |
| `code=policy_denied policy=write_mode` | The project does not let agents propose (`write_mode` is `none`), or submit. A project owner or manager must change it; see "Letting the agent propose changes". |
| `code=forbidden policy=write_scope` | The key was not given `content:propose` and `content:submit`, so the program kept to reading. Give them (see above) and start the program again. |
| `state=awaiting_person` (not an error) | The project requires a person to submit. Open the proposal in Dynodoc and submit it yourself. |
| A `stale:` note, or `code=stale_base` | Someone changed the same text after Claude read it. Claude reads it again and proposes again on the newer version; the people who review see the overlap in any case. |
| `code=idempotency_conflict`, `code=proposal_closed` | A request id was reused for different edits (Claude should leave the id out), or the proposal was already merged, declined or withdrawn (Claude starts a new one). |
| `code=budget_exceeded` after `propose` | The person's limit of proposals a day or of open proposals is reached (step 1 above). Merge, decline or withdraw some, or raise the limit. |
| `code=bad_request field=...` | Claude gave a wrong value for that field. It usually corrects itself. |
| Claude does not list Dynodoc tools | The program did not start. Run the same command in a terminal; any problem (wrong path, a key it cannot open, a web address that is not `https://`) is printed there, because the program writes its messages to the terminal's error output and never to Claude. |
| The program asks for a password or says there is no passphrase source | The key is password-protected. Set the variable `DYNODOC_KEY_PASSPHRASE_FILE` to a file that contains the password (the file must be readable only by you), or make the key with `--keystore`. Whether Claude passes this variable to the program is **unverified: check at build time**. |

Do not paste the key file or the password into a chat. Nothing in this setup needs it.

## What was checked, and what was not

The program and the plugin files are covered by automated tests (the protocol, every tool
definition against the written specification, the requests it sends, error handling, that
nothing but protocol messages ever leaves the program's standard output, and that the
key's token never appears in messages or logs). For proposals they also cover: that the
default connection lists six tools and never asks for a write permission, that `--write` lists
eight and asks for three, that a refused write permission leaves a working reading session,
that a retried `propose` carries the same request id (after a renewed sign-in and after a
broken connection), that every answer says the change is only a proposal, that nothing is sent
unless Claude asks (a `propose` is never followed by a `submit`), that oversized or misspelt
edits are refused before anything is sent, and that the examples in the plugin's guide are
valid calls. The tests use a stand-in Dynodoc server.

The web screens have their own automated tests (the settings form, the usage table, the stop
button and the agent-key choice), and a browser test that ran them against a local test
installation with the agent connector switched off.

**Not done:** running Claude Code or Claude Desktop against it, running the screens or the
write tools against an installation with the agent connector switched on (the write routes of
the server have their own tests; the connector program was never run against them), and any
measurement of how the AI behaves with these tools, including whether it keeps to the
"nothing is live" wording. The vendor facts above marked "unverified" must be re-checked when this is
first installed.
