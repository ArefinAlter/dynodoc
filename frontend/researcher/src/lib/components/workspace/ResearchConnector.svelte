<script lang="ts">
  import { onMount } from 'svelte';
  import { X, Download, Link2 } from '@lucide/svelte';
  import type {
    ResearchProvider,
    ResearchFile,
    ResearchChoice,
    ResearchRequest
  } from '$lib/workspace/connectors';
  let {
    onImport,
    onClose
  }: { onImport: (file: File, info: ResearchFile) => Promise<void>; onClose: () => void } =
    $props();
  let modal: HTMLDialogElement;
  onMount(() => modal.showModal());
  let provider = $state<ResearchProvider>('kobo'),
    origin = $state('https://kf.kobotoolbox.org'),
    token = $state(''),
    username = $state(''),
    apiPath = $state('/api/');
  let projects = $state<ResearchChoice[]>([]),
    forms = $state<ResearchChoice[]>([]),
    project = $state(''),
    form = $state('');
  let kind = $state<'questionnaire' | 'submissions'>('questionnaire'),
    busy = $state(false),
    error = $state('');
  async function call(action: ResearchRequest['action']) {
    const response = await fetch('/api/research', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        provider,
        origin,
        token,
        project,
        form,
        action,
        kind,
        username,
        api_path: apiPath
      })
    });
    const data = await response.json();
    if (!response.ok)
      throw new Error(data.error || 'The research tool could not complete this request.');
    return data;
  }
  async function run(work: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await work();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Please try again.';
    } finally {
      busy = false;
    }
  }
  async function loadForms() {
    forms = (await call('forms')).items;
    form = forms[0]?.id || '';
  }
  async function connect() {
    await run(async () => {
      const list = (await call('projects')).items as ResearchChoice[];
      if (provider === 'odk') {
        projects = list;
        project = list[0]?.id || '';
        if (project) await loadForms();
      } else {
        forms = list;
        form = list[0]?.id || '';
      }
      if (!list.length) error = 'No accessible questionnaires were found for this account.';
    });
  }
  async function download() {
    await run(async () => {
      const info = (await call('import')) as ResearchFile;
      const bytes = Uint8Array.from(atob(info.content_base64), (c) => c.charCodeAt(0));
      await onImport(new File([bytes], info.filename, { type: info.content_type }), info);
      token = '';
      onClose();
    });
  }
  function reset() {
    projects = [];
    forms = [];
    project = '';
    form = '';
    error = '';
  }
</script>

<dialog
  bind:this={modal}
  aria-labelledby="connector-title"
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) {
      token = '';
      onClose();
    }
  }}
>
  <header>
    <span><Link2 size={20} /></span>
    <div>
      <h2 id="connector-title">Bring in your fieldwork</h2>
      <p>Connect your research tools.</p>
    </div>
    <button
      aria-label="Close research connection"
      disabled={busy}
      onclick={() => {
        token = '';
        onClose();
      }}><X size={20} /></button
    >
  </header>
  <p class="intro">
    Import a questionnaire or a fresh copy of responses. Changes you make here stay in Dynodoc.
  </p>
  <label
    >Research tool<select
      bind:value={provider}
      disabled={busy}
      onchange={() => {
        reset();
        token = '';
        origin =
          provider === 'kobo'
            ? 'https://kf.kobotoolbox.org'
            : provider === 'surveymonkey'
              ? 'https://api.surveymonkey.com'
              : '';
        username = '';
        kind = ['surveycto', 'surveymonkey'].includes(provider) ? 'submissions' : 'questionnaire';
      }}
      ><option value="kobo">KoboToolbox</option><option value="odk">ODK Central</option><option
        value="redcap">REDCap</option
      ><option value="surveycto">SurveyCTO</option><option value="surveymonkey">SurveyMonkey</option
      ></select
    ></label
  >
  <label
    >Server address<input
      type="url"
      bind:value={origin}
      disabled={busy}
      placeholder={provider === 'kobo'
        ? 'https://kf.kobotoolbox.org'
        : 'https://your-study.getodk.cloud'}
      oninput={reset}
    /></label
  >
  <label
    >{provider === 'kobo'
      ? 'Kobo API key'
      : provider === 'odk'
        ? 'ODK session token'
        : provider === 'surveycto'
          ? 'API user password'
          : 'API access token'}<input
      type="password"
      bind:value={token}
      autocomplete="off"
      disabled={busy}
      oninput={reset}
    /></label
  >
  {#if provider === 'redcap'}<label
      >API path<input bind:value={apiPath} placeholder="/redcap/api/" disabled={busy} /></label
    >{/if}
  {#if provider === 'surveycto'}<label
      >API username<input bind:value={username} autocomplete="off" disabled={busy} /></label
    ><label>Form ID<input bind:value={form} disabled={busy} /></label>{/if}
  <p class="hint">
    {provider === 'kobo'
      ? 'Use the API key from your Kobo account settings. Choose eu.kobotoolbox.org for the European server.'
      : provider === 'odk'
        ? 'Use a current Central Web User session token with permission to read the project. App User tokens do not provide this access.'
        : 'Use an account with API export permission. Provider subscriptions and project permissions may restrict access.'}
    Your token is used for this connection and is not saved.
  </p>
  {#if provider !== 'surveycto'}<button
      class="connect"
      disabled={busy || !origin || !token}
      onclick={connect}
      >{busy
        ? 'Connecting…'
        : forms.length
          ? 'Reload questionnaires'
          : 'Find questionnaires'}</button
    >
  {/if}{#if projects.length}<label
      >Project<select bind:value={project} disabled={busy} onchange={() => void run(loadForms)}
        >{#each projects as item}<option value={item.id}>{item.name}</option>{/each}</select
      ></label
    >{/if}
  {#if forms.length || (provider === 'surveycto' && form && username)}
    {#if forms.length}<label
        >Questionnaire<select bind:value={form} disabled={busy}
          >{#each forms as item}<option value={item.id}>{item.name}</option>{/each}</select
        ></label
      >
    {/if}<label
      >What would you like to bring in?<select bind:value={kind} disabled={busy}
        >{#if !['surveycto', 'surveymonkey'].includes(provider)}<option value="questionnaire"
            >Questionnaire for editing</option
          >{/if}<option value="submissions">Current responses as a spreadsheet</option></select
      ></label
    >
    <p class="hint">
      {kind === 'questionnaire'
        ? 'Uses the XLSForm source. Media and external datasets stay in the collection platform.'
        : 'Up to 2,999 responses and 100 columns. Repeat tables, attachments and encrypted responses need a separate export.'}
    </p>
    <button class="primary" disabled={busy || !form} onclick={download}
      ><Download size={16} />{busy ? 'Fetching your work…' : 'Preview import'}</button
    >
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</dialog>

<style>
  dialog::backdrop {
    background: var(--color-overlay);
  }
  dialog {
    border: 0;
    width: min(550px, 100%);
    max-height: 90vh;
    max-height: 90dvh;
    overflow: auto;
    padding: 28px;
    background: var(--color-surface-raised);
    border-radius: 18px;
    box-shadow: var(--shadow-dialog);
    --connector-solid: #285a43;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  header > span {
    background: var(--color-success-bg);
    color: var(--color-success);
    padding: 10px;
    border-radius: 12px;
  }
  header > button {
    margin-left: auto;
    border: 0;
    background: none;
    padding: 4px;
  }
  h2 {
    font-size: 21px;
    margin: 0 0 5px;
  }
  header p {
    font-size: 13px;
    margin: 0;
    color: var(--color-muted);
  }
  .intro {
    font-size: 14px;
    line-height: 1.6;
    margin: 22px 0;
    color: var(--color-ink-soft);
  }
  label {
    display: block;
    font-size: 13px;
    font-weight: 600;
    margin: 14px 0;
  }
  input,
  select {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 11px;
    margin-top: 7px;
    background: var(--color-surface-raised);
    font: inherit;
  }
  .hint {
    color: var(--color-ink-soft);
    line-height: 1.6;
    font-size: 13px;
  }
  button {
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.55;
    cursor: wait;
  }
  .connect,
  .primary {
    width: 100%;
    padding: 11px;
    border-radius: 8px;
    font-weight: 600;
    border: 1px solid var(--color-success-border);
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  .primary {
    display: flex;
    justify-content: center;
    gap: 8px;
    align-items: center;
    background: var(--connector-solid);
    color: var(--color-on-accent);
    margin-top: 18px;
  }
  .error {
    color: var(--color-danger);
    background: var(--color-danger-bg);
    padding: 12px;
    font-size: 13px;
    border-radius: 8px;
    line-height: 1.5;
  }
  @media screen {
    :global([data-theme='dark']) dialog {
      --connector-solid: #178548;
    }
  }
</style>
