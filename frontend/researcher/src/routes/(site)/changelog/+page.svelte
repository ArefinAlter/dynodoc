<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Ptero from '$lib/art/Ptero.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { Btn, Section } from '$lib/components/site';

  const description =
    'Plain-language release notes for Dynodoc, newest first: what changed for you, and whether it runs in the hosted pilot or only in source.';

  type Entry = {
    id: string;
    iso: string;
    date: string;
    state: 'live' | 'source';
    title: string;
    summary: string;
    changes: string[];
    refs: string;
  };

  // Newest first. Distilled from docs/PROJECT-STATUS.md; no dates beyond those recorded there.
  const entries: Entry[] = [
    {
      id: 'schema-40-release',
      iso: '2026-10-13',
      date: '13 Oct 2026',
      state: 'live',
      title: 'Per-project key grants and a review permission go live',
      summary:
        'The hosted pilot moved to application 0f67372 on schema 40 after a verified backup and a rehearsal on a copy of production. The history digests before and after are identical.',
      changes: [
        'A signing key can open only the projects and permissions its owner grants. New keys start with no access; existing keys keep working until switched. Changes apply on the next request.',
        'Approving a review is a separate permission from publishing history, so a key that can push cannot approve unless it was granted that too.',
        'Your profile lists the projects you starred, and the key settings show how much access each key has.',
        'The licence metadata for the private workspace is replaced with a proprietary notice; the engine, API and command-line tools stay open source under the MIT licence.'
      ],
      refs: 'Application 0f67372 · schema 40 · engine 370de91'
    },
    {
      id: 'schema-39-release',
      iso: '2026-10-13',
      date: '13 Oct 2026',
      state: 'live',
      title: 'Issues, labels and stars go live; the operator is named',
      summary:
        'The hosted pilot moved to application 1b469a2 on schema 39 after a verified backup and a rehearsal on a copy of production. The history digests before and after are identical.',
      changes: [
        'Projects have an Issues tab with labels, assignees and immutable comments, and projects can be starred. Only members of a project can see or change them.',
        'The privacy, terms, security and support pages name Verne Systems as the operator and give its registered address and contact. They are still drafts and are not yet in effect.',
        'Only the engine, API and command-line tools are open source (MIT). The workspace and website are not published as open source, so the source download now points to the engine repository.',
        'Two read-only command-line reports are available for local repositories: a retention plan and a dry-run merge report for files edited on both sides.'
      ],
      refs: 'Application 1b469a2 · schema 39 · engine 676d143'
    },
    {
      id: 'schema-38-release',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'live',
      title: 'New site, GitHub-style Projects and project-scoped device sessions go live',
      summary:
        'The hosted pilot moved to application 49ada1e on schema 38 after a verified backup and a rehearsal on a copy of production. The history digests before and after are identical.',
      changes: [
        'Projects, profiles and the admin console have a repository-style layout with a command palette, activity feed, contribution graph and a read-only Branches tab.',
        'A device holding an enrolled signing key can request a one-hour token for one project and renew it by signing again. Tokens are separated from account tokens and re-checked on every request.',
        'The schema and API for shared project history, signing keys, storage budgets and private transfers are deployed. Shared writers stay off and no receipt-signing key is installed, so publishing to shared branches is still refused.',
        'Add-in manifests are renamed for a future store listing. No listing exists yet.'
      ],
      refs: 'Application 49ada1e · schema 38 · engine e5b9d94'
    },
    {
      id: 'dependencies-and-audit',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'source',
      title: 'Dependencies refreshed, and a streaming audit check',
      summary:
        'Nine of the ten open dependency updates to the standalone engine were reviewed and merged. The tenth stays open because it fails to build and needs code changes.',
      changes: [
        'The event log can now verify forward from a trusted position without loading everything. Review caught two bugs in the first version, a skipped negative sequence and an overflow at the maximum sequence, and both are fixed with tests.',
        'Verifying from the start still checks every stored row. Starting later needs an independently trusted anchor.',
        'No schema or hash format changed. All 360 tests pass in CI.'
      ],
      refs: 'Application 15f42c6 · engine 422e011'
    },
    {
      id: 'selected-history-sync',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'source',
      title: 'Selected-history sync from the command line',
      summary:
        'A client can plan, push, fetch and fast-forward project history against the new shared-history API, sending only what you selected.',
      changes: [
        'A plan lists every commit and dependency that would become shared, including private ancestry. Push cannot add anything the plan did not show.',
        'Fetch checks signatures, objects and receipts. Pull only fast-forwards a branch you chose and never writes to an open document.',
        'Retries reuse what was already accepted. Stale references and corrupted objects stop with an error.'
      ],
      refs: 'Application 47cb87f · engine 4ca7f16'
    },
    {
      id: 'shared-project-history',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'source',
      title: 'Shared project history, protected main and signed receipts',
      summary:
        'Schema 36 lets a project publish history explicitly, protect main with review rules and sign each branch update.',
      changes: [
        'Contributors publish candidates and advance feature branches. Main applies the current project and file rules and independent approvals.',
        'Each branch update, its signed receipt and its audit entry are written together or not at all.',
        'Erasing a document removes the affected published history. Affected heads become unavailable and never fall back to older content.'
      ],
      refs: 'Application 23ed7bf · engine 5f86db2'
    },
    {
      id: 'transfers-budgets-keys',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'source',
      title: 'Private transfers, storage budgets and device keys',
      summary:
        'Three pieces that make remote history safe to build on: who may upload, how much, and which device signed.',
      changes: [
        'Signing keys are bound to an account through a short-lived challenge. Revoking one is permanent, and earlier signatures stay verifiable.',
        'Shared storage has durable budgets (by default 1 GiB per document and 4 GiB per creator), and capacity errors say so plainly.',
        'Private object and commit staging means a guessed hash cannot expose someone else’s draft. Quotas and erasure are enforced.'
      ],
      refs: 'Application be538a6 · schemas 33 to 35'
    },
    {
      id: 'recovery-receipts-snapshots',
      iso: '2026-10-12',
      date: '12 Oct 2026',
      state: 'source',
      title: 'Project revert and cherry-pick, receipts and shared snapshots',
      summary:
        'The local project repository can undo or replay a change anywhere in its history, and a client can detect a rolled-back branch.',
      changes: [
        'Revert and cherry-pick apply a change, or its inverse, to the current commit and record where it came from in a signed commit. Tested fifty commits back with later work preserved.',
        'Signed receipts and a separate client journal detect rollback, forks and missing updates. Authority keys rotate only with both keys’ signatures.',
        'Named, deployed and public snapshots can share objects with drafts, behind an opt-in setting.'
      ],
      refs: 'Application 7d889ac, 3218315, f8f0adb'
    },
    {
      id: 'security-runtime',
      iso: '2026-10-11',
      date: '11 Oct 2026',
      state: 'live',
      title: 'Security and runtime safeguards go live',
      summary:
        'The hosted pilot moved to application 858801c on schema 29 after a verified backup and an upgrade and rollback rehearsal.',
      changes: [
        'Sessions are revocable and sign-out is enforced on the server. Saved snapshots are checked against event replay before reads, public copies and restores use them.',
        'The API runs with restricted database credentials, and migrations are a separate operator step.',
        'Project rules are a floor for file rules, metadata changes are audited, and browser policy and workload limits are scoped to the app.'
      ],
      refs: 'Application 858801c · schema 29'
    },
    {
      id: 'projects-first',
      iso: '2026-10-10',
      date: '10 Oct 2026',
      state: 'live',
      title: 'Projects becomes the front page',
      summary:
        'After sign-in you land on Projects: files, drafts, review and history in one place, with the web editors one tab away.',
      changes: [
        'Member-only profiles, multiple files per project, a main version and private drafts per file, pull requests and history.',
        'A separate Web editor tab opens the basic editor and returns to the repository.',
        'During testing, original uploads are capped at 3 MB.'
      ],
      refs: 'Application ecbc643 · schema 26'
    },
    {
      id: 'history-and-editors',
      iso: '2026-10-10',
      date: '10 Oct 2026',
      state: 'live',
      title: 'Range recovery and six editor test builds',
      summary:
        'History can be recovered by range, and every editor connector can sign in through the browser.',
      changes: [
        'Revert or cherry-pick a revision range into a private draft with explicit choices on overlap, then review it like any change.',
        'Download historical DOCX, XLSX and PPTX files through the supported exporters.',
        'Word, Excel, PowerPoint, Google Docs, Sheets and Slides connectors are downloadable as test packages. This is test distribution, not store publication.'
      ],
      refs: 'Application 536a448 · schema 25'
    }
  ];
</script>

<svelte:head>
  <title>Changelog · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content="Changelog · Dynodoc" />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content="Changelog · Dynodoc" />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main">
  <InfoHero
    eyebrow="CHANGELOG"
    title="Changelog"
    lede="What changed, in plain language, newest first. Production runs application 0f67372 on schema 40. Entries marked In source describe work that was implemented before the 12 October release; the newest entry says which parts are now deployed."
  >
    {#snippet actions()}
      <Btn href="/roadmap" arrow>See the roadmap</Btn>
      <Btn href="/features" variant="secondary">Read the features</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="ptero" art={Ptero} />
    {/snippet}
  </InfoHero>

  <Section
    id="entries"
    eyebrow="RELEASE NOTES"
    title="October 2026."
    lede="Eight entries from the project record, newest first."
    layout="split"
  >
    <ol class="log">
      {#each entries as entry (entry.id)}
        <li id={entry.id} class="entry" data-reveal>
          <div class="when">
            <time datetime={entry.iso} class="num">{entry.date}</time>
            <span class="chip chip--{entry.state}"
              >{entry.state === 'live' ? 'Live' : 'In source'}</span
            >
          </div>
          <article aria-labelledby="{entry.id}-title">
            <h3 id="{entry.id}-title" class="h-3">{entry.title}</h3>
            <p class="summary">{entry.summary}</p>
            <ul class="changes">
              {#each entry.changes as change (change)}<li>{change}</li>{/each}
            </ul>
            <p class="refs mono">{entry.refs}</p>
          </article>
        </li>
      {/each}
    </ol>
  </Section>

  <Section
    tone="ink"
    align="center"
    eyebrow="WHAT NEXT"
    title="The next entries are on the roadmap."
    lede="Rollout of schema 36 and project-scoped sign-in are the next steps that would change what runs in the hosted pilot."
  >
    <div class="closing" data-reveal>
      <Btn href="/roadmap" size="lg" arrow>Open the roadmap</Btn>
    </div>
  </Section>
</main>

<style>
  .log {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--color-border-strong);
  }
  .entry {
    display: grid;
    gap: 16px;
    padding-block: clamp(24px, 3.5vw, 40px);
    border-bottom: 1px solid var(--color-border);
  }
  .when {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    align-self: start;
  }
  .when time {
    color: var(--color-ink);
    font-size: 0.8125rem;
  }
  .chip {
    display: inline-block;
    padding: 1px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    font-family: var(--site-font-mono);
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.05em;
    line-height: 1.7;
    text-transform: uppercase;
    color: var(--color-ink-soft);
  }
  .chip--live {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  article {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  .summary {
    margin: 0;
    max-width: 62ch;
    color: var(--color-ink);
    font-size: 1.0625rem;
  }
  .changes {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0 0 0 1.1em;
    max-width: 66ch;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .changes li::marker {
    color: var(--color-muted);
  }
  .refs {
    margin: 4px 0 0;
    color: var(--color-muted);
    font-size: 0.75rem;
    overflow-wrap: anywhere;
  }
  .closing {
    display: flex;
    justify-content: center;
  }
  @media (min-width: 860px) {
    .entry {
      grid-template-columns: minmax(150px, 2fr) minmax(0, 8fr);
      gap: clamp(24px, 4vw, 56px);
    }
    .when {
      display: grid;
      justify-items: start;
    }
  }
</style>
