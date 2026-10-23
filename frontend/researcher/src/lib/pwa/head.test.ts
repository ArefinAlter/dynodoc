import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { isAppRoute } from './head';

describe('which pages advertise the web app', () => {
  it('leaves the manifest and Home Screen metas off every marketing page', () => {
    for (const id of [
      '/(site)',
      '/(site)/features',
      '/(site)/compatibility',
      '/(site)/open-source',
      '/(site)/privacy'
    ])
      expect(isAppRoute(id), id).toBe(false);
  });

  it('keeps them on the sign-in page and the workspace', () => {
    for (const id of [
      '/login',
      '/workspace',
      '/projects',
      '/projects/[projectId]',
      '/documents/[documentId]',
      '/profile',
      '/admin'
    ])
      expect(isAppRoute(id), id).toBe(true);
  });

  it('treats an error page without a route like an app page, as the bar colour does', () => {
    expect(isAppRoute(null)).toBe(true);
    expect(isAppRoute(undefined)).toBe(true);
  });
});

// The tags are written once, in the root layout, behind that check. A copy left in the HTML
// template would reach every page again, marketing pages included.
describe('where the install tags are written', () => {
  const root = process.cwd();
  const template = readFileSync(resolve(root, 'src/app.html'), 'utf8');
  const layout = readFileSync(resolve(root, 'src/routes/+layout.svelte'), 'utf8');
  const TAGS = [
    'rel="manifest"',
    'name="application-name"',
    'name="apple-mobile-web-app-title"',
    'name="apple-mobile-web-app-capable"',
    'name="mobile-web-app-capable"',
    'name="apple-mobile-web-app-status-bar-style"'
  ];

  it('keeps them out of the HTML template', () => {
    for (const tag of TAGS) expect(template, tag).not.toContain(tag);
  });

  it('writes each of them in the layout, inside the app-route condition', () => {
    const block = layout.slice(layout.indexOf('{#if isAppRoute('));
    expect(block.length).toBeGreaterThan(0);
    const end = block.indexOf('{/if}');
    for (const tag of TAGS) {
      expect(layout, tag).toContain(tag);
      expect(block.indexOf(tag), tag).toBeGreaterThan(-1);
      expect(block.indexOf(tag), tag).toBeLessThan(end);
    }
  });

  it('keeps the icons in the template, which every page may show', () => {
    expect(template).toContain('rel="apple-touch-icon"');
    expect(template).toContain('rel="icon"');
  });
});
