# Acceptance run, Dynodoc desktop 0.0.1b (pre-alpha)

Run on 26 and 27 October 2026 on one Windows 11 PC (x86-64), with the real Microsoft Word, Excel and
PowerPoint installed on it and Microsoft Edge as the person's browser. Decisions:
[019](decisions/019-standalone-desktop-local-workspace.md),
[020](decisions/020-desktop-web-sign-in-and-sync.md) and
[021](decisions/021-desktop-frameless-window-and-web-workspace-window.md). The drivers are in
[`apps/desktop/scripts/acceptance`](../apps/desktop/scripts/acceptance/README.md); the logs they wrote are in
[`docs/acceptance-0.0.1b`](acceptance-0.0.1b), and three screenshots of the installed app are in [`docs/images`](images).

## How it was run

- **The window is real.** The app is started on a scratch home (a debug build reads
  `DYNODOC_DESKTOP_HOME`; a portable copy keeps its data beside the program) and driven through the
  WebView2 remote debugging port: clicks, typing, reading what the page shows, screenshots. The owner's
  real app folders were never read or written; credential-store entries made by the runs are named for
  the scratch folders and were removed afterwards.
- **What is real:** the bundled PostgreSQL 16.15 and the Dynodoc engine (the same binaries the installer
  carries), the Office codec, real `.docx`, `.xlsx` and `.pptx` files saved by real Word, Excel and
  PowerPoint (edited through their COM automation, or with python-docx and re-saved by Word), the
  system's default program opening an exported file, the operating system's credential store, a
  self-hosted web tier built from this branch with a throw-away engine behind it, Microsoft Edge signing
  in on that server's own pages.
- **What is a stand-in:** the model provider (a mock on loopback that speaks the Anthropic, OpenAI and
  Gemini wire formats and plays a small script; no live provider was called, see scenario 5), and the
  file and browser dialogs in the debug build (a seam answers them; the installed release build was
  driven through the real Windows dialog). A debug build has seams a release build does not read:
  `DYNODOC_DESKTOP_HOME`, `DYNODOC_PROGRAMS`, `DYNODOC_TEST_PICK`, `DYNODOC_TEST_BROWSER`.
- **Not claimed:** nothing here was run on Linux, macOS, a second Windows version, a machine without
  the Office programs, or a physical second computer.

## The seven scenarios

| # | Scenario | Result | Run on | Evidence |
|---|---|---|---|---|
| 1 | First launch: the app starts its own database and engine, asks for a name, makes a first project; no server, no account | **Passed** | the installed release build (installer files, data kept in a folder beside the program) | [scenarios 1 to 3](acceptance-0.0.1b/scenarios-1-3-installed.txt) |
| 2 | Real Word, Excel and PowerPoint files: import, preview, history and differences, drafts, change requests with a review comment, merge, a conflict resolved by choosing a side, a named version restored as new changes, export of a copy that opens in the program the system uses, verification of every hash chain | **Passed** | the installed release build, real Office | same file |
| 3 | A folder bound to a project: files written to it and saved by real Word, Excel and PowerPoint become drafts, with a notice | **Passed** | the installed release build, real Office | same file |
| 4 | Sign in to a server with the system browser, see its projects, link a project, sync both ways, a conflict that waits for a choice, sign out, sign in again, restart | **Passed with limits** | a debug build of the same code, a self-hosted server (this branch's web tier and a throw-away engine), Microsoft Edge as the browser | [scenario 4](acceptance-0.0.1b/scenario-4-debug-build-fresh-server.txt) and [a repeat that fails](acceptance-0.0.1b/scenario-4-repeat-on-the-same-server-fails.txt) |
| 5 | The agent with a model provider of the person's own: keys, consent before the first send, caps, offline switch, tools, proposals as cards, refusal of anything that approves or merges, stop, no key on disk or in a log | **Passed with limits** (a stand-in provider, not a live one) | the installed release build | [scenario 5](acceptance-0.0.1b/scenario-5-installed.txt) |
| 6 | Settings are kept in a file and applied again after a restart; a backup of the whole workspace is written and restored over a later change | **Passed** | the installed release build, the real Windows file dialogs | [scenario 6](acceptance-0.0.1b/scenario-6-installed.txt) |
| 7 | Kill the app and the database in the middle of writes: the data is there, the engine starts, every hash chain verifies | **Passed** | the installed release build | [scenario 7](acceptance-0.0.1b/scenario-7-installed.txt) |
| W | The window that draws its own frame (owner request of 27 October): drag, resize, maximise, controls, menu, light and dark; a server's pages in a window of their own with no access to the app | **Passed on Windows; Linux checked with limits** | the installed release build; the Linux .deb under Xvfb and openbox | [window frame](acceptance-0.0.1b/window-frame-installed.txt), [web window](acceptance-0.0.1b/web-window-isolation-installed.txt), [Linux](acceptance-0.0.1b/linux-smoke.txt) |

## Details

### 1 to 3, the installed build

The installer was run silently into a folder, a `dynodoc-data` folder was made beside the program (which is what makes the program keep everything there), and the program was driven through WebView2's debugging port. The Windows file dialogs of this release build were answered by a small program that finds each dialog by its title and types into it, as a person would. A first start took about a minute on a busy machine (the database is made once). Three real files (a `.docx`, an `.xlsx` and a `.pptx`) were imported with previews that show real content; every chain verified (`Verified 3 revisions in 3 files`, `9`, `13`). Real Word, Excel and PowerPoint then saved edits into the bound folder (Word after a python-docx edit, Excel and PowerPoint edited through their automation interfaces and saved), each became a draft with a notice, was reviewed with a comment, and merged. A second copy of an older revision, edited at the same sentence, produced a conflict card; "Use all of theirs" merged it; a named version was restored as new changes and merged (`retained for ten years` again). Copies exported to the folder opened in the system's default program.

### 4, sign-in and sync

Against a self-hosted server built from this branch (the web tier with its development sign-in, a throw-away engine whose access tokens last 30 seconds so every request renews the session) and Microsoft Edge: the app asked the system browser to open the server's sign-in page with a challenge, a state and a loopback port (the verifier is not in the address); the page showed the same four-character pairing code as the app; after Continue the browser landed on the app's loopback address and the app showed "Signed in as Priya Raman"; the session is in the Windows Credential Manager and nowhere else (`accounts.json` holds addresses and names only). A project was linked and three files were created on the server with the same nodes, ids and content (each side's own chain verifies); an edit on each side travelled both ways (`1 sent, 1 received`); the same paragraph changed on both sides wrote nothing until a side was chosen; sign-out removed the credential entry and the server rejected the old session; a second sign-in used a new pairing code and state; after a restart with the access token expired the session renewed itself. Run on the final code on a freshly made server.

### 5, the agent

A provider stand-in on loopback speaking the Anthropic, OpenAI (Ollama kind) and Gemini wire formats. The key went to the Credential Manager and was never in the page, the preferences file or a log (a scan of 2,284 files under the app's folder found it nowhere, and the stand-in never logged it); nothing was sent before the person allowed that address, with the request shown first; the page itself cannot reach the provider (blocked by the page policy); tool results travelled back in the second and third requests; a thread without the proposing tool had the proposal refused; "merge it" was refused as "there is no such tool" and the request stayed open; a proposed comment became a card that posted only on pressing the button and was marked as drafted by the agent; Stop closed the connection; offline, a cap of zero and a reached day cap sent nothing; a key the provider echoed back was masked on screen; everything held across a restart.

### 6 and 7

Settings: the theme and options were written to a file (no key in it) and applied after a restart. Backup: a 14 MB archive of the whole workspace (with no secret-like entries), a project made afterwards, then the restore through the real dialogs: the later project was gone, the earlier ones were back, every chain verified, and the replaced data was kept beside it. Crash: 80 new documents were written in a burst, then the app and the database were killed; after restart the database recovered from its log, the three existing files had every event (`contiguous`), the server-side and the command-line verifiers agreed, and of the burst 80 were complete, none half-written.

### The window

On the installed release build, with real mouse input guarded so that nothing is sent to another program's window: the window has no system title bar and keeps its sizing frame (the system answers HTLEFT, HTRIGHT, HTTOP, HTBOTTOM and the three corners outside the visible edge), a real drag of a corner and of the left edge resized it, a real drag of the bar moved it, a double click on the bar maximised and another restored it, and the three controls minimised, maximised and restored (the first attempts of this automation, made while another program's window covered Dynodoc, did nothing and are not counted; the driver now refuses to send input or take a picture unless Dynodoc is in front and uncovered). Light and dark, hover on each control (red on close), the application menu, the sidebar closed, maximised, restored, not the active window (the bar dims) and a window 1024 wide (the controls do not cover Fetch) were photographed. A page that a server sent, in the web window, tried 13 commands (the app's own, the window's, the event listener): every one was refused ("not allowed by ACL") and the window's title said "Dynodoc web workspace - 127.0.0.1:... (the server's page)". Ctrl+R and F5 reloaded the client with real key presses (on the debug build), and the client window could not be sent to a server's address. Linux: the .deb installed on Ubuntu 22.04, ran as an ordinary user under Xvfb with openbox, made its database and engine, showed the frameless window (no decoration hints, no frame extents, a one-pixel border), and the window was moved, resized by the left and right edges, maximised and restored with the button, minimised, and closed with the button, after which no process was left. See also decision [021](decisions/021-desktop-frameless-window-and-web-workspace-window.md).

## What could not be tested, and known limits

- **No live model provider was called.** The agent was checked against a stand-in that speaks the three wire formats; a real provider may answer differently. Keep caps low on first use.
- **The hosted server was not changed.** The sign-in through the loopback address needs one addition to the server's sign-in page (decision 020); it is built and tested on a self-hosted server, and it is not deployed on the hosted workspace (the owner's checklist is `docs/operations/2026-10-26-app-signin-loopback.md`). Against the hosted server the app detects that and uses the `dynodoc://` link of 0.0.1a, which works with the installer build and not with the portable zip. Scenario 4 was not run against the hosted server.
- **Scenario 4 ran on a debug build**, because it needs a seam that opens the browser's address in Edge under test; a release build hands the address to the system's own opener, which was not exercised against a real default browser.
- **Syncing the same files into a second project of the same server fails.** Repeating scenario 4 on a server that already held the first run's files failed at the first sync with "It changed on the other side while syncing" (a 409): the engine keeps node identifiers unique across the whole server, so files that were already sent to one project cannot be sent into another. Linking again to the project they went to is the way; the message says nothing of the cause. Seen twice, reproduced on demand, not fixed in this release.
- **Linux.** The .deb was installed and run on Ubuntu 22.04 under a virtual display with a window manager, driven by xdotool; it was not run on a physical desktop, with Wayland, on another distribution or by another window manager. The AppImage was built (it carries the same files) and was not started. A double click on the title bar did not maximise the window in that test (GTK takes the pointer for the move, and neither Tauri's handler nor the fallback added for it saw the second press); the maximise button does. The credential store was not present, so keys and sign-ins use the files the app falls back to.
- **Windows 11 Snap Layouts** do not open when hovering the maximise button of the frameless window (decision 021); dragging to an edge and Win+arrows work.
- **Upgrade over an earlier install, the installer's data choices at uninstall, and the ordinary per-user data folder** (`%LOCALAPPDATA%\online.dynodoc.client`) were not part of these runs, which kept their data beside the program for isolation; the installer and its uninstall hook were tried earlier on a copy.
- **Word.** The document edits that Word saved were made with python-docx and re-saved by Word, not typed in Word's window; Excel and PowerPoint were edited through their automation interfaces. A workbook with charts is not carried whole by the Excel codec (it keeps values and formulas, not chart parts).
- **One computer.** Nothing was run on a second computer, another Windows version, a machine without the Office programs, or macOS (not built).
- **Unsigned.** Windows SmartScreen warns before the installer and the program. There is no automatic update.
- **Automation.** The first attempts of the window automation, made while another program's window was in front of Dynodoc, sent a few mouse and keyboard events to that window; no harm is known. The driver now checks what is under the pointer and in front before every input.
