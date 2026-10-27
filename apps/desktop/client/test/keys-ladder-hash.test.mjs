// Keybindings (combination strings and conflicts), the provider ladder's checks, and SHA-256.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_BINDINGS, comboOf, normalize, conflicts, usable, commandFor } from '../src/lib/keys.ts';
import { validate, active, keyHint, PROVIDERS } from '../src/lib/ladder.ts';
import { sha256, chainHash, GENESIS } from '../src/lib/sha256.ts';

const ev = (key, o = {}) => ({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...o });

test('the default bindings have no conflict and use GitHub Desktop’s keys', () => {
  assert.deepEqual(conflicts(DEFAULT_BINDINGS), []);
  const by = Object.fromEntries(DEFAULT_BINDINGS.map((b) => [b.command, b.combo]));
  assert.equal(by['view.changes'], 'Ctrl+1');
  assert.equal(by['view.history'], 'Ctrl+2');
  assert.equal(by['repo.list'], 'Ctrl+T');
  assert.equal(by['branch.list'], 'Ctrl+B');
  assert.equal(by['sync'], 'Ctrl+Shift+P');
  assert.equal(by['branch.new'], 'Ctrl+Shift+N');
  assert.ok(!Object.values(by).includes('Ctrl+R'), 'Ctrl+R is the window’s Reload');
});

test('comboOf names a key event; Cmd counts as Ctrl; Shift+digit uses the physical key', () => {
  assert.equal(comboOf(ev('k', { ctrlKey: true })), 'Ctrl+K');
  assert.equal(comboOf(ev('K', { metaKey: true })), 'Ctrl+K');
  assert.equal(comboOf(ev('P', { ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+P');
  assert.equal(comboOf(ev('!', { ctrlKey: true, shiftKey: true, code: 'Digit1' })), 'Ctrl+Shift+1');
  assert.equal(comboOf(ev('Shift', { shiftKey: true })), '');
  assert.equal(comboOf(ev('Enter', { ctrlKey: true })), 'Ctrl+Enter');
});

test('normalize puts modifiers in one order', () => {
  assert.equal(normalize('shift+ctrl+p'), 'Ctrl+Shift+P');
  assert.equal(normalize('Cmd+K'), 'Ctrl+K');
  assert.equal(normalize('Ctrl+Shift'), '');
});

test('conflicts finds a combination used twice and a reserved one', () => {
  const list = [...DEFAULT_BINDINGS.map((b) => ({ ...b }))];
  list.find((b) => b.command === 'fetch').combo = 'ctrl+1';
  list.find((b) => b.command === 'settings').combo = 'Ctrl+R';
  const c = conflicts(list);
  assert.deepEqual(c.find((x) => x.combo === 'Ctrl+1').commands.sort(), ['fetch', 'view.changes']);
  assert.ok(c.find((x) => x.combo === 'Ctrl+R').commands.includes('(reserved)'));
});

test('usable needs a modifier or a function key, and refuses reserved keys', () => {
  assert.equal(usable('Ctrl+J'), true);
  assert.equal(usable('F5'), true);
  assert.equal(usable('J'), false);
  assert.equal(usable('Ctrl+C'), false);
  assert.equal(usable('Ctrl+R'), false);
});

test('commandFor maps an event to its command', () => {
  assert.equal(commandFor(DEFAULT_BINDINGS, ev('2', { ctrlKey: true, code: 'Digit2' })), 'view.history');
  assert.equal(commandFor(DEFAULT_BINDINGS, ev('q', { ctrlKey: true })), null);
});

const entry = (kind, o = {}) => {
  const p = PROVIDERS.find((x) => x.kind === kind);
  return { id: kind, kind, enabled: true, model: p.model, base: p.base, keyHint: 'abcd', ...o };
};

test('the ladder lists the providers the brief names', () => {
  assert.deepEqual(PROVIDERS.map((p) => p.kind), ['anthropic', 'openai', 'google', 'mistral', 'deepseek', 'qwen', 'kimi', 'glm', 'ollama', 'compat']);
});

test('ladder checks: keys, plain http only to this computer, no credentials in the address', () => {
  assert.deepEqual(validate([entry('anthropic')]), []);
  assert.match(validate([entry('openai', { keyHint: '' })])[0].message, /needs a key/);
  assert.deepEqual(validate([entry('ollama', { keyHint: '' })]), []);
  assert.match(validate([entry('ollama', { base: 'http://10.0.0.5:11434' })])[0].message, /Plain http/);
  assert.match(validate([entry('openai', { base: 'https://u:p@api.example.org/v1' })])[0].message, /No user name/);
  assert.match(validate([entry('openai', { base: 'not a url' })])[0].message, /not a URL/);
  assert.match(validate([entry('openai', { model: 'bad model!' })])[0].message, /model id/);
});

test('the active entry is the first enabled one without issues', () => {
  const list = [entry('openai', { enabled: false }), entry('anthropic', { keyHint: '' }), entry('ollama', { keyHint: '' }), entry('google')];
  assert.equal(active(list)?.kind, 'ollama');
  assert.equal(active([entry('openai', { enabled: false })]), null);
});

test('keyHint shows four characters of a long key and nothing of a short one', () => {
  assert.equal(keyHint('sk-test-0123456789abcd'), 'abcd');
  assert.equal(keyHint('short'), '····');
  assert.equal(keyHint('   '), '');
});

test('SHA-256 matches the standard vectors', () => {
  assert.equal(sha256(''), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  assert.equal(sha256('abc'), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  assert.equal(sha256('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq'), '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1');
  assert.equal(sha256('a'.repeat(1000)), '41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3');
  assert.equal(sha256('héllo বাংলা'), sha256(new TextEncoder().encode('héllo বাংলা')));
});

test('the chain rule links each hash to the one before it', () => {
  const a = chainHash(GENESIS, sha256('one'));
  const b = chainHash(a, sha256('two'));
  assert.notEqual(a, b);
  assert.equal(b, sha256(`${a}:${sha256('two')}`));
});
