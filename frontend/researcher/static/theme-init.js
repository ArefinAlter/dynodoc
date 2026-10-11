// Apply the saved day/night preference before first paint (see src/lib/theme.ts).
(function () {
  var root = document.documentElement,
    pref = 'system';
  try {
    var saved = document.cookie.match(/(?:^|; )dynodoc_theme=(light|dark|system)/);
    pref = (saved && saved[1]) || localStorage.getItem('dynodoc.theme') || 'system';
  } catch {
    // Storage may be unavailable in a restricted editor host.
  }
  if (pref !== 'light' && pref !== 'dark') pref = 'system';
  var dark =
    pref === 'dark' ||
    (pref === 'system' &&
      !!window.matchMedia &&
      window.matchMedia('(prefers-color-scheme: dark)').matches);
  root.setAttribute('data-theme', dark ? 'dark' : 'light');
  root.setAttribute('data-theme-preference', pref);
})();
