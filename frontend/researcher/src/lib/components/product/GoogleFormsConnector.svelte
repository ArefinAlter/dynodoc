<script lang="ts">
  import { onMount } from 'svelte';
  import type { ResearchFile } from '$lib/workspace/connectors';
  let { onImport }: { onImport: (file: File, info: ResearchFile) => Promise<void> } = $props();
  let connected = $state(false),
    form = $state(''),
    kind = $state('questionnaire'),
    busy = $state(false),
    error = $state('');
  let dialog: HTMLDialogElement;
  onMount(() => {
    void fetch('/api/google-forms')
      .then((r) => r.json())
      .then((x) => {
        connected = x.connected;
      });
    if (new URLSearchParams(location.search).has('forms')) {
      dialog.showModal();
      if (new URLSearchParams(location.search).get('forms') !== 'connected')
        error =
          'Connection did not finish. Start again and allow access to form definitions and responses.';
    }
  });
  async function importForm() {
    busy = true;
    error = '';
    try {
      const response = await fetch('/api/google-forms', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ form, kind })
      });
      const info = await response.json();
      if (!response.ok) throw new Error(info.error || 'Could not import');
      const bytes = Uint8Array.from(atob(info.content_base64), (c) => c.charCodeAt(0));
      await onImport(new File([bytes], info.filename, { type: info.content_type }), info);
      dialog.close();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not import';
    } finally {
      busy = false;
    }
  }
  async function disconnect() {
    const r = await fetch('/api/google-forms', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ action: 'disconnect' })
    });
    if (r.ok) connected = false;
  }
</script>

<button class="launch" onclick={() => dialog.showModal()}>Import from Google Forms</button>
<dialog bind:this={dialog} aria-labelledby="forms-title">
  <h2 id="forms-title">Bring your Google Forms into the research record</h2>
  <p>
    Import questions as an editable instrument, or responses as a spreadsheet. Google Forms stays
    unchanged. Review conversion notes before creating the document.
  </p>
  {#if error}<p role="alert">{error}</p>{/if}{#if !connected}<p>
      Connect the Google account that owns or can edit the form. This asks for read-only access to
      form definitions and responses for one hour.
    </p>
    <a class="primary" href="/integrations/google/connect">Connect Google Forms</a>{:else}<form
      onsubmit={(e) => {
        e.preventDefault();
        void importForm();
      }}
    >
      <label
        >Form editor URL or form ID<input
          bind:value={form}
          required
          placeholder="https://docs.google.com/forms/d/…/edit"
        /></label
      ><label
        >Bring in<select bind:value={kind}
          ><option value="questionnaire">Questionnaire</option><option value="submissions"
            >Responses</option
          ></select
        ></label
      ><button class="primary" disabled={busy}>{busy ? 'Importing…' : 'Review import'}</button>
    </form>
    <button disabled={busy} onclick={() => void disconnect()}>Disconnect this browser</button
    >{/if}<button disabled={busy} onclick={() => dialog.close()}>Close</button>
  <p class="small">
    You can also remove Dynodoc’s access in your Google Account connections settings.
  </p>
</dialog>

<style>
  .launch {
    margin-top: 8px;
    width: 100%;
    font-size: 12px;
  }
  button,
  .primary {
    display: inline-block;
    padding: 10px 12px;
    border: 1px solid #b8cbe0;
    background: #edf3fc;
    color: #244d80;
    border-radius: 5px;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
    text-decoration: none;
  }
  dialog {
    max-width: 560px;
    width: calc(100% - 36px);
    padding: 26px;
    border: 1px solid #c5d3e2;
    border-radius: 12px;
    color: #29455f;
  }
  dialog::backdrop {
    background: #132e4b66;
  }
  h2 {
    font-size: 25px;
    line-height: 1.3;
  }
  p {
    line-height: 1.7;
    font-size: 15px;
    color: #4e6680;
  }
  form,
  label {
    display: grid;
    gap: 10px;
    margin: 12px 0;
  }
  input,
  select {
    font: inherit;
    color: #29455f;
    padding: 11px;
    border: 1px solid #b8cbe0;
    border-radius: 5px;
    background: white;
  }
  .primary {
    background: #245baa;
    color: white;
  }
  dialog > button {
    margin-top: 14px;
    margin-right: 8px;
  }
  .small {
    font-size: 13px;
  }
</style>
