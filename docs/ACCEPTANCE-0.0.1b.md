# Acceptance run, Dynodoc desktop 0.0.1b (pre-alpha)

Run on 26 and 27 October 2026 on one Windows 11 PC (x86-64), with the real Microsoft Word, Excel and
PowerPoint installed on it and Microsoft Edge as the person's browser. Decisions:
[019](decisions/019-standalone-desktop-local-workspace.md),
[020](decisions/020-desktop-web-sign-in-and-sync.md). The drivers are in
[`apps/desktop/scripts/acceptance`](../apps/desktop/scripts/acceptance/README.md); the evidence (logs and
screenshots) is kept with the release, and a few small screenshots are in [`docs/images`](images).

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

SCENARIO_TABLE

## Details

DETAILS

## What could not be tested, and known limits

LIMITS
