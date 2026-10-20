// Injected by the shell into every page of the main window (before the page's own scripts).
// WebView2 raises no "new window" event for a click on <a target="_blank">, only for
// window.open(), so such a click would do nothing at all. This turns it into window.open(),
// which the shell's new-window rule then handles: a link to the configured server loads in
// the one window, any other web link opens in the system browser, anything else is dropped.
// It has no access to the app: it only calls the page's own window.open.
(function () {
  'use strict';
  document.addEventListener(
    'click',
    function (event) {
      if (event.defaultPrevented || event.button !== 0) return;
      var node = event.target;
      var link = node && node.closest ? node.closest('a[href]') : null;
      if (!link || link.hasAttribute('download')) return;
      if (!/^_(blank|new)$/i.test(link.getAttribute('target') || '')) return;
      event.preventDefault();
      window.open(link.href, '_blank');
    },
    true
  );
})();
