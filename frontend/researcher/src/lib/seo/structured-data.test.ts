import { expect, it } from 'vitest';
import { scriptTag } from './structured-data';

it('keeps HTML-looking values inside JSON-LD without creating executable elements', () => {
  const value = { name: '</script><script>alert(1)</script>', nested: ['<img src=x>'] };
  const template = document.createElement('template');
  template.innerHTML = scriptTag(value);
  expect(template.content.children).toHaveLength(1);
  const script = template.content.querySelector('script')!;
  expect(script.type).toBe('application/ld+json');
  expect(script.textContent).not.toContain('<');
  expect(JSON.parse(script.textContent!)).toEqual(value);
});
