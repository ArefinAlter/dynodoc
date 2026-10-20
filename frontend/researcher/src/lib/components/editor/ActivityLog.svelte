<script lang="ts">
  import type { ActivityEvent } from '$lib/types/editor';

  interface Props {
    activity: ActivityEvent[];
    open: boolean;
  }

  let { activity, open }: Props = $props();
</script>

<aside class:open class="activity">
  <div class="header">
    <h2>Activity</h2>
    <span>Event-backed timeline target</span>
  </div>

  <div class="events">
    {#each activity as event}
      <div class="event">
        <div class="meta">{event.time} · {event.actor}</div>
        <div class="text">
          <strong>{event.actor}</strong>
          {event.action} <em>{event.target}</em>
        </div>
      </div>
    {/each}
  </div>
</aside>

<style>
  .activity {
    position: fixed;
    right: 1rem;
    top: 5.5rem;
    width: min(24rem, calc(100vw - 2rem));
    max-height: calc(100vh - 7rem);
    max-height: calc(100dvh - 7rem);
    transform: translateX(calc(100% + 2rem));
    transition: transform 0.2s ease;
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    background: var(--color-surface);
    box-shadow: var(--shadow-menu);
    overflow: hidden;
  }

  .activity.open {
    transform: translateX(0);
  }

  .header {
    padding: 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  h2 {
    margin: 0;
    font-family: var(--font-display);
    font-weight: 500;
  }

  span,
  .meta {
    color: var(--color-muted);
    font-family: var(--font-ui);
    font-size: 0.8rem;
  }

  .events {
    max-height: calc(100vh - 12rem);
    max-height: calc(100dvh - 12rem);
    overflow: auto;
  }

  .event {
    padding: 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  .text {
    margin-top: 0.35rem;
  }
</style>
