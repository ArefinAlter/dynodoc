<script lang="ts">
  import { absoluteTime, relativeTime } from '$lib/workspace/repo-ui';
  let { at, prefix = '' }: { at: string | null | undefined; prefix?: string } = $props();
</script>

{#if at && relativeTime(at)}<time datetime={at} title={absoluteTime(at)}
    >{prefix}{relativeTime(at)}</time
  >{/if}
