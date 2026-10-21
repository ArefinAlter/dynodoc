// Injected by the shell into every page of the main window, BEFORE any script of the page runs
// (at document creation). It has no access to the app; it only calls window.open, which the
// shell's new-window rule then handles. Two jobs:
//
// 1. WebView2 raises no "new window" event for a click on <a target="_blank">, only for
//    window.open(), so such a click would do nothing at all. This turns it into window.open.
//
// 2. WebView2 does not say whether a person clicked, so a page's script could make the
//    system browser or mail program open over and over, or open local addresses. A TRUSTED
//    click (event.isTrusted: a script cannot fake one) on a link that leaves the page, or on a
//    mail link, is passed to the shell as a marker address that carries a secret. Only such a
//    marked request may open a mail link or a local address; every other request is limited by
//    the shell. The shell also limits marked requests (one a second, ten a minute).
//
// KEEPING THE SECRET FROM THE PAGE. The page runs after this script and may replace anything
// it can reach: Function.prototype.call/apply/bind, Reflect, Array/String/RegExp prototypes,
// Element.prototype.closest, URL, window.open, addEventListener, the getters of Event, Node
// and HTMLAnchorElement. So this script touches NOTHING the page can replace at click time:
//   * everything it will ever call is captured here, once, at injection (the built-in
//     functions themselves, not names looked up later);
//   * calls go through "uncurried" bound functions (bind.bind(call)), which hold the original
//     Function.prototype.call and never look up `.call` or `.apply` again;
//   * the DOM is read with the captured native getters, which throw on anything that is not a
//     real node/event/anchor, so a fake link object is rejected by the brand check;
//   * there are no array methods, iterators, regular expressions, closest() or URL objects;
//     only string comparison and concatenation of primitive strings;
//   * the secret is in this closure only, and the marker address is built from primitive
//     strings and handed to the captured native window.open, which no page code sees.
// If any built-in this needs is missing, nothing is installed (the page then gets the
// browser's plain behaviour, which the shell limits).
(function () {
  'use strict';
  var secret = '__GESTURE_SECRET__';
  var marker = 'https://gesture.dynodoc.invalid/';
  try {
    var uncurry = Function.prototype.bind.bind(Function.prototype.call);
    var describe = Object.getOwnPropertyDescriptor;
    var getter = function (proto, name) {
      return uncurry(describe(proto, name).get);
    };
    var eventTarget = getter(Event.prototype, 'target');
    var eventDefaultPrevented = getter(Event.prototype, 'defaultPrevented');
    var eventButton = getter(MouseEvent.prototype, 'button');
    var eventPreventDefault = uncurry(Event.prototype.preventDefault);
    var nodeParent = getter(Node.prototype, 'parentNode');
    var nodeName = getter(Node.prototype, 'nodeName');
    var anchorHref = getter(HTMLAnchorElement.prototype, 'href');
    var anchorProtocol = getter(HTMLAnchorElement.prototype, 'protocol');
    var anchorOrigin = getter(HTMLAnchorElement.prototype, 'origin');
    var hasAttribute = uncurry(Element.prototype.hasAttribute);
    var getAttribute = uncurry(Element.prototype.getAttribute);
    var lowerCase = uncurry(String.prototype.toLowerCase);
    var encode = encodeURIComponent;
    var win = window;
    var open = uncurry(win.open);
    var addListener = uncurry(EventTarget.prototype.addEventListener);
    var doc = document;
    var here = win.location.origin;
    if (typeof here !== 'string' || typeof open !== 'function') return;

    // The nearest <a> that has an href, found without closest(). A node that is not a real
    // anchor makes the captured getter throw, and a fake object makes the parent getter throw.
    var findLink = function (start) {
      var node = start;
      for (var depth = 0; depth < 100 && node; depth += 1) {
        var href = null;
        try {
          // Ask the (brand-checked) anchor getter only of nodes that say they are an <a>;
          // anything that is not a real HTML anchor makes it throw and is passed over.
          if (nodeName(node) === 'A') href = anchorHref(node);
        } catch (notAnchor) {
          href = null;
        }
        if (typeof href === 'string' && href !== '') return node;
        try {
          node = nodeParent(node);
        } catch (notNode) {
          return null;
        }
      }
      return null;
    };

    var onClick = function (event) {
      try {
        // isTrusted is an own, non-configurable property of the event the browser made.
        if (event.isTrusted !== true) return;
        if (eventDefaultPrevented(event) === true || eventButton(event) !== 0) return;
        var link = findLink(eventTarget(event));
        if (link === null || hasAttribute(link, 'download')) return;
        var protocol = anchorProtocol(link);
        var href = anchorHref(link);
        var web = protocol === 'http:' || protocol === 'https:';
        if (protocol === 'mailto:' || (web && anchorOrigin(link) !== here)) {
          eventPreventDefault(event);
          open(win, marker + secret + '?u=' + encode(href), '_blank');
          return;
        }
        if (web) {
          var target = getAttribute(link, 'target');
          if (typeof target === 'string') target = lowerCase(target);
          if (target === '_blank' || target === '_new') {
            eventPreventDefault(event);
            open(win, href, '_blank');
          }
        }
      } catch (ignored) {
        // Never break the page's own click handling.
      }
    };
    addListener(doc, 'click', onClick, true);
  } catch (unsupported) {
    // A built-in is missing: install nothing rather than something the page could subvert.
  }
})();
