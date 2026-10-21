// Injected by the shell into every page of the main window (before the page's own scripts).
// It has no access to the app; it only calls window.open, which the shell's new-window rule
// then handles. Two jobs:
//
// 1. WebView2 raises no "new window" event for a click on <a target="_blank">, only for
//    window.open(), so such a click would do nothing at all. This turns it into window.open.
//
// 2. WebView2 does not say whether a person clicked, so a page's script could make the
//    system browser or mail program open over and over, or open local addresses. A TRUSTED
//    click (event.isTrusted: a script cannot fake one) on a link that leaves the page, or on
//    a mail link, is passed to the shell as a marker address that carries a secret known only
//    to this script (a closure variable, made at start-up). Only such a marked request may
//    open a mail link or a local address; every other request is limited by the shell.
(function () {
  'use strict';
  var secret = '__GESTURE_SECRET__';
  var nativeOpen = window.open; // kept now, so a page that replaces window.open cannot see the secret
  var marker = 'https://gesture.dynodoc.invalid/';
  document.addEventListener(
    'click',
    function (event) {
      if (!event.isTrusted || event.defaultPrevented || event.button !== 0) return;
      var node = event.target;
      var link = node && node.closest ? node.closest('a[href]') : null;
      if (!link || link.hasAttribute('download') || typeof link.href !== 'string') return;
      var target;
      try {
        target = new URL(link.href);
      } catch (e) {
        return;
      }
      var web = target.protocol === 'http:' || target.protocol === 'https:';
      var mail = target.protocol === 'mailto:';
      if (mail || (web && target.origin !== window.location.origin)) {
        event.preventDefault();
        nativeOpen.call(window, marker + secret + '?u=' + encodeURIComponent(link.href), '_blank');
        return;
      }
      if (web && /^_(blank|new)$/i.test(link.getAttribute('target') || '')) {
        event.preventDefault();
        nativeOpen.call(window, link.href, '_blank');
      }
    },
    true
  );
})();
