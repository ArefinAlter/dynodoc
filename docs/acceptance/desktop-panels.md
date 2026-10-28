# Desktop panel resizing

The existing manager UI keeps list/detail and thread/chat columns within available space. Grid and flex children can shrink, long facts wrap, and the split list scales with the workspace. In smaller windows, thread details remain available as an overlay with a close control inside it. Committed launcher assets were rebuilt from the client sources and checked for reproducibility.

Validation on 28 October 2026: frontend type checks, existing frontend tests and a production asset build passed. Fresh Windows Edge profiles passed an automated interaction run at 1366×768, 1024×600, 960×500, 800×600, 640×480, 390×500, 1280×400 and 1920×1080, including a return to the wide layout. Root scrolling stayed inside the window. Thread details could be opened, closed from within the pane, and reopened at narrow widths. Screenshots were inspected. These browser checks exercise the shared desktop CSS; installed Linux acceptance is not inferred from them.

The additional Windows Edge route matrix passed 8 workspace views at 1024×600, 800×600, 640×480 and 1280×400. Windows native builds and WebView2 acceptance are recorded separately when complete. The existing 0.0.1c downloads are unchanged by this source patch.
