// The launcher page. No framework, no remote scripts. It talks to the app only through the
// seven commands in src-tauri/src/commands.rs; every value it sends is checked again in Rust.
(function () {
  'use strict';

  var core = window.__TAURI__ && window.__TAURI__.core;
  var invoke = core && core.invoke;

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
    connect(state.cloud, 'cloud-error', $('use-cloud'), 'Opening ...');
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

  function showKey(publicKey, where) {
    show($('key-none'), false);
    show($('key-create'), false);
    show($('key-have'), true);
    $('public-key').value = publicKey || '';
    $('key-where').textContent = where ? 'Stored at ' + where : '';
  }

  async function loadKey() {
    var status = await invoke('device_key_status');
    if (!status.exists) {
      show($('key-none'), true);
      show($('key-create'), true);
      show($('key-have'), false);
      return;
    }
    try {
      showKey(await invoke('device_public_key'), status.path);
    } catch (e) {
      show($('key-have'), false);
      say('key-error', message(e));
    }
  }

  $('key-create').addEventListener('click', async function () {
    say('key-error', '');
    say('key-status', '');
    busy($('key-create'), true, 'Creating ...');
    try {
      var publicKey = await invoke('device_key_create');
      var status = await invoke('device_key_status');
      showKey(publicKey, status.path);
      say('key-status', 'Key created. Add its public key in the web app to use it.');
    } catch (e) {
      say('key-error', message(e));
    }
    busy($('key-create'), false);
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
    $('about').textContent = 'Dynodoc desktop ' + state.version + ' (' + state.platform + ')';
    $('help').href = state.support;
    if (state.server) {
      say('current', 'Currently set to ' + state.server + '.');
      $('address').value = state.server === state.cloud ? '' : state.server;
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
    if (location.hash === '#signin' && !$('signin').hidden) $('link').focus();
    if (location.hash === '#key' && !$('key').hidden) $('key').scrollIntoView();
  }

  start();
})();
