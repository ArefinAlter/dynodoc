# Decision 021: the desktop window draws its own frame, and a server's pages open in a window of their own

Date: 27 October 2026. Owner: Verne. Status: built for 0.0.1b (pre-alpha). Extends decisions 016
(devices, processors, desktop and mobile apps) and 019 (the standalone desktop). The security model
it keeps is the one in [DESKTOP-AND-MOBILE-APPS.md](../DESKTOP-AND-MOBILE-APPS.md).

## The question

The 0.0.1b desktop app looked like a standard Windows window: a system title bar and a native menu bar
(File, Edit, View, Help) over a client that is designed after T3 Code. The owner's words: "it is not
blended like T3 Code, it looks like a standard windows window; it should be more stylized". Ogun Studio,
Zeus and Asten already draw their own frame. The same had to be true of Dynodoc without weakening what
the shell guarantees: only the bundled client may call the app's commands, and a page from a server may
call nothing.

## What changed

**A frameless client window (Windows and Linux).** The shell creates the `main` window with decorations
off. On Windows it keeps the system shadow (`shadow: true`), which gives the rounded corners of Windows
11 and the resize borders; on Linux the client draws a one-pixel border (there is no system frame to do
it). The minimum size (800 by 560) is kept. Before the first paint the window is given the client's own
background colour, light or dark by the saved choice and otherwise by the system, so it never flashes
the wrong one. The window-state plugin no longer saves or restores the decorations flag: the frame is
the code's choice, not something an old saved state may undo.

**The client's top bars are the title bar.** The 40 px top bar and the sidebar's brand row carry
`data-tauri-drag-region="deep"` (Tauri's own script: a press on anything that is not a button or an
input moves the window, a double click maximises or restores it). The client draws minimise, maximise
or restore and close at the right edge, 46 px wide and the bar's height, with the system's red under the
pointer on close, dimmed while the window is not the active one, in light and dark. The top bar leaves
room for them, so they never cover Fetch (the server pill goes first at 1100 px and narrower). The
client sets the window's own title to the view on screen for the task bar and Alt+Tab.

**The native menu bar is replaced by an application menu.** The brand (the mark and the name, with a
chevron) in the sidebar's header opens it; with the sidebar closed the mark alone in the top bar does.
It holds what the File, View and Help menus held that still has a place: Settings, Accounts and
servers, Keyboard shortcuts, Command palette (Ctrl+K), Light or dark, Web workspace, Reload (Ctrl+R),
Developer tools (debug builds only), Dynodoc support, About, Quit. The same actions are in the command
palette. Edit is not needed: the web view's own cut, copy and paste, undo and select-all work without a
menu on Windows and Linux. "Change server", "sign in" and "this computer's key" are in Settings,
Accounts since decision 020. Ctrl+R (and F5) now call a command of the app, because a plain reload of
the client is refused by the shell by design (the launcher page loads only when the shell asked for it).
macOS keeps the system's frame and one native menu bar with all of the above (it is not built in this
release).

**"Dynodoc client" and Ctrl+Shift+D.** In the old menu Ctrl+Shift+D brought the launcher back from a
server's page. The client window now never shows a server's page, so there is nothing to come back
from in it, and in the client Ctrl+Shift+D stays what the key table says everywhere else, Withdraw
draft. "Dynodoc client" with Ctrl+Shift+D lives in the web window's menu, where it brings the client
window forward.

## A server's pages open in a window of their own

The old window showed either the client or, after "Web workspace", the server's own pages. A frameless
window cannot do that safely: the pages would have no title bar of their own to move or close the
window with, and the window-control permissions would have to be granted to a window that shows a
server's pages. They are not. The `web` window is created for the server's pages (the pill in the top
bar, the application menu, a `dynodoc://` link that names a page, the sign-in finish page):

- it has the system's frame and its own small menu (Dynodoc client, Open server in browser, Close
  window; Edit; Reload; Help);
- its title says what it is: "Dynodoc web workspace - host (the server's page)";
- it is in **no capability**. The capability files name the `main` window and `local: true`, so a page
  in `web` cannot call a command of the app or a window command, whatever it loads;
- its navigation rules are the old ones for a server's page (same-server pages load, the sign-in
  finish page only as the shell opens it, public links go to the system browser at a limited rate), and
  it can never load the bundled launcher, whose grant only the `main` window may take;
- the client window is decided with no server, so a server's page can never load in it either: a link
  to the server from the client is a link out, or refused;
- closing the client window closes the web window with it.

## The window permissions, exactly

`capabilities/window-controls.json` is a second capability, `local: true`, for the `main` window on
Windows and Linux, with seven permissions and no others: `core:window:allow-minimize`,
`allow-toggle-maximize`, `allow-close`, `allow-start-dragging`, `allow-internal-toggle-maximize`,
`allow-is-maximized`, `allow-set-title`. Nothing that resizes, hides, destroys or creates a window,
nothing of the event, webview or app plugins, and no `core:default`. The page learns that the window is
maximised by asking on resize; it does not listen for events. Two commands of the app's own were added
(`window_reload`, `devtools_open`), with the same second lock as the others (the page must be the
bundled launcher). A probe against the real window (acceptance README) shows `is_decorated`, `set_size`,
`hide`, `event|listen` and `webview_close` refused with "not allowed", and the allowed ones working.

Tests that carry this (Rust): the capability tests now expect three files and the exact list above for
`window-controls`, no `remote`, `local` true, windows `["main"]`, platforms `["linux","windows"]`; no
other capability holds a plugin permission; every `plugin:window|...` in the client bundle is on the
list; the web window's label is in no capability file; the client is decided without a server and the
web window never takes the launcher grant; only the client builder turns decorations off; one builder
makes both windows, so the click script, the app's user-agent suffix and the inspector switch are the
same. No existing assertion was loosened: the two that named `navigate_main` for a server's page now
name `navigate_web`, and the count of `navigate_main` calls is pinned.

## Snap Layouts

Hovering the maximise button of a frameless window does not open the Windows 11 Snap Layouts flyout:
the system offers it only when the window answers its hit test for that button, which means native code
in the window procedure. That is not a risk worth taking in a shell whose rule is "no native access for
anything but the bundled client", so it is left out. Dragging the bar to a screen edge (Aero Snap),
Win+arrows and Win+Z work. Re-open the question if the owner wants the flyout, with a review of the
extra code.

## Not decided here

The linux window was checked headless under Xvfb with a window manager (resize, move, controls), not on
a desktop with Wayland or a compositor of its own. Whether to give the web window a frame of the
client's design (it would then show an address bar the person can trust) is a later question: a window
of the system's is the clearest marking that the page is not the app's.
