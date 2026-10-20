<script lang="ts" module>
  export interface ToolField {
    key: string;
    label: string;
    type?: 'text' | 'number' | 'select' | 'checkbox' | 'color' | 'textarea';
    value?: string;
    options?: { value: string; label: string }[];
    required?: boolean;
    min?: number;
    max?: number;
    step?: string;
    list?: string[];
  }
  export interface ToolDialog {
    title: string;
    fields: ToolField[];
    submit?: string;
    note?: string;
    onSubmit: (values: Record<string, string>) => void;
    actions?: { label: string; run: () => void }[];
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { X } from '@lucide/svelte';
  let { config, onClose }: { config: ToolDialog; onClose: () => void } = $props();
  let dialog: HTMLDialogElement,
    values = $state<Record<string, string>>({}),
    error = $state('');
  $effect(() => {
    values = Object.fromEntries(config.fields.map((f) => [f.key, f.value ?? '']));
    error = '';
  });
  onMount(() => dialog.showModal());
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="office-tool-title">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      try {
        config.onSubmit(values);
        dialog.close();
      } catch (e) {
        error = e instanceof Error ? e.message : 'Could not apply this change.';
      }
    }}
  >
    <header>
      <h2 id="office-tool-title">{config.title}</h2>
      <button type="button" aria-label="Close dialog" onclick={() => dialog.close()}
        ><X size={18} /></button
      >
    </header>
    {#if config.note}<p>{config.note}</p>{/if}
    {#each config.fields as field}
      <label class:check={field.type === 'checkbox'}
        >{field.label}
        {#if field.type === 'select'}<select
            aria-label={field.label}
            bind:value={values[field.key]}
            required={field.required}
            >{#each field.options || [] as option}<option value={option.value}
                >{option.label}</option
              >{/each}</select
          >
        {:else if field.type === 'textarea'}<textarea
            aria-label={field.label}
            bind:value={values[field.key]}
            required={field.required}
            rows="4"
          ></textarea>
        {:else if field.type === 'checkbox'}<input
            aria-label={field.label}
            type="checkbox"
            checked={values[field.key] === 'true'}
            onchange={(e) => (values[field.key] = String(e.currentTarget.checked))}
          />
        {:else}<input
            aria-label={field.label}
            type={field.type || 'text'}
            value={values[field.key]}
            oninput={(e) => (values[field.key] = e.currentTarget.value)}
            required={field.required}
            min={field.min}
            max={field.max}
            step={field.step}
            list={field.list ? `office-tool-${field.key}` : undefined}
          />{#if field.list}<datalist id={`office-tool-${field.key}`}
              >{#each field.list as item}<option value={item}></option>{/each}</datalist
            >{/if}{/if}
      </label>
    {/each}
    {#if config.actions?.length}<div class="extras">
        {#each config.actions as action}<button
            type="button"
            onclick={() => {
              try {
                action.run();
                dialog.close();
              } catch (e) {
                error = e instanceof Error ? e.message : 'Could not apply this change.';
              }
            }}>{action.label}</button
          >{/each}
      </div>{/if}
    {#if error}<p role="alert" class="error">{error}</p>{/if}
    <footer>
      <button type="button" onclick={() => dialog.close()}>Cancel</button><button
        class="primary"
        type="submit">{config.submit || 'Apply'}</button
      >
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: 430px;
    max-width: calc(100vw - 32px);
    max-height: 85vh;
    max-height: 85dvh;
    border: 1px solid var(--color-border-strong);
    border-radius: 7px;
    padding: 20px;
    background: var(--color-surface-raised);
    color: var(--color-ink-strong);
    box-shadow: var(--shadow-dialog);
    --office-dialog-solid: #107c41;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
  }
  dialog::backdrop {
    background: var(--color-overlay);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  h2 {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
  }
  header button {
    border: 0;
    display: grid;
    place-items: center;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  label.check {
    flex-direction: row-reverse;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  input,
  textarea,
  select,
  button {
    font: inherit;
    color: inherit;
    border: 1px solid var(--color-border-strong);
    border-radius: 3px;
    padding: 7px;
    background: var(--color-surface-raised);
    min-width: 0;
  }
  input[type='checkbox'] {
    width: 16px;
    height: 16px;
    accent-color: var(--office-dialog-solid);
  }
  input[type='color'] {
    width: 60px;
    height: 32px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  button {
    cursor: pointer;
  }
  .primary {
    background: var(--office-dialog-solid);
    border-color: var(--office-dialog-solid);
    color: var(--color-on-accent);
    min-width: 80px;
  }
  p {
    margin: 0;
    line-height: 1.5;
    color: var(--color-ink-soft);
  }
  .error {
    color: var(--color-danger);
  }
  .extras {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .extras button {
    font-size: 12px;
  }
  button:focus-visible,
  input:focus-visible,
  select:focus-visible,
  textarea:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  @media screen {
    :global([data-theme='dark']) dialog {
      --office-dialog-solid: #178548;
    }
  }
</style>
