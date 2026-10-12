<script lang="ts">
  import { avatarHue } from '$lib/workspace/repo-ui';
  import { initials } from '$lib/workspace/profile';
  let { name, size = 24 }: { name: string; size?: number } = $props();
</script>

<span class="avatar" aria-hidden="true" style:--h={avatarHue(name)} style:--size="{size}px"
  >{initials(name)}</span
>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    font-size: calc(var(--size) * 0.4);
    font-weight: 600;
    line-height: 1;
    letter-spacing: 0;
    background: hsl(var(--h) 55% 90%);
    color: hsl(var(--h) 60% 24%);
    box-shadow: inset 0 0 0 1px hsl(var(--h) 40% 75%);
  }
  :global(:root[data-theme='dark']) .avatar {
    background: hsl(var(--h) 35% 22%);
    color: hsl(var(--h) 80% 85%);
    box-shadow: inset 0 0 0 1px hsl(var(--h) 30% 34%);
  }
</style>
