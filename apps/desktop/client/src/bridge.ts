// The desktop app's own launcher controls, unchanged in behaviour: choosing the server, signing in
// with the browser or an emailed link, and this computer's key. Their markup is the launcher's
// <main id="launcher"> in index.html (ids, texts and order as before, so the Windows probes still
// find them); the client moves that element into Settings, Accounts. This page talks to the app
// only through the eight commands in src-tauri/src/commands.rs, and every value it sends is
// checked again in Rust.
//
// Tauri's bridge: this page calls window.__TAURI_INTERNALS__.invoke, the function that
// @tauri-apps/api/core wraps. (withGlobalTauri is off, so window.__TAURI__ does not exist, here or
// in a server's pages; the bridge object is still injected everywhere and the permission files
// refuse every call except the launcher's own.)

export interface DesktopState {
  version: string;
  platform: string;
  cloud: string;
  support: string;
  server: string | null;
  unconfirmed: string | null;
  tools: boolean;
  browser_signin: boolean;
  /** `custom` when the window has no system title bar and the client draws its own. */
  frame: 'custom' | 'native';
  /** The web view's inspector can be opened (debug builds only). */
  devtools: boolean;
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<any>;

interface Internals {
  invoke?: Invoke;
}

function bridgeInvoke(): Invoke | null {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: Internals }).__TAURI_INTERNALS__;
  return internals && typeof internals.invoke === 'function' ? internals.invoke : null;
}

export function inDesktop(): boolean {
  return bridgeInvoke() !== null;
}

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`launcher element #${id} is missing`);
  return el;
}
function show(el: HTMLElement, on: boolean): void {
  el.hidden = !on;
}
function say(id: string, text?: string): void {
  const el = $(id);
  el.textContent = text || '';
  // A live region (role alert or status) stays in the page so that filling it is announced.
  if (!el.getAttribute('role')) show(el, Boolean(text));
}
function message(e: unknown): string {
  if (typeof e === 'string') return e;
  const m = e instanceof Error ? e.message : e && typeof e === 'object' && 'message' in e ? String((e as { message: unknown }).message) : '';
  return m || 'Something went wrong. Please try again.';
}
function busy(button: HTMLElement, on: boolean, working?: string): void {
  const b = button as HTMLButtonElement;
  if (on) {
    b.dataset.label = b.textContent ?? '';
    b.textContent = working ?? '';
  } else if (b.dataset.label) {
    b.textContent = b.dataset.label;
  }
  b.disabled = on;
}

let invoke: Invoke | null = null;
let current: DesktopState | null = null;

export function desktopState(): DesktopState | null {
  return current;
}

/** Reads the app's state again (after a command changed the server). */
export async function refreshDesktopState(): Promise<DesktopState | null> {
  if (!invoke) return current;
  try {
    current = (await invoke('get_state')) as DesktopState;
  } catch {
    /* keep what we had */
  }
  return current;
}

/** One of the app's commands (the new ones in remote.rs reach the configured server). */
export async function call(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  const fn = invoke ?? bridgeInvoke();
  if (!fn) throw new Error('This needs the Dynodoc desktop app.');
  try {
    return await fn(cmd, args);
  } catch (e) {
    throw new Error(message(e));
  }
}

async function connect(address: string, errorId: string, button: HTMLElement, working: string): Promise<boolean> {
  if (!invoke) return false;
  say(errorId, '');
  busy(button, true, working);
  try {
    const opened = await invoke('set_server', { address });
    say('current', 'Opening ' + opened + ' ...');
  } catch (e) {
    say(errorId, message(e));
    busy(button, false);
    return false;
  }
  return true;
}

/** Opens the saved server's web workspace in its own window (the same command as "Use this address"). */
export async function openServer(): Promise<string | null> {
  if (!invoke || !current || !current.server) return null;
  try {
    return await invoke('set_server', { address: current.server });
  } catch (e) {
    return Promise.reject(new Error(message(e)));
  }
}

// Whether a key file exists (and looks finished). This reads no key: the public half is fetched
// only when the person asks, because that decrypts the key through the operating system's key
// store, which may prompt.
async function loadKey(): Promise<void> {
  if (!invoke) return;
  const status = await invoke('device_key_status');
  show($('key-public'), false);
  if (!status.exists) {
    show($('key-none'), true);
    show($('key-create'), true);
    show($('key-have'), false);
    return;
  }
  show($('key-none'), false);
  show($('key-create'), false);
  if (status.damaged) {
    show($('key-have'), false);
    say('key-error', status.recovery);
    return;
  }
  show($('key-have'), true);
  $('key-where').textContent = 'Stored at ' + status.path;
}

function wire(): void {
  $('use-cloud').addEventListener('click', function () {
    if (!current) return; // the app has not answered get_state yet
    connect(current.cloud, 'cloud-error', $('use-cloud'), 'Opening ...');
  });

  $('confirm').addEventListener('click', function () {
    if (!current || !current.unconfirmed) return;
    connect(current.unconfirmed, 'confirm-error', $('confirm'), 'Opening ...');
  });

  $('own-form').addEventListener('submit', async function (event) {
    event.preventDefault();
    const ok = await connect(($('address') as HTMLInputElement).value, 'address-error', $('connect'), 'Connecting ...');
    if (!ok) $('address').focus();
  });

  $('signin-form').addEventListener('submit', async function (event) {
    event.preventDefault();
    if (!invoke) return;
    say('link-error', '');
    busy($('signin-go'), true, 'Signing in ...');
    try {
      await invoke('open_sign_in_link', { link: ($('link') as HTMLInputElement).value });
      ($('link') as HTMLInputElement).value = '';
    } catch (e) {
      say('link-error', message(e));
      busy($('signin-go'), false);
      $('link').focus();
      return;
    }
    busy($('signin-go'), false);
  });

  // Sign in with the system browser. The command takes no argument: the app opens the chosen
  // server's own sign-in page, and the page cannot be asked for anything else.
  $('browser-signin-go').addEventListener('click', async function () {
    if (!invoke) return;
    say('browser-signin-error', '');
    say('browser-signin-status', '');
    show($('browser-signin-pairing'), false);
    busy($('browser-signin-go'), true, 'Opening your browser ...');
    try {
      // The four characters this request is known by; the browser page shows the same four.
      const pairing = await invoke('start_browser_sign_in');
      $('browser-signin-code').textContent = pairing;
      show($('browser-signin-pairing'), true);
      say(
        'browser-signin-status',
        'Your browser has opened. Sign in there, check that the pairing code matches, choose Continue, ' +
          'and you will come back to this app. This request is good for 10 minutes.',
      );
    } catch (e) {
      say('browser-signin-error', message(e));
    }
    busy($('browser-signin-go'), false);
  });

  $('key-create').addEventListener('click', async function () {
    if (!invoke) return;
    say('key-error', '');
    say('key-status', '');
    busy($('key-create'), true, 'Creating ...');
    try {
      const publicKey = await invoke('device_key_create');
      await loadKey();
      ($('public-key') as HTMLInputElement).value = publicKey;
      show($('key-show'), false);
      show($('key-public'), true);
      say('key-status', 'Key created. Add its public key in the web app to use it.');
    } catch (e) {
      say('key-error', message(e));
      try {
        await loadKey();
      } catch {
        /* the message above stands */
      }
    }
    busy($('key-create'), false);
  });

  $('key-show').addEventListener('click', async function () {
    if (!invoke) return;
    say('key-error', '');
    say('key-status', '');
    busy($('key-show'), true, 'Reading the key ...');
    try {
      ($('public-key') as HTMLInputElement).value = await invoke('device_public_key');
      show($('key-show'), false);
      show($('key-public'), true);
    } catch (e) {
      say('key-error', message(e));
    }
    busy($('key-show'), false);
  });

  $('key-copy').addEventListener('click', async function () {
    const field = $('public-key') as HTMLInputElement;
    try {
      await navigator.clipboard.writeText(field.value);
      say('key-status', 'Public key copied.');
    } catch {
      field.focus();
      field.select();
      say('key-status', 'Select the public key and copy it with your keyboard.');
    }
  });
}

/** Starts the launcher controls. Returns the app's state, or null outside the desktop app. */
export async function startBridge(): Promise<DesktopState | null> {
  invoke = bridgeInvoke();
  if (!invoke || !document.getElementById('launcher')) return null;
  wire();
  try {
    current = (await invoke('get_state')) as DesktopState;
  } catch (e) {
    say('app-error', message(e));
    return null;
  }
  const state = current;
  // A phone has no menu bar and no window: say what it has.
  const phone = state.platform === 'android' || state.platform === 'ios';
  $('about').textContent = (phone ? 'Dynodoc ' : 'Dynodoc desktop ') + state.version + ' (' + state.platform + ')';
  if (phone) {
    $('lede').textContent =
      'Dynodoc opens your workspace in this app. Choose the Dynodoc cloud, or the address of a ' +
      'server your organisation runs. To choose a different server later, sign out in Dynodoc: ' +
      'this screen comes back.';
  }
  ($('help') as HTMLAnchorElement).href = state.support;
  if (state.unconfirmed) {
    $('unconfirmed-address').textContent = state.unconfirmed;
    show($('unconfirmed'), true);
  }
  if (state.server) {
    say('current', 'Currently set to ' + state.server + '.');
    ($('address') as HTMLInputElement).value = state.server === state.cloud ? '' : state.server;
    // Where this app cannot name itself to the server the browser sign-in would only be refused.
    show($('browser-signin'), state.browser_signin);
    show($('signin-if-button'), state.browser_signin);
    show($('signin'), true);
  }
  if (state.tools) {
    show($('key'), true);
    try {
      await loadKey();
    } catch (e) {
      say('key-error', message(e));
    }
    try {
      const tools = await invoke('tools_version');
      $('about').textContent += '. Tools: ' + tools + '.';
    } catch (e) {
      $('about').textContent += '. Local tools unavailable: ' + message(e);
    }
  }
  return state;
}

/** Only these two fixed values are ever read from the address. */
export function launcherSection(): 'signin' | 'key' | null {
  if (location.hash === '#signin') return 'signin';
  if (location.hash === '#key') return 'key';
  return null;
}

/** After the launcher is on screen: focus or scroll to what the menu item asked for. */
export function focusSection(section: 'signin' | 'key' | null): void {
  if (section === 'signin' && !$('browser-signin').hidden) $('browser-signin-go').focus();
  if (section === 'key' && !$('key').hidden) $('key').scrollIntoView();
}
