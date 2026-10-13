<script lang="ts">
  import { onMount } from 'svelte';
  import {
    GRANT_LIMIT,
    ProofFileError,
    createChallenge,
    deviceMessage,
    enrollKey,
    loadProjectChoices,
    loadRegistry,
    revokeKey,
    type DeviceRegistry,
    type ProjectChoice,
    type Subject
  } from '$lib/workspace/device-grants';
  import DeviceKeyAccess from './DeviceKeyAccess.svelte';

  let registry = $state<DeviceRegistry | null>(null);
  let publicKey = $state('');
  let challenge = $state<Record<string, unknown> | null>(null);
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);
  let pendingRevoke = $state('');
  let enrolOpen = $state(false);
  /** The key connected in this visit; its access block leads with the next step. */
  let freshKey = $state('');
  let projects = $state<ProjectChoice[]>([]);
  let projectsState = $state<'loading' | 'ready' | 'failed'>('loading');
  let projectsRequested = false;
  let section = $state<HTMLElement | null>(null);

  async function load() {
    registry = await loadRegistry();
    // The grant picker needs the account's projects, but only once a key can use them.
    if (!projectsRequested && registry.keys.some((k) => !k.revokedAt && k.projectAccess)) {
      projectsRequested = true;
      void loadProjectChoices()
        .then((list) => {
          projects = list;
          projectsState = 'ready';
        })
        .catch(() => {
          projectsState = 'failed';
        });
    }
  }
  /**
   * The command palette links to #signing-keys. This section mounts before the profile's
   * other sections have loaded above it, so keep it in view while the page settles, and
   * stop as soon as the person scrolls or presses a key.
   */
  function holdAnchor() {
    const target = section;
    if (!target) return () => {};
    const events = ['wheel', 'touchstart', 'pointerdown', 'keydown'] as const;
    const show = () => target.scrollIntoView({ block: 'start', behavior: 'instant' });
    const observer = new ResizeObserver(show);
    const timer = setTimeout(end, 4000);
    function end() {
      clearTimeout(timer);
      observer.disconnect();
      for (const name of events) window.removeEventListener(name, end);
    }
    observer.observe(document.body);
    for (const name of events) window.addEventListener(name, end, { passive: true });
    show();
    return end;
  }
  onMount(() => {
    const release = location.hash === '#signing-keys' ? holdAnchor() : undefined;
    void load().catch((e) => {
      error = deviceMessage(e, 'load');
    });
    return release;
  });
  async function act(action: () => Promise<void>, subject: Subject) {
    busy = true;
    error = '';
    notice = '';
    try {
      await action();
    } catch (e) {
      error = deviceMessage(e, subject);
    } finally {
      busy = false;
    }
  }
  async function requestChallenge() {
    challenge = null;
    await act(async () => {
      challenge = await createChallenge(publicKey);
    }, 'challenge');
  }
  function downloadChallenge() {
    if (!challenge) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(challenge)], { type: 'application/json' })
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = 'dynodoc-device-challenge.json';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  /** The change is saved; a failed refresh must not read as a failed change. */
  async function reload(done: string) {
    try {
      await load();
      return done;
    } catch {
      return `${done} The list could not be refreshed. Reload the page to see it.`;
    }
  }
  async function submitProof(file: File | undefined) {
    if (!file || !registry || !challenge) return;
    await act(async () => {
      if (file.size > 2048)
        throw new ProofFileError(
          'The enrollment proof must be at most 2 KB. Choose the proof JSON file.'
        );
      let proof: { challenge?: { identity?: unknown; authority?: unknown; id?: unknown } } | null;
      try {
        proof = JSON.parse(await file.text());
      } catch {
        throw new ProofFileError(
          'That file is not valid JSON. Choose the proof file created by engine-cli device prove.'
        );
      }
      if (
        proof?.challenge?.identity !== registry?.identity ||
        proof?.challenge?.authority !== registry?.authority ||
        proof?.challenge?.id !== challenge?.id
      ) {
        throw new ProofFileError(
          'This proof does not match the challenge from your current account.'
        );
      }
      const enrolled = await enrollKey(proof);
      challenge = null;
      publicKey = '';
      enrolOpen = false;
      freshKey = enrolled.id || String(proof?.challenge?.id ?? '');
      notice = await reload('Signing key connected to your account.');
    }, 'enroll');
  }
  async function revoke(id: string) {
    await act(async () => {
      await revokeKey(id);
      pendingRevoke = '';
      notice = await reload('Signing key revoked. To reconnect this device, enroll a new key.');
    }, 'revoke-key');
  }
  /** Called by a key's access block after it changed something. */
  async function changed() {
    freshKey = '';
    await load();
  }
</script>

<section
  class="repo-panel"
  id="signing-keys"
  aria-labelledby="signing-keys-title"
  bind:this={section}
>
  <div class="repo-panel-body repo-form">
    <h2 id="signing-keys-title">Signing keys</h2>
    <p class="repo-muted">
      Connect keys used to sign your local project history. Your keys are visible only to you. A new
      key can open no project until you grant it one.
    </p>
    {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
    {#if notice}<p class="repo-notice" role="status">{notice}</p>{/if}
    {#if registry}
      {#if registry.keys.length === 0}<p>No signing keys connected.</p>{/if}
      {#each registry.keys as device (device.id)}
        <div class="repo-panel-body key-row">
          <code class="key-value">{device.publicKey}</code>
          <p class="repo-small repo-muted">
            Connected {new Date(device.registeredAt).toLocaleDateString()}
            {#if device.revokedAt}
              · Revoked {new Date(device.revokedAt).toLocaleDateString()}{/if}
          </p>
          {#if !device.revokedAt}
            {#if device.projectAccess}
              <DeviceKeyAccess
                {device}
                {projects}
                {projectsState}
                grantLimit={registry.grantLimit || GRANT_LIMIT}
                fresh={freshKey === device.id}
                onChanged={changed}
              />
            {/if}
            {#if pendingRevoke === device.id}
              <p>
                Revoke this key permanently? Its past signatures remain in history. You will need a
                new key to reconnect.
              </p>
              <button disabled={busy} onclick={() => void revoke(device.id)}
                >Confirm revocation</button
              >
              <button disabled={busy} onclick={() => (pendingRevoke = '')}>Cancel</button>
            {:else}
              <button disabled={busy} onclick={() => (pendingRevoke = device.id)}>Revoke key</button
              >
            {/if}
          {/if}
        </div>
      {/each}
      {#if registry.keys.some((k) => !k.revokedAt && k.projectAccess)}
        <p class="repo-small repo-muted">
          Access tokens issued to a key last at most one hour and end on their own. Revoking a grant
          takes effect on that key's next request.
        </p>
      {/if}
      <details bind:open={enrolOpen}>
        <summary>Connect a local signing key</summary>
        <p>
          For the engine CLI preview, obtain the public key with <code
            >engine-cli device public-key signing-key.der</code
          >. Keep the private key on your device.
        </p>
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void requestChallenge();
          }}
        >
          <label
            >Public key<input
              bind:value={publicKey}
              required
              pattern={'[0-9a-f]{64}'}
              maxlength="64"
              spellcheck="false"
              autocomplete="off"
            /></label
          >
          <button disabled={busy}>{busy ? 'Working…' : 'Create enrollment challenge'}</button>
        </form>
        {#if challenge}
          <p>
            Download the challenge, sign it locally within five minutes, then select the resulting
            proof file.
          </p>
          <button disabled={busy} onclick={downloadChallenge}>Download challenge</button>
          <pre
            class="key-command">engine-cli device prove dynodoc-device-challenge.json --authority {registry.authority} --identity {registry.identity} --key signing-key.der --out device-proof.json</pre>
          <label
            >Enrollment proof JSON<input
              type="file"
              accept=".json,application/json"
              disabled={busy}
              onchange={(event) => {
                const file = event.currentTarget.files?.[0];
                event.currentTarget.value = '';
                void submitProof(file);
              }}
            /></label
          >
        {/if}
        <p class="repo-small repo-muted">
          Up to {registry.activeLimit} active keys and {registry.totalLimit} registrations per account.
          Remote project push and pull are still in development.
        </p>
      </details>
    {:else if !error}<p role="status">Loading signing keys…</p>{/if}
  </div>
</section>

<style>
  .key-value,
  .key-command {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .key-row {
    border-bottom: 1px solid var(--repo-border, #d0d7de);
  }
  #signing-keys {
    scroll-margin-top: 16px;
  }
</style>
