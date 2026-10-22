// Injected on ANDROID ONLY, into every page and frame of the main window. It has no secret, no
// access to the app and does not call window.open; the shell's navigation rule decides
// everything.
//
// Why it exists. Android's web view has no second window here (setSupportMultipleWindows is off),
// and in that case it DROPS a tap on <a target="_blank"> without any callback to the app: the
// link does nothing at all (measured on Android 16, WebView 133). So on a tap (or a script's
// click) on such a link, the target is turned into "_self" for the moment the browser reads it
// and put back at once. The link then loads in the one window, where the shell's navigation rule
// treats it like any other address: a page of the configured server loads, any other public
// web address goes to the system browser (at a limited rate), everything else is dropped.
//
// What it is not. It is not a security measure: a page that disables it (replaces
// Element.prototype.getAttribute, stops the event, ...) only gets the Android default for its own
// links, which is nothing. It runs before the page's scripts when the web view supports that
// (document-start scripts, WebView 86 and later) and shortly after the page starts otherwise;
// either is early enough, the listener only matters at click time.
(function () {
  'use strict';
  try {
    document.addEventListener(
      'click',
      function (event) {
        try {
          var node = event.target;
          for (var depth = 0; node && depth < 100; depth += 1) {
            if (node.nodeName === 'A' && node.hasAttribute('href')) {
              var target = node.getAttribute('target');
              if (typeof target !== 'string') return;
              var lower = target.toLowerCase();
              if (lower !== '_blank' && lower !== '_new') return;
              node.setAttribute('target', '_self');
              setTimeout(function () {
                node.setAttribute('target', target);
              }, 0);
              return;
            }
            node = node.parentNode;
          }
        } catch (ignored) {
          // Never break the page's own click handling.
        }
      },
      true
    );
  } catch (unsupported) {
    // No document or no addEventListener: nothing to do.
  }
})();
