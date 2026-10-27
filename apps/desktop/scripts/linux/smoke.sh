#!/bin/bash
# Smoke test of the Linux build under a virtual display (Xvfb) with a window manager (openbox), as an ordinary
# user (PostgreSQL refuses to run as root). It installs the .deb, starts the app, enters a name on the
# first screen, waits for the workspace, and then does with the mouse what a person does to a window that
# draws its own frame: move it by the bar, resize it by an edge and a corner, maximise it by double click
# and with the button, restore it, and close it. Screenshots and a log go to $OUT.
#
# Run from the root of a Linux system that has the .deb (WSL: wsl -d <distro> -u root -- bash <this file>):
#   DEB=/mnt/d/.../Dynodoc_0.0.1-beta_amd64.deb OUT=/mnt/d/.../evidence/linux bash smoke.sh
# Needs: xvfb openbox xdotool x11-utils imagemagick libwebkit2gtk-4.1-0 libgtk-3-0 and a user named `dyno`.
set -u
DEB=${DEB:?DEB is the .deb to install}
OUT=${OUT:?OUT is where to put the screenshots and the log}
APPUSER=${APPUSER:-dyno}
mkdir -p "$OUT"
LOG="$OUT/smoke.log"
: > "$LOG"
say() { echo "$*" | tee -a "$LOG"; }

dpkg -i "$DEB" >> "$LOG" 2>&1 && say "installed: $(dpkg -s dynodoc | grep -E '^(Version|Installed-Size)' | tr '\n' ' ')" || { say "dpkg -i failed"; exit 1; }
chmod 777 "$OUT"
# The part that runs as the ordinary user.
cat > /tmp/dynodoc-smoke-user.sh <<'USERPART'
#!/bin/bash
set -u
OUT=$1
LOG="$OUT/smoke.log"
say() { echo "$*" | tee -a "$LOG"; }
export DISPLAY=:99 GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1
unset WAYLAND_DISPLAY
rm -rf "$HOME/.local/share/online.dynodoc.client" "$HOME/.config/online.dynodoc.client" "$HOME/.cache/online.dynodoc.client"
Xvfb :99 -screen 0 1600x900x24 -nolisten tcp > "$OUT/xvfb.log" 2>&1 &
XVFB=$!
sleep 2
openbox > "$OUT/openbox.log" 2>&1 &
sleep 1
shot() { import -window root "$OUT/$1.png" 2>/dev/null; }
geom() { xdotool getwindowgeometry --shell "$1" 2>/dev/null | tr '\n' ' '; }
parse() { eval "$(xdotool getwindowgeometry --shell "$1")"; X0=$X; Y0=$Y; W0=$WIDTH; H0=$HEIGHT; }
crop() { parse "$WID"; convert "$OUT/$1.png" -crop "${W0}x${H0}+${X0}+${Y0}" +repage "$OUT/$2.png" 2>/dev/null; }

/usr/bin/dynodoc-desktop > "$OUT/app.stdout" 2> "$OUT/app.stderr" &
APP=$!
say "started pid $APP"
WID=""
for i in $(seq 1 60); do
  WID=$(xdotool search --onlyvisible --name 'Dynodoc' 2>/dev/null | head -1)
  [ -n "$WID" ] && break
  sleep 1
done
[ -z "$WID" ] && { say "no window appeared"; tail -20 "$OUT/app.stderr" | tee -a "$LOG"; kill $APP 2>/dev/null; kill $XVFB; exit 1; }
say "window $WID: $(xdotool getwindowname "$WID")"
sleep 6
parse "$WID"
say "geometry at start: ${W0}x${H0} at ${X0},${Y0}"
say "window manager hints (no decorations means 0x2 .. 0x0 in the third field): $(xprop -id "$WID" _MOTIF_WM_HINTS 2>/dev/null)"
say "frame extents the window manager gave it: $(xprop -id "$WID" _NET_FRAME_EXTENTS 2>/dev/null)"
shot 01-first-screen; crop 01-first-screen 01-first-screen-window

# The first screen asks for a name (the field has the focus).
xdotool windowfocus "$WID" 2>/dev/null; sleep 0.5
# Click the name field (the first field of the card, 359 px down in the 1280 by 820 window).
xdotool mousemove $((X0 + W0 / 2)) $((Y0 + 359)) click 1; sleep 0.5
xdotool type --delay 60 "Priya Raman"
sleep 0.5
shot 02-name;
xdotool key Return
say "name entered; waiting for the workspace"
READY=0
for i in $(seq 1 240); do
  if grep -aq "the local workspace is ready" "$HOME/.local/share/online.dynodoc.client/workspace/logs/app.log" 2>/dev/null && [ "$(grep -ac 'the local workspace is ready' "$HOME/.local/share/online.dynodoc.client/workspace/logs/app.log")" -ge 1 ]; then READY=1; fi
  [ "$READY" = 1 ] && [ $i -gt 25 ] && break
  sleep 1
done
say "workspace log: $(tail -3 "$HOME/.local/share/online.dynodoc.client/workspace/logs/app.log" 2>/dev/null | tr '\n' '|')"
sleep 8
parse "$WID"
shot 03-workspace; crop 03-workspace 03-workspace-window
say "geometry now: ${W0}x${H0} at ${X0},${Y0}"
ps -eo pid,user,args | grep -E "postgres|engine-api" | grep -v grep | cut -c1-110 | tee -a "$LOG"

# Move the window by dragging the bar (an empty part of it).
BX=$((X0 + W0 / 2 + 40)); BY=$((Y0 + 20))
xdotool mousemove $BX $BY mousedown 1; sleep 0.3
for s in 1 2 3 4 5 6 7 8; do xdotool mousemove_relative -- 12 8; sleep 0.05; done
xdotool mouseup 1; sleep 0.8
parse "$WID"; say "after dragging the bar: ${W0}x${H0} at ${X0},${Y0}"
# Resize by the bottom right corner (the window draws no frame; the shell makes the border a resize area).
xdotool mousemove $((X0 + W0 - 2)) $((Y0 + H0 - 2)) mousedown 1; sleep 0.3
for s in 1 2 3 4 5 6 7 8; do xdotool mousemove_relative -- -10 -6; sleep 0.05; done
xdotool mouseup 1; sleep 0.8
parse "$WID"; say "after dragging the bottom right corner: ${W0}x${H0} at ${X0},${Y0}"
# Resize by the left edge.
xdotool mousemove $((X0 + 1)) $((Y0 + H0 / 2)) mousedown 1; sleep 0.3
for s in 1 2 3 4 5 6; do xdotool mousemove_relative -- 8 0; sleep 0.05; done
xdotool mouseup 1; sleep 0.8
parse "$WID"; say "after dragging the left edge: ${W0}x${H0} at ${X0},${Y0}"
shot 04-moved-resized; crop 04-moved-resized 04-moved-resized-window
# Hover the close button (the red one), then the maximise button.
xdotool mousemove $((X0 + W0 - 23)) $((Y0 + 20)); sleep 0.6; shot 05-hover-close; crop 05-hover-close 05-hover-close-window
# Double click on the bar maximises, again restores.
xdotool mousemove $((X0 + W0 / 2 + 40)) $((Y0 + 20)) click --repeat 2 --delay 90 1; sleep 1.2
parse "$WID"; say "after a double click on the bar: ${W0}x${H0} at ${X0},${Y0}"
shot 06-maximized-by-double-click
xdotool mousemove $((X0 + W0 / 2 + 40)) $((Y0 + 20)) click --repeat 2 --delay 90 1; sleep 1.2
parse "$WID"; say "after another double click: ${W0}x${H0} at ${X0},${Y0}"
# The maximise button, then again.
xdotool mousemove $((X0 + W0 - 69)) $((Y0 + 20)) click 1; sleep 1.2
parse "$WID"; say "after the maximise button: ${W0}x${H0} at ${X0},${Y0}"
shot 07-maximized-by-button
xdotool mousemove $((X0 + W0 - 69)) $((Y0 + 20)) click 1; sleep 1.2
parse "$WID"; say "after the button again: ${W0}x${H0} at ${X0},${Y0}"
shot 08-restored; crop 08-restored 08-restored-window
# The application menu: the brand in the sidebar header.
xdotool mousemove $((X0 + 95)) $((Y0 + 20)) click 1; sleep 1
xdotool mousemove $((X0 + W0 / 2)) $((Y0 + H0 - 80)); sleep 0.5
shot 09-application-menu; crop 09-application-menu 09-application-menu-window
xdotool key Escape; sleep 0.5
# Minimise with the button and bring it back.
xdotool mousemove $((X0 + W0 - 115)) $((Y0 + 20)) click 1; sleep 1.2
say "after the minimise button, visible windows named Dynodoc: $(xdotool search --onlyvisible --name 'Dynodoc' 2>/dev/null | wc -l)"
xdotool windowmap "$WID" windowactivate "$WID" 2>/dev/null; sleep 1
# Close with the button: the app must end, with its database and engine.
parse "$WID"
xdotool mousemove $((X0 + W0 - 23)) $((Y0 + 20)) click 1
for i in $(seq 1 40); do kill -0 $APP 2>/dev/null || break; sleep 1; done
if kill -0 $APP 2>/dev/null; then say "the app is still running after the close button"; else say "the app ended after the close button"; fi
sleep 3
say "left running: $(ps -eo pid,args | grep -E 'postgres|engine-api|dynodoc-desktop' | grep -v grep | wc -l) process(es)"
tail -5 "$HOME/.local/share/online.dynodoc.client/workspace/logs/app.log" 2>/dev/null | tee -a "$LOG"
echo "--- stderr (first lines)" >> "$LOG"; head -20 "$OUT/app.stderr" >> "$LOG"
kill $XVFB 2>/dev/null
USERPART
chmod +x /tmp/dynodoc-smoke-user.sh
su - "$APPUSER" -c "bash /tmp/dynodoc-smoke-user.sh '$OUT'"
say "done"
