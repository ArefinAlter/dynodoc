<script lang="ts">
  import { onMount } from 'svelte';
  import { workspaceRequest } from '$lib/workspace/api';

  interface Device {
    id: string;
    public_key: string;
    registered_at: string;
    revoked_at: string | null;
  }
  interface Registry {
    authority: string;
    identity: string;
    keys: Device[];
    active_limit: number;
    total_limit: number;
  }
  let registry = $state<Registry | null>(null);
  let publicKey = $state('');
  let challenge = $state<Record<string, unknown> | null>(null);
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);
  let pendingRevoke = $state('');

  async function load() {
    registry = await workspaceRequest<Registry>('/devices');
  }
  onMount(() => {
    void load().catch((e) => {
      error = e instanceof Error ? e.message : 'Could not load your signing keys';
    });
  });
  async function act(action: () => Promise<void>) {
    busy = true;
    error = '';
    notice = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update your signing keys';
    } finally {
      busy = false;
    }
  }
  async function requestChallenge() {
    challenge = null;
    await act(async () => {
      challenge = await workspaceRequest<Record<string, unknown>>('/devices/challenge', {
        public_key: publicKey.trim()
      });
    });
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
  async function submitProof(file: File | undefined) {
    if (!file || !registry || !challenge) return;
    await act(async () => {
      if (file.size > 2048)
        throw new Error('The enrollment proof must be at most 2 KB. Choose the proof JSON file.');
      const proof = JSON.parse(await file.text());
      if (
        proof?.challenge?.identity !== registry?.identity ||
        proof?.challenge?.authority !== registry?.authority ||
        proof?.challenge?.id !== challenge?.id
      ) {
        throw new Error('This proof does not match the challenge from your current account.');
      }
      await workspaceRequest('/devices', proof);
      challenge = null;
      publicKey = '';
      await load();
      notice = 'Signing key connected to your account.';
    });
  }
  async function revoke(id: string) {
    await act(async () => {
      await workspaceRequest(`/devices/${id}/revoke`, {});
      pendingRevoke = '';
      await load();
      notice = 'Signing key revoked. To reconnect this device, enroll a new key.';
    });
  }
</script>

<section class="repo-panel" aria-labelledby="signing-keys-title">
  <div class="repo-panel-body repo-form">
    <h2 id="signing-keys-title">Signing keys</h2>
    <p class="repo-muted">
      Connect keys used to sign your local project history. Your keys are visible only to you.
      Connecting a key does not change project access.
    </p>
    {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
    {#if notice}<p class="repo-notice" role="status">{notice}</p>{/if}
    {#if registry}
      {#if registry.keys.length === 0}<p>No signing keys connected.</p>{/if}
      {#each registry.keys as device (device.id)}
        <div class="repo-panel-body key-row">
          <code class="key-value">{device.public_key}</code>
          <p class="repo-small repo-muted">
            Connected {new Date(device.registered_at).toLocaleDateString()}
            {#if device.revoked_at}
              · Revoked {new Date(device.revoked_at).toLocaleDateString()}{/if}
          </p>
          {#if !device.revoked_at}
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
      <details>
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
          Up to {registry.active_limit} active keys and {registry.total_limit} registrations per account.
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
</style>
