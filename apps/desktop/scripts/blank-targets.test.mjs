// node --test scripts/blank-targets.test.mjs
// Runs the script Android injects (src-tauri/src/blank_targets.js) against fake DOM nodes.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const source = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'src', 'blank_targets.js'), 'utf8');

/** A node with just what the script touches. */
function node(name, attrs = {}, parent = null) {
  const n = {
    nodeName: name,
    parentNode: parent,
    attrs: { ...attrs },
    log: [],
    hasAttribute(k) { return k in this.attrs; },
    getAttribute(k) { return k in this.attrs ? this.attrs[k] : null; },
    setAttribute(k, v) { this.log.push([k, v]); this.attrs[k] = v; },
  };
  return n;
}

function run() {
  const listeners = [];
  const timers = [];
  const context = {
    document: { addEventListener: (type, fn, capture) => listeners.push({ type, fn, capture }) },
    setTimeout: (fn) => timers.push(fn),
  };
  vm.runInNewContext(source, context);
  return { listeners, timers, flush: () => timers.splice(0).forEach((fn) => fn()) };
}

test('it installs one capturing click listener and nothing else', () => {
  const { listeners } = run();
  assert.equal(listeners.length, 1);
  assert.equal(listeners[0].type, 'click');
  assert.equal(listeners[0].capture, true);
});

test('a tap on a _blank link makes its target _self for that click, then restores it', () => {
  const { listeners, flush } = run();
  for (const original of ['_blank', '_BLANK', '_new']) {
    const link = node('A', { href: 'https://example.org/', target: original });
    const inner = node('SPAN', {}, link);
    let during = null;
    // The browser reads the target while the event is still being dispatched: simulate by
    // reading it right after the listener returns, before any timer runs.
    listeners[0].fn({ target: inner });
    during = link.attrs.target;
    assert.equal(during, '_self', original);
    flush();
    assert.equal(link.attrs.target, original, `${original} restored`);
  }
});

test('other links, other targets and non-links are left alone', () => {
  const { listeners, timers } = run();
  for (const attrs of [{ href: '/a' }, { href: '/a', target: '_self' }, { href: '/a', target: 'frame1' }, { href: '/a', target: '_top' }]) {
    const link = node('A', attrs);
    listeners[0].fn({ target: link });
    assert.deepEqual(link.log, [], JSON.stringify(attrs));
  }
  const noHref = node('A', { target: '_blank' });
  listeners[0].fn({ target: noHref });
  assert.deepEqual(noHref.log, []);
  listeners[0].fn({ target: node('DIV') });
  assert.equal(timers.length, 0);
});

test('a hostile or broken node never throws into the page', () => {
  const { listeners } = run();
  const boom = { get nodeName() { throw new Error('x'); } };
  assert.doesNotThrow(() => listeners[0].fn({ target: boom }));
  assert.doesNotThrow(() => listeners[0].fn({ target: null }));
  assert.doesNotThrow(() => listeners[0].fn({}));
  // A parent chain that never ends is cut off.
  const loop = node('DIV');
  loop.parentNode = loop;
  assert.doesNotThrow(() => listeners[0].fn({ target: loop }));
});

test('with no document nothing is installed and nothing throws', () => {
  assert.doesNotThrow(() => vm.runInNewContext(source, {}));
});

test('the script holds no secret and does not open anything', () => {
  const code = source.split('\n').filter((l) => !l.trim().startsWith('//')).join('\n');
  for (const banned of ['__GESTURE_SECRET__', 'window.open', 'open(', 'invoke', '__TAURI', 'fetch(', 'location', 'eval(']) {
    assert.ok(!code.includes(banned), banned);
  }
});
