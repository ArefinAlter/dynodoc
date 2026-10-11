import { expect, test, type Page } from '@playwright/test';
import { ulid } from 'ulid';
import { randomUUID } from 'node:crypto';
test.skip(process.env.PILOT_E2E !== 'true', 'Requires the isolated signed-in development stack.');
async function login(page: Page, email: string) {
  await page.goto('/login');
  await page.getByLabel('Your name').fill('Project tester');
  await page.getByLabel('Work email').fill(email);
  await page.getByRole('button', { name: 'Email me a sign-in link' }).click();
  await page.getByRole('button', { name: /Continue in local development/ }).click();
  await expect(page).toHaveURL(/\/projects$/);
}
async function api(page: Page, path: string, body?: unknown) {
  return page.evaluate(
    async ({ path, body }) => {
      const r = await fetch('/api/engine' + path, {
        method: body === undefined ? 'GET' : 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: body === undefined ? undefined : JSON.stringify(body)
      });
      const data = await r.json();
      if (!r.ok) throw new Error(JSON.stringify(data));
      return data;
    },
    { path, body }
  );
}

test('A project editor can import a file but its ownership cannot bypass project rules', async ({
  browser
}) => {
  test.setTimeout(120000);
  const managerContext = await browser.newContext(),
    creatorContext = await browser.newContext(),
    stamp = Date.now();
  try {
    const manager = await managerContext.newPage(),
      creator = await creatorContext.newPage();
    await login(manager, `policy-manager-${stamp}@example.test`);
    const project = await api(manager, '/projects', { name: `Protected project ${stamp}` });
    const email = `policy-creator-${stamp}@example.test`;
    await api(manager, `/spaces/${project.id}/members`, { email, role: 'editor' });
    await login(creator, email);
    await creator.goto(`/projects/${project.id}`);
    await creator.locator('input[type=file]').setInputFiles({
      name: 'Owned report.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('Imported by a project editor')
    });
    await creator.getByRole('link', { name: 'Owned report', exact: true }).click();
    const files = await api(creator, `/projects/${project.id}/files`);
    const file = files.items[0];
    const people = await api(creator, `/documents/${file.id}/people`);
    expect(people.you.role).toBe('owner');
    expect(people.you.can.edit_team).toBe(false);
    expect(people.you.can.manage_rules).toBe(false);
    const oversized = await creator.evaluate(async (id: string) => {
      const response = await fetch(`/api/engine/documents/${id}/metadata`, {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'x-dynodoc-editor': 'office-v5' },
        body: JSON.stringify({ title: 'x'.repeat(80 * 1024) })
      });
      return response.status;
    }, file.id);
    expect(oversized).toBe(413);
    await creator.getByRole('link', { name: 'Web editor', exact: true }).click();
    await expect(
      creator.getByText('The team version is protected.', { exact: true })
    ).toBeVisible();
    await expect(creator.getByLabel('Document title')).toBeDisabled();
    await creator.getByRole('button', { name: 'Share', exact: true }).click();
    await expect(creator.getByText('Minimum rules from', { exact: false })).toBeVisible();
    await expect(creator.getByRole('checkbox', { name: /Protect the team version/ })).toBeChecked();
    await expect(
      creator.getByRole('checkbox', { name: /Protect the team version/ })
    ).toBeDisabled();
    await expect(creator.getByRole('button', { name: 'Save rules', exact: true })).toHaveCount(0);
    await manager.goto(`/documents/${file.id}`);
    await manager.getByRole('button', { name: 'Share', exact: true }).click();
    await expect(
      manager.getByRole('checkbox', { name: /Protect the team version/ })
    ).toBeDisabled();
    await manager.getByLabel('Approvals needed before merging').selectOption('1');
    await manager.getByRole('button', { name: 'Save rules', exact: true }).click();
    await expect(manager.getByText('Review rules saved.')).toBeVisible();
    expect((await api(creator, `/documents/${file.id}/people`)).policy.required_approvals).toBe(1);
  } finally {
    await managerContext.close();
    await creatorContext.close();
  }
});
test('Projects collects external edits, hides private drafts, reviews a bundle and revokes its connector', async ({
  browser
}) => {
  test.setTimeout(120000);
  const stamp = Date.now(),
    ownerContext = await browser.newContext({ acceptDownloads: true }),
    contributorContext = await browser.newContext();
  try {
    const owner = await ownerContext.newPage(),
      contributor = await contributorContext.newPage();
    await login(owner, `project-owner-${stamp}@example.test`);
    await owner.getByRole('button', { name: 'New project', exact: true }).click();
    await owner.getByLabel('Project name').fill(`External authors ${stamp}`);
    await owner.getByRole('button', { name: 'Create project' }).click();
    await expect(owner.getByRole('button', { name: 'Files', exact: true })).toBeVisible();
    const projectURL = owner.url(),
      projectId = projectURL.split('/').pop();
    const doc = await api(owner, '/documents', { title: 'Connector report' }),
      root = ulid(),
      block = ulid();
    await api(owner, `/documents/${doc.id}/metadata`, { settings: { kind: 'document' } });
    await api(owner, `/documents/${doc.id}/batch`, {
      base_seq: 0,
      ops: [
        {
          type: 'NodeCreated',
          node_id: root,
          node_type: 'form',
          pos: '000000',
          fields: { label: 'Report' }
        },
        {
          type: 'NodeCreated',
          node_id: block,
          node_type: 'item',
          parent_id: root,
          pos: '00000000',
          fields: {
            label: 'Original paragraph',
            content: { type: 'paragraph', content: [{ type: 'text', text: 'Original paragraph' }] }
          }
        }
      ]
    });
    await api(owner, `/documents/${doc.id}/location`, { space_id: projectId });
    await owner.reload();
    await expect(owner.getByText('Connector report', { exact: true })).toBeVisible();
    await owner.getByRole('button', { name: 'Settings', exact: true }).click();
    await owner
      .getByLabel('Email', { exact: true })
      .fill(`project-contributor-${stamp}@example.test`);
    await owner.getByLabel('Send an email invitation with a link').uncheck();
    await owner.getByRole('button', { name: 'Add person' }).click();
    await expect(owner.getByRole('status')).toContainText('Access updated');
    await login(contributor, `project-contributor-${stamp}@example.test`);
    await contributor.goto(projectURL);
    await contributor.getByRole('button', { name: 'Your drafts', exact: true }).click();
    await expect(contributor.getByText('No private drafts on this page.')).toBeVisible();
    const base = await api(owner, `/documents/${doc.id}/provenance`);
    const bundle = (name: string, submit: boolean) => ({
      name,
      note: 'Observed in the external editor',
      submit,
      bundle: {
        format: 'dynodoc.change-bundle',
        version: 1,
        bundle_id: randomUUID(),
        client_id: randomUUID(),
        document_id: doc.id,
        base_seq: base.through_seq,
        base_chain_hash: base.chain_hash,
        capture: {
          host: 'google-docs',
          mode: 'observed_snapshot',
          document_name: 'renamed-report.docx'
        },
        changes: [
          {
            id: randomUUID(),
            observed_at: new Date().toISOString(),
            operation: {
              type: 'FieldEdited',
              node_id: block,
              field: 'content',
              value: {
                type: 'paragraph',
                content: [{ type: 'text', text: 'Edited in Google Docs' }]
              }
            }
          }
        ]
      }
    });
    await api(owner, `/documents/${doc.id}/provenance`, bundle('Owner private work', false));
    await contributor.reload();
    await expect(contributor.getByText('Owner private work', { exact: true })).toHaveCount(0);
    await contributor.getByRole('button', { name: 'Files', exact: true }).click();
    await contributor.getByRole('button', { name: 'Sync Connector report', exact: true }).click();
    await expect(contributor.getByRole('combobox', { name: 'Editor', exact: true })).toBeVisible();
    await contributor
      .getByRole('combobox', { name: 'Editor', exact: true })
      .selectOption('google-docs');
    await contributor.getByRole('button', { name: 'Create connection key' }).click();
    const key = await contributor.getByLabel('Copy this key into your connector').inputValue();
    const push = bundle('Google Docs contribution', true),
      endpoint = `/api/connectors/documents/${doc.id}/bundles`;
    const sent = await contributorContext.request.post(endpoint, {
      headers: { authorization: `Bearer ${key}` },
      data: push
    });
    expect(sent.status()).toBe(200);
    const receipt = await sent.json();
    const retry = await contributorContext.request.post(endpoint, {
      headers: { authorization: `Bearer ${key}` },
      data: push
    });
    expect(await retry.json()).toEqual(receipt);
    await owner.getByRole('button', { name: 'Pull requests', exact: true }).click();
    await expect(owner.getByText('Google Docs contribution', { exact: true })).toBeVisible();
    await owner.getByRole('button', { name: 'Review changes' }).click();
    await expect(owner.getByRole('dialog').locator('ins').last()).toContainText('in Google Docs');
    await owner.getByRole('button', { name: /^Merge / }).click();
    await expect(owner.getByText(/Connector report · merged/)).toBeVisible();
    const checkpoint = await contributorContext.request.get(
      `/api/connectors/documents/${doc.id}/checkpoint`,
      { headers: { authorization: `Bearer ${key}` } }
    );
    expect(checkpoint.status()).toBe(200);
    const latest = await checkpoint.json();
    expect(latest.through_seq).toBeGreaterThan(base.through_seq);
    expect(latest.proposals[0].status).toBe('merged');
    expect(latest.state.nodes[block].current_fields.content.content[0].text).toBe(
      'Edited in Google Docs'
    );
    const historical = await contributorContext.request.get(
      `/api/connectors/documents/${doc.id}/checkpoint?through_seq=${base.through_seq}`,
      { headers: { authorization: `Bearer ${key}` } }
    );
    expect(historical.status()).toBe(200);
    const starting = await historical.json();
    expect(starting.chain_hash).toBe(base.chain_hash);
    expect(starting.state).toEqual(base.state);
    expect(starting.proposals[0].status).toBe('merged');
    expect(
      (
        await contributorContext.request.get(
          `/api/connectors/documents/${doc.id}/checkpoint?through_seq=${latest.through_seq + 1}`,
          { headers: { authorization: `Bearer ${key}` } }
        )
      ).status()
    ).toBe(400);
    await contributor.getByRole('button', { name: 'Revoke', exact: true }).click();
    await expect(contributor.getByText('· revoked', { exact: false })).toBeVisible();
    expect(
      (
        await contributorContext.request.get(`/api/connectors/documents/${doc.id}/checkpoint`, {
          headers: { authorization: `Bearer ${key}` }
        })
      ).status()
    ).toBe(401);
  } finally {
    await ownerContext.close().catch(() => {});
    await contributorContext.close().catch(() => {});
  }
});
