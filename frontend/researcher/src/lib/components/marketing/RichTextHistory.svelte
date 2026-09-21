<script lang="ts">
  import { onMount } from 'svelte';
  import { GitMerge, History, Bold, PencilLine, Check, ArrowRight } from '@lucide/svelte';
  let wording = $state(true);
  let formatting = $state(true);
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
</script>

<section class="version-story" aria-labelledby="version-story-title">
  <div class="story-copy">
    <p class="eyebrow">VERSION CONTROL, BUILT FOR DOCUMENTS</p>
    <h2 id="version-story-title">The words matter.<br />So does everything around them.</h2>
    <p>
      A heading is more than a line of text. An emphasis, a link, a revised question: each carries
      meaning. Dynodoc records wording and formatting as parts of the same document history.
    </p>
    <p>
      Explore a personal draft, choose what to share, and bring compatible edits together. When
      changes compete, you decide what belongs in the next version.
    </p>
    <a href="/features">Explore document version control <ArrowRight size={17} /></a>
  </div>
  <div
    class="example"
    role="group"
    aria-label="Interactive example of combining wording and formatting changes"
  >
    <div class="paper original">
      <span><History size={16} /> Earlier version</span>
      <p>The participants were interviewed in June.</p>
    </div>
    <div class="choices">
      <button
        type="button"
        disabled={!ready}
        aria-pressed={wording}
        onclick={() => (wording = !wording)}
      >
        <PencilLine size={19} /><span
          ><strong>Sam refines the wording</strong><small>participants → respondents</small></span
        ><span class="check"
          >{#if wording}<Check size={15} />{/if}</span
        >
      </button>
      <button
        type="button"
        disabled={!ready}
        aria-pressed={formatting}
        onclick={() => (formatting = !formatting)}
      >
        <Bold size={19} /><span
          ><strong>Maya adds emphasis</strong><small>Make the interview month bold</small></span
        ><span class="check"
          >{#if formatting}<Check size={15} />{/if}</span
        >
      </button>
    </div>
    <div class="merge-line">
      <GitMerge size={19} /><span>Choose the changes to bring together</span>
    </div>
    <div class="paper result" aria-live="polite" aria-atomic="true">
      <span>Combined version</span>
      <p>
        The <span class:changed={wording}>{wording ? 'respondents' : 'participants'}</span> were
        interviewed in <span class:emphasis={formatting}>June</span>.
      </p>
      <small
        >{wording && formatting
          ? 'Both changes fit. Both contributions stay visible in history.'
          : wording || formatting
            ? 'One change selected. The earlier version remains in history.'
            : 'Earlier wording and formatting preserved.'}</small
      >
    </div>
    <p class="caption">
      An illustrated example. Separate edits in a text paragraph can combine; competing edits stay
      open for review.
    </p>
  </div>
</section>

<style>
  .version-story {
    display: grid;
    grid-template-columns: 1fr 1.05fr;
    gap: clamp(2rem, 6vw, 6rem);
    max-width: 1240px;
    margin: auto;
    padding: clamp(3.5rem, 8vw, 7rem) 1.5rem;
    align-items: center;
  }
  .eyebrow {
    font-size: 0.75rem;
    font-weight: 750;
    letter-spacing: 0.09em;
    color: #315c8a;
  }
  h2 {
    font-size: clamp(2rem, 3.6vw, 3.3rem);
    line-height: 1.12;
    letter-spacing: -0.035em;
    margin: 1rem 0 1.5rem;
    color: #203546;
  }
  .story-copy > p:not(.eyebrow) {
    font-size: 1.08rem;
    line-height: 1.75;
    color: #455a69;
    margin: 0 0 1rem;
  }
  a {
    display: inline-flex;
    gap: 0.6rem;
    align-items: center;
    color: #214f80;
    font-weight: 650;
    text-underline-offset: 0.25em;
    margin-top: 0.5rem;
  }
  .example {
    border-radius: 1.25rem;
    padding: clamp(1rem, 3vw, 2rem);
    background: #eaf2fa;
    border: 1px solid #ceddeb;
  }
  .paper {
    padding: 1.3rem;
    border: 1px solid #d9e0e6;
    border-radius: 0.65rem;
    background: #fffdfa;
    box-shadow: 0 3px 0 #dfe8ef;
  }
  .paper > span {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-size: 0.8rem;
    font-weight: 650;
    color: #4f6274;
  }
  .paper p {
    line-height: 1.7;
    font-size: 1.13rem;
    margin: 1rem 0 0.25rem;
    color: #24394c;
  }
  .choices {
    display: grid;
    gap: 0.6rem;
    margin: 1.4rem 0;
  }
  button {
    cursor: pointer;
    display: flex;
    gap: 0.7rem;
    align-items: center;
    width: 100%;
    padding: 1rem;
    text-align: left;
    border: 1px solid #bccddd;
    border-radius: 0.6rem;
    background: #f6f9fc;
    color: #38566c;
    font: inherit;
    transition:
      background 0.2s,
      border-color 0.2s;
  }
  button[aria-pressed='true'] {
    background: #dceafa;
    border-color: #739bc5;
    color: #1d466e;
  }
  button > span:nth-child(2) {
    flex: 1;
  }
  strong {
    display: block;
    font-size: 0.9rem;
    font-weight: 650;
  }
  button small {
    display: block;
    margin-top: 0.3rem;
    font-size: 0.8rem;
  }
  .check {
    height: 1.15rem;
    width: 1.15rem;
    display: grid;
    place-items: center;
    border: 1px solid #7c96b0;
    border-radius: 0.25rem;
  }
  button[aria-pressed='true'] .check {
    background: #245887;
    color: white;
  }
  .merge-line {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    font-size: 0.8rem;
    color: #355d7e;
    margin-bottom: 1rem;
  }
  .result {
    border-color: #96b5d2;
  }
  .result small {
    display: block;
    font-size: 0.78rem;
    color: #4d6374;
    line-height: 1.6;
    margin-top: 1rem;
  }
  .changed {
    background: #dcecf8;
    border-bottom: 2px solid #7ca8cb;
  }
  .emphasis {
    font-weight: 750;
  }
  .caption {
    font-size: 0.76rem;
    line-height: 1.6;
    color: #4d6579;
    margin: 1rem 0 0;
  }
  button:focus-visible,
  a:focus-visible {
    outline: 3px solid #245887;
    outline-offset: 4px;
  }
  @media (max-width: 760px) {
    .version-story {
      grid-template-columns: 1fr;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    button {
      transition: none;
    }
  }
</style>
