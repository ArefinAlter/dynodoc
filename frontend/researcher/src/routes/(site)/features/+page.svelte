<script lang="ts">
  import { env } from '$env/dynamic/public';
  import { FileText, FileSpreadsheet, Presentation, FileCode2 } from '@lucide/svelte';
  import Dino from '$lib/art/Dino.svelte';
  import Stego from '$lib/art/Stego.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { AddinMock, AuditMock, ReviewMock, TimelineMock } from '$lib/components/mock';
  import { Btn, CopyCode, Faq, Section, scrollable } from '$lib/components/site';
  import { workspaceHref } from '$lib/components/site/nav';
  import { scriptTag, type FaqEntry } from '$lib/seo';
  import BranchDiagram from './BranchDiagram.svelte';
  import FeatureRow from './FeatureRow.svelte';
  import IdentityDiagram from './IdentityDiagram.svelte';
  import RolesPanel from './RolesPanel.svelte';

  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));

  const description =
    'Stable identity for every paragraph, cell and shape; reviewed change requests under project rules; private drafts; recoverable history; tamper-evident records; local-first sync; connectors for six editors.';

  const faq: FaqEntry[] = [
    {
      question: 'Do we have to switch editors?',
      answer:
        'No. Dynodoc sits beside Word, Excel, PowerPoint, Google Docs, Google Sheets and Google Slides through add-ins, or takes an uploaded file. The basic web editors are optional. The add-ins are test builds today.'
    },
    {
      question: 'Is this Git for documents?',
      answer:
        'It borrows the useful parts of Git: commits, parents, branches and review. The unit is a paragraph, cell or shape instead of a line of text, and you work through actions in a web workspace. No Git commands are needed.'
    },
    {
      question: 'What runs today, and what exists only in source?',
      answer:
        'The hosted pilot runs application 49ada1e on database schema 38, as of 12 October 2026. Shared project history is deployed with its writers off and no receipt-signing key installed, so publishing to shared branches is refused. The local repository and the sync client are implemented and tested in source. The roadmap lists each item with its status.'
    },
    {
      question: 'How large can a file be?',
      answer:
        'During testing, original uploads are limited to 3,000,000 bytes. Acceptance with 100, 250 and 500 MB files is future work, and no capacity figure is claimed.'
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

  const syncSteps = [
    [
      'plan',
      'engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main plan COMMIT --out push-plan.json'
    ],
    [
      'push',
      'engine-cli sync work.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal pins.sqlite --branch main push push-plan.json'
    ],
    [
      'pull',
      'engine-cli sync other.sqlite PROJECT --remote remote.json --token-file account-token.txt --journal other-pins.sqlite --branch main pull local-main'
    ]
  ] as const;

  const hosts = [
    ['word', 'Word'],
    ['excel', 'Excel'],
    ['powerpoint', 'PowerPoint'],
    ['docs', 'Google Docs'],
    ['sheets', 'Google Sheets'],
    ['slides', 'Google Slides']
  ] as const;

  const formats = [
    ['Word', 'DOCX · DOC', 'word', FileText],
    ['Excel', 'XLSX · CSV', 'excel', FileSpreadsheet],
    ['PowerPoint', 'PPTX', 'powerpoint', Presentation],
    ['Text and code', 'TXT · MD · TEX', 'plain', FileCode2]
  ] as const;

  const compare = [
    {
      topic: 'Who changed what',
      track: 'Yes. The author name is stored in the file and anyone with the file can change it.',
      history: 'Per saved version, by the hosting service’s account. Coarse.',
      dyno: 'Every event carries an authenticated account. Commits in the project repository are signed.'
    },
    {
      topic: 'Review before it lands',
      track: 'Accept or reject inside the file. No required approvals and no record of who agreed.',
      history: 'No.',
      dyno: 'Change requests with required approvals, policy floors and a recorded merge.'
    },
    {
      topic: 'Try something privately',
      track: 'No.',
      history: 'No. Copy the file.',
      dyno: 'Private draft per file. Project-wide branches are listed read-only under Branches (in source).'
    },
    {
      topic: 'Recover part of an old state',
      track: 'Reject your own pending changes.',
      history: 'Restore or copy a whole version.',
      dyno: 'Restore selected blocks, revert or cherry-pick a range. Each is a reviewed new change.'
    },
    {
      topic: 'Detect a rewritten record',
      track: 'No.',
      history: 'Controlled by the host. You cannot verify it yourself.',
      dyno: 'Hash-chained events anyone with access can verify, with the limits stated above.'
    },
    {
      topic: 'History across files and editors',
      track: 'One file, one editor.',
      history: 'One file, one service.',
      dyno: 'One project history across files, through six connectors (test builds).'
    },
    {
      topic: 'Live co-authoring',
      track: 'Yes, in Word and Docs.',
      history: 'Handled by the host editor, not by version history.',
      dyno: 'No. Changes are deliberate and reviewed. There are no live cursors.'
    },
    {
      topic: 'Formatting changes',
      track: 'Tracks many formatting edits.',
      history: 'Whole-file fidelity.',
      dyno: 'Add-ins capture text and cell values only today. Web editors keep supported formatting.'
    },
    {
      topic: 'Setup',
      track: 'Built in.',
      history: 'Built in.',
      dyno: 'An account, a project and an add-in or an upload.'
    }
  ];
</script>

<svelte:head>
  <title>Features · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content="Features · Dynodoc" />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content="Features · Dynodoc" />
  <meta name="twitter:description" content={description} />
  <!-- Static first-party FAQ markup built from the visible answers; no visitor values. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html faqJsonLd}
</svelte:head>

<main id="main" use:scrollable>
  <InfoHero
    eyebrow="FEATURES"
    title="Features"
    lede="Every paragraph, cell and slide shape keeps an identity. Every change is recorded, reviewed and recoverable. This page says what exists, where it runs and where the edges are."
  >
    {#snippet actions()}
      <Btn href={workspace} arrow>Open Projects</Btn>
      <Btn href="#compare" variant="secondary">Compare with Track changes</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="stego" art={Stego} />
    {/snippet}
  </InfoHero>

  <section class="container legend" aria-label="How to read the status labels">
    <p class="eyebrow">Status labels</p>
    <ul>
      <li>
        <span class="lg lg--live">Live</span> In the hosted pilot today (application 49ada1e, database
        schema 38).
      </li>
      <li>
        <span class="lg lg--source">In source</span> Implemented and tested in source, not yet deployed.
      </li>
      <li>
        <span class="lg lg--test">Test build</span> A downloadable test package, not a marketplace listing.
      </li>
    </ul>
  </section>

  <Section
    id="record"
    eyebrow="THE RECORD"
    title="A history that knows what each part is."
    lede="Two ideas hold everything else up: every part of a document keeps a permanent identity, and every change is appended to a log that can be checked."
    layout="split"
  >
    <FeatureRow
      id="identity"
      n="01"
      title="Every paragraph, cell and shape keeps its identity."
      chips={[{ kind: 'live', label: 'Live' }]}
      body="Dynodoc reads a file as logical nodes: paragraphs, table cells, worksheet rows and cells, slides and shapes. Each gets a permanent ID the first time it is seen. Moving, rewording or reformatting changes the node’s content, never who it is, so history follows the thing, not its position."
      points={[
        'Text inside a node is kept as formatting runs. Bolding a word splits runs and joins them again; the node and its history stay put.',
        'Equal-looking text in unrelated places never becomes one node.',
        'A matching file name or similar content can suggest a binding. It never grants access or authorship.'
      ]}
      boundary="Stable IDs for structures nested inside tables, and the rules for splitting, joining and moving them, are still in the engine plan (part 4)."
      caption="Illustrative mockup with synthetic names. Run boundaries are drawn dashed."
    >
      {#snippet visual()}
        <IdentityDiagram
          label="A paragraph keeps its node ID while its formatting runs split and join"
        />
      {/snippet}
    </FeatureRow>

    <FeatureRow
      id="history"
      n="02"
      flip
      title="A history you can check, not just trust."
      chips={[
        { kind: 'live', label: 'Live', detail: 'hash-chained events' },
        { kind: 'source', label: 'In source', detail: 'signed commits and receipts' }
      ]}
      body="Each content change is a typed event linked to the one before it by a SHA-256 hash chain, so editing an old event breaks every link after it. Current state is folded from the log; snapshots are derived and verified, never authoritative. In the project repository, commits are signed with device keys and branch updates carry signed receipts."
      points={[
        'Anyone with access to a document can verify the chain’s internal consistency.',
        'Device keys are bound to an account and can be revoked permanently. Earlier signatures stay verifiable.',
        'A client can pin a branch head in its own journal and detect rollback, forks and missing updates.'
      ]}
      boundary="A hash chain shows the record is consistent, not who typed a sentence. Catching a complete rewrite by whoever runs the database needs an independently kept anchor; external witnesses are not built yet."
      caption="Illustrative mockup. Event names and values are synthetic."
    >
      {#snippet visual()}
        <AuditMock label="An audit trail of five events, each verified in the chain" />
      {/snippet}
    </FeatureRow>
  </Section>

  <Section
    id="review"
    eyebrow="REVIEW"
    title="Nothing becomes the team’s version by accident."
    lede="Edits travel as proposals. Rules decide who must agree, and every outcome is recorded with the account that did it."
    layout="split"
    tone="muted"
  >
    <FeatureRow
      id="change-requests"
      n="03"
      title="Change requests, review and merge under rules."
      chips={[{ kind: 'live', label: 'Live' }]}
      body="A change request carries selected block changes against a base. Reviewers see what changed in content, formatting and structure, approve or ask for changes, and a merge records its parents, resolutions and result without discarding either contributor."
      points={[
        'Independent edits combine after validation. Overlapping edits and shared constraints stop for an explicit resolution instead of a silent guess.',
        'Project rules are a floor: a file rule can raise the approvals required, never lower them. Owning a file does not bypass review.',
        'Approvals, merges and overrides are written to the audit trail with the acting account.'
      ]}
      boundary="Review is deliberate. There is no live co-editing and there are no collaborator cursors."
      caption="Illustrative mockup with synthetic names."
    >
      {#snippet visual()}
        <ReviewMock label="A change request with two approvals, ready to merge" />
      {/snippet}
    </FeatureRow>

    <FeatureRow
      id="drafts"
      n="04"
      flip
      title="Private drafts and branches."
      chips={[
        { kind: 'live', label: 'Live', detail: 'per-file drafts' },
        { kind: 'source', label: 'In source', detail: 'project-wide branches' }
      ]}
      body="Try different wording in a private draft without touching the team’s version. Compare it with main, submit it for review, or throw it away. In the project repository, branches and commits span several files at once, with the same review rules at a protected main."
      points={[
        'Hosted Projects: a private draft per file, with a main-and-draft comparison and pull requests.',
        'Local repository: signed multi-file commits, parents, branches moved by compare-and-swap, and every best merge base.',
        'Shared history (schema 36): candidates are published explicitly, reviews bind the exact candidate and base, and main is protected.'
      ]}
      boundary="The Projects interface lists project-wide branches and reviews read-only. Creating and merging them there, and adoption by the add-ins, are in progress."
      caption="Diagram: a private draft branches from main, main moves on, and the merge commit has two parents."
    >
      {#snippet visual()}
        <BranchDiagram
          label="A draft branches from main and later merges back as a commit with two parents"
        />
      {/snippet}
    </FeatureRow>
  </Section>

  <Section
    id="recover"
    eyebrow="RECOVER AND SYNC"
    title="Go back without losing what came after."
    lede="Old states are readable, recoverable and portable. Recovery adds new work and names its source; it never rewinds the log."
    layout="split"
  >
    <FeatureRow
      id="recovery"
      n="05"
      title="Named versions, restore, revert and cherry-pick."
      chips={[
        { kind: 'live', label: 'Live', detail: 'per-file recovery' },
        { kind: 'source', label: 'In source', detail: 'commit-addressed recovery' }
      ]}
      body="Any retained revision can be inspected, not only the ones you named. Download the state before an edit, restore selected blocks, revert the net effect of a range of changes, or replay a chosen change onto another base. Each is a new, reviewable change with its source recorded."
      points={[
        'Name a stage and publish a frozen read-only copy; revoke the link later.',
        'Review shows deletions and structural dependencies, not only the recovered text.',
        'In the project repository, commit-addressed revert and cherry-pick are signed, and are tested fifty commits back with later work preserved.'
      ]}
      boundary="Native downloads of old revisions use the supported exporters, so complex formatting can still be lost. See the compatibility guide."
      caption="Illustrative mockup. Revision numbers and hashes are synthetic."
    >
      {#snippet visual()}
        <TimelineMock
          label="A revision timeline where each revision is signed and linked to the one before"
        />
      {/snippet}
    </FeatureRow>

    <FeatureRow
      id="local-first"
      n="06"
      flip
      title="Local-first project history and selected-history sync."
      chips={[{ kind: 'source', label: 'In source', detail: 'needs schema 36' }]}
      body="The project repository lives on your device in SQLite, so commits survive a restart and offline work. A command-line client plans what would become shared, pushes only the selected history and the objects the server is missing, then fetches and fast-forwards a branch while checking every signature and receipt."
      points={[
        'The plan lists every commit and dependency that would become shared, including private ancestry. A push cannot add anything the plan did not show.',
        'Retries cost nothing: accepted objects and commits are reused, and stale refs and corrupted objects fail closed.',
        'Pull fast-forwards one chosen local branch with compare-and-swap. It never writes to an open document.'
      ]}
      boundary="Schema 36 is not deployed to the hosted pilot, and the client uses an ordinary account token. Project-scoped device sign-in is still open."
      caption="Commands from the selected-history sync guide. PROJECT and COMMIT are IDs; the file names are yours."
    >
      {#snippet visual()}
        <ol class="steps">
          {#each syncSteps as [name, command], index (name)}
            <li>
              <span class="num">STEP {String(index + 1).padStart(2, '0')} · {name}</span>
              <CopyCode code={command} label="{name} command" />
            </li>
          {/each}
        </ol>
      {/snippet}
    </FeatureRow>
  </Section>

  <Section
    id="control"
    eyebrow="CONTROL AND CONNECT"
    title="Your permissions, your editors, your files."
    lede="Dynodoc is the review and history layer. Microsoft and Google stay the editors, and access rules follow the people involved."
    layout="split"
    tone="muted"
  >
    <FeatureRow
      id="access"
      n="07"
      title="Roles, audit and erasure."
      chips={[{ kind: 'live', label: 'Live' }]}
      body="Access follows the team. Owners and managers control sharing; editors, reviewers and viewers get what their role allows. Reads, downloads, proposals and merges recheck current access every time, and permission and metadata changes keep their own operational audit."
      points={[
        'Project profiles are visible only to shared members and never include an email address.',
        'Revoked access ends history access. Public links and content hashes never grant it.',
        'Erasure is explicit and audited. Content is removed, and opaque commitments keep the remaining record verifiable.'
      ]}
      boundary="These are product controls. They are not a compliance certification or a third-party audit."
      caption="Illustrative mockup with synthetic names."
    >
      {#snippet visual()}
        <RolesPanel
          label="Project members with roles, and a project approval floor a file rule cannot lower"
        />
      {/snippet}
    </FeatureRow>

    <FeatureRow
      id="editors"
      n="08"
      flip
      title="Six editors, one history."
      chips={[{ kind: 'test', label: 'Test build', detail: 'no marketplace listing' }]}
      body="Add-ins for Word, Excel and PowerPoint and add-ons for Google Docs, Sheets and Slides record edits to existing text and cell values while the pane is open. Send them for review, then preview approved team changes and apply them in one step. Sign-in happens in the browser; each connection reaches one file, expires after seven days and can be revoked."
      points={[
        'Word and Docs: the text of existing paragraphs. Excel and Sheets: cell values, formula text and date-only values. PowerPoint and Slides: text in existing shapes. Up to 2,000 items per file.',
        'Nothing is written to your file until you choose Apply, and the add-in stops if the file changed after the preview.',
        'Not a background recorder or a keystroke log. Capture runs only while the pane is open.'
      ]}
      boundary="Downloadable test builds only. No marketplace listing exists, and installed-editor acceptance has not been run."
      caption="Illustrative mockup of the Word task pane. The other five hosts use the same flow."
    >
      {#snippet visual()}
        <div class="stack">
          <AddinMock host="word" label="The Dynodoc pane open next to a Word document" />
          <ul class="hosts" aria-label="Supported editors">
            {#each hosts as [kind, name] (kind)}
              <li><i class="dot dot--{kind}" aria-hidden="true"></i>{name}</li>
            {/each}
          </ul>
        </div>
      {/snippet}
    </FeatureRow>

    <FeatureRow
      id="formats"
      n="09"
      title="File adapters."
      chips={[{ kind: 'live', label: 'Live' }]}
      body="Adapters read each format into the same logical nodes and runs, so review and history work the same across files. Add Word, Excel and PowerPoint files, plain text, Markdown and LaTeX to a project. Original uploads are kept, and supported content exports back."
      points={[
        '.docx and .doc, .xlsx and .csv, .pptx, .txt, .md and .tex can be added to a project.',
        'During testing, original files are limited to 3,000,000 bytes.',
        'Layout-heavy features, macros, charts and embedded objects may not carry across. Check the compatibility guide before relying on a round trip.'
      ]}
      boundary="RTF is the next planned adapter and is not certified. Preserving a formula is not a promise that two calculation engines agree."
      caption="Formats accepted by the Projects upload today."
    >
      {#snippet visual()}
        <ul class="formats">
          {#each formats as [name, extensions, kind, Icon] (name)}
            <li class="format format--{kind}">
              <Icon size={22} aria-hidden="true" />
              <b>{name}</b>
              <span class="mono">{extensions}</span>
            </li>
          {/each}
        </ul>
      {/snippet}
    </FeatureRow>
  </Section>

  <Section
    id="compare"
    eyebrow="COMPARE"
    title="Track changes, version history and Dynodoc."
    lede="The built-in tools are good at what they do, and Dynodoc does not replace live co-authoring. The table shows where a recorded, reviewed history adds something and where it gives something up."
    layout="split"
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">
          Comparison of Track changes, built-in version history and Dynodoc
        </caption>
        <thead>
          <tr>
            <th scope="col">Question</th>
            <th scope="col">Track changes</th>
            <th scope="col">Version history</th>
            <th scope="col" class="us">Dynodoc</th>
          </tr>
        </thead>
        <tbody>
          {#each compare as row (row.topic)}
            <tr>
              <th scope="row">{row.topic}</th>
              <td>{row.track}</td>
              <td>{row.history}</td>
              <td class="us">{row.dyno}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="small note" data-reveal>
      Track changes and version history describe the behavior of Word and Google Docs, Sheets and
      Slides in general; individual products and plans differ. Dynodoc figures describe the hosted
      pilot or, where marked, source.
    </p>
  </Section>

  <Section
    id="questions"
    eyebrow="QUESTIONS"
    title="Short answers."
    lede="Longer answers are in the compatibility guide and on the roadmap."
    layout="split"
    tone="muted"
  >
    <div data-reveal><Faq items={faq} exclusive /></div>
    <div class="links" data-reveal>
      <Btn href="/compatibility" variant="link" arrow>Compatibility guide</Btn>
      <Btn href="/roadmap" variant="link" arrow>Roadmap</Btn>
      <Btn href="/help" variant="link" arrow>Help</Btn>
    </div>
  </Section>

  <Section
    tone="ink"
    align="center"
    eyebrow="START SMALL"
    title="Start with one file."
    lede="Create an invite-only project, add a document, and make the first change request."
  >
    <div class="closing" data-reveal>
      <Btn href={workspace} size="lg" arrow>Open Projects</Btn>
      <Btn href="/why-dynodoc" size="lg" variant="secondary">Why provenance matters</Btn>
    </div>
  </Section>
</main>

<style>
  .legend {
    display: grid;
    gap: 12px;
    padding-block: 24px;
  }
  .legend ul {
    display: grid;
    gap: 10px 28px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .lg {
    display: inline-block;
    margin-right: 8px;
    padding: 1px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    font-family: var(--site-font-mono);
    font-size: 0.6875rem;
    letter-spacing: 0.04em;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--color-ink);
  }
  .lg--live {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .lg--test {
    border-color: var(--color-warn-border);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  @media (min-width: 900px) {
    .legend {
      grid-template-columns: 140px minmax(0, 1fr);
      align-items: start;
    }
    .legend ul {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  .steps {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps li {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .stack {
    display: grid;
    gap: 14px;
    min-width: 0;
  }
  .hosts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .hosts li {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius-sm);
    background: var(--color-surface);
    font-size: 0.875rem;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    background: var(--color-muted);
  }
  .dot--word {
    background: #185abd;
  }
  .dot--excel,
  .dot--sheets {
    background: #107c41;
  }
  .dot--powerpoint {
    background: #c43e1c;
  }
  .dot--docs {
    background: #1a73e8;
  }
  .dot--slides {
    background: #f4b400;
  }

  .formats {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .format {
    display: grid;
    gap: 6px;
    padding: 18px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface);
  }
  .format b {
    font-weight: 560;
  }
  .format .mono {
    font-size: 0.8125rem;
    color: var(--color-ink-soft);
  }
  .format--word :global(svg) {
    color: var(--color-word);
  }
  .format--excel :global(svg) {
    color: var(--color-excel);
  }
  .format--powerpoint :global(svg) {
    color: var(--color-powerpoint);
  }
  .format--plain :global(svg) {
    color: var(--color-ink-soft);
  }

  .table-wrap {
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
  }
  table {
    width: 100%;
    min-width: 760px;
    border-collapse: collapse;
    font-size: 0.9375rem;
    line-height: 1.5;
  }
  thead th {
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border-strong);
    background: var(--color-surface-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-align: left;
    text-transform: uppercase;
    color: var(--color-ink);
  }
  thead th.us {
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  tbody th,
  tbody td {
    padding: 14px;
    border-bottom: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
    color: var(--color-ink-soft);
  }
  tbody th {
    width: 18%;
    color: var(--color-ink);
    font-weight: 560;
  }
  tbody td.us {
    background: color-mix(in srgb, var(--color-accent-bg) 55%, transparent);
    color: var(--color-ink);
  }
  tbody tr:last-child th,
  tbody tr:last-child td {
    border-bottom: 0;
  }
  .note {
    max-width: 70ch;
    margin: 18px 0 0;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 28px;
    margin-top: 24px;
  }
  .closing {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 12px;
  }
</style>
