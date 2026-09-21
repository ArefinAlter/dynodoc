<script lang="ts">
  import { FileText, GitBranch, MessageSquare, ShieldCheck, ArrowRight } from '@lucide/svelte';
  let selected = $state(0);
  const steps = [
    {
      title: 'Capture the starting point',
      label: 'Bring it in',
      icon: FileText,
      text: 'Import the document and its source file. Keep the original beside an editable copy, with conversion notes you can review.',
      example: 'Project proposal · Original document',
      tag: 'Source retained'
    },
    {
      title: 'Make room for another approach',
      label: 'Explore a draft',
      icon: GitBranch,
      text: 'Try revised wording in a personal draft. Your team can continue working while you prepare a proposal.',
      example: '“Launch the first phase in September.”',
      tag: 'Personal draft'
    },
    {
      title: 'Review the change that matters',
      label: 'Review together',
      icon: MessageSquare,
      text: 'Compare a paragraph, table or cell. Keep the conversation close to the edit and resolve competing changes before sharing.',
      example: 'Delivery date changed from August to September.',
      tag: 'Decision recorded'
    },
    {
      title: 'Know which version went forward',
      label: 'Share a milestone',
      icon: ShieldCheck,
      text: 'Name the approved version, export it for delivery, or publish a read-only copy with a link you can revoke.',
      example: 'Approved for release · September revision',
      tag: 'Saved version'
    }
  ];
  const step = $derived(steps[selected]);
</script>

<div class="journey">
  <div class="path" aria-label="Explore a document revision">
    {#each steps as item, i}<button
        class:active={selected === i}
        aria-pressed={selected === i}
        onclick={() => (selected = i)}><item.icon size={23} /><span>{item.label}</span></button
      >{#if i < steps.length - 1}<ArrowRight class="connector" size={20} />{/if}{/each}
  </div>
  <div class="explanation">
    <div>
      <h3>{step.title}</h3>
      <p>{step.text}</p>
    </div>
    <div class="example">
      <span class="status"><i></i>{step.tag}</span><FileText size={25} />
      <p>{step.example}</p>
      <div class="lines" aria-hidden="true"><i></i><i></i><i></i></div>
    </div>
  </div>
  <p class="caption">
    Interactive example. Select a step to see how a revision moves through the workspace.
  </p>
</div>

<style>
  .journey {
    background: #edf4fd;
    border: 1px solid #c5d8ef;
    border-radius: 14px;
    padding: 32px;
    color: #284963;
  }
  .path {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .path button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 72px;
    border: 1px solid #c8d8eb;
    border-radius: 8px;
    background: #fafcff;
    color: #325575;
    font: 15px var(--font-ui);
    padding: 12px;
    cursor: pointer;
  }
  .path button.active {
    background: #215cad;
    color: white;
    border-color: #215cad;
    box-shadow: 0 4px 0 #b5cdeb;
  }
  .path :global(.connector) {
    color: #7c9abd;
    flex-shrink: 0;
  }
  .explanation {
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    gap: 50px;
    align-items: center;
    margin: 36px 10px 20px;
  }
  h3 {
    font-size: 29px;
    line-height: 1.25;
    margin: 0 0 17px;
    letter-spacing: -0.6px;
  }
  .explanation p {
    font-size: 17px;
    line-height: 1.85;
    color: #3b5b77;
    margin: 0;
  }
  .example {
    background: #fffefb;
    border: 1px solid #c0d4e9;
    padding: 23px;
    border-radius: 7px;
    min-height: 190px;
    box-shadow: 7px 7px 0 #d8e7f8;
  }
  .example > p {
    font-size: 16px;
    line-height: 1.5;
    margin: 12px 0;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: #266442;
    margin-bottom: 15px;
  }
  .status i {
    width: 6px;
    height: 6px;
    background: #20834e;
    border-radius: 50%;
  }
  .lines i {
    display: block;
    height: 5px;
    background: #dce6ed;
    border-radius: 3px;
    margin-top: 8px;
  }
  .lines i:last-child {
    width: 65%;
  }
  .caption {
    font-size: 13px;
    color: #4e6d88;
    margin: 23px 10px 0;
  }
  @media (max-width: 720px) {
    .journey {
      padding: 19px;
    }
    .path {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 9px;
    }
    .path :global(.connector) {
      display: none;
    }
    .path button {
      font-size: 13px;
      min-height: 68px;
    }
    .explanation {
      grid-template-columns: 1fr;
      gap: 23px;
      margin: 28px 0 15px;
    }
    h3 {
      font-size: 25px;
    }
    .explanation p {
      font-size: 16px;
    }
  }
</style>
