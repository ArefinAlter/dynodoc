<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Egg from '$lib/art/Egg.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { Btn, Section, Stat } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
  import { production } from '$lib/site/production';

  // Highest database schema in the repository's migrations. Change it with the migrations.
  const SOURCE_SCHEMA = 47;

  const DOCS = `${GITHUB_URL}/blob/main/docs`;
  const description =
    'What Dynodoc has built, what exists only in source, what is in progress and what comes later. Dated where a record gives a date, with no promised release dates.';

  type Item = { title: string; body: string; when?: string };
  type Group = {
    id: 'live' | 'source' | 'progress' | 'later';
    label: string;
    heading: string;
    blurb: string;
    items: Item[];
  };

  const groups: Group[] = [
    {
      id: 'live',
      label: 'Live',
      heading: 'Running in the hosted pilot.',
      blurb: `Deployed at application ${production.app} on database schema ${production.schema}, activated on ${production.date} (UTC) after a verified backup and a rehearsal on a production copy.`,
      items: [
        {
          title: 'Projects workspace',
          body: 'Invite-only projects, member profiles, multiple files, private drafts per file, change requests, comparisons and history. Uploads are capped at 3 MB while testing.',
          when: '10 Oct 2026'
        },
        {
          title: 'Historical recovery',
          body: 'Read any retained revision, restore selected blocks, revert or cherry-pick a revision range into a reviewed draft, and download supported native history.',
          when: '10 Oct 2026'
        },
        {
          title: 'Six editor test builds',
          body: 'Browser sign-in and file approval for Word, Excel, PowerPoint, Google Docs, Sheets and Slides, as downloadable test packages.',
          when: '10 Oct 2026'
        },
        {
          title: 'Security and runtime safeguards',
          body: 'Revocable sessions, verified snapshots, restricted database credentials, project policy floors, scoped CSP and HSTS, and workload limits.',
          when: '11 Oct 2026'
        },
        {
          title: 'New public site, GitHub-style Projects and admin console',
          body: 'The rebuilt site, repository-style Projects, profile, command palette, activity feed and read-only Branches tab, and the reorganised admin console.',
          when: '12 Oct 2026'
        },
        {
          title: 'Issues, labels and stars',
          body: 'Project issues with labels, assignees and immutable comments, and project stars, for invite-only projects (schema 39).',
          when: '13 Oct 2026'
        },
        {
          title: 'Per-project device-key grants and a review permission (schema 40)',
          body: 'A device key opens only the projects and permissions its owner grants, and approving reviews needs its own permission. Key-file permission hardening ships with the command-line tools.',
          when: '13 Oct 2026'
        },
        {
          title: 'Receipt-authority rotation (schema 41)',
          body: 'Dual-signed rotation of the receipt-signing authority, a recorded recovery path for a lost or leaked key, and clients that refuse a silent break. No authority is installed yet.',
          when: '14 Oct 2026'
        },
        {
          title: 'In-house spreadsheet formula engine',
          body: 'Spreadsheet calculation no longer depends on a copyleft library; 458 function names, incremental recalculation and reference rebasing, checked against Excel-evaluated formulas.',
          when: '14 Oct 2026'
        },
        {
          title: 'Shared-history schema and project-scoped device sessions',
          body: 'Schemas 30 to 38 are deployed: signing keys, storage budgets, private transfers, shared project history, activity read APIs and one-hour project-scoped device tokens. Shared writers stay off and no receipt-signing key is installed, so publishing to shared branches is refused.',
          when: '12 Oct 2026'
        }
      ]
    },
    {
      id: 'source',
      label: 'In source',
      heading: 'Built and tested, not enabled on the hosted pilot.',
      blurb:
        'Implemented in the repository and tested there. These are client libraries, opt-in writers and changes waiting for a release that the hosted pilot does not use yet, and none of it counts as installed-editor acceptance.',
      items: [
        {
          title: 'Typed shared storage',
          body: 'Chunked content, typed checkpoints and shared named, deployed and public snapshots, behind opt-in writers (through schema 32).',
          when: '10 to 12 Oct 2026'
        },
        {
          title: 'Local project repository',
          body: 'A SQLite repository on your device: signed multi-file commits, parents, branches with compare-and-swap, merge bases, comparison, revert and cherry-pick.',
          when: '12 Oct 2026'
        },
        {
          title: 'Signed receipts and device keys',
          body: 'Signed ref receipts with a separate client trust journal, and account-bound signing keys with permanent revocation (schema 33).',
          when: '12 Oct 2026'
        },
        {
          title: 'Storage budgets and private transfers',
          body: 'Durable shared-object budgets (schema 34) and account-private object and commit staging with erasure checks (schema 35).',
          when: '12 Oct 2026'
        },
        {
          title: 'Shared project history',
          body: 'Explicit publication, protected main, reviews bound to the exact base and candidate, and atomic signed receipts (schema 36).',
          when: '12 Oct 2026'
        },
        {
          title: 'Retention collector and same-file merge (command line)',
          body: 'A read-only retention plan, a collector that removes only staged objects no commit references after you confirm the exact plan, and a merge that applies clean node-level merges of a file edited on both sides. Commits are never pruned.',
          when: '13 Oct 2026'
        },
        {
          title: 'Encrypted key files and compressed local storage',
          body: 'Passphrase-protected signing keys with optional Windows, macOS or Linux key storage, and transparent compression of objects in the local repository.',
          when: '14 Oct 2026'
        },
        {
          title: 'Selected-history sync client',
          body: 'Plan, push, fetch and fast-forward pull from the command line, with verified objects and receipts.',
          when: '12 Oct 2026'
        },
        {
          title: 'Browser test matrix and installable web app',
          body: 'In source; release pending. The web app’s tests now also run in Firefox and in WebKit, the engine Safari uses, with phone-sized screens emulated. Layout and touch fixes for small screens and an installable web app (a manifest and a service worker that keeps only public script, style and font files and an offline page) are built. Real phones and Safari itself have not been tested, and installing on a real operating system has not been tried.',
          when: '21 Oct 2026'
        },
        {
          title: 'Six-platform builds of the command-line tools and engine',
          body: 'In source; release pending. The public engine repository builds and tests on Linux, macOS and Windows, on Intel or AMD and on Arm processors, and a release workflow builds the command-line tools for all six. These are automated builds only: no prebuilt tools are published, and the binaries would be unsigned.',
          when: '21 Oct 2026'
        },
        {
          title: 'Approvals bound to the version reviewed',
          body: 'In source; release pending. An approval counts only for the draft the reviewer looked at. If the draft changes afterwards, the approval stops counting and a new one is needed. Needs schema 45, which has not been released.',
          when: '22 Oct 2026'
        },
        {
          title: 'AI-agent connectors (off by default)',
          body: 'In source; release pending. An AI agent can read what a project’s policy allows and propose changes as a draft. A draft reaches the shared version only after the person the agent acts for submits it and the project’s usual review rules are met. Agents are off unless a project turns them on, the hosted pilot does not offer them, and none has been run with a real AI client.',
          when: '18 to 22 Oct 2026'
        }
      ]
    },
    {
      id: 'progress',
      label: 'In progress',
      heading: 'Being worked on now.',
      blurb:
        'These come next. They are listed in the order the dependencies run, not as dated commitments.',
      items: [
        {
          title: 'Receipt authority and shared writers',
          body: 'Schema 41 is deployed and adds server-side authority rotation and a recorded recovery path, so a lost or leaked key is an incident rather than a rebuild. Shared writers stay off and no signing key is installed until key custody is decided and rotation has been rehearsed on a production copy.'
        },
        {
          title: 'Project-scoped sign-in',
          body: 'Project-scoped sessions, per-project grants and a review scope are live. Passphrase-encrypted key files and operating-system key storage exist in the command-line tools. Hardware keys and memory-hard key derivation remain.'
        },
        {
          title: 'Projects and add-in adoption',
          body: 'A read-only Branches tab is live. Creating, reviewing and merging project-wide branches in the interface, and adoption by the six connectors, remain.'
        },
        {
          title: 'Storage packing and retention',
          body: 'A retention plan, a collector for staged objects and transparent compression of local objects exist. Pack files with shared dictionaries, publication of only changed nodes, and commit pruning with a retention policy do not.'
        },
        {
          title: 'Same-file offline convergence',
          body: 'Clean same-file merges can be applied from the command line. Constraint handling across documents, resolving real conflicts, and tests across restarts and partitions remain.'
        },
        {
          title: 'Installed-editor acceptance',
          body: 'The owner runs a prepared worksheet against real Word, Excel, PowerPoint and Google editors. No results are recorded yet.'
        }
      ]
    },
    {
      id: 'later',
      label: 'Later',
      heading: 'Needs earlier work first.',
      blurb: 'Intended, with no date and no promise.',
      items: [
        {
          title: 'Marketplace publication',
          body: 'Paused. Listing copy, scopes and a submission checklist are prepared. Nothing has been submitted and no listing exists. No date.'
        },
        {
          title: 'Independent witnesses',
          body: 'External timestamps or gossip, and server key rotation and compromise recovery, so a complete rewrite by the operator can be detected.'
        },
        {
          title: 'Large documents and service scale',
          body: 'Streaming, resumable ingest tested with 100, 250 and 500 MB files, ten collaborators, many teams, and measured capacity and cost.'
        },
        {
          title: 'More formats, lossless structures',
          body: 'RTF, lossless native structures and assets, and background and offline capture in the editors.'
        },
        {
          title: 'Project workspace depth',
          body: 'Creating and merging project-wide branches in the web workspace, project-scoped search and releases. Compare, activity, issues and stars exist.'
        }
      ]
    }
  ];

  const rules = [
    [
      'No capacity claims.',
      'Hundreds-of-megabyte files and very large user counts are targets. They are not measured capacity today.'
    ],
    [
      'No acceptance claims.',
      'Source tests and CI are not installed-editor or marketplace acceptance, and are never reported as such.'
    ],
    [
      'No Office parity goal.',
      'The web editors stay, but matching Word or Excel feature for feature is not the roadmap.'
    ]
  ] as const;
</script>

<svelte:head>
  <title>Roadmap · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content="Roadmap · Dynodoc" />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content="Roadmap · Dynodoc" />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main">
  <InfoHero
    eyebrow="ROADMAP"
    lede="What is live, what exists only in source, what is being worked on and what comes later. A date appears only where a record gives one. Last reviewed 23 October 2026."
  >
    {#snippet heading()}The roadmap, <span class="hl">without promises</span>.{/snippet}
    {#snippet actions()}
      <Btn href="/features" arrow>See the features</Btn>
      <Btn href="/changelog" variant="secondary">Read the changelog</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="egg" art={Egg} />
    {/snippet}
  </InfoHero>

  <Section
    id="standing"
    eyebrow="WHERE THINGS STAND"
    title="Four numbers that keep this honest."
    lede="The distance between source and production is the most useful thing to know before relying on any feature."
    layout="split"
  >
    <ul class="stats">
      <li data-reveal>
        <Stat
          value={production.schema}
          label="Database schema on the hosted pilot ({production.date})"
        />
      </li>
      <li data-reveal>
        <Stat value={SOURCE_SCHEMA} label="Schema reached in source, release pending" />
      </li>
      <li data-reveal><Stat value={6} label="Editor connectors, all test builds" /></li>
      <li data-reveal><Stat value={0} label="Marketplace listings submitted or approved" /></li>
    </ul>
  </Section>

  <Section
    id="board"
    eyebrow="THE LIST"
    title="Everything, by state."
    lede="Four states, each with its own meaning. An item moves down this page only when evidence says it did."
    layout="split"
    tone="muted"
  >
    <div class="groups">
      {#each groups as group (group.id)}
        <section class="group" aria-labelledby="g-{group.id}" data-reveal>
          <header class="g-head">
            <span class="chip chip--{group.id}">{group.label}</span>
            <h3 id="g-{group.id}" class="h-3">{group.heading}</h3>
            <p>{group.blurb}</p>
            <p class="count num">{group.items.length} items</p>
          </header>
          <ul class="items">
            {#each group.items as item (item.title)}
              <li class="card card--raised item">
                <h4>{item.title}</h4>
                <p>{item.body}</p>
                {#if item.when}<p class="when num">{item.when}</p>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  </Section>

  <Section
    id="rules"
    eyebrow="GROUND RULES"
    title="What this page will not do."
    lede="A roadmap can quietly turn into a promise. These three rules stop that."
    layout="split"
  >
    <ol class="rules">
      {#each rules as [title, body], index (title)}
        <li data-reveal>
          <span class="num">{String(index + 1).padStart(2, '0')}</span>
          <h3 class="h-3">{title}</h3>
          <p>{body}</p>
        </li>
      {/each}
    </ol>
    <div class="sources" data-reveal>
      <p class="eyebrow">Where this comes from</p>
      <div class="links">
        <Btn href="{DOCS}/ROADMAP-ACCEPTANCE.md" variant="link" external arrow
          >Acceptance ledger</Btn
        >
        <Btn href="{DOCS}/ENGINE-EVOLUTION-PLAN.md" variant="link" external arrow
          >Evolution plan</Btn
        >
        <Btn href="{DOCS}/PROJECT-STATUS.md" variant="link" external arrow>Project status</Btn>
      </div>
    </div>
  </Section>

  <Section
    tone="ink"
    align="center"
    eyebrow="FOLLOW ALONG"
    title="Watch it move."
    lede="Each change that reaches production or source is written up in plain language."
  >
    <div class="closing" data-reveal>
      <Btn href="/changelog" size="lg" arrow>Read the changelog</Btn>
      <Btn href="/why-dynodoc" size="lg" variant="secondary">Why provenance matters</Btn>
    </div>
  </Section>
</main>

<style>
  .stats {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius);
    background: var(--color-surface);
  }
  .stats li {
    padding: 24px;
    border-bottom: 1px solid var(--color-border);
  }
  .stats li:last-child {
    border-bottom: 0;
  }

  .groups {
    display: grid;
    gap: clamp(36px, 5vw, 64px);
  }
  .group {
    display: grid;
    gap: 20px;
    padding-top: clamp(24px, 3vw, 36px);
    border-top: 1px solid var(--color-border-strong);
  }
  .group:first-child {
    padding-top: 0;
    border-top: 0;
  }
  .g-head {
    display: grid;
    gap: 10px;
    align-content: start;
    justify-items: start;
  }
  .g-head p {
    margin: 0;
    max-width: 38ch;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .g-head .count {
    color: var(--color-muted);
  }
  .chip {
    display: inline-block;
    padding: 2px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.05em;
    line-height: 1.7;
    text-transform: uppercase;
    color: var(--color-ink);
  }
  .chip--live {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .chip--progress {
    border-color: var(--color-warn-border);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .chip--later {
    border-style: dashed;
    color: var(--color-ink-soft);
  }
  .items {
    display: grid;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .item {
    display: grid;
    gap: 8px;
    align-content: start;
  }
  .item h4 {
    margin: 0;
    font-size: 1.0625rem;
    font-weight: 560;
    letter-spacing: -0.01em;
    color: var(--color-ink-strong);
  }
  .item p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .item .when {
    margin-top: 4px;
    color: var(--color-muted);
    font-size: 0.75rem;
  }

  .rules {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rules li {
    display: grid;
    gap: 8px;
    align-content: start;
    padding-top: 18px;
    border-top: 1px solid var(--color-border-strong);
  }
  .rules p {
    margin: 0;
    color: var(--color-ink-soft);
  }
  .sources {
    display: grid;
    gap: 10px;
    margin-top: clamp(28px, 4vw, 48px);
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 28px;
  }
  .closing {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 12px;
  }

  @media (min-width: 720px) {
    .stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .stats li:nth-child(odd) {
      border-right: 1px solid var(--color-border);
    }
    .stats li:nth-last-child(-n + 2) {
      border-bottom: 0;
    }
    .items {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (min-width: 960px) {
    .stats {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
    .stats li {
      border-bottom: 0;
      border-right: 1px solid var(--color-border);
    }
    .stats li:last-child {
      border-right: 0;
    }
    .group {
      grid-template-columns: minmax(0, 4fr) minmax(0, 8fr);
      gap: clamp(24px, 4vw, 56px);
    }
    .rules {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 28px;
    }
  }
</style>
