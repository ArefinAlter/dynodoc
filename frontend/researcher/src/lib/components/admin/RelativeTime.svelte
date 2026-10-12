<script lang="ts">
  import { onMount } from 'svelte';
  import { relativeTime } from './health';
  let { value, fallback = 'Unavailable' }: { value?: string | null; fallback?: string } = $props();
  const time = $derived(value ? Date.parse(value) : NaN);
  const valid = $derived(Number.isFinite(time));
  // Server rendering and first paint use a fixed UTC string so hydration never mismatches;
  // the relative wording and the local-time tooltip are applied once the browser is known.
  const fixed = $derived(
    valid ? new Date(time).toISOString().slice(0, 16).replace('T', ' ') + ' UTC' : ''
  );
  let now = $state<number | null>(null);
  onMount(() => {
    now = Date.now();
  });
</script>

{#if valid}<time
    datetime={new Date(time).toISOString()}
    title={now === null ? undefined : new Date(time).toLocaleString()}
    >{now === null ? fixed : relativeTime(time, now)}</time
  >{:else}{fallback}{/if}
