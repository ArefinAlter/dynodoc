// What the shell runs in the finish page once it has loaded (signin.rs `finish_script`). The two
// placeholders are replaced with text the shell has checked character by character: the origin of
// the server the sign-in was started for, and `code=..&state=..&verifier=..` (URL-safe characters
// only). The script reads nothing from the page and sends nothing anywhere: it sets
// `location.hash`, once, and only
//  - on exactly the finish page of that origin, with nothing else in its address (so a window that
//    has been sent anywhere else by anything gets nothing), and
//  - after the page has said it is ready (the attribute `data-finish-ready`, set by the page once
//    it listens for a change of the fragment; the page ignores any fragment that was there when it
//    loaded, so setting it earlier would do nothing).
// If the page is not ready after about five seconds the script gives up and the secrets are gone:
// the person starts again.
(function () {
  'use strict';
  var origin = '__ORIGIN__';
  var fragment = '__FRAGMENT__';
  var tries = 0;
  function attempt() {
    if (
      location.origin !== origin ||
      location.pathname !== '/app-signin/finish' ||
      location.search !== '' ||
      location.hash !== ''
    ) {
      return;
    }
    if (document.documentElement.getAttribute('data-finish-ready') === '1') {
      location.hash = fragment;
      return;
    }
    tries += 1;
    if (tries < 100) setTimeout(attempt, 50);
  }
  attempt();
})();
