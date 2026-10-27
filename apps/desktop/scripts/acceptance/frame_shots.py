#!/usr/bin/env python3
"""Checks and photographs the frameless Dynodoc window the way a person uses it (Windows only).

    python frame_shots.py --out DIR --scheme light --exe dynodoc-desktop|Dynodoc [--ui-reference A:/ogun-demo/tools/ui-reference]

Uses the Win32 helpers of Ogun's tools/ui-reference/capture_t3.py (the same ones capture_app.py uses): real
mouse and keyboard input, sent only while the app is in front, and screen grabs of the window's own rectangle.
It prints one JSON object per check and saves PNGs named `<scheme>-NN-<what>.png` in DIR:

  style      the window has no caption and still has a sizing frame
  hit tests  what the system answers at the edges and corners (HTLEFT .. HTBOTTOMRIGHT) and on the bar
  resize     a real drag of the right-bottom corner and of the left edge changes the window's size
  move       a real drag of the title bar moves the window
  maximize   a double click on the bar maximises, another restores; the control buttons do the same
  shots      the window, hover on each control, the application menu, the sidebar closed,
             maximised, restored, not the active window, and a 1024 px wide window
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import json
import sys
import time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument('--out', required=True)
ap.add_argument('--scheme', required=True)
ap.add_argument('--exe', default='dynodoc-desktop')
ap.add_argument('--title', default='Dynodoc')
ap.add_argument('--ui-reference', default='A:/ogun-demo/tools/ui-reference')
ap.add_argument('--skip-checks', action='store_true')
a = ap.parse_args()
sys.path.insert(0, a.ui_reference)
import capture_t3 as cap  # noqa: E402
from PIL import ImageGrab  # noqa: E402

u = cap.user32
try:
    u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
except Exception:
    u.SetProcessDPIAware()

out = Path(a.out)
out.mkdir(parents=True, exist_ok=True)
hwnd, title, pid, image = cap.find_window(a.title, a.exe)
n = 0


def say(name: str, **kw) -> None:
    print(json.dumps({'check': name, **kw}))


def rect():
    r = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    return r.left, r.top, r.right, r.bottom


def client():
    return cap.client_rect(hwnd)


def front():
    cap.bring_front(hwnd)
    time.sleep(0.35)


# Nothing is sent to a window that is not Dynodoc's, and nothing is photographed that another window covers: a person
# may be working on this computer, and real mouse and keyboard input goes to whatever is under the pointer or in front.
def root_of(h: int) -> int:
    return int(u.GetAncestor(h, 2)) if h else 0  # GA_ROOT


def ours_at(x: int, y: int) -> bool:
    return root_of(u.WindowFromPoint(wt.POINT(int(x), int(y)))) == hwnd


def guard(x=None, y=None) -> None:
    if root_of(u.GetForegroundWindow()) != hwnd:
        cap.bring_front(hwnd)
        time.sleep(0.5)
        if root_of(u.GetForegroundWindow()) != hwnd:
            raise SystemExit('Dynodoc is not in front and cannot be brought there; nothing was sent')
    if x is not None and not ours_at(x, y):
        raise SystemExit(f'another window is at ({x},{y}); nothing was sent')


def press(combo: str) -> None:
    guard()
    cap.press(combo)


def uncovered() -> bool:
    l, t, r, b = cap.frame_rect(hwnd)
    w, h = r - l, b - t
    points = [(l + 40, t + 80), (l + 40, b - 40), (r - 40, b - 40), (l + w // 2, b - 40), (l + 40, t + h // 2), (r - 40, t + h // 2)]
    return all(ours_at(x, y) for x, y in points)


def place(w: int, h: int, x: int = 120, y: int = 70) -> None:
    if u.IsZoomed(hwnd):
        u.ShowWindow(hwnd, 9)  # SW_RESTORE
        time.sleep(0.4)
    u.SetWindowPos(hwnd, 0, x, y, w, h, 0x0004)  # SWP_NOZORDER
    time.sleep(0.5)
    l, t, r, b = client()
    # An undecorated window's client area is its window; correct any difference once.
    if (r - l, b - t) != (w, h):
        u.SetWindowPos(hwnd, 0, x, y, w + (w - (r - l)), h + (h - (b - t)), 0x0004)
        time.sleep(0.5)


def shot(what: str, need_front: bool = True) -> None:
    global n
    n += 1
    time.sleep(0.3)
    if need_front:
        guard()
    if not uncovered():
        print(json.dumps({'skipped': what, 'why': 'another window covers part of Dynodoc, so a picture of the screen would show it'}))
        return
    l, t, r, b = cap.frame_rect(hwnd)
    img = ImageGrab.grab(bbox=(l, t, r, b), all_screens=True)
    path = out / f'{a.scheme}-{n:02d}-{what}.png'
    img.save(path)
    print(json.dumps({'shot': str(path), 'size': list(img.size)}))


def at(x: int, y: int):
    l, t, _, _ = client()
    return l + x, t + y


def move_to(x: int, y: int) -> None:
    # SetCursorPos is exact in a DPI-aware process; the absolute SendInput mapping drifts by a few pixels.
    guard(x, y)
    u.SetCursorPos(int(x), int(y))


def click_screen(x: int, y: int) -> None:
    move_to(x, y)
    time.sleep(0.1)
    cap.send_inputs([cap.mouse_input(0x0002)])
    time.sleep(0.04)
    cap.send_inputs([cap.mouse_input(0x0004)])


def hover(x: int, y: int) -> None:
    move_to(*at(x, y))
    time.sleep(0.5)


def click(x: int, y: int) -> None:
    click_screen(*at(x, y))
    time.sleep(0.5)


def drag(x0: int, y0: int, x1: int, y1: int) -> None:
    """A real drag: the pointer is placed exactly, then pressed and moved by relative input, as a mouse does."""
    guard()
    sx, sy = at(x0, y0)
    dx, dy = x1 - x0, y1 - y0
    cap.send_inputs([cap.mouse_input(0x0004)])  # make sure no button is held
    move_to(sx, sy)
    time.sleep(0.4)
    cap.send_inputs([cap.mouse_input(0x0002)])  # left down
    time.sleep(0.25)
    steps = 15
    try:
        for i in range(steps):
            cap.send_inputs([cap.mouse_input(0x0001, dx // steps, dy // steps)])  # relative move
            time.sleep(0.05)
        time.sleep(0.2)
    finally:
        cap.send_inputs([cap.mouse_input(0x0004)])  # left up
    time.sleep(0.8)


def nc_hit(x: int, y: int) -> int:
    """What the system answers when asked what is at the screen point (WM_NCHITTEST)."""
    sx, sy = at(x, y)
    lparam = (sy & 0xFFFF) << 16 | (sx & 0xFFFF)
    return int(u.SendMessageW(hwnd, 0x0084, 0, lparam))


HT = {0: 'HTNOWHERE', 1: 'HTCLIENT', 2: 'HTCAPTION', 10: 'HTLEFT', 11: 'HTRIGHT', 12: 'HTTOP', 13: 'HTTOPLEFT', 14: 'HTTOPRIGHT', 15: 'HTBOTTOM', 16: 'HTBOTTOMLEFT', 17: 'HTBOTTOMRIGHT', -1: 'HTTRANSPARENT'}

front()
place(1280, 720)

if not a.skip_checks:
    style = u.GetWindowLongPtrW(hwnd, -16) & 0xFFFFFFFF
    say('style', caption=bool(style & 0x00C00000), thickframe=bool(style & 0x00040000), maximizebox=bool(style & 0x00010000), minimizebox=bool(style & 0x00020000), raw=hex(style))
    l, t, r, b = client()
    w, h = r - l, b - t
    hits = {}
    for name, (x, y) in {'left': (-4, h // 2), 'right': (w + 3, h // 2), 'top': (w // 2, 2), 'bottom': (w // 2, h + 3), 'top-left': (-4, 2), 'bottom-right': (w + 3, h + 3), 'bar': (w // 2, 20), 'middle': (w // 2, h // 2)}.items():
        code = nc_hit(x, y)
        hits[name] = HT.get(code, code)
    say('hit tests', **hits)
    # Resize by a real drag of the bottom right corner, then the left edge.
    before = rect()
    drag(w + 3, h + 3, w - 77, h - 57)
    after = rect()
    say('resize bottom-right', before=[before[2] - before[0], before[3] - before[1]], after=[after[2] - after[0], after[3] - after[1]], changed=(after[2] - after[0]) != (before[2] - before[0]))
    place(1280, 720)
    before = rect()
    drag(-4, 360, 56, 360)
    after = rect()
    say('resize left edge', before=[before[2] - before[0], before[3] - before[1]], after=[after[2] - after[0], after[3] - after[1]], changed=(after[2] - after[0]) != (before[2] - before[0]))
    place(1280, 720)
    # Move by dragging the bar (an empty part of it).
    before = rect()
    drag(w // 2 + 40, 20, w // 2 + 140, 80)
    after = rect()
    say('move by the bar', moved_by=[after[0] - before[0], after[1] - before[1]])
    place(1280, 720)
    # Double click on the bar maximises, again restores.
    click_screen(*at(w // 2 + 40, 20))
    time.sleep(0.08)
    click_screen(*at(w // 2 + 40, 20))
    time.sleep(0.8)
    zoomed = bool(u.IsZoomed(hwnd))
    shot('maximized-by-double-click')
    cw = client()[2] - client()[0]
    click_screen(*at(cw // 2 + 40, 20))
    time.sleep(0.08)
    click_screen(*at(cw // 2 + 40, 20))
    time.sleep(0.8)
    say('double click on the bar', maximized=zoomed, restored=not bool(u.IsZoomed(hwnd)))
    place(1280, 720)

place(1280, 720)
front()
W = 1280
shot('main')
hover(W - 115, 20)
shot('hover-minimize')
hover(W - 69, 20)
shot('hover-maximize')
hover(W - 23, 20)
shot('hover-close')
hover(W // 2, 300)
# The application menu: the brand in the sidebar header.
click(95, 20)
hover(900, 560)  # off the button, so no tooltip covers the menu
shot('application-menu')
press('escape')
time.sleep(0.4)
# Sidebar closed and open again (Ctrl+backslash).
press('ctrl+backslash')
time.sleep(0.6)
shot('sidebar-closed')
click(70, 20)
hover(900, 560)
shot('sidebar-closed-menu')
press('escape')
time.sleep(0.5)
click(28, 20)
time.sleep(0.6)
# Maximise and restore with the controls.
click(W - 69, 20)
time.sleep(0.6)
shot('maximized')
mw = client()[2] - client()[0]
click(mw - 69, 20)
time.sleep(0.6)
say('control buttons', restored=not bool(u.IsZoomed(hwnd)))
shot('restored')
# Not the active window: a small window of this script takes the focus (in a corner away from Dynodoc).
import tkinter as tk

holder = tk.Tk()
holder.title('focus holder')
holder.geometry('150x40+2+2')
holder.update()
holder.focus_force()
holder.update()
time.sleep(0.8)
if root_of(u.GetForegroundWindow()) != hwnd:
    shot('not-active', need_front=False)
holder.destroy()
front()
# The narrow window (1024 wide), where the controls must not cover Fetch.
place(1024, 640)
front()
shot('narrow-1024')
place(1280, 720)
front()
