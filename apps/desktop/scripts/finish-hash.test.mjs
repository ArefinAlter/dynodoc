// node --test scripts/finish-hash.test.mjs
// Runs the script the shell hands to the sign-in finish page (src-tauri/src/finish_hash.js, with its
// two placeholders filled in as signin.rs `finish_script` fills them) against a fake page.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const template = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri', 'src', 'finish_hash.js'),
  'utf8'
);

const ORIGIN = 'https://app.dynodoc.online';
const CODE = 'ab'.repeat(32);
const FRAGMENT = `code=${CODE}&state=b3JkZXJseS1zdGF0ZS12YWx1ZQ&verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk`;

const script = (origin = ORIGIN, fragment = FRAGMENT) =>
  template.replace('__ORIGIN__', origin).replace('__FRAGMENT__', fragment);

/** A page with what the script touches. `ready` says when the page marks itself ready. */
function page({ origin = ORIGIN, pathname = '/app-signin/finish', search = '', hash = '', readyAfter = 0, framed = false } = {}) {
  const writes = [];
  const timers = [];
  let polls = 0;
  const location = {
    origin,
    pathname,
    search,
    get hash() {
      return hash;
    },
    set hash(value) {
      writes.push(value);
      hash = value;
    }
  };
  const document = {
    documentElement: {
      getAttribute(name) {
        assert.equal(name, 'data-finish-ready');
        polls += 1;
        return polls > readyAfter ? '1' : null;
      }
    }
  };
  const context = { location, document, setTimeout: (fn, ms) => timers.push({ fn, ms }) };
  // `window.top === window` in a top-level page; in a frame `top` is the parent's window.
  context.window = context;
  context.top = framed ? { someOtherWindow: true } : context;
  return {
    location,
    writes,
    timers,
    polls: () => polls,
    run: (source = script()) => vm.runInNewContext(source, context),
    /** Run the timers one round at a time until none is left (or `limit` rounds). */
    drain(limit = 500) {
      let rounds = 0;
      while (timers.length && rounds < limit) {
        timers.splice(0).forEach(({ fn }) => fn());
        rounds += 1;
      }
      return rounds;
    }
  };
}

test('on the finish page of the right origin, once the page is ready, it sets the fragment', () => {
  const p = page();
  p.run();
  assert.deepEqual(p.writes, [FRAGMENT]);
  assert.equal(p.timers.length, 0);
});

test('it waits for the page to say it is ready, and sets the fragment only then', () => {
  const p = page({ readyAfter: 7 });
  p.run();
  assert.deepEqual(p.writes, [], 'not before');
  const rounds = p.drain();
  assert.deepEqual(p.writes, [FRAGMENT]);
  assert.equal(rounds, 7);
  assert.equal(p.timers.length, 0, 'it stops polling once done');
});

test('it gives up after about five seconds and never sets anything', () => {
  const p = page({ readyAfter: Infinity });
  p.run();
  const rounds = p.drain();
  assert.deepEqual(p.writes, []);
  assert.equal(rounds, 99, 'fewer than a hundred tries at fifty milliseconds each');
  const waits = page({ readyAfter: Infinity });
  waits.run();
  assert.ok(waits.timers.every((t) => t.ms === 50));
});

test('any other page gets nothing, not even a poll of the page', () => {
  const wrong = [
    { origin: 'https://evil.example' },
    { origin: 'https://app.dynodoc.online:8443' },
    { origin: 'http://app.dynodoc.online' },
    { origin: 'https://app.dynodoc.online.evil.example' },
    { pathname: '/app-signin/finish/' },
    { pathname: '/app-signin' },
    { pathname: '/workspace' },
    { pathname: '/' },
    { pathname: '/app-signin/finish/x' },
    { search: '?x=1' },
    { search: '?' },
    { hash: '#code=old' },
    { hash: '#' },
    { origin: 'null' },
    { origin: 'http://tauri.localhost' }
  ];
  for (const options of wrong) {
    const p = page({ ...options, readyAfter: 0 });
    p.run();
    p.drain();
    assert.deepEqual(p.writes, [], JSON.stringify(options));
    assert.equal(p.polls(), 0, `it did not even look: ${JSON.stringify(options)}`);
  }
});

test('inside a frame it does nothing, not even a poll of the page', () => {
  const p = page({ framed: true, readyAfter: 0 });
  p.run();
  p.drain();
  assert.deepEqual(p.writes, []);
  assert.equal(p.polls(), 0);
  assert.equal(p.timers.length, 0);
});

test('a window sent elsewhere while it waits is left alone', () => {
  for (const move of [
    (l) => (l.pathname = '/workspace'),
    (l) => (l.origin = 'https://evil.example'),
    (l) => (l.search = '?x=1'),
    (l) => (l.hash = '#other')
  ]) {
    const p = page({ readyAfter: 3 });
    p.run();
    p.timers.splice(0).forEach(({ fn }) => fn());
    // The window moved on before the finish page said it was ready.
    p.writes.length = 0;
    move(p.location);
    const before = p.writes.length;
    p.drain();
    // Whatever the move itself wrote (a hash), the script wrote nothing after it.
    assert.equal(p.writes.length, before, String(move));
    assert.ok(!p.writes.includes(FRAGMENT));
  }
});

test('it is made only of the template and the values: nothing else is read, sent or run', () => {
  const code = template
    .split('\n')
    .filter((line) => !line.trim().startsWith('//'))
    .join('\n');
  for (const banned of [
    'fetch(',
    'XMLHttpRequest',
    'postMessage',
    'eval(',
    'Function(',
    'innerHTML',
    '__TAURI',
    'invoke',
    'cookie',
    'localStorage',
    'console.',
    'location.href',
    'location.assign',
    'location.replace',
    'window.open'
  ]) {
    assert.ok(!code.includes(banned), banned);
  }
  assert.equal(template.split('__ORIGIN__').length - 1, 1);
  assert.equal(template.split('__FRAGMENT__').length - 1, 1);
  assert.equal(code.split('location.hash =').length - 1, 1);
});

test('the filled-in script has no placeholder left and carries only the two values', () => {
  const filled = script();
  assert.ok(!filled.includes('__ORIGIN__') && !filled.includes('__FRAGMENT__'));
  assert.ok(filled.includes(`var origin = '${ORIGIN}';`));
  assert.ok(filled.includes(`var fragment = '${FRAGMENT}';`));
});
