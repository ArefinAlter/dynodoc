<script lang="ts">
  import { LockKeyhole, ShieldCheck } from '@lucide/svelte';
  import { MockWindow } from '$lib/components/mock';

  /** Mockup: members and roles, a project policy floor that a file rule cannot lower, audit. */
  let { label }: { label?: string } = $props();
  const members = [
    ['MC', 'maya', 'Maya Chen', 'Owner', 'Manages sharing and settings'],
    ['SO', 'sam', 'Sam Okoye', 'Editor', 'Edits and proposes changes'],
    ['PN', 'priya', 'Priya Nair', 'Reviewer', 'Reviews and discusses']
  ] as const;
</script>

<MockWindow title="dynodoc.com/projects/acme/board-pack/settings" {label}>
  <div class="rp">
    <div class="head">
      <b>acme / board-pack</b>
      <span class="mk-badge"><LockKeyhole size={11} /> Invite-only</span>
    </div>

    <h4>People</h4>
    <ul class="mk-list who">
      {#each members as [ini, av, name, role, what] (name)}
        <li>
          <span class="mk-av is-{av}">{ini}</span>
          <span class="nm"><b>{name}</b><span class="mk-muted"> {what}</span></span>
          <span class="mk-pill">{role}</span>
        </li>
      {/each}
    </ul>

    <h4>Approvals needed on main</h4>
    <dl class="rules">
      <div>
        <dt>Project rule</dt>
        <dd>2 approvals <span class="mk-badge is-done">floor</span></dd>
      </div>
      <div>
        <dt>Board update.docx</dt>
        <dd>1 approval <span class="mk-muted">is below the floor</span></dd>
      </div>
      <div>
        <dt>In effect</dt>
        <dd><b>2 approvals</b> <span class="mk-badge is-success">enforced</span></dd>
      </div>
    </dl>

    <div class="audit">
      <ShieldCheck size={14} />
      <span
        >Audit entry: Maya Chen set a project rule. Before and after values are committed, not kept
        readable after erasure.</span
      >
    </div>
  </div>
</MockWindow>

<style>
  .rp {
    display: grid;
    gap: 12px;
    padding: 14px 16px 16px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 14px;
  }
  h4 {
    margin: 4px 0 0;
    color: var(--mock-muted);
    font: 600 11px/1.4 var(--mock-font);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .who {
    display: grid;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
  }
  .who li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
  }
  .who li + li {
    border-top: 1px solid var(--mock-border);
  }
  .nm {
    min-width: 0;
    flex: 1;
    font-size: 13px;
  }
  .nm .mk-muted {
    display: block;
    font-size: 12px;
  }
  .rules {
    display: grid;
    margin: 0;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
  }
  .rules > div {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px 12px;
    padding: 8px 10px;
  }
  .rules > div + div {
    border-top: 1px solid var(--mock-border);
  }
  dt {
    color: var(--mock-muted);
    font-size: 12px;
  }
  dd {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 13px;
  }
  .audit {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: 8px;
    padding: 10px;
    border-radius: 6px;
    background: var(--mock-subtle);
    color: var(--mock-muted);
    font-size: 12px;
  }
  .audit :global(svg) {
    margin-top: 2px;
    color: var(--mock-success);
  }
</style>
