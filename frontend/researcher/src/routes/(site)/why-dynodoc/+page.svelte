<script lang="ts">
  import Brachio from '$lib/art/Brachio.svelte';
  import Dino from '$lib/art/Dino.svelte';
  import Fossil from '$lib/art/Fossil.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { Btn, DiffCard, Faq, Section, scrollable } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
  import { scriptTag, type FaqEntry } from '$lib/seo';

  const DESIGN = `${GITHUB_URL}/blob/main/docs`;

  const description =
    'A document shows what it says now, not who changed it, who agreed or what it said last week. Why a verifiable history needs stable identity, an append-only log, deterministic merge and checkable evidence.';

  const copies = [
    'Board update.docx',
    'Board update (1).docx',
    'Board update_final.docx',
    'Board update_final_v2.docx',
    'Board update_final_v2_MC edits.docx',
    'Board update_final_v7 (USE THIS ONE).docx'
  ];

  const problems = [
    [
      'Saving overwrites the earlier version.',
      'Each save replaces the last. A budget or a contract keeps its conclusion without the reasoning, the discarded alternative or the person who objected.'
    ],
    [
      'Author names are editable metadata.',
      'Track changes names are metadata stored in the file, and whoever holds the file can edit them. A name in a margin is not evidence of who made a change.'
    ],
    [
      'Approvals are recorded somewhere else.',
      'Who agreed is in an email thread, a chat message or someone’s memory. The file does not know it was approved, so it cannot show it.'
    ]
  ] as const;

  const questions = [
    [
      'What exactly changed?',
      'Typed events name the node, the field and the before and after values, down to one paragraph, cell or shape.',
      'live'
    ],
    [
      'Who did it?',
      'The authenticated account that made the change, not a name typed into the file. Project commits are also signed with a device key.',
      'both'
    ],
    [
      'Who agreed, and under which rule?',
      'Change requests record each approval, the rule that required it and the merge that applied it.',
      'live'
    ],
    [
      'What did it say last week, and can I get it back?',
      'Every retained revision can be read. Recovery restores selected parts as a new reviewed change, so newer work survives.',
      'live'
    ],
    [
      'Has the record itself been altered?',
      'A hash chain and verified snapshots expose edits to old events. Signed receipts and pinned heads extend the check to branches.',
      'both'
    ],
    [
      'Can I take it with me?',
      'Export supported files and retained originals. In source, a full project repository lives on your own device.',
      'both'
    ]
  ] as const;

  const limits = [
    [
      'A chain shows consistency, not authorship.',
      'A hash chain proves a record has not been edited in place. It does not prove a person typed the words, or that a person rather than a tool did.'
    ],
    [
      'Add-ins observe; they do not record every keystroke.',
      'The editor add-ins compare snapshots while their pane is open. They capture observed changes, not who or what composed a sentence, and not edits made while the pane is closed.'
    ],
    [
      'Whoever runs the database can rewrite the whole chain.',
      'Only an anchor kept somewhere else would reveal that. Independent witnesses are on the roadmap and are not built.'
    ],
    [
      'The server holds your content.',
      'Reconstructing and reviewing versions needs the content stored with its history. Storing changes is not storing nothing. A client-encrypted mode is not built.'
    ]
  ] as const;

  const faq: FaqEntry[] = [
    {
      question: 'Is this a blockchain?',
      answer:
        'No. It is an append-only log in which each event includes a hash of the one before it, the same idea Git uses for commits. There is no token, mining or distributed consensus.'
    },
    {
      question: 'Does it replace Track changes?',
      answer:
        'It complements it. Track changes lives inside one file. Dynodoc records changes across files and adds review rules, recovery and a verifiable history. It does not offer live co-authoring.'
    },
    {
      question: 'Can it prove who typed a sentence?',
      answer:
        'No. It records the authenticated account that submitted a change and, in the project repository, the device key that signed a commit. The add-ins observe changes while their pane is open. None of that proves who or what composed the words.'
    },
    {
      question: 'Where does my content live?',
      answer:
        'On the Dynodoc server. Reconstructing and reviewing versions requires the content to be stored there with its history. Access is rechecked on every read, and audited erasure removes content. A client-encrypted mode is not built.'
    },
    {
      question: 'Do I need to know Git?',
      answer:
        'No. You save a version, create a draft, review changes and merge them. The ideas come from version control; the vocabulary does not.'
    }
  ];
  const faqJsonLd = scriptTag({
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: faq.map((entry) => ({
      '@type': 'Question',
      name: entry.question,
      acceptedAnswer: { '@type': 'Answer', text: entry.answer }
    }))
  });
</script>

<svelte:head>
  <title>Why provenance matters · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content="Why provenance matters · Dynodoc" />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content="Why provenance matters · Dynodoc" />
  <meta name="twitter:description" content={description} />
  <!-- Static first-party FAQ markup built from the visible answers; no visitor values. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html faqJsonLd}
</svelte:head>

<main id="main" use:scrollable>
  <InfoHero
    title="Why document provenance matters"
    lede="A file shows what it says now. It does not record who changed it, who agreed, or what it said last week. Dynodoc keeps that record next to the file."
  >
    {#snippet actions()}
      <Btn href="/features" arrow>Explore the features</Btn>
      <Btn href="#principles" variant="secondary">Read the design principles</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="fossil" art={Fossil} />
    {/snippet}
  </InfoHero>

  <Section
    id="problem"
    title="How document history gets lost"
    lede="Most documents keep only their latest state, and teams make copies to keep the rest."
    layout="split"
  >
    <div class="problem">
      <div class="stack" data-reveal>
        <ol class="copies" aria-label="An example folder of copies of one document">
          {#each copies as name, index (name)}
            <li style="--i: {index}"><span class="mono">{name}</span></li>
          {/each}
        </ol>
        <p class="small">
          Six copies of one document. None of them records which edit came from whom, or why.
        </p>
      </div>
      <ol class="observations">
        {#each problems as [title, body] (title)}
          <li data-reveal>
            <h3 class="h-3">{title}</h3>
            <p>{body}</p>
          </li>
        {/each}
      </ol>
    </div>
  </Section>

  <Section
    id="needs"
    title="What a document history should answer"
    lede="Each answer says how Dynodoc handles the question and whether that runs in the hosted workspace today."
    layout="split"
    tone="muted"
  >
    <ol class="questions">
      {#each questions as [question, answer, status] (question)}
        <li class="card card--raised" data-reveal>
          <h3 class="h-3">{question}</h3>
          <p>{answer}</p>
          <p class="tags">
            {#if status === 'live' || status === 'both'}<span class="badge badge--accent">Live</span
              >{/if}
            {#if status === 'both'}<span class="badge">Partly in source</span>{/if}
          </p>
        </li>
      {/each}
    </ol>
  </Section>

  <Section
    id="principles"
    title="Design principles"
    lede="Each comes from the versioning design volumes in the engine repository."
    layout="split"
  >
    <div class="principles">
      <article class="principle" data-reveal>
        <div class="p-text">
          <h3 class="h-2">Stable identity</h3>
          <p>
            Every paragraph, cell and shape gets an ID once, and it never changes or comes back
            under another meaning. Rewording or moving something edits the thing; it does not delete
            one thing and create another. The design volume treats the identity rules as more
            important than the choice of architecture.
          </p>
          <p class="ref small">Design volume 1, section 4.2 (stable identity).</p>
        </div>
        <DiffCard
          typing
          path="Board update.docx · node 01K7R2M4XQ"
          meta="Maya Chen"
          lines={[
            { kind: 'hunk', text: '@@ paragraph · Q3 summary @@' },
            { kind: 'del', text: 'text: Q3 budget draft' },
            { kind: 'add', text: 'text: Q3 budget, approved' },
            { kind: 'ctx', text: 'id:   01K7R2M4XQ (unchanged)' }
          ]}
        />
      </article>

      <article class="principle principle--flip" data-reveal>
        <div class="p-text">
          <h3 class="h-2">An append-only event log</h3>
          <p>
            Current state is computed from recorded events. Snapshots are shortcuts that are checked
            against the log, never the authority. A correction is a new event, not an edit to an old
            one, so earlier events are not rewritten in place.
          </p>
          <p class="ref small">
            Design volume 1, section 5.2 (event log) and section 2.2 (content addressing).
          </p>
        </div>
        <div class="log">
          <div><span>#41</span><i>node.created</i><code>a91f03c7</code></div>
          <div><span>#42</span><i>field.edited</i><code>5be2d814</code></div>
          <div><span>#43</span><i>suggestion.accepted</i><code>c04a77f2</code></div>
          <p class="mono">each event hashes the one before it</p>
        </div>
      </article>

      <article class="principle" data-reveal>
        <div class="p-text">
          <h3 class="h-2">Deterministic merge</h3>
          <p>
            Merging matches parts by ID, applies a rule for each kind of field and validates the
            outcome. The same inputs give the same result. Independent edits combine without
            conflict markers. Anything ambiguous stops for a person, who sees the whole paragraph,
            cell or slide, and the resolution is recorded.
          </p>
          <p class="ref small">
            Design volume 1, sections 1.1, 4.4 and 4.5 (typed merge and validation).
          </p>
        </div>
        <DiffCard
          typing
          path="Q3 budget.xlsx · Contractors, Q3"
          meta="Two edits, one cell"
          lines={[
            { kind: 'hunk', text: '@@ conflict needs a person @@' },
            { kind: 'del', text: 'Sam Okoye:   7,600' },
            { kind: 'add', text: 'Priya Nair:  7,900' },
            { kind: 'ctx', text: 'neither is chosen automatically' }
          ]}
        />
      </article>

      <article class="principle principle--flip" data-reveal>
        <div class="p-text">
          <h3 class="h-2">Evidence with stated limits</h3>
          <p>
            A hash chain, signed commits and receipts can be checked by anyone with access, and each
            has a limit. Integrity does not show authorship, a signature does not carry a trusted
            time, and observed editor activity is not a keystroke log. The list beside this says
            which claim each piece of evidence supports.
          </p>
          <p class="ref small">
            Design volume 1, section 7.4, and the correctness contract in the product specification.
          </p>
        </div>
        <ul class="evidence">
          <li><b>Hash chain</b><span>the record is consistent</span></li>
          <li><b>Account</b><span>who submitted it</span></li>
          <li><b>Device signature</b><span>which key signed it</span></li>
          <li><b>Receipt</b><span>what a branch pointed to</span></li>
          <li><b>Witness</b><span>when, independently</span><em>not built</em></li>
        </ul>
      </article>
    </div>

    <div class="refs" data-reveal>
      <Btn href="{DESIGN}/ARCHITECTURE.md" variant="secondary" external arrow>Architecture</Btn>
      <Btn href="{DESIGN}/VERSIONING-CONFORMANCE.md" variant="secondary" external arrow
        >Versioning implementation and theory</Btn
      >
      <Btn href="{DESIGN}/PRODUCT-SPEC.md" variant="secondary" external arrow
        >Product specification</Btn
      >
    </div>
  </Section>

  <Section id="limits" title="What the history does not prove" layout="split" tone="muted">
    <div class="limits">
      <ul class="limit-list">
        {#each limits as [title, body] (title)}
          <li class="card card--raised" data-reveal>
            <h3 class="h-3">{title}</h3>
            <p>{body}</p>
          </li>
        {/each}
      </ul>
      <div class="plate" aria-hidden="true"><Dino name="brachio" art={Brachio} /></div>
    </div>
  </Section>

  <Section id="faq" title="Common questions" layout="split">
    <div data-reveal><Faq items={faq} exclusive /></div>
  </Section>

  <Section
    tone="ink"
    align="center"
    title="Features in detail"
    lede="The features page describes each part with a mockup, a status label and its limits."
  >
    <div class="closing" data-reveal>
      <Btn href="/features" size="lg" arrow>Explore the features</Btn>
      <Btn href="/roadmap" size="lg" variant="secondary">Read the roadmap</Btn>
    </div>
  </Section>
</main>

<style>
  .problem {
    display: grid;
    gap: clamp(32px, 5vw, 64px);
  }
  .stack {
    display: grid;
    gap: 14px;
    align-content: start;
  }
  .copies {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0 0 0 4px;
    list-style: none;
  }
  .copies li {
    position: relative;
    margin-left: calc(var(--i) * 14px);
    margin-top: -1px;
    padding: 10px 14px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius-sm);
    background: var(--color-surface-raised);
    box-shadow: 2px 3px 0 var(--color-border);
    font-size: 0.875rem;
    overflow-wrap: anywhere;
  }
  .copies li:last-child {
    border-color: var(--color-ink);
    background: var(--color-surface);
  }
  .observations {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--color-border-strong);
  }
  .observations li {
    display: grid;
    gap: 8px;
    align-content: start;
    padding-block: 22px;
    border-bottom: 1px solid var(--color-border);
  }
  .observations p {
    margin: 0;
    max-width: 52ch;
    color: var(--color-ink-soft);
  }

  .questions {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .questions li {
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 10px;
    align-content: start;
  }
  .questions p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .questions .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }

  .principles {
    display: grid;
    gap: 0;
  }
  .principle {
    display: grid;
    gap: clamp(24px, 4vw, 56px);
    align-items: center;
    padding-block: clamp(32px, 5vw, 56px);
    border-top: 1px solid var(--color-border);
  }
  .principle:first-child {
    border-top: 0;
    padding-top: 0;
  }
  .p-text {
    display: grid;
    gap: 14px;
    justify-items: start;
    min-width: 0;
  }
  .p-text p {
    margin: 0;
    max-width: 56ch;
    color: var(--color-ink-soft);
    font-size: 1.0625rem;
  }
  .p-text .ref {
    font-size: 0.8125rem;
  }
  .refs {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: clamp(28px, 4vw, 48px);
  }

  .log,
  .evidence {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
    font-family: var(--site-font-mono);
    font-size: 0.8125rem;
  }
  .log div {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 14px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
  }
  .log span {
    color: var(--color-muted);
  }
  .log i {
    flex: 1;
    min-width: 8ch;
    font-style: normal;
    color: var(--color-ink);
  }
  .log code {
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--color-surface-muted);
    color: var(--color-ink-soft);
  }
  .log p {
    margin: 0;
    padding: 12px 16px;
    color: var(--color-muted);
    font-size: 0.75rem;
  }
  .evidence li {
    display: grid;
    gap: 2px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
  }
  .evidence li:last-child {
    border-bottom: 0;
  }
  .evidence b {
    font-weight: 600;
    color: var(--color-ink);
  }
  .evidence span {
    color: var(--color-ink-soft);
  }
  .evidence em {
    font-style: normal;
    color: var(--color-warn-ink);
  }

  .limits {
    display: grid;
    gap: clamp(28px, 4vw, 56px);
    align-items: end;
  }
  .limit-list {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .limit-list li {
    display: grid;
    gap: 10px;
    align-content: start;
  }
  .limit-list p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .plate {
    width: min(100%, 400px);
    justify-self: center;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }

  .closing {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 12px;
  }

  @media (min-width: 900px) {
    .problem {
      grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
      align-items: start;
    }
    .questions {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .principle {
      grid-template-columns: minmax(0, 6fr) minmax(0, 5fr);
    }
    .principle--flip .p-text {
      order: 2;
    }
    .limits {
      grid-template-columns: minmax(0, 7fr) minmax(0, 4fr);
    }
    .limit-list {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
