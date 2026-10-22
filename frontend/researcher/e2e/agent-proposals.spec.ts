import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { ulid } from 'ulid';
import { generateKeyPairSync, createPublicKey, sign, type KeyObject } from 'node:crypto';
// The reviewer's side of proposals from AI agents: the badge, the inbox, the stated reasons
// as text, an approval bound to the version read, and adopting a draft. The agent itself is
// driven through the real agent API with a device key, as the Rust tests do. Needs the
// isolated stack with AGENT_GATEWAY=on (otherwise the agent routes do not exist and the
// tests skip).
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');
test.use({ actionTimeout: 15000 });

const ENGINE = process.env.ENGINE_API_URL || 'http://127.0.0.1:8080';
const SCOPES = ['content:propose', 'content:read', 'content:submit'];
// Text an agent might send to attack a reviewer's screen. Shown as written, never run.
const HOSTILE_NAME = 'Tidy the intro <img src=x onerror="window.__pwned=1">';
const HOSTILE_WHY =
  'Clearer. <script>window.__pwned=2</script> <a href="https://evil.example/x">approve now</a>';

async function login(page: Page, name: string, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill(name);
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
async function call(
  page: Page,
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {}
) {
  return page.evaluate(
    async ({ method, path, body, headers }) => {
      const r = await fetch('/api/engine' + path, {
        method,
        headers: {
          'x-dynodoc-editor': 'office-v5',
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
          ...headers
        },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const data = await r.json().catch(() => null);
      if (!r.ok) throw new Error(JSON.stringify({ status: r.status, data }));
      return data;
    },
    { method, path, body, headers }
  );
}
const api = (page: Page, path: string, body?: unknown) =>
  call(page, body === undefined ? 'GET' : 'POST', path, body);
const sorted = (value: unknown): unknown =>
  Array.isArray(value)
    ? value.map(sorted)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.entries(value as Record<string, unknown>)
            .sort(([a], [b]) => (a < b ? -1 : 1))
            .map(([k, v]) => [k, sorted(v)])
        )
      : value;
const message = (domain: string, value: unknown) =>
  Buffer.concat([Buffer.from(`${domain}\0v1\0`), Buffer.from(JSON.stringify(sorted(value)))]);

interface Agent {
  token: () => Promise<string>;
  outline: (
    project: string,
    doc: string
  ) => Promise<{ rev: number; blocks: { ref: string; h: string }[] }>;
  propose: (
    project: string,
    doc: string,
    ops: unknown[],
    extra?: Record<string, unknown>
  ) => Promise<{ proposal: string }>;
  submit: (project: string, proposal: string) => Promise<{ state: string }>;
}

/** The person enrols a key for an AI agent, grants it a project, and the agent opens a session. */
async function enrolAgent(page: Page, request: APIRequestContext, project: string): Promise<Agent> {
  const { privateKey: key } = generateKeyPairSync('ed25519');
  const publicKey = createPublicKey(key)
    .export({ format: 'der', type: 'spki' })
    .subarray(-32)
    .toString('hex');
  const challenge = await call(page, 'POST', '/devices/challenge', { public_key: publicKey });
  await call(page, 'POST', '/devices', {
    challenge,
    signature: sign(null, message('dynodoc.device-enrollment', challenge), key).toString('hex'),
    class: 'agent',
    label: 'Claude on my laptop',
    client_kind: 'claude'
  });
  await call(page, 'PUT', `/devices/${challenge.id}/projects/${project}`, { scopes: SCOPES });
  const open = async (k: KeyObject) => {
    const c = await request.post(`${ENGINE}/projects/${project}/device-sessions/challenge`, {
      data: { public_key: publicKey, scopes: SCOPES }
    });
    expect(c.ok(), await c.text()).toBe(true);
    const { challenge: ch, mac } = await c.json();
    const o = await request.post(`${ENGINE}/projects/${project}/device-sessions`, {
      data: {
        challenge: ch,
        mac,
        signature: sign(null, message('dynodoc.device-session', ch), k).toString('hex')
      }
    });
    expect(o.ok(), await o.text()).toBe(true);
    return (await o.json()).access_token as string;
  };
  const token = () => open(key);
  const headers = async () => ({ authorization: `Bearer ${await token()}` });
  return {
    token,
    outline: async (p, doc) => {
      const r = await request.get(`${ENGINE}/projects/${p}/agent/files/${doc}/outline`, {
        headers: await headers()
      });
      expect(r.ok(), await r.text()).toBe(true);
      return r.json();
    },
    propose: async (p, doc, ops, extra = {}) => {
      const outline = await (
        await request.get(`${ENGINE}/projects/${p}/agent/files/${doc}/outline`, {
          headers: await headers()
        })
      ).json();
      const r = await request.post(`${ENGINE}/projects/${p}/agent/files/${doc}/propose`, {
        headers: await headers(),
        data: { rev: outline.rev, request_id: crypto.randomUUID(), ops, ...extra }
      });
      expect(r.ok(), await r.text()).toBe(true);
      return r.json();
    },
    submit: async (p, proposal) => {
      const r = await request.post(`${ENGINE}/projects/${p}/agent/proposals/${proposal}/submit`, {
        headers: await headers(),
        data: {}
      });
      expect(r.ok(), await r.text()).toBe(true);
      return r.json();
    }
  };
}

/** A project of one document, agents turned on, the owner as the agent's person. */
async function seed(
  page: Page,
  request: APIRequestContext,
  stamp: number,
  writeMode: 'draft' | 'change_request',
  reviewerEmail: string
) {
  const project = await api(page, '/projects', { name: `Proposals ${stamp}` });
  const policy = await api(page, `/projects/${project.id}/agent-policy`);
  test.skip(
    !policy.gateway,
    'The agent gateway is off in this stack (AGENT_GATEWAY=on turns it on).'
  );
  const doc = await api(page, '/documents', { title: `Plan ${stamp}` });
  await api(page, `/documents/${doc.id}/metadata`, { settings: { kind: 'document' } });
  await api(page, `/documents/${doc.id}/location`, { space_id: project.id });
  const root = ulid();
  const paragraphs = [
    'Intro: we study trust in small groups.',
    'Method: twelve interviews in three weeks.',
    'Result: favours build trust.'
  ];
  const ids = paragraphs.map(() => ulid());
  await api(page, `/documents/${doc.id}/batch`, {
    base_seq: 0,
    ops: [
      {
        type: 'NodeCreated',
        node_id: root,
        node_type: 'form',
        pos: '00000000',
        fields: { label: 'Plan' }
      },
      ...paragraphs.map((text, i) => ({
        type: 'NodeCreated',
        node_id: ids[i],
        node_type: 'item',
        parent_id: root,
        pos: `${(i + 1) * 10}`.padStart(8, '0'),
        fields: { label: text, content: { type: 'paragraph', content: [{ type: 'text', text }] } }
      }))
    ]
  });
  // One approval is required before anything merges, and the reviewer joins the project.
  const settings = await api(page, `/projects/${project.id}`);
  await api(page, `/projects/${project.id}/settings`, {
    name: settings.name,
    description: '',
    revision: settings.revision,
    protect_team_version: false,
    required_approvals: 1,
    merge_roles: 'editors'
  });
  await api(page, `/spaces/${project.id}/members`, { email: reviewerEmail, role: 'approver' });
  await call(
    page,
    'PUT',
    `/projects/${project.id}/agent-policy`,
    {
      enabled: true,
      write_mode: writeMode,
      submit_requires_person: false,
      proposals_per_day: 100,
      open_proposals: 50
    },
    { 'if-match': '"0"' }
  );
  const agent = await enrolAgent(page, request, project.id);
  return { project: project.id as string, doc: doc.id as string, agent, paragraphs };
}
const setText = (ref: string, h: string, text: string, why: string) => ({
  op: 'set_text',
  ref,
  text,
  base: h,
  why
});

test('A reviewer sees what an AI agent proposed, reads its words as text, and approves the version they read', async ({
  browser,
  request
}) => {
  test.setTimeout(240000);
  const stamp = Date.now(),
    rita = `rita-${stamp}@example.test`;
  const olivia = await (await browser.newContext()).newPage();
  const reviewerContext = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const reviewer = await reviewerContext.newPage();
  try {
    await login(olivia, 'Olivia Owner', `olivia-${stamp}@example.test`);
    const { project, doc, agent } = await seed(olivia, request, stamp, 'change_request', rita);

    // The agent proposes a change with its reasons and sends it for review by itself.
    const first = await agent.outline(project, doc);
    const made = await agent.propose(
      project,
      doc,
      [
        setText(
          first.blocks[1].ref,
          first.blocks[1].h,
          'Method: twelve interviews over three weeks.',
          HOSTILE_WHY
        )
      ],
      { name: HOSTILE_NAME }
    );
    expect((await agent.submit(project, made.proposal)).state).toBe('open');

    await login(reviewer, 'Rita Reviewer', rita);
    await reviewer.addInitScript(() => {
      (window as unknown as { __pwned?: number }).__pwned = undefined;
    });

    // Reachable from the project page: the tab says how many are waiting.
    await reviewer.goto(`/projects/${project}`);
    const tab = reviewer
      .getByRole('navigation', { name: 'Project sections' })
      .getByRole('button', { name: /AI proposals/ });
    await expect(tab).toBeVisible();
    await expect(tab.locator('.count')).toHaveText('1');
    await tab.click();
    await expect(reviewer).toHaveURL(/tab=agent/);
    const inbox = reviewer.getByRole('heading', { name: 'Proposals from AI agents' });
    await expect(inbox).toBeVisible();

    // The row says who wrote it in words, names the client, and shows the agent's name as text.
    const row = reviewer.locator('article[data-state="open"]');
    await expect(row).toHaveCount(1);
    await expect(row.getByText('AI agent', { exact: true })).toBeVisible();
    await expect(row).toContainText('Waiting for review');
    await expect(row).toContainText('in Claude');
    await expect(row.locator('strong')).toHaveText(HOSTILE_NAME);
    await expect(row.locator('img')).toHaveCount(0);
    expect(
      await reviewer.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)
    ).toBeUndefined();
    expect(
      (await new AxeBuilder({ page: reviewer }).include('.rp-primary').analyze()).violations
    ).toEqual([]);

    // The team moves on: the warning that the agent read an older file appears, in words.
    const latest = await api(olivia, `/documents/${doc}`);
    const edited = await api(olivia, `/documents/${doc}`).then(() => latest);
    const nodes = Object.values(
      edited.state.nodes as Record<
        string,
        { id: string; type: string; current_fields: { label?: string } }
      >
    );
    const target = nodes.find(
      (n) => n.type === 'item' && n.current_fields.label?.startsWith('Result')
    )!;
    await api(olivia, `/documents/${doc}/batch`, {
      base_seq: edited.latest_seq,
      ops: [
        {
          type: 'FieldEdited',
          node_id: target.id,
          field: 'content',
          value: {
            type: 'paragraph',
            content: [{ type: 'text', text: 'Result: favours build trust fast.' }]
          }
        },
        {
          type: 'FieldEdited',
          node_id: target.id,
          field: 'label',
          value: 'Result: favours build trust fast.'
        }
      ]
    });
    await reviewer.reload();
    await expect(reviewer.locator('article[data-state="open"] [role="note"]')).toContainText(
      /\d newer changes? since the AI agent read it/
    );

    // Open the review by keyboard: badge, who and which client, the reasons as text.
    const open = row.getByRole('button', { name: /^Review changes/ });
    await open.focus();
    await reviewer.keyboard.press('Enter');
    const dialog = reviewer.getByRole('dialog');
    await expect(dialog.getByRole('heading', { level: 2 })).toHaveText(HOSTILE_NAME);
    await expect(dialog.locator('.top .agent-badge')).toContainText('AI agent');
    const block = dialog.locator('section.agent');
    await expect(block).toContainText('Proposed by an AI agent');
    await expect(block).toContainText('proposed this through Claude');
    await expect(block).toContainText('shown exactly as written');
    await expect(block.locator('.why-text')).toHaveText(HOSTILE_WHY);
    await expect(dialog.locator('img, a[href*="evil.example"]')).toHaveCount(0);
    expect(
      await reviewer.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)
    ).toBeUndefined();
    expect(
      (await new AxeBuilder({ page: reviewer }).include('[role="dialog"]').analyze()).violations
    ).toEqual([]);

    // While Rita reads, the agent adds to its proposal. Her click is for the old version.
    const second = await agent.outline(project, doc);
    await agent.propose(
      project,
      doc,
      [
        setText(
          second.blocks[0].ref,
          second.blocks[0].h,
          'Intro: we study trust in small teams.',
          'More precise.'
        )
      ],
      { proposal: made.proposal }
    );
    await dialog.getByLabel('Review comment').fill('Reads well to me.');
    await dialog.getByRole('button', { name: 'Approve', exact: true }).click();
    const moved = dialog.getByTestId('request-moved');
    await expect(moved).toBeVisible();
    await expect(moved).toContainText(
      'This request changed while you were reading it. Nothing was recorded. Read the new version, then decide.'
    );
    await expect(moved).toBeFocused();
    await expect(dialog.getByRole('alert')).toHaveCount(0);
    await expect(dialog.getByLabel('Review comment')).toHaveValue('Reads well to me.');
    // The new version includes the agent's second reason and nothing was approved yet.
    await expect(block.locator('.why-text')).toHaveCount(2);
    const before = await api(reviewer, `/documents/${doc}/change-requests/${made.proposal}`);
    expect(before.review.approvals).toBe(0);

    // Approving what is on screen now counts.
    await dialog.getByRole('button', { name: 'Approve', exact: true }).click();
    await expect(dialog).toHaveCount(0);
    const after = await api(reviewer, `/documents/${doc}/change-requests/${made.proposal}`);
    expect(after.review.approvals).toBe(1);
    expect(after.review.satisfied).toBe(true);
    expect(after.review.reviews[0].revision).toBe(after.revision);

    // The same agent request wears the badge in the pull request list and in the history.
    await reviewer.goto(`/projects/${project}?tab=requests`);
    await expect(
      reviewer.locator('.rp-pr').first().getByText('AI agent', { exact: true })
    ).toBeVisible();
    await reviewer.goto(`/projects/${project}?tab=activity`);
    await expect(reviewer.getByText('AI agent', { exact: true }).first()).toBeVisible();
  } finally {
    await reviewerContext.close();
    await olivia.context().close();
  }
});

test('The person an AI agent acts for adopts its draft; the draft stays private until then', async ({
  browser,
  request
}) => {
  test.setTimeout(240000);
  const stamp = Date.now(),
    rita = `rita-adopt-${stamp}@example.test`;
  const oliviaContext = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const olivia = await oliviaContext.newPage();
  const reviewerContext = await browser.newContext();
  const reviewer = await reviewerContext.newPage();
  try {
    await login(olivia, 'Olivia Owner', `olivia-adopt-${stamp}@example.test`);
    const { project, doc, agent } = await seed(olivia, request, stamp, 'draft', rita);
    const outline = await agent.outline(project, doc);
    const made = await agent.propose(
      project,
      doc,
      [
        setText(
          outline.blocks[0].ref,
          outline.blocks[0].h,
          'Intro: we study trust in teams.',
          'Teams, not groups.'
        )
      ],
      { name: 'Reword the intro' }
    );

    // Only the person sees the draft; the reviewer's inbox is empty.
    await login(reviewer, 'Rita Reviewer', rita);
    await reviewer.goto(`/projects/${project}?tab=agent`);
    await expect(reviewer.getByText('No AI agent has a proposal waiting.')).toBeVisible();
    await expect(reviewer.locator('article')).toHaveCount(0);

    await olivia.goto(`/projects/${project}?tab=agent`);
    const row = olivia.locator('article[data-state="draft"]');
    await expect(row).toContainText('Reword the intro');
    await expect(row).toContainText('Waiting for you to send');
    await expect(row).toContainText('(your agent)');
    await expect(olivia.getByTestId('agent-budget')).toContainText(
      'started 1 of 100 proposals today'
    );
    await row.getByRole('button', { name: /^Review and send/ }).click();
    const dialog = olivia.getByRole('dialog');
    await expect(dialog.locator('.status')).toHaveText('draft');
    await expect(dialog).toContainText('This is a draft only you can see');
    await expect(dialog.getByRole('button', { name: /Merge/ })).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: 'Approve', exact: true })).toHaveCount(0);

    // The agent adds to the draft while she reads: adopting is refused calmly, nothing is sent.
    const again = await agent.outline(project, doc);
    await agent.propose(
      project,
      doc,
      [
        setText(
          again.blocks[2].ref,
          again.blocks[2].h,
          'Result: favours build lasting trust.',
          'Stronger.'
        )
      ],
      { proposal: made.proposal }
    );
    await dialog.getByLabel('Note for the reviewers').fill('Please check the wording.');
    await dialog.getByRole('button', { name: /Adopt and send for review/ }).click();
    const moved = dialog.getByTestId('request-moved');
    await expect(moved).toContainText(
      'The AI agent added to this draft while you were reading it. Nothing was sent.'
    );
    await expect(moved).toBeFocused();
    await expect(dialog.getByRole('alert')).toHaveCount(0);
    expect(
      (await api(olivia, `/documents/${doc}/drafts/${made.proposal}`)).draft.submitted_at
    ).toBeNull();

    await dialog.getByRole('button', { name: /Adopt and send for review/ }).click();
    await expect(dialog).toHaveCount(0);
    await expect(olivia.locator('article[data-state="open"]')).toContainText('Waiting for review');

    // The audit shows both who proposed it and who adopted it.
    const detail = await api(olivia, `/documents/${doc}/change-requests/${made.proposal}`);
    expect(detail.request.agent.principal.name).toBe('Olivia Owner');
    expect(detail.request.mine).toBe(true);
    expect(detail.review.can_approve).toBe(false);

    // Now the reviewer sees it, and she cannot approve her own agent's request.
    await reviewer.reload();
    await expect(reviewer.locator('article[data-state="open"]')).toContainText('Reword the intro');
    await olivia.reload();
    await olivia
      .locator('article')
      .getByRole('button', { name: /^Review changes/ })
      .click();
    await expect(olivia.getByRole('dialog')).toContainText('you cannot approve it');
    await expect(
      olivia.getByRole('dialog').getByRole('button', { name: 'Approve', exact: true })
    ).toHaveCount(0);
  } finally {
    await reviewerContext.close();
    await oliviaContext.close();
  }
});

test('The inbox works on a phone: no sideways scroll, controls a finger can hit', async ({
  browser,
  request
}) => {
  test.setTimeout(240000);
  const stamp = Date.now();
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    isMobile: true
  });
  const page = await context.newPage();
  try {
    await login(page, 'Olivia Phone', `olivia-phone-${stamp}@example.test`);
    const { project, doc, agent } = await seed(
      page,
      request,
      stamp,
      'draft',
      `rita-phone-${stamp}@example.test`
    );
    const outline = await agent.outline(project, doc);
    await agent.propose(
      project,
      doc,
      [
        setText(
          outline.blocks[0].ref,
          outline.blocks[0].h,
          'Intro: we study trust in teams.',
          'Teams.'
        )
      ],
      {
        name: 'A proposal with a rather long name that has to wrap on a narrow screen without scrolling sideways'
      }
    );
    await page.goto(`/projects/${project}?tab=agent`);
    const row = page.locator('article[data-state="draft"]');
    await expect(row).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth
    );
    expect(overflow).toBeLessThanOrEqual(1);
    for (const control of [
      row.getByRole('button', { name: /^Review and send/ }),
      page.getByLabel('Show')
    ]) {
      const box = (await control.boundingBox())!;
      expect(box.height).toBeGreaterThanOrEqual(43);
    }
    expect((await new AxeBuilder({ page }).include('.rp-primary').analyze()).violations).toEqual(
      []
    );
    await row.getByRole('button', { name: /^Review and send/ }).tap();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    const box = (await dialog.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(391);
  } finally {
    await context.close();
  }
});

test('A notification about an agent’s request carries the badge and a way to the inbox', async ({
  browser,
  request
}) => {
  test.setTimeout(240000);
  const stamp = Date.now(),
    rita = `rita-bell-${stamp}@example.test`;
  const olivia = await (await browser.newContext()).newPage();
  const reviewerContext = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const reviewer = await reviewerContext.newPage();
  try {
    await login(olivia, 'Olivia Owner', `olivia-bell-${stamp}@example.test`);
    const { project, doc, agent } = await seed(olivia, request, stamp, 'change_request', rita);
    const outline = await agent.outline(project, doc);
    const made = await agent.propose(
      project,
      doc,
      [
        setText(
          outline.blocks[0].ref,
          outline.blocks[0].h,
          'Intro: we study trust in teams.',
          'Teams.'
        )
      ],
      { name: 'Reword the intro' }
    );
    await agent.submit(project, made.proposal);
    await login(reviewer, 'Rita Reviewer', rita);
    await reviewer.goto('/projects');
    await reviewer.getByRole('button', { name: /^Notifications/ }).click();
    const panel = reviewer.getByRole('dialog', { name: 'Notifications' });
    await expect(panel.getByText('AI agent', { exact: true }).first()).toBeVisible();
    await expect(panel).not.toContainText('agents.invalid');
    await panel.getByRole('button', { name: 'All AI proposals' }).first().click();
    await expect(reviewer).toHaveURL(new RegExp(`/projects/${project}\\?tab=agent`));
    await expect(reviewer.locator('article[data-state="open"]')).toBeVisible();
  } finally {
    await reviewerContext.close();
    await olivia.context().close();
  }
});
