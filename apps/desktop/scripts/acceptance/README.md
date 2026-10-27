# Acceptance drivers (0.0.1b)

The scripts that drove the real desktop window for [ACCEPTANCE-0.0.1b.md](../../../../docs/ACCEPTANCE-0.0.1b.md).
They are evidence of what was run, not polished tooling: paths are those of the machine the run was
made on (`D:/verne-build/dynodoc/...`), so edit them before use.

| File | What it does |
|---|---|
| `cdp.mjs`, `run.mjs` | Start the debug app on a scratch home with WebView2's remote debugging port, drive the page (click, type, read, screenshot), close it the way a person does |
| `acc2.mjs` | Scenarios 1 to 3: first launch, import of real Office files, history, drafts, change requests, comments, merges with conflicts, restore, export and open, folder watch with real Word, Excel and PowerPoint |
| `acc4.mjs`, `browser.mjs`, `startweb.mjs`, `devstack.sh`, `restart-engine.sh` | Scenario 4: a self-hosted server (web tier of this branch with the development sign-in link, a throw-away engine whose access tokens last 30 seconds), Microsoft Edge as the person's browser (Playwright), sign-in, link, sync, conflicts, sign-out, restart |
| `acc5.mjs`, `mockllm.mjs` | Scenario 5: the agent against a mock provider that speaks the Anthropic, OpenAI and Gemini wire formats |
| `acc6.mjs` | Scenario 6: settings persistence, backup and restore |
| `acc7.mjs` | Scenario 7: crash safety |
| `dialog-daemon.ps1`, `file-dialog.ps1` | Answer the real Windows Open, Save and folder dialogs of an installed build by their titles (the release build has no seam for them) |
| `smoke-install.mjs` | First smoke of an installed copy (installer files, data beside the program) |
| `frame-shots.mjs`, `frame_shots.py` | The frameless window: real drags, resize, double click, controls, menu, light and dark photographs. The driver refuses to send input or take a picture unless Dynodoc is in front and uncovered |
| `webwin.mjs` | The web workspace window: a stand-in server page tries every command; all must be refused |
| `../linux/smoke.sh` | The Linux .deb under Xvfb and openbox as an ordinary user: window, workspace, move, resize, maximise, close |

The debug build's test seams (`DYNODOC_DESKTOP_HOME`, `DYNODOC_PROGRAMS`, `DYNODOC_TEST_PICK`,
`DYNODOC_TEST_BROWSER`) are not in release builds.
