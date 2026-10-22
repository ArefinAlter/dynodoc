// The launcher page. No framework, no remote scripts. It talks to the app only through the
// seven commands in src-tauri/src/commands.rs; every value it sends is checked again in Rust.
//
// Tauri's bridge: this page calls window.__TAURI_INTERNALS__.invoke, the function that
// @tauri-apps/api/core wraps. (withGlobalTauri is off, so window.__TAURI__ does not exist,
// here or in a server's pages; the bridge object is still injected everywhere and the
// permission files refuse every call except the launcher's own seven.)
(function () {
  'use strict';

  var bridge = window.__TAURI_INTERNALS__;
  var invoke = bridge && bridge.invoke;

  function $(id) {
    return document.getElementById(id);
  }
  function show(el, on) {
    el.hidden = !on;
  }
  function say(id, text) {
    var el = $(id);
    el.textContent = text || '';
    // A live region (role alert or status) stays in the page so that filling it is announced.
    if (!el.getAttribute('role')) show(el, Boolean(text));
  }
  function message(e) {
    return typeof e === 'string' ? e : (e && e.message) || 'Something went wrong. Please try again.';
  }
  function busy(button, on, working) {
    if (on) {
      button.dataset.label = button.textContent;
      button.textContent = working;
    } else if (button.dataset.label) {
      button.textContent = button.dataset.label;
    }
    button.disabled = on;
  }

  if (!invoke) {
    say('app-error', 'This page only works inside the Dynodoc desktop app.');
    return;
  }

  var state = null;

  async function connect(address, errorId, button, working) {
    say(errorId, '');
    busy(button, true, working);
    try {
      var opened = await invoke('set_server', { address: address });
      say('current', 'Opening ' + opened + ' ...');
    } catch (e) {
      say(errorId, message(e));
      busy(button, false);
      return false;
    }
    return true;
  }

  $('use-cloud').addEventListener('click', function () {
    if (!state) return; // the app has not answered get_state yet
    connect(state.cloud, 'cloud-error', $('use-cloud'), 'Opening ...');
  });

  $('confirm').addEventListener('click', function () {
    if (!state) return;
    connect(state.unconfirmed, 'confirm-error', $('confirm'), 'Opening ...');
  });

  $('own-form').addEventListener('submit', async function (event) {
    event.preventDefault();
    var ok = await connect($('address').value, 'address-error', $('connect'), 'Connecting ...');
    if (!ok) $('address').focus();
  });

  $('signin-form').addEventListener('submit', async function (event) {
    event.preventDefault();
    say('link-error', '');
    busy($('signin-go'), true, 'Signing in ...');
    try {
      await invoke('open_sign_in_link', { link: $('link').value });
      $('link').value = '';
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
    say('browser-signin-error', '');
    say('browser-signin-status', '');
    busy($('browser-signin-go'), true, 'Opening your browser ...');
    try {
      await invoke('start_browser_sign_in');
      say(
        'browser-signin-status',
        'Your browser has opened. Sign in there, choose Continue, and you will come back to this app. ' +
          'This request is good for 10 minutes.'
      );
    } catch (e) {
      say('browser-signin-error', message(e));
    }
    busy($('browser-signin-go'), false);
  });

  // Whether a key file exists (and looks finished). This reads no key: the public half is
  // fetched only when the person asks, because that decrypts the key through the operating
  // system's key store, which may prompt.
  async function loadKey() {
    var status = await invoke('device_key_status');
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

  $('key-create').addEventListener('click', async function () {
    say('key-error', '');
    say('key-status', '');
    busy($('key-create'), true, 'Creating ...');
    try {
      var publicKey = await invoke('device_key_create');
      await loadKey();
      $('public-key').value = publicKey;
      show($('key-show'), false);
      show($('key-public'), true);
      say('key-status', 'Key created. Add its public key in the web app to use it.');
    } catch (e) {
      say('key-error', message(e));
      try {
        await loadKey();
      } catch (ignored) {
        /* the message above stands */
      }
    }
    busy($('key-create'), false);
  });

  $('key-show').addEventListener('click', async function () {
    say('key-error', '');
    say('key-status', '');
    busy($('key-show'), true, 'Reading the key ...');
    try {
      $('public-key').value = await invoke('device_public_key');
      show($('key-show'), false);
      show($('key-public'), true);
    } catch (e) {
      say('key-error', message(e));
    }
    busy($('key-show'), false);
  });

  $('key-copy').addEventListener('click', async function () {
    var field = $('public-key');
    try {
      await navigator.clipboard.writeText(field.value);
      say('key-status', 'Public key copied.');
    } catch (e) {
      field.focus();
      field.select();
      say('key-status', 'Select the public key and copy it with your keyboard.');
    }
  });

  async function start() {
    try {
      state = await invoke('get_state');
    } catch (e) {
      say('app-error', message(e));
      return;
    }
    // A phone has no menu bar and no window: say what it has.
    var phone = state.platform === 'android' || state.platform === 'ios';
    $('about').textContent = (phone ? 'Dynodoc ' : 'Dynodoc desktop ') + state.version + ' (' + state.platform + ')';
    if (phone) {
      $('lede').textContent =
        'Dynodoc opens your workspace in this app. Choose the Dynodoc cloud, or the address of a ' +
        'server your organisation runs. To choose a different server later, sign out in Dynodoc: ' +
        'this screen comes back.';
    }
    $('help').href = state.support;
    if (state.unconfirmed) {
      $('unconfirmed-address').textContent = state.unconfirmed;
      show($('unconfirmed'), true);
    }
    if (state.server) {
      say('current', 'Currently set to ' + state.server + '.');
      $('address').value = state.server === state.cloud ? '' : state.server;
      show($('browser-signin'), true);
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
        var tools = await invoke('tools_version');
        $('about').textContent += '. Tools: ' + tools + '.';
      } catch (e) {
        $('about').textContent += '. Local tools unavailable: ' + message(e);
      }
    }
    // Only these two fixed values are ever read from the address.
    if (location.hash === '#signin' && !$('browser-signin').hidden) $('browser-signin-go').focus();
    if (location.hash === '#key' && !$('key').hidden) $('key').scrollIntoView();
  }

  start();
})();
