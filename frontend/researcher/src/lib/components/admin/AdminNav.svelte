<script lang="ts" module>
  import type { Activity } from '@lucide/svelte';
  export type NavItem = {
    label: string;
    href: string;
    /** Section element id this link scrolls to. */
    section: string;
    icon: typeof Activity;
    count?: number | null;
    /** Record kinds this link stands for; the first is selected when the link is used. */
    kinds?: string[];
    tone?: 'danger';
  };
  export type NavGroup = { items: NavItem[] };
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { ChevronDown } from '@lucide/svelte';

  let {
    groups,
    activeKind = '',
    onpick
  }: { groups: NavGroup[]; activeKind?: string; onpick?: (kind: string) => void } = $props();

  const items = $derived(groups.flatMap((g) => g.items));
  let current = $state('overview');
  let enhanced = $state(false);
  let open = $state(false);
  const isActive = (item: NavItem) =>
    item.kinds
      ? current === item.section && item.kinds.includes(activeKind)
      : current === item.section;
  // Records links share one section: the one for the browsed kind is highlighted.
  const active = $derived(items.find(isActive) ?? items.find((i) => i.section === current));

  onMount(() => {
    enhanced = true;
    const targets = [...new Set(items.map((i) => i.section))]
      .map((id) => document.getElementById(id))
      .filter((el): el is HTMLElement => !!el);
    if (!('IntersectionObserver' in window) || !targets.length) return;
    const inView = new Set<string>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting) inView.add(entry.target.id);
          else inView.delete(entry.target.id);
        // The first section (in page order) inside the reading band is the current one.
        const first = targets.find((t) => inView.has(t.id));
        if (first) current = first.id;
      },
      { rootMargin: '-80px 0px -60% 0px' }
    );
    for (const target of targets) observer.observe(target);
    return () => observer.disconnect();
  });
  function pick(item: NavItem) {
    open = false;
    current = item.section;
    if (item.kinds) onpick?.(item.kinds[0]);
  }
</script>

<div class="side" class:enhanced>
  <button
    type="button"
    class="toggle btn"
    aria-expanded={open}
    aria-controls="admin-sections"
    onclick={() => (open = !open)}
  >
    <span><span class="sr-only">Sections: </span>{active?.label ?? 'Sections'}</span>
    <ChevronDown size={16} aria-hidden="true" />
  </button>
  <nav id="admin-sections" aria-label="Administration sections" class:open>
    {#each groups as group}
      <ul>
        {#each group.items as item}
          {@const Icon = item.icon}
          <li>
            <a
              href={item.href}
              class:active={isActive(item)}
              class:danger={item.tone === 'danger'}
              aria-current={isActive(item) ? 'location' : undefined}
              onclick={() => pick(item)}
            >
              <Icon size={16} aria-hidden="true" /><span class="name">{item.label}</span
              >{#if item.count != null}<span class="counter"
                  >{Number(item.count).toLocaleString('en')}</span
                >{/if}
            </a>
          </li>
        {/each}
      </ul>
    {/each}
  </nav>
</div>

<style>
  .side {
    position: sticky;
    top: 16px;
    align-self: start;
  }
  .side .toggle {
    display: none;
    width: 100%;
    justify-content: space-between;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  ul + ul {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--a-border-muted);
  }
  a {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 4px 8px 4px 12px;
    border-radius: 6px;
    color: var(--a-fg);
    transition: background-color 0.12s ease;
  }
  a :global(svg) {
    flex: none;
    color: var(--a-muted);
    transition: transform 0.15s ease;
  }
  a:hover {
    background: var(--a-hover);
    text-decoration: none;
  }
  a:hover :global(svg) {
    transform: translateX(1px);
  }
  a.active {
    background: var(--a-hover);
    font-weight: 600;
  }
  a.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 3px;
    border-radius: 6px;
    background: var(--a-accent);
  }
  a.danger,
  a.danger :global(svg) {
    color: var(--a-danger);
  }
  a.danger.active::before {
    background: var(--a-danger);
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  @media (max-width: 860px) {
    .side {
      position: static;
    }
    /* Without script the full list stays visible; with it the list folds behind a button. */
    .side.enhanced .toggle {
      display: inline-flex;
    }
    .enhanced nav:not(.open) {
      display: none;
    }
    nav {
      margin-top: 8px;
      padding: 8px;
      border: 1px solid var(--a-border);
      border-radius: 6px;
      background: var(--a-subtle);
    }
  }
</style>
