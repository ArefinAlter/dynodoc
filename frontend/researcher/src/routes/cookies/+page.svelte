<script lang="ts">
  import { onMount } from 'svelte';
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
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

<ProductPage
  title="Cookies & local storage"
  description="What Dynodoc stores in your browser, why it is there, and how to control optional performance measurements."
  eyebrow="Service information · Updated 19 September 2026"
>
  <section>
    <h2>Sign-in and security</h2>
    <p>
      The workspace uses a secure, HTTP-only session cookie to keep you signed in. Short-lived
      cookies protect Google sign-in against forged requests. These are scoped to the host that sets
      them; your homepage visit does not disclose your workspace session to the analytics endpoint.
    </p>
    <div class="table-wrap">
      <table>
        <thead><tr><th>Storage</th><th>Purpose</th><th>Duration</th></tr></thead><tbody
          ><tr
            ><td><code>dynodoc_session</code></td><td
              >Encrypted sign-in session. Access tokens are refreshed while your account remains
              active.</td
            ><td
              >Up to 30 days; cleared on sign-out. Server-side revocation can end access sooner.</td
            ></tr
          ><tr
            ><td><code>oauth_state</code>, <code>oauth_verifier</code></td><td
              >Validate a Google sign-in request and its callback.</td
            ><td>Up to 10 minutes; removed when the callback is processed.</td></tr
          ><tr
            ><td><code>forms_pending</code></td><td
              >Bind the Google Forms connection to the signed-in researcher.</td
            ><td>Up to 10 minutes.</td></tr
          ><tr
            ><td><code>dynodoc_forms</code></td><td
              >Encrypted, HTTP-only read-only Google Forms connection, used only by the Forms import
              endpoint.</td
            ><td>Up to one hour; cleared when you disconnect or sign out.</td></tr
          ><tr
            ><td><code>dynodoc_guide_v1</code> (local storage)</td><td
              >Remember that you have seen or dismissed the welcome guide.</td
            ><td>Until cleared in your browser. You can replay the guide at any time.</td></tr
          ><tr
            ><td><code>dynodoc_metrics</code></td><td
              >Remember your performance-measurement preference across Dynodoc subdomains.</td
            ><td>Up to 180 days.</td></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section>
    <h2>Performance measurements in PostgreSQL</h2>
    <p>
      Dynodoc measures page loads and web vitals to understand speed and responsiveness. These
      samples use a random identifier for the current page load, a coarse page category, a metric
      name and a number. The identifier is not saved in a tracking cookie or reused as a visitor
      identity.
    </p>
    <p>
      Samples do not include document titles, text, form answers, full URLs, search terms, account
      emails or authentication cookies. They are sent to the Dynodoc workspace service and stored in
      PostgreSQL, with a 30-day retention job. Network infrastructure necessarily receives
      connection information such as an IP address; server and provider logs are separate from these
      metric records.
    </p>
    <p>
      Page-load counts are not counts of unique people. Do Not Track and Global Privacy Control
      signals suppress this collection. We do not use these samples for advertising or cross-site
      profiling.
    </p>
  </section>
  <section class="callout">
    <h2>Your preference</h2>
    <p>
      Optional performance measurements are {disabled ? 'disabled' : 'enabled'} for this browser unless
      a browser privacy signal disables them. Changing this preference affects future measurements; it
      does not remove samples already received.
    </p>
    <div class="preferences">
      <button onclick={() => preference(true)} aria-pressed={disabled}>Disable measurements</button
      ><button onclick={() => preference(false)} aria-pressed={!disabled}
        >Enable measurements</button
      >
    </div>
    {#if saved}<p role="status">
        Preference saved. Reload open Dynodoc tabs for the change to take effect.
      </p>{/if}
  </section>
  <section>
    <h2>Connected services</h2>
    <p>
      Google, Vercel and other services may apply their own storage and privacy practices when you
      visit or authenticate with them. A Google Forms connection is separate from normal Google
      sign-in. Disconnecting this browser clears its token; you can also remove Dynodoc’s grant in
      your Google Account.
    </p>
  </section>
  <section>
    <h2>Browser controls and questions</h2>
    <p>
      You can inspect, block or clear cookies and local storage in your browser settings. Blocking
      essential sign-in storage prevents authenticated workspace use. Clearing the guide preference
      may show the welcome message again. See the <a href="/privacy">privacy policy</a> for account and
      content handling, including retention and the operator-contact information still awaiting confirmation.
    </p>
  </section>
</ProductPage>

<style>
  .preferences {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .preferences button {
    font: inherit;
    font-size: 15px;
    padding: 12px 17px;
    border: 1px solid #9db8db;
    border-radius: 6px;
    background: white;
    color: #24548e;
    cursor: pointer;
  }
  .preferences button[aria-pressed='true'] {
    background: #225baa;
    color: white;
  }
</style>
