<script lang="ts">
  import { onMount } from 'svelte';
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  const sections = [
    { id: 'signin', title: 'Sign-in and security' },
    { id: 'preferences', title: 'Preferences on your device' },
    { id: 'offline', title: 'Offline page and cache' },
    { id: 'addins', title: 'Editor add-ins' },
    { id: 'measurements', title: 'Performance measurements' },
    { id: 'controls', title: 'Connected services and browser controls' }
  ];
  let disabled = $state(false),
    saved = $state(false);
  onMount(() => {
    disabled = document.cookie.split(';').some((x) => x.trim() === 'dynodoc_metrics=off');
  });
  function preference(value: boolean) {
    disabled = value;
    const shared =
      location.hostname === 'dynodoc.online' || location.hostname.endsWith('.dynodoc.online')
        ? '; Domain=dynodoc.online'
        : '';
    document.cookie = `dynodoc_metrics=${value ? 'off' : 'on'}; Path=/; Max-Age=15552000; SameSite=Lax${location.protocol === 'https:' ? '; Secure' : ''}${shared}`;
    saved = true;
  }
</script>

<LegalPage
  title="Cookies & local storage"
  description="What Dynodoc stores in your browser and on your device, why it is there, how long it lasts and how to control the optional performance measurements."
  {sections}
>
  <section id="signin">
    <h2>Sign-in and security</h2>
    <p>
      The workspace uses a secure, HTTP-only session cookie to keep you signed in. Short-lived
      cookies protect Google sign-in against forged requests. Each is scoped to the host that sets
      it, so a visit to the public website does not disclose your workspace session. There are no
      advertising cookies and no cross-site tracking identifiers.
    </p>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Sign-in cookies" tabindex="0">
      <table>
        <thead><tr><th>Storage</th><th>Purpose</th><th>Duration</th></tr></thead><tbody
          ><tr
            ><td><code>dynodoc_session</code></td><td
              >Encrypted sign-in session. HTTP-only, SameSite=Lax and Secure over HTTPS. Access
              tokens are renewed while your account stays active.</td
            ><td
              >30 days from its last renewal (renewed while your account stays active); cleared on
              sign-out. Server-side revocation can end it sooner.</td
            ></tr
          ><tr
            ><td><code>oauth_state</code>, <code>oauth_verifier</code>, <code>oauth_next</code></td
            ><td
              >Validate a Google sign-in request and its callback, and remember which page to return
              to.</td
            ><td>Up to 10 minutes; removed when the callback is processed.</td></tr
          ><tr
            ><td><code>forms_pending</code></td><td
              >Bind the optional Google Forms authorization to the signed-in account.</td
            ><td>Up to 10 minutes.</td></tr
          ><tr
            ><td><code>dynodoc_forms</code></td><td
              >Encrypted, HTTP-only, read-only Google Forms access token, used only by the import
              endpoint.</td
            ><td>Up to one hour; cleared when you disconnect or sign out.</td></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      The email sign-in link is not a cookie. It is valid for 15 minutes and Dynodoc keeps the page
      you wanted to reach in local storage briefly so that the link, which opens in a new tab, can
      return you there.
    </p>
  </section>
  <section id="preferences">
    <h2>Preferences on your device</h2>
    <p>
      These items remember choices you made. None is sent to an advertiser or used to track you
      across sites, and the website works without them.
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Preference storage" tabindex="0">
      <table>
        <thead><tr><th>Storage</th><th>Purpose</th><th>Duration</th></tr></thead><tbody
          ><tr
            ><td><code>dynodoc_theme</code> (cookie), <code>dynodoc.theme</code></td><td
              >Your light, dark or system choice, applied before the page is painted. The cookie is
              shared across dynodoc.online subdomains. Set only when you use the theme switch.</td
            ><td>Cookie: one year. Local storage: until cleared.</td></tr
          ><tr
            ><td><code>dynodoc_metrics</code> (cookie)</td><td
              >Remember your performance-measurement choice across subdomains. Set only when you
              change it on this page.</td
            ><td>Up to 180 days.</td></tr
          ><tr
            ><td><code>dynodoc.workspace-view</code></td><td>List or grid view of your files.</td
            ><td>Until cleared.</td></tr
          ><tr
            ><td><code>dynodoc:sheet:…</code></td><td
              >The last sheet tab you viewed in a spreadsheet, per file.</td
            ><td>Until cleared.</td></tr
          ><tr
            ><td><code>dynodoc.sheet-size-notice</code> (session storage)</td><td
              >That you dismissed the small-screen spreadsheet notice.</td
            ><td>Until the tab is closed.</td></tr
          ><tr
            ><td><code>dynodoc.light-pages</code></td><td
              >Your choice to show documents on white pages in night mode.</td
            ><td>Until cleared.</td></tr
          ><tr
            ><td><code>dynodoc.login-next</code></td><td
              >A same-site page address to return to after an email sign-in link opens in a new tab.</td
            ><td>Removed when used; ignored after 20 minutes.</td></tr
          ><tr
            ><td><code>dynodoc.session-ended</code></td><td
              >A timestamp that tells your other tabs you signed out.</td
            ><td>Until cleared.</td></tr
          ><tr
            ><td
              ><code>dynodoc.browser-recovery</code> (preference),
              <code>dynodoc-browser-recovery</code>
              (IndexedDB)</td
            ><td
              >Optional recovery copies of unsaved edits, only after you enable them in the editor’s
              File menu. They contain document content: use them only on a trusted browser.</td
            ><td
              >Copies expire after seven days, at the next storage check, and are cleared on
              sign-out or when you disable recovery. At most 30 copies of 10 MiB each. The
              preference stays until cleared.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section id="offline">
    <h2>Offline page and cache</h2>
    <p>
      The website and the workspace register a service worker, which makes the workspace installable
      as a web app. A service worker is a small script your browser keeps for a site and runs in the
      background. This one does two things. If a page cannot load because the network or the server
      cannot be reached, it shows a short “You are offline” page. It also keeps copies of the site’s
      public script, style and font files after first use, so repeat visits start faster. It never
      stores a page, a document, an API reply, a sign-in response or anything from your account, and
      it does not make documents available offline. The administration, sign-in link, shared-link,
      print and add-in pages, and a few technical routes, do not register it.
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="table-scroll"
      role="region"
      aria-label="Offline page and cache storage"
      tabindex="0"
    >
      <table>
        <thead><tr><th>Storage</th><th>Purpose</th><th>Duration</th></tr></thead><tbody
          ><tr
            ><td>Service worker (<code>/service-worker.js</code>)</td><td
              >Shows the offline page and keeps the public files below. It is not used for tracking.</td
            ><td>Until you clear the site’s data or remove the worker in your browser.</td></tr
          ><tr
            ><td>Cache Storage (<code>dynodoc-shell-…</code>, <code>dynodoc-assets-…</code>)</td><td
              >The offline page, two icons, and public script, style and font files. The names carry
              the build number.</td
            ><td
              >Until you clear the site’s data. The caches of older builds are deleted when a new
              build takes over.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section id="addins">
    <h2>Editor add-ins</h2>
    <p>
      The add-ins set no cookies. Office add-ins keep their capture queue in IndexedDB (the
      <code>dynodoc-provenance</code> database) inside the Office web view, and the connection key
      only in the pane’s memory. Google add-ons keep the queue and key in Apps Script UserProperties
      of the Google user who installed them. None of this is sent anywhere except to Dynodoc when
      you push. See <a href="/privacy#addins">the privacy policy</a> for what is held and how to clear
      it.
    </p>
  </section>
  <section id="measurements">
    <h2>Performance measurements</h2>
    <p>
      Dynodoc measures page loads and web vitals to understand speed and responsiveness. A sample
      holds a random identifier for the current page load, a coarse page category, a metric name and
      a number. The identifier is not saved in a cookie and is not reused as a visitor identity.
    </p>
    <p>
      Samples do not include document titles, text, full addresses, search terms, account emails or
      authentication cookies. They are sent without cookies to the Dynodoc workspace service, stored
      in PostgreSQL and removed after 30 days. Network infrastructure necessarily receives
      connection information such as an IP address; server and provider logs are separate from these
      records.
    </p>
    <p>
      Page-load counts are not counts of unique people. Do Not Track and Global Privacy Control
      signals suppress this collection. The samples are not used for advertising or cross-site
      profiling.
    </p>
    <div class="callout">
      <h3>Your preference</h3>
      <p>
        Optional performance measurements are {disabled ? 'disabled' : 'enabled'} for this browser unless
        a browser privacy signal disables them. Changing this preference affects future measurements;
        it does not remove samples already received.
      </p>
      <div class="preferences">
        <button type="button" onclick={() => preference(true)} aria-pressed={disabled}
          >Disable measurements</button
        ><button type="button" onclick={() => preference(false)} aria-pressed={!disabled}
          >Enable measurements</button
        >
      </div>
      {#if saved}<p role="status">
          Preference saved. Reload open Dynodoc tabs for the change to take effect.
        </p>{/if}
    </div>
  </section>
  <section id="controls">
    <h2>Connected services and browser controls</h2>
    <p>
      Google, Microsoft, Vercel and other services may apply their own storage and privacy practices
      when you visit or authenticate with them or use an add-in inside their products. A Google
      Forms connection is separate from Google sign-in; disconnecting clears its token, and you can
      also remove Dynodoc’s grant in your Google Account.
    </p>
    <p>
      You can inspect, block or clear cookies and local storage in your browser settings. Blocking
      essential sign-in storage prevents signed-in use. Clearing preferences resets the theme and
      view choices. See the <a href="/privacy">privacy policy</a> for account and content handling,
      including retention and the <a href="/privacy#contact">operator’s contact details</a>.
    </p>
  </section>
</LegalPage>

<style>
  .preferences {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .preferences button {
    min-height: 44px;
    font: inherit;
    font-size: 15px;
    padding: 10px 17px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius, 6px);
    background: var(--color-surface-raised);
    color: var(--color-ink);
    cursor: pointer;
  }
  .preferences button:hover {
    border-color: var(--color-ink-soft);
  }
  .preferences button[aria-pressed='true'] {
    border-color: var(--color-ink-strong);
    background: var(--color-ink-strong);
    color: var(--color-parchment);
  }
  .callout h3 {
    margin-top: 0;
  }
  .table-scroll table {
    min-width: 640px;
  }
  .table-scroll td:first-child code {
    white-space: nowrap;
  }
</style>
