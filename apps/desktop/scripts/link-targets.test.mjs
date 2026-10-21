// node --test scripts/link-targets.test.mjs
//
// Runs the REAL injected script (src-tauri/src/link_targets.js, with a test secret) in a
// fresh JavaScript realm that has a small browser (events, nodes, anchors with the same brand
// checks and accessors as a browser's), then lets a hostile page script run after it and
// dispatches clicks. The page replaces everything a page can reach: Function.prototype
// call/apply/bind, Reflect, Object, Array, String and RegExp methods, URL, window.open,
// encodeURIComponent, Element.prototype.closest and friends, addEventListener, the accessors
// of Event, Node and HTMLAnchorElement, Error.prepareStackTrace. It records every call.
//
// Asserted: the script makes NO call into anything the page replaced while handling a click;
// the page never sees the secret or the marker address; a fake link (a plain object, or a
// patched getter) cannot start a hand-off; an untrusted event cannot either; and a real
// click on a real link still produces exactly the right marker.
//
// A control run feeds the previous, vulnerable script (which called nativeOpen.call and
// closest) through the same harness and asserts that the harness DOES catch its leak.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { inspect } from 'node:util';
import vm from 'node:vm';

const here = dirname(fileURLToPath(import.meta.url));
const SECRET = '5ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e7abcd';
const MARKER = 'https://gesture.dynodoc.invalid/';
const REAL = readFileSync(join(here, '..', 'src-tauri', 'src', 'link_targets.js'), 'utf8').replace('__GESTURE_SECRET__', SECRET);

// The script as it was before the fix: it looked up .call and closest at click time.
const VULNERABLE = `(function () {
  'use strict';
  var secret = '${SECRET}';
  var nativeOpen = window.open;
  var marker = '${MARKER}';
  document.addEventListener('click', function (event) {
    if (!event.isTrusted || event.defaultPrevented || event.button !== 0) return;
    var node = event.target;
    var link = node && node.closest ? node.closest('a[href]') : null;
    if (!link || link.hasAttribute('download') || typeof link.href !== 'string') return;
    var target;
    try { target = new URL(link.href); } catch (e) { return; }
    var web = target.protocol === 'http:' || target.protocol === 'https:';
    var mail = target.protocol === 'mailto:';
    if (mail || (web && target.origin !== window.location.origin)) {
      event.preventDefault();
      nativeOpen.call(window, marker + secret + '?u=' + encodeURIComponent(link.href), '_blank');
      return;
    }
    if (web && /^_(blank|new)$/i.test(link.getAttribute('target') || '')) {
      event.preventDefault();
      nativeOpen.call(window, link.href, '_blank');
    }
  }, true);
})();`;

// ------------------------------------------------------------------ a small browser
// Classes use private fields as brand checks, as a browser's interfaces do: an accessor
// called on anything that is not the right kind of object throws a TypeError.
const BROWSER = `(function (host) {
  const NativeURL = URL;
  class EventTarget {
    addEventListener(type, fn, capture) { host.register(type, fn, capture); }
  }
  class Node extends EventTarget {
    #node = true; #parent = null; #name;
    constructor(name) { super(); this.#name = name; }
    get parentNode() { if (!(#node in this)) throw new TypeError('Illegal invocation'); return this.#parent; }
    get nodeName() { if (!(#node in this)) throw new TypeError('Illegal invocation'); return this.#name; }
    static adopt(parent, child) { child.#parent = parent; }
  }
  class Element extends Node {
    #element = true; #attrs = Object.create(null);
    constructor(name, attrs) { super(name); for (const key in attrs) this.#attrs[key] = attrs[key]; }
    hasAttribute(name) { if (!(#element in this)) throw new TypeError('Illegal invocation'); return name in this.#attrs; }
    getAttribute(name) { if (!(#element in this)) throw new TypeError('Illegal invocation'); return name in this.#attrs ? this.#attrs[name] : null; }
    closest() { return null; } // a page replaces it; the real script never calls it
  }
  class HTMLAnchorElement extends Element {
    #anchor = true; #href; #protocol; #origin;
    constructor(href, attrs) {
      super('A', href === null ? { ...attrs } : { ...attrs, href });
      if (href !== null) { const u = new NativeURL(href); this.#href = u.href; this.#protocol = u.protocol; this.#origin = u.origin; }
      else { this.#href = ''; this.#protocol = ':'; this.#origin = 'null'; }
    }
    get href() { if (!(#anchor in this)) throw new TypeError('Illegal invocation'); return this.#href; }
    get protocol() { if (!(#anchor in this)) throw new TypeError('Illegal invocation'); return this.#protocol; }
    get origin() { if (!(#anchor in this)) throw new TypeError('Illegal invocation'); return this.#origin; }
  }
  class Event {
    #event = true; #target; #prevented = false;
    constructor(target, trusted) {
      this.#target = target;
      Object.defineProperty(this, 'isTrusted', { value: trusted, enumerable: true, configurable: false });
    }
    get target() { if (!(#event in this)) throw new TypeError('Illegal invocation'); return this.#target; }
    get defaultPrevented() { if (!(#event in this)) throw new TypeError('Illegal invocation'); return this.#prevented; }
    preventDefault() { if (!(#event in this)) throw new TypeError('Illegal invocation'); this.#prevented = true; }
    static prevented(e) { return e.#prevented; }
  }
  class MouseEvent extends Event {
    #mouse = true; #button;
    constructor(target, trusted, button) { super(target, trusted); this.#button = button; }
    get button() { if (!(#mouse in this)) throw new TypeError('Illegal invocation'); return this.#button; }
  }
  const document = new Node('#document');
  document.addEventListener = EventTarget.prototype.addEventListener;
  globalThis.window = globalThis;
  globalThis.document = document;
  globalThis.location = { origin: 'https://server.example' };
  Object.assign(globalThis, { EventTarget, Node, Element, HTMLAnchorElement, Event, MouseEvent });
  globalThis.open = function open(url, target) { host.opened('' + url, '' + target); return null; };
  globalThis.__make = {
    anchor: (href, attrs) => new HTMLAnchorElement(href, attrs),
    element: (name) => new Element(name, {}),
    child: (parent, child) => { Node.adopt(parent, child); return child; },
    click: (target, trusted = true, button = 0) => new MouseEvent(target, trusted, button),
    prevented: (e) => Event.prevented(e),
  };
})`;

// ------------------------------------------------------------------ a hostile page
// Replaces everything and records every call it receives (this and arguments).
const HOSTILE = `(function (record, options) {
  const reflectApply = Reflect.apply;
  const hook = (owner, name) => {
    const original = owner[name];
    if (typeof original !== 'function') return;
    owner[name] = function () { record(name, this, arguments); return reflectApply(original, this, arguments); };
  };
  const hookAll = (owner, names) => names.forEach((n) => hook(owner, n));
  hookAll(Function.prototype, ['call', 'apply', 'bind']);
  hookAll(Reflect, ['apply', 'construct', 'get', 'getPrototypeOf', 'ownKeys']);
  hookAll(Object, ['getOwnPropertyDescriptor', 'defineProperty', 'getPrototypeOf', 'keys', 'assign', 'create', 'freeze']);
  hookAll(Array.prototype, ['push', 'slice', 'indexOf', 'join', 'concat', 'map', 'forEach', 'includes', 'filter', 'some', 'every', 'pop', 'shift']);
  hookAll(String.prototype, ['toLowerCase', 'toUpperCase', 'concat', 'indexOf', 'startsWith', 'endsWith', 'replace', 'replaceAll', 'split', 'slice', 'substring', 'includes', 'charAt', 'trim', 'match']);
  hookAll(RegExp.prototype, ['test', 'exec']);
  hookAll(JSON, ['stringify', 'parse']);
  hookAll(globalThis, ['encodeURIComponent', 'decodeURIComponent', 'encodeURI', 'parseInt', 'String']);
  Array.prototype[Symbol.iterator] = function* () { record('Symbol.iterator', this, arguments); };
  Object.prototype.toString = function () { record('Object.toString', this, arguments); return '[hostile]'; };
  Object.prototype.valueOf = function () { record('Object.valueOf', this, arguments); return this; };
  const RealURL = URL;
  globalThis.URL = function URL() { record('new URL', this, arguments); return new RealURL(...arguments); };
  hookAll(Element.prototype, ['closest', 'hasAttribute', 'getAttribute']);
  Element.prototype.closest = function () { record('closest', this, arguments); return { href: 'http://127.0.0.1:8080/admin/delete', hasAttribute: () => false, getAttribute: () => null }; };
  hookAll(EventTarget.prototype, ['addEventListener']);
  document.addEventListener = function () { record('document.addEventListener', this, arguments); };
  const lie = (proto, name, value) => Object.defineProperty(proto, name, { configurable: true, get() { record('getter ' + name, this, arguments); return value; } });
  lie(HTMLAnchorElement.prototype, 'href', 'http://127.0.0.1:8080/admin/delete');
  lie(HTMLAnchorElement.prototype, 'protocol', 'http:');
  lie(HTMLAnchorElement.prototype, 'origin', 'https://server.example');
  if (!options.keepRealTarget) lie(Event.prototype, 'target', { href: 'http://127.0.0.1:8080/admin/delete', hasAttribute: () => false, getAttribute: () => null, closest: () => null });
  lie(Event.prototype, 'defaultPrevented', false);
  lie(MouseEvent.prototype, 'button', 0);
  lie(Node.prototype, 'parentNode', null);
  lie(Node.prototype, 'nodeName', 'A');
  Event.prototype.preventDefault = function () { record('preventDefault', this, arguments); };
  Error.prepareStackTrace = function (error, sites) { record('prepareStackTrace', error, sites); return ''; };
  globalThis.open = function () { record('page window.open', this, arguments); return null; };
})`;

// Everything a leak could look like: the secret, the marker address, or a function that
// carries the secret in its source. Walks values handed to the hooks.
function leaks(values, functionToString) {
  const found = [];
  const seen = new Set();
  const visit = (value, path, depth) => {
    if (value === null || value === undefined || depth > 6) return;
    const type = typeof value;
    if (type === 'string') {
      if (value.includes(SECRET) || value.includes('gesture.dynodoc.invalid')) found.push(`${path}: ${value.slice(0, 80)}`);
      return;
    }
    if (type !== 'object' && type !== 'function') return;
    if (seen.has(value)) return;
    seen.add(value);
    if (type === 'function') {
      try {
        const source = functionToString(value);
        if (source.includes(SECRET)) found.push(`${path}: a function whose source holds the secret`);
      } catch { /* not inspectable */ }
    }
    let keys = [];
    try { keys = Reflect.ownKeys(value); } catch { return; }
    for (const key of keys) {
      let item;
      try { item = value[key]; } catch { continue; }
      visit(item, `${path}.${String(key)}`, depth + 1);
    }
  };
  values.forEach((v, i) => visit(v, `#${i}`, 0));
  return found;
}

function world({ script = REAL, withAnchor = true } = {}) {
  const registered = [];
  const opened = [];
  const recorded = [];
  const sandbox = {
    host: {
      register: (type, fn, capture) => registered.push({ type, fn, capture }),
      opened: (url, target) => opened.push({ url, target }),
    },
    recorded,
    URL, // a fresh realm has none; a browser does
  };
  const ctx = vm.createContext(sandbox);
  const functionToString = vm.runInContext('Function.prototype.toString', ctx);
  vm.runInContext(`${BROWSER}(host)`, ctx);
  if (!withAnchor) vm.runInContext('delete globalThis.HTMLAnchorElement', ctx);
  const make = vm.runInContext('__make', ctx);
  return {
    ctx, registered, opened, recorded, make,
    toString: (fn) => functionToString.call(fn),
    inject: () => vm.runInContext(script, ctx),
    hostile: (options = {}) => {
      const record = (name, thisArg, args) => { recorded[recorded.length] = [name, thisArg, args]; };
      vm.runInContext(`${HOSTILE}`, ctx)(record, options);
    },
    dispatch: (event) => { for (const l of registered) l.fn(event); },
  };
}

function tree(w, { href = 'https://other.example/x?y=1&z=%20', attrs = {}, depth = 2 } = {}) {
  const { make } = w;
  const section = make.element('SECTION');
  const link = make.child(section, make.anchor(href, attrs));
  let node = link;
  for (let i = 0; i < depth; i += 1) node = make.child(node, make.element('SPAN'));
  return { section, link, inner: node };
}

// Fixtures and events are made BEFORE the hostile page runs (a real page's DOM exists before
// its script), so that nothing the harness itself does is counted as the script's doing.
const click = (w, target, trusted = true, button = 0) => w.make.click(target, trusted, button);

test('the real script installs one capturing click listener at injection', () => {
  const w = world();
  w.inject();
  assert.equal(w.registered.length, 1);
  assert.equal(w.registered[0].type, 'click');
  assert.equal(w.registered[0].capture, true);
});

test('a real click on a link to another site opens exactly the marker, even against a hostile page', () => {
  for (const hostile of [false, true]) {
    const w = world();
    w.inject();
    const t = tree(w);
    const event = click(w, t.inner);
    if (hostile) w.hostile();
    const before = w.recorded.length;
    w.dispatch(event);
    const expected = `${MARKER}${SECRET}?u=${encodeURIComponent('https://other.example/x?y=1&z=%20')}`;
    assert.deepEqual(w.opened, [{ url: expected, target: '_blank' }], `hostile=${hostile}`);
    assert.equal(w.make.prevented(event), true);
    assert.equal(w.recorded.length, before, 'the script called nothing the page replaced');
    assert.deepEqual(leaks(w.recorded.flat(2), w.toString), []);
  }
});

test('a mail link is marked too, and its text is passed through unchanged', () => {
  const w = world();
  w.inject();
  const link = w.make.anchor('mailto:support@dynodoc.online?subject=a%20b');
  const event = click(w, link);
  w.hostile();
  const before = w.recorded.length;
  w.dispatch(event);
  assert.equal(w.opened.length, 1);
  assert.equal(w.opened[0].url, `${MARKER}${SECRET}?u=${encodeURIComponent('mailto:support@dynodoc.online?subject=a%20b')}`);
  assert.equal(w.recorded.length, before);
  assert.deepEqual(leaks(w.recorded.flat(2), w.toString), []);
});

test('a hostile page that lies through patched getters and closest cannot choose what opens', () => {
  const w = world();
  w.inject();
  const button = w.make.child(w.make.element('SECTION'), w.make.element('BUTTON'));
  const same = tree(w, { href: 'https://server.example/projects/1' });
  const real = tree(w, { href: 'https://other.example/real' });
  const events = [click(w, button), click(w, same.inner), click(w, real.inner)];
  w.hostile(); // every anchor now "says" http://127.0.0.1:8080/admin/delete to page code
  const before = w.recorded.length;
  // 1. A real click on a harmless button: nothing opens, though closest() would return a link.
  w.dispatch(events[0]);
  assert.deepEqual(w.opened, []);
  // 2. A real click on a real same-site link: nothing opens (no target), whatever the page's
  //    getters say about it.
  w.dispatch(events[1]);
  assert.deepEqual(w.opened, []);
  // 3. A real click on a real link to another site opens THAT link, not the page's lie.
  w.dispatch(events[2]);
  assert.equal(w.opened.length, 1);
  assert.equal(w.opened[0].url, `${MARKER}${SECRET}?u=${encodeURIComponent('https://other.example/real')}`);
  assert.ok(!w.opened[0].url.includes('127.0.0.1'));
  assert.equal(w.recorded.length, before, 'no call into anything the page replaced');
});

test('a fake link object cannot start a hand-off', () => {
  const w = world();
  w.inject();
  const real = w.make.element('SPAN');
  const realEvent = click(w, real);
  w.hostile();
  const fakeLink = { href: 'http://127.0.0.1:8080/admin', protocol: 'http:', origin: 'http://127.0.0.1:8080', nodeName: 'A', parentNode: null, hasAttribute: () => false, getAttribute: () => null };
  const fakeEvent = { isTrusted: true, defaultPrevented: false, button: 0, target: fakeLink, preventDefault() { throw new Error('must not be called'); } };
  w.dispatch(fakeEvent); // brand checks fail inside the script, which stays quiet
  w.dispatch({ ...fakeEvent, target: { ...fakeLink, parentNode: fakeLink } });
  w.dispatch(realEvent); // a real node, but not a link
  assert.deepEqual(w.opened, []);
});

test('an event the page made is not a click', () => {
  const w = world();
  w.inject();
  const t = tree(w);
  const events = [click(w, t.inner, false), click(w, t.inner, true, 1), click(w, t.inner)];
  w.hostile();
  for (const event of events) w.dispatch(event);
  assert.equal(w.opened.length, 1, 'only the trusted, primary-button click opened');
  assert.equal(w.opened[0].url.startsWith(MARKER), true);
});

test('links the script must leave alone are left alone', () => {
  const w = world();
  w.inject();
  const events = [];
  for (const [href, attrs] of [
    ['javascript:alert(1)', {}],
    ['https://other.example/file.pdf', { download: '' }],
    ['https://server.example/x', {}],
    ['data:text/html,hi', {}],
    ['file:///C:/Windows/win.ini', {}],
    [null, {}],
  ]) {
    events.push(click(w, w.make.anchor(href, attrs)));
  }
  w.hostile();
  for (const event of events) w.dispatch(event);
  assert.deepEqual(w.opened, []);
});

test('target=_blank on this site is turned into window.open without the secret', () => {
  for (const target of ['_blank', '_BLANK', '_new']) {
    const w = world();
    w.inject();
    const event = click(w, w.make.anchor('https://server.example/print/1', { target }));
    w.hostile();
    const before = w.recorded.length;
    w.dispatch(event);
    assert.deepEqual(w.opened, [{ url: 'https://server.example/print/1', target: '_blank' }], target);
    assert.equal(w.recorded.length, before);
  }
});

test('the secret is not in anything the page could reach, whatever it asks for', () => {
  const w = world();
  w.inject();
  const events = [0, 1, 2, 3, 4].map((i) => click(w, tree(w, { href: `https://other.example/${i}` }).inner));
  w.hostile();
  for (const event of events) w.dispatch(event);
  assert.equal(w.opened.length, 5);
  const globals = vm.runInContext('globalThis', w.ctx);
  assert.deepEqual(leaks([globals, w.recorded.flat(2)], w.toString), []);
  // Neither the global object nor the prototypes the page holds mention the secret.
  assert.ok(!inspect(globals, { depth: 3, showHidden: true }).includes(SECRET));
});

test('without the DOM interfaces it needs, the script installs nothing and does not throw', () => {
  const w = world({ withAnchor: false });
  assert.doesNotThrow(() => w.inject());
  assert.equal(w.registered.length, 0);
});

test('CONTROL: the previous script leaks its secret to the same hostile page, and the harness sees it', () => {
  const w = world({ script: VULNERABLE });
  w.inject();
  const event = click(w, tree(w).inner);
  w.hostile({ keepRealTarget: true });
  // Page code that merely patched Function.prototype.call sees the secret after one click.
  w.dispatch(event);
  const found = leaks(w.recorded.flat(2), w.toString);
  assert.ok(found.some((f) => f.includes('gesture.dynodoc.invalid')), 'the harness should see the marker leak');
});

test('CONTROL: with the previous script a patched closest() makes any real click open an address the page chose', () => {
  const w = world({ script: VULNERABLE });
  w.inject();
  const event = click(w, w.make.child(w.make.element('SECTION'), w.make.element('BUTTON')));
  w.hostile({ keepRealTarget: true });
  w.dispatch(event);
  assert.equal(w.opened.length, 1);
  assert.ok(decodeURIComponent(w.opened[0].url).includes('127.0.0.1'), "the page's fake link was marked as a click");
});
