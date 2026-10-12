<script lang="ts">
  import { GitBranch, LockKeyhole, ChevronDown } from '@lucide/svelte';
  import type { Branch } from '$lib/workspace/project-history';
  import HashChip from './HashChip.svelte';
  import RelTime from './RelTime.svelte';
  let {
    branch,
    expanded,
    unavailable = false,
    controls,
    onToggle
  }: {
    branch: Branch;
    expanded: boolean;
    unavailable?: boolean;
    controls: string;
    onToggle: () => void;
  } = $props();
</script>

<tr class:selected={expanded}>
  <th scope="row">
    <span class="name"
      ><GitBranch size={15} aria-hidden="true" /><span class="text">{branch.name}</span></span
    >{#if branch.isDefault}<span class="badge">Default</span>{/if}{#if branch.isProtected}<span
        class="badge"
        title="Moving this branch needs the project's review approvals"
        ><LockKeyhole size={11} aria-hidden="true" />Protected</span
      >{/if}
  </th>
  <td><HashChip hash={branch.commit} label="commit hash" /></td>
  <td class="num">{branch.generation.toLocaleString()}</td>
  <td class="when">
    {#if branch.receipt?.issuedAt}<RelTime at={branch.receipt.issuedAt} />{:else}<span class="faint"
        >Not provided</span
      >{/if}
  </td>
  <td class:bad={unavailable}>{unavailable ? 'Unavailable after erasure' : 'Available'}</td>
  <td class="act">
    <button
      type="button"
      aria-expanded={expanded}
      aria-controls={controls}
      aria-label={`${expanded ? 'Hide details' : 'Details'} for ${branch.name}`}
      onclick={onToggle}
      >{expanded ? 'Hide details' : 'Details'}<ChevronDown
        size={13}
        aria-hidden="true"
        class={expanded ? 'flip' : ''}
      /></button
    >
  </td>
</tr>

<style>
  th,
  td {
    padding: 8px 12px;
    border-top: 1px solid var(--rp-border-muted);
    vertical-align: middle;
    text-align: left;
    font-weight: 400;
  }
  tr.selected th,
  tr.selected td {
    background: var(--rp-accent-bg);
  }
  .name {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 8px;
    font-weight: 600;
  }
  .name :global(svg) {
    flex: none;
    color: var(--rp-muted);
  }
  .text {
    overflow-wrap: anywhere;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-right: 6px;
    padding: 0 7px;
    border: 1px solid var(--rp-border);
    border-radius: 999px;
    color: var(--rp-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 20px;
    white-space: nowrap;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .when,
  .faint {
    color: var(--rp-muted);
    white-space: nowrap;
  }
  .bad {
    color: var(--rp-danger);
    font-weight: 600;
  }
  .act {
    text-align: right;
    white-space: nowrap;
  }
  .act button :global(svg) {
    transition: transform 0.15s ease;
  }
  .act button :global(svg.flip) {
    transform: rotate(180deg);
  }
  @media (prefers-reduced-motion: reduce) {
    .act button :global(svg) {
      transition: none;
    }
  }
</style>
