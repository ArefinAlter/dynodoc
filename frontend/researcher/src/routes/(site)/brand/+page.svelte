<script lang="ts">
  import { Check, Download, X } from '@lucide/svelte';
  import Brachio from '$lib/art/Brachio.svelte';
  import Dino from '$lib/art/Dino.svelte';
  import Egg from '$lib/art/Egg.svelte';
  import Fossil from '$lib/art/Fossil.svelte';
  import Ptero from '$lib/art/Ptero.svelte';
  import Rex from '$lib/art/Rex.svelte';
  import Stego from '$lib/art/Stego.svelte';
  import Tracks from '$lib/art/Tracks.svelte';
  import Trike from '$lib/art/Trike.svelte';
  import type { DinoName } from '$lib/art';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { Btn, CopyCode, Section, scrollable } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
  import OperatorDetails from '$lib/components/marketing/OperatorDetails.svelte';
  import { OPERATOR } from '$lib/site/operator';

  const description =
    'Dynodoc logo and mark downloads, colour tokens, typography, usage rules, original dinosaur artwork and boilerplate text for press and partners.';

  const logos = [
    {
      file: 'mark.svg',
      name: 'Mark',
      note: 'Square mark for avatars and small spaces.',
      dark: false,
      width: 96
    },
    {
      file: 'logo.svg',
      name: 'Logo',
      note: 'Mark and wordmark for paper, white and light backgrounds.',
      dark: false,
      width: 240
    },
    {
      file: 'logo-dark.svg',
      name: 'Logo for dark backgrounds',
      note: 'Same logo with a light wordmark, for ink and dark backgrounds.',
      dark: true,
      width: 240
    }
  ] as const;

  const dos = [
    'Use the files as supplied, on a background with enough contrast.',
    'Leave clear space around the logo of at least the height of the mark’s green badge.',
    'Write the name as Dynodoc: one word, capital D.'
  ];
  const donts = [
    'Recolour, outline, stretch, rotate or add effects.',
    'Place the logo on busy images or low-contrast colours.',
    'Suggest endorsement by, or affiliation with, Microsoft, Google or anyone else.'
  ];

  const sitePalette = [
    ['Paper', '#f6f3ea', '--color-parchment', 'Page background'],
    ['Surface', '#fbf9f3', '--color-surface', 'Cards and panels'],
    ['Ink', '#1b1a17', '--color-ink', 'Body text, primary buttons'],
    ['Ink soft', '#45423a', '--color-ink-soft', 'Secondary text'],
    ['Muted', '#67645a', '--color-muted', 'Captions and labels'],
    ['Rule', '#d9d4c4', '--color-rule', 'Hairlines and borders'],
    ['Accent', '#0f6b3f', '--color-accent', 'Links, marks, emphasis'],
    ['Accent wash', '#e3efe6', '--color-accent-bg', 'Callouts and badges'],
    ['Highlighter', '#d7f25c', '--site-highlighter', 'One highlighted phrase in a heading']
  ] as const;
  const nightPalette = [
    ['Night paper', '#12120f', 'Page background'],
    ['Night surface', '#191914', 'Cards and panels'],
    ['Night ink', '#ece8dc', 'Body text'],
    ['Night accent', '#6fcf97', 'Links and marks']
  ] as const;
  const logoPalette = [
    ['Mark blue', '#2059b0'],
    ['Sky', '#bed9f4'],
    ['Check green', '#217346'],
    ['Ivory', '#fffdf7'],
    ['Wordmark navy', '#1a3045']
  ] as const;

  const plates: { name: DinoName; title: string; art: typeof Rex; note: string }[] = [
    { name: 'rex', title: 'Rex', art: Rex, note: 'Fetching a document.' },
    { name: 'stego', title: 'Stego', art: Stego, note: 'Pages for plates.' },
    { name: 'trike', title: 'Trike', art: Trike, note: 'Head lowered, frill raised.' },
    { name: 'brachio', title: 'Brachio', art: Brachio, note: 'Beside a bar chart.' },
    { name: 'ptero', title: 'Ptero', art: Ptero, note: 'Delivering a document.' },
    { name: 'fossil', title: 'Fossil', art: Fossil, note: 'Layers of history.' },
    { name: 'egg', title: 'Egg', art: Egg, note: 'Hatching.' },
    { name: 'tracks', title: 'Tracks', art: Tracks, note: 'A trail of footprints.' }
  ];

  const short =
    'Dynodoc is a document provenance, review and synchronization platform for Word, Excel, PowerPoint and their Google equivalents.';
  const long =
    'Dynodoc keeps a verifiable history of changes to documents edited in familiar tools. Every paragraph, cell and slide shape has a stable identity. Changes are recorded in an append-only, hash-chained log, reviewed as change requests, merged under project rules and recoverable without rewriting history. Dynodoc is a product of Verne Systems. The engine, API and command-line tools are open source under the MIT licence; the workspace and website are not published as open source. It is in active development: a hosted pilot is running, and the Microsoft and Google add-ins are available as test builds with no marketplace listing yet.';

  const facts = [
    ['Name', 'Dynodoc'],
    ['Website', 'dynodoc.online'],
    ['Workspace', 'app.dynodoc.online'],
    ['Operator', `${OPERATOR.legalName}, trading as ${OPERATOR.tradingAs}, Dhaka, Bangladesh`],
    [
      'Licence',
      'Engine, API and command-line tools: MIT. Workspace and website: not published as open source.'
    ],
    ['Status', 'Hosted pilot. Add-ins are test builds; no marketplace listing exists.']
  ] as const;
</script>

<svelte:head>
  <title>Brand · Dynodoc</title>
  <meta name="description" content={description} />
  <meta property="og:title" content="Brand · Dynodoc" />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content="Brand · Dynodoc" />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main" use:scrollable>
  <InfoHero
    eyebrow="BRAND"
    title="Brand and press kit"
    lede="Logos, colour, type and original artwork for press, partners and anyone writing about Dynodoc. Please use them as supplied."
  >
    {#snippet actions()}
      <a class="btn btn--primary" href="/brand/logo.svg" download
        ><Download size={16} aria-hidden="true" />Download logo.svg</a
      >
      <Btn href="#words" variant="secondary">Boilerplate text</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="rex" art={Rex} />
    {/snippet}
  </InfoHero>

  <Section
    id="logo"
    eyebrow="LOGO"
    title="Logo and mark."
    lede="Three SVG files with transparent backgrounds, and a social preview image."
    layout="split"
  >
    <ul class="logos">
      {#each logos as logo (logo.file)}
        <li class="card card--raised" data-reveal>
          <div class="stage" class:stage--dark={logo.dark}>
            <img
              src="/brand/{logo.file}"
              alt="Dynodoc {logo.name.toLowerCase()}"
              width={logo.width}
            />
          </div>
          <h3 class="h-3">{logo.name}</h3>
          <p>{logo.note}</p>
          <a class="btn btn--secondary btn--sm" href="/brand/{logo.file}" download
            ><Download size={15} aria-hidden="true" />Download {logo.file}</a
          >
        </li>
      {/each}
    </ul>

    <div class="rules">
      <div class="rule-col" data-reveal>
        <h3 class="eyebrow">Do</h3>
        <ul>
          {#each dos as line (line)}
            <li><Check size={16} aria-hidden="true" /><span>{line}</span></li>
          {/each}
        </ul>
      </div>
      <div class="rule-col" data-reveal>
        <h3 class="eyebrow">Do not</h3>
        <ul>
          {#each donts as line (line)}
            <li class="no"><X size={16} aria-hidden="true" /><span>{line}</span></li>
          {/each}
        </ul>
      </div>
    </div>
    <p class="small social" data-reveal>
      Social preview, 1200 by 630 pixels:
      <a class="link-sweep" href="/brand/social.png" download>social.png</a>.
    </p>
  </Section>

  <Section
    id="colour"
    eyebrow="COLOUR"
    title="Paper, ink and one green."
    lede="The site uses warm paper, near-black ink and a single forest green. These are the exact values and the tokens that carry them."
    layout="split"
    tone="muted"
  >
    <div class="palette-block" data-reveal>
      <h3 class="eyebrow">Site, day</h3>
      <ul class="swatches">
        {#each sitePalette as [name, hex, token, use] (hex)}
          <li>
            <span class="chip" style="--c: {hex}" aria-hidden="true"></span>
            <b>{name}</b>
            <code>{hex}</code>
            <span class="token mono">{token}</span>
            <span class="use">{use}</span>
          </li>
        {/each}
      </ul>
    </div>
    <div class="palette-block" data-reveal>
      <h3 class="eyebrow">Site, night</h3>
      <ul class="swatches">
        {#each nightPalette as [name, hex, use] (hex)}
          <li>
            <span class="chip" style="--c: {hex}" aria-hidden="true"></span>
            <b>{name}</b>
            <code>{hex}</code>
            <span class="use">{use}</span>
          </li>
        {/each}
      </ul>
    </div>
    <div class="palette-block" data-reveal>
      <h3 class="eyebrow">Inside the logo</h3>
      <ul class="swatches">
        {#each logoPalette as [name, hex] (hex)}
          <li>
            <span class="chip" style="--c: {hex}" aria-hidden="true"></span>
            <b>{name}</b>
            <code>{hex}</code>
          </li>
        {/each}
      </ul>
      <p class="small">
        The logo keeps its own blue and green. They are not interface colours; do not mix them into
        the paper palette.
      </p>
    </div>
  </Section>

  <Section
    id="type"
    eyebrow="TYPE"
    title="Geist and Geist Mono."
    lede="Geist for text and headings, Geist Mono for labels, numbers and code. Both are open fonts that the site hosts itself."
    layout="split"
  >
    <div class="type">
      <div class="specimen card card--raised" data-reveal>
        <span class="eyebrow">Geist</span>
        <p class="aa" aria-hidden="true">Aa</p>
        <p class="sample">Every change has an author, a reason and a place in the record.</p>
        <p class="small">Regular, medium and semibold. Headings use tight tracking.</p>
      </div>
      <div class="specimen card card--raised mono-spec" data-reveal>
        <span class="eyebrow">Geist Mono</span>
        <p class="aa" aria-hidden="true">01</p>
        <p class="sample">[ LABEL ] · STEP 01 · e7d31b09</p>
        <p class="small">Labels, numbering, hashes and code.</p>
      </div>
    </div>
    <p class="small note" data-reveal>
      Geist and Geist Mono are designed by Vercel with Basement Studio and licensed under the SIL
      Open Font License 1.1. Public Sans is the fallback, then the system sans-serif.
    </p>
  </Section>

  <Section
    id="art"
    eyebrow="ART"
    title="The dinosaur plates."
    lede="Pencil and halftone, one plate per idea. Each is a vector drawing that follows the page’s ink and green."
    layout="split"
    tone="muted"
  >
    <ul class="plates">
      {#each plates as plate (plate.name)}
        <li class="card card--raised plate" data-reveal>
          <div class="art"><Dino name={plate.name} art={plate.art} animate={false} /></div>
          <div class="meta">
            <h3 class="h-3">{plate.title}</h3>
            <p>{plate.note}</p>
          </div>
          <a class="btn btn--secondary btn--sm" href="/brand/art/{plate.name}.svg" download
            ><Download size={15} aria-hidden="true" />Download {plate.name}.svg</a
          >
        </li>
      {/each}
    </ul>
    <div class="callout" data-reveal>
      <p>
        <b>Original artwork.</b> These plates were created for Dynodoc. They are not stock illustration
        or third-party art. Use them to illustrate writing about Dynodoc. For any other use, ask first,
        and do not alter them to imply endorsement.
      </p>
    </div>
  </Section>

  <Section
    id="words"
    eyebrow="WORDS"
    title="Boilerplate and facts."
    lede="Copy these as written. They claim only what exists today."
    layout="split"
  >
    <div class="words">
      <div data-reveal>
        <h3 class="h-3">One sentence</h3>
        <CopyCode code={short} prompt="" label="one-sentence description" />
      </div>
      <div data-reveal>
        <h3 class="h-3">One paragraph</h3>
        <CopyCode code={long} prompt="" label="paragraph description" />
      </div>
      <dl class="facts" data-reveal>
        {#each facts as [term, value] (term)}
          <div>
            <dt>{term}</dt>
            <dd>{value}</dd>
          </div>
        {/each}
        <div>
          <dt>Open engine</dt>
          <dd>
            <Btn href={GITHUB_URL} variant="link" external arrow
              >github.com/ArefinAlter/dynodoc-engine</Btn
            >
          </dd>
        </div>
      </dl>
    </div>
  </Section>

  <Section
    id="contact"
    eyebrow="CONTACT"
    title="Press and licensing."
    lede="Press and licensing enquiries go to the operator. The same details appear in the legal pages."
    layout="split"
    tone="muted"
  >
    <div class="prose" data-reveal>
      <OperatorDetails emailLabel="Press and licensing email" />
      <p>
        The same details appear in the <a href="/privacy#contact">privacy policy</a> and the
        <a href="/terms">terms</a>, which are drafts. Operator website:
        <a href={OPERATOR.website}>{OPERATOR.websiteLabel}</a>.
      </p>
      <p class="small">
        Microsoft, Word, Excel, PowerPoint, Google, Google Docs, Google Sheets and Google Slides are
        trademarks of their owners. Dynodoc is not affiliated with or endorsed by them.
      </p>
    </div>
  </Section>
</main>

<style>
  .logos {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .logos li {
    display: grid;
    gap: 10px;
    align-content: start;
    justify-items: start;
  }
  .logos p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .stage {
    display: grid;
    width: 100%;
    min-height: 160px;
    place-items: center;
    padding: 24px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius-sm);
    background: #f6f3ea;
    background-image: radial-gradient(
      circle at 1px 1px,
      rgb(27 26 23 / 0.17) 1px,
      transparent 1.3px
    );
    background-size: 18px 18px;
  }
  .stage--dark {
    background-color: #12120f;
    background-image: radial-gradient(
      circle at 1px 1px,
      rgb(236 232 220 / 0.15) 1px,
      transparent 1.3px
    );
  }
  .stage img {
    display: block;
    max-width: 100%;
    height: auto;
  }

  .rules {
    display: grid;
    gap: 24px;
    margin-top: clamp(32px, 4vw, 56px);
  }
  .rule-col {
    display: grid;
    gap: 12px;
    align-content: start;
    padding-top: 16px;
    border-top: 1px solid var(--color-border-strong);
  }
  .rule-col ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rule-col li {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 12px;
    align-items: start;
    font-size: 0.9375rem;
  }
  .rule-col li :global(svg) {
    margin-top: 3px;
    color: var(--color-accent);
  }
  .rule-col li.no :global(svg) {
    color: var(--color-danger);
  }
  .social {
    margin: 24px 0 0;
  }
  .social a {
    color: var(--color-accent-strong);
  }

  .palette-block {
    display: grid;
    gap: 14px;
    align-content: start;
    padding-top: 16px;
    border-top: 1px solid var(--color-border-strong);
  }
  .palette-block + .palette-block {
    margin-top: clamp(24px, 3vw, 40px);
  }
  .swatches {
    display: grid;
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .swatches li {
    display: grid;
    gap: 2px;
    align-content: start;
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius-sm);
    background: var(--color-surface-raised);
    font-size: 0.875rem;
  }
  .chip {
    display: block;
    height: 56px;
    margin-bottom: 8px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius-sm);
    background: var(--c);
  }
  .swatches b {
    font-weight: 560;
  }
  .swatches code {
    color: var(--color-ink);
    font-size: 0.8125rem;
  }
  .swatches .token {
    color: var(--color-muted);
    font-size: 0.75rem;
    overflow-wrap: anywhere;
  }
  .swatches .use {
    color: var(--color-ink-soft);
  }

  .type {
    display: grid;
    gap: 16px;
  }
  .specimen {
    display: grid;
    gap: 10px;
    align-content: start;
  }
  .specimen .aa {
    margin: 0;
    font-size: clamp(4rem, 3rem + 5vw, 7rem);
    line-height: 1;
    letter-spacing: -0.04em;
    font-weight: 520;
    color: var(--color-ink-strong);
  }
  .specimen .sample {
    margin: 0;
    font-size: 1.125rem;
    color: var(--color-ink);
    overflow-wrap: anywhere;
  }
  .mono-spec .aa,
  .mono-spec .sample {
    font-family: var(--site-font-mono);
  }
  .note {
    max-width: 70ch;
    margin: 20px 0 0;
  }

  .plates {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .plate {
    display: grid;
    gap: 14px;
    align-content: start;
    justify-items: start;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }
  .plate .art {
    width: 100%;
    padding: 12px 8px 4px;
    border-bottom: 1px solid var(--color-border);
  }
  .plate .meta {
    display: grid;
    gap: 2px;
  }
  .plate .meta p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.875rem;
  }
  .callout {
    margin-top: clamp(24px, 3vw, 40px);
    max-width: 72ch;
  }
  .callout p {
    margin: 0;
  }

  .words {
    display: grid;
    gap: 28px;
    max-width: 72ch;
  }
  .words > div {
    display: grid;
    gap: 10px;
  }
  .facts {
    display: grid;
    margin: 0;
    border-top: 1px solid var(--color-border-strong);
  }
  .facts > div {
    display: grid;
    gap: 2px 20px;
    padding-block: 12px;
    border-bottom: 1px solid var(--color-border);
  }
  .facts dt {
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .facts dd {
    margin: 0;
  }

  @media (min-width: 720px) {
    .logos {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .rules {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 40px;
    }
    .swatches {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .type {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .plates {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .facts > div {
      grid-template-columns: 140px minmax(0, 1fr);
    }
  }
  @media (min-width: 1000px) {
    .plates {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .plate:last-child {
      grid-column: span 2;
      align-content: center;
    }
  }
</style>
