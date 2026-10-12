<script lang="ts">
  import { ScrollText } from '@lucide/svelte';
  import type { HeadReceipt } from '$lib/workspace/project-history';
  import { absoluteTime } from '$lib/workspace/repo-ui';
  import RelTime from './RelTime.svelte';
  let { receipt }: { receipt: HeadReceipt | null } = $props();
</script>

<section class="card" aria-labelledby="receipt-title">
  <h4 id="receipt-title"><ScrollText size={15} aria-hidden="true" />Latest signed receipt</h4>
  {#if receipt}
    <dl>
      <dt>Branch</dt>
      <dd>{receipt.branch}</dd>
      <dt>Generation</dt>
      <dd>{receipt.generation.toLocaleString()}</dd>
      <dt>Commit</dt>
      <dd><code class="hex">{receipt.commit}</code></dd>
      <dt>Previous receipt</dt>
      <dd>
        {#if receipt.previous}<code class="hex">{receipt.previous}</code>{:else}None. This is the
          first update of the branch.{/if}
      </dd>
      <dt>Declared time</dt>
      <dd>
        {#if receipt.issuedAt}{absoluteTime(receipt.issuedAt)}
          <span class="faint">(<RelTime at={receipt.issuedAt} />)</span>{:else}Not provided{/if}
        <small>Declared by the issuer, not a trusted timestamp.</small>
      </dd>
      <dt>Authority</dt>
      <dd><code>{receipt.authority}</code></dd>
      <dt>Authority key</dt>
      <dd><code class="hex">{receipt.issuerKey}</code></dd>
      <dt>Project</dt>
      <dd><code>{receipt.project}</code></dd>
    </dl>
  {:else}
    <p class="faint">The server did not return a receipt for this branch head.</p>
  {/if}
  <p class="note">
    Shown exactly as the server returned it. This page does not check the signature, does not pin
    the authority key and cannot detect a rolled-back or forked branch. Receipts are checked by the
    command line trust journal against a key you obtained independently, with
    <code>engine-cli receipt … verify</code>.
  </p>
</section>

<style>
  .card {
    min-width: 0;
    padding: 12px 16px;
    border: 1px solid var(--rp-border);
    border-radius: 6px;
    background: var(--rp-canvas);
  }
  h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: 14px;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0 0 10px;
  }
  dt {
    color: var(--rp-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  code {
    font-family: var(--rp-mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .hex {
    display: inline-block;
    max-width: 32.5ch;
    word-break: break-all;
  }
  .faint {
    color: var(--rp-muted);
  }
  small {
    display: block;
    color: var(--rp-muted);
    font-size: 12px;
  }
  .note {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--rp-attention-border);
    border-radius: 6px;
    background: var(--rp-attention-bg);
    color: var(--rp-fg);
    font-size: 13px;
  }
  @media (max-width: 480px) {
    dl {
      grid-template-columns: minmax(0, 1fr);
      gap: 0;
    }
    dd {
      margin-bottom: 8px;
    }
  }
</style>
