<script lang="ts">
  import { ArrowUpRight } from '@lucide/svelte';
  import { Btn, Eyebrow, Faq, Section, spotlight } from '$lib/components/site';
  import Dino from '$lib/art/Dino.svelte';
  import Trike from '$lib/art/Trike.svelte';
  import { pageMeta, scriptTag, type FaqEntry } from '$lib/seo';

  const ISSUES = 'https://github.com/ArefinAlter/dynodoc-engine/issues';
  const meta = pageMeta('/support');
  const title = meta?.title ?? 'Support · Dynodoc';
  const description =
    meta?.description ??
    'How to get help with Dynodoc: documentation, bug reports, security reports and what to expect.';

  const routes = [
    {
      label: 'Read the guide',
      href: '/help',
      text: 'Create a project, capture changes, review and merge, restore earlier content and verify history.'
    },
    {
      label: 'Editors and add-ins',
      href: '/add-ins',
      text: 'Requirements, test installation and what each add-in stores and sends.'
    },
    {
      label: 'Compatibility and limits',
      href: '/compatibility',
      text: 'What is captured today, minimum host versions, file formats and size limits.'
    },
    {
      label: 'Security report',
      href: '/security',
      text: 'How to report a vulnerability without publishing it.'
    },
    {
      label: 'Privacy and terms',
      href: '/privacy',
      text: 'How data is handled, and the terms proposed for using the service.'
    }
  ];

  // Answers are checked against extensions/INSTALL.md and docs/listings/*.md.
  const faq: FaqEntry[] = [
    {
      question: 'The add-in says my editor is not supported. What now?',
      answer:
        'The add-ins need Word with WordApi 1.3, Excel with ExcelApi 1.13 or PowerPoint with PowerPointApi 1.4. PowerPoint 2021 and earlier do not have PowerPointApi 1.4; use a current Microsoft 365 build or Office 2024. Google add-ins run in a desktop browser, not in the mobile apps. In any unsupported editor you can still push an edited file for review from Projects.'
    },
    {
      question: 'My connection stopped working.',
      answer:
        'A connection reaches one file and expires seven days after you approve it. It also stops if someone revokes it under Sync & connections. Choose Sign in to Dynodoc again in the pane or sidebar and approve the file. Office panes hold the connection in memory, so reopening the pane needs a new sign-in.'
    },
    {
      question: 'Capture did not record my edit.',
      answer:
        'Capture runs only while the pane or sidebar is open and capture is switched on. It records edits to existing paragraph text, cell values, formula source, date-only values and shape text. New, deleted or moved content and formatting are not captured; use an edited-file push for those.'
    },
    {
      question: 'Google says storage is full and capture stopped.',
      answer:
        'Google keeps about 500 KB of saved add-on data per user and script, shared by every file you connect, and each file queue is capped at 100 KB. When the store is full the sidebar stops capture and keeps your previous saved queue. Use Export local provenance to keep a copy of your queue. The add-on has no clear action today.'
    },
    {
      question: 'I renamed or copied the file and the connection stopped.',
      answer:
        'Word rechecks the saved file address. After Save As or a rename the active connection stops and its queue stays exportable. Reopen the pane in the saved copy and choose the exported recovery file before reconnecting.'
    },
    {
      question: 'My file is rejected as too large.',
      answer:
        'During testing an original file can be at most 3 MB (3,000,000 bytes). Workbook imports are also limited to 3,000 rows and 100 columns. Split or trim the file and try again.'
    }
  ];
  const faqScript = scriptTag({
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
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
  <!-- Static, first-party FAQ markup; no visitor or document value is used. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html faqScript}
</svelte:head>

<main id="main">
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__grid">
      <div class="hero__copy">
        <Eyebrow label="SUPPORT" />
        <h1 class="h-1">Get help with <span class="hl">Dynodoc</span>.</h1>
        <p class="lede">
          Where to read, how to report a problem and what to expect back. Dynodoc is in testing, so
          this page also says plainly what is not available yet.
        </p>
        <div class="actions">
          <Btn href="/help" arrow>Read the guide</Btn>
          <Btn href={ISSUES} variant="secondary" external arrow>Report a problem on GitHub</Btn>
        </div>
      </div>
      <div class="hero__art" aria-hidden="true">
        <Dino name="trike" art={Trike} />
      </div>
    </div>
  </header>

  <Section
    id="routes"
    eyebrow="WAYS TO GET HELP"
    title="Start with the page that fits."
    lede="Most questions about setup, limits and editors are answered in these pages."
  >
    <ul class="routes" data-reveal>
      {#each routes as route, i (route.href)}
        <li>
          <a class="card card--interactive route" href={route.href}>
            <span class="num">{String(i + 1).padStart(2, '0')}</span>
            <span class="h-3">{route.label}</span>
            <span class="small">{route.text}</span>
            <ArrowUpRight class="route__arrow" size={18} aria-hidden="true" />
          </a>
        </li>
      {/each}
      <li>
        <a
          class="card card--interactive route"
          href={ISSUES}
          target="_blank"
          rel="noopener noreferrer"
        >
          <span class="num">06</span>
          <span class="h-3">Report a bug<span class="sr-only"> (opens in a new tab)</span></span>
          <span class="small"
            >GitHub Issues is the place to report a problem or ask a question.</span
          >
          <ArrowUpRight class="route__arrow" size={18} aria-hidden="true" />
        </a>
      </li>
    </ul>
  </Section>

  <Section
    id="report"
    eyebrow="REPORT A PROBLEM"
    title="Report a bug or ask a question."
    lede="GitHub Issues is the channel for bug reports and questions today. Issues are public, so write each one as if a stranger will read it."
    layout="split"
  >
    <div class="report" data-reveal>
      <div class="card">
        <h3 class="h-3">Include</h3>
        <ul class="list">
          <li>The page or editor involved, and its version (for example, Word for Windows).</li>
          <li>What you expected and what happened instead.</li>
          <li>Steps to reproduce, using a small copy of a file with nothing sensitive in it.</li>
          <li>Screenshots with any private content hidden.</li>
        </ul>
      </div>
      <div class="card card--flat">
        <h3 class="h-3">Leave out</h3>
        <ul class="list">
          <li>Connection keys, pairing codes, passwords and sign-in links.</li>
          <li>Document contents you would not publish.</li>
          <li>Security vulnerabilities. Report those privately, as described below.</li>
        </ul>
      </div>
    </div>
    <div class="actions" data-reveal>
      <Btn href={ISSUES} external arrow>Open GitHub Issues</Btn>
    </div>
  </Section>

  <Section
    id="security"
    eyebrow="SECURITY"
    title="Found a vulnerability?"
    lede="Do not open a public issue. The security page explains what to report and how."
    tone="muted"
  >
    <div class="actions" data-reveal>
      <Btn href="/security" variant="secondary" arrow>Security page</Btn>
    </div>
  </Section>

  <Section
    id="contact"
    eyebrow="CONTACT AND STATUS"
    title="What exists today, and what does not."
    lede="These are stated as they are so that nobody waits on a channel that is not there."
  >
    <dl class="facts" data-reveal>
      <div>
        <dt>Support email</dt>
        <dd>
          <span class="badge">Awaiting confirmation</span>
          <p>
            No support address has been decided yet. It will be listed here once the operator
            confirms one. Until then, use GitHub Issues.
          </p>
        </dd>
      </div>
      <div>
        <dt>Service status</dt>
        <dd>
          <span class="badge">No status page</span>
          <p>
            There is no live status page. If you cannot reach the service, report it on GitHub
            Issues with the time and the page you were using.
          </p>
        </dd>
      </div>
      <div>
        <dt>Response times</dt>
        <dd>
          <span class="badge">Not promised</span>
          <p>
            Dynodoc is in testing. Reports are read as capacity allows, with no response-time
            commitment and no service-level agreement.
          </p>
        </dd>
      </div>
      <div>
        <dt>Operator details</dt>
        <dd>
          <span class="badge">Awaiting confirmation</span>
          <p>
            The legal operator, contact details and jurisdiction are not final. The
            <a href="/privacy">privacy policy</a> and <a href="/terms">terms</a> remain drafts until they
            are.
          </p>
        </dd>
      </div>
    </dl>
  </Section>

  <Section
    id="add-in-help"
    eyebrow="ADD-IN HELP"
    title="Common questions about the editor add-ins."
    layout="split"
    tone="muted"
  >
    <div data-reveal>
      <Faq items={faq} exclusive />
    </div>
  </Section>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .hero__grid {
    display: grid;
    gap: clamp(28px, 5vw, 64px);
    align-items: center;
    padding-block: clamp(48px, 8vw, 104px) clamp(40px, 6vw, 80px);
  }
  .hero__copy {
    display: grid;
    gap: 22px;
    justify-items: start;
  }
  .hero__copy h1 {
    max-width: 14ch;
  }
  .hero__art {
    max-width: 440px;
    color: var(--color-ink-soft);
    --art-accent: var(--color-accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .routes {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .route {
    display: grid;
    gap: 8px;
    height: 100%;
    padding-right: 52px;
  }
  .route :global(.route__arrow) {
    position: absolute;
    top: 20px;
    right: 20px;
    color: var(--color-muted);
    transition:
      transform 0.25s var(--site-ease),
      color 0.2s var(--site-ease);
  }
  .route:hover :global(.route__arrow),
  .route:focus-visible :global(.route__arrow) {
    color: var(--color-accent);
    transform: translate(2px, -2px);
  }

  .report {
    display: grid;
    gap: 16px;
    margin-bottom: 28px;
  }
  .list {
    display: grid;
    gap: 10px;
    margin: 14px 0 0;
    padding-left: 1.2em;
    color: var(--color-ink-soft);
  }
  .list li::marker {
    color: var(--color-muted);
  }

  .facts {
    display: grid;
    margin: 0;
    border-top: 1px solid var(--color-border-strong);
  }
  .facts > div {
    display: grid;
    gap: 10px;
    padding-block: 22px;
    border-bottom: 1px solid var(--color-border);
  }
  .facts dt {
    color: var(--color-ink-strong);
    font-size: 1.0625rem;
    font-weight: 560;
  }
  .facts dd {
    display: grid;
    gap: 10px;
    justify-items: start;
    margin: 0;
  }
  .facts dd p {
    max-width: 62ch;
    margin: 0;
    color: var(--color-ink-soft);
  }
  .facts a {
    color: var(--color-accent-strong);
    text-underline-offset: 3px;
  }

  @media (min-width: 720px) {
    .routes {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .report {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .facts > div {
      grid-template-columns: minmax(150px, 220px) minmax(0, 1fr);
      gap: 32px;
    }
  }
  @media (min-width: 900px) {
    .hero__grid {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    }
  }
  @media (min-width: 1000px) {
    .routes {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
</style>
