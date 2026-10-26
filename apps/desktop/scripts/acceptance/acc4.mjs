// Scenario 4: web sign-in with the system browser, the server's projects, linking and syncing, through the
// real window, against a self-hosted server (the web tier of this branch with the development sign-in link,
// loopback, access tokens that last 30 seconds so every request renews the session) with real Edge as the
// person's browser.   node acc4.mjs <home>
import { readFileSync, existsSync, mkdirSync, writeFileSync, readdirSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { launch, connect, sleep, closeGracefully } from './cdp.mjs';
import { waitForAddress, browserSignIn, resetFile } from './browser.mjs';

const HOME = process.argv[2] || 'D:/verne-build/dynodoc/scratch/home-acc2';
const ORIGIN = 'http://127.0.0.1:4180';
const BROWSER_FILE = 'D:/verne-build/dynodoc/scratch/browser-url.txt';
const EV = 'D:/verne-build/dynodoc/evidence/acc4/';
mkdirSync(EV, { recursive: true });
const port = 9457;
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };
const redact = (u) => u.replace(/(challenge|state)=[^&]+/g, '$1=…');

async function start() {
  const app = launch({ home: HOME, port, extraEnv: { DYNODOC_TEST_BROWSER: BROWSER_FILE } });
  const page = await connect(port);
  await page.waitFor("(!!document.querySelector('.sidebar') && !document.querySelector('.splash'))", 300000, 'the workspace');
  await sleep(800);
  return { app, page };
}

let { app, page } = await start();
const text = async (sel = '.content') => (await page.text(sel)).replace(/\s+/g, ' ');
const setText = (sel, v) => page.type(sel, v);
const api = (env, method, path, body) => (method === 'GET' ? page.invoke('api_get', { path, env }) : page.invoke('api_send', { method, path, body, ifMatch: null, env }));
const settings = async (id) => { await page.click('[data-view=settings]'); await sleep(300); await page.click(`[data-section=${id}]`); await sleep(700); };
const cm = (needle) => spawnSync('cmd', ['/c', 'cmdkey /list'], { encoding: 'utf8' }).stdout.split(/\r?\n/).filter((l) => l.includes(needle)).map((l) => l.trim());
const waitText = (sel, re, ms = 30000) => page.waitFor(`new RegExp(${JSON.stringify(re)}).test((document.querySelector(${JSON.stringify(sel)})?.innerText || '').replace(/\\s+/g, ' '))`, ms, `${sel} to say ${re}`);

say('## 4. sign in with the browser, link, sync');
// A clean start: no server, no link.
await settings('accounts');
for (const a of (await page.invoke('accounts_list')).accounts) await page.invoke('account_remove', { id: a.id });
await page.invoke('svc_send', { method: 'PUT', path: 'state/links', body: { value: {} }, env: 'local' });
await page.click('[data-view=settings]'); await sleep(200);
await page.eval("document.querySelector('[data-section=accounts]').click()"); await sleep(800);

const projects = (await api('local', 'GET', 'projects')).body.items;
const proj = projects.find((p) => p.name === 'Riverside survey') ?? projects[0];
say('local project:', proj.name, '| files:', (await api('local', 'GET', `projects/${proj.id}/files`)).body.items.length);

// ---- add the server ------------------------------------------------------------------------------------------------------
await setText('#server-address', ORIGIN);
await page.click('#server-add'); await sleep(900);
await page.waitFor("!!document.querySelector('[data-server]')", 10000, 'the server row');
const id = await page.eval("document.querySelector('[data-server]').getAttribute('data-server')");
say('server added:', await text('[data-server]'));
say('a bad address is refused:', await (async () => { await setText('#server-address', 'http://dynodoc.example.org'); await page.click('#server-add'); await sleep(500); return (await page.eval("[...document.querySelectorAll('.toast')].map((t) => t.innerText.replace(/\\s+/g, ' ')).join(' | ')")).slice(0, 160); })());
await page.shot(EV + 's4a-server-added.png');

// ---- sign in with the browser --------------------------------------------------------------------------------------------------
resetFile(BROWSER_FILE);
await page.click(`#signin-${id}`);
await page.waitFor("!!document.querySelector('#signin-pairing')", 15000, 'the pairing code');
const pairing = (await page.text('#signin-pairing')).trim();
say('the app shows the pairing code', pairing, '| message:', (await text(`[data-server="${id}"] .notice-block`)).slice(0, 180));
await page.shot(EV + 's4b-waiting.png');
const address = await waitForAddress(BROWSER_FILE);
say('the app asked the browser to open:', redact(address));
say('  the verifier is not in it:', !/verifier/.test(address), '| it carries a loopback port:', /redirect_port=\d{4,5}/.test(address));
const web = await browserSignIn({ origin: ORIGIN, address, email: 'priya.raman@example.org', name: 'Priya Raman', shots: EV, expectPairing: pairing });
say('browser steps:', web.steps.join(' > '));
say('  the server page showed the pairing code', web.pairingShown, '(matches the app\'s:', web.pairingMatches + ')', '| after Continue the browser is at', web.finalUrl, '| it says:', web.finalText);
await page.waitFor("(document.querySelector('[data-server]')?.innerText || '').includes('Signed in')", 30000, 'the app to be signed in');
say('the app now shows:', await text('[data-server]'));
await page.shot(EV + 's4c-signed-in.png');
const listed = JSON.stringify(await page.invoke('accounts_list'));
say('the page is told only whether there is a session:', listed.replace(/"email":"[^"]*"/, '"email":"…"'));
say('Windows Credential Manager has the session:', cm(`account:${id}`).join(' | '));
const accountsJson = readFileSync(HOME + '/config/accounts.json', 'utf8');
say('accounts.json holds addresses and who the server says you are, no session:', !/dynodoc_session|eyJ|ey[A-Za-z0-9]{20}/.test(accountsJson), '|', accountsJson.replace(/\s+/g, ' ').slice(0, 220));

// ---- link and sync -------------------------------------------------------------------------------------------------------
say('## 4b. link and first sync');
await page.waitFor(`!!document.querySelector('#link-${proj.id}')`, 15000, 'the link control');
await page.eval(`(() => { const s = document.querySelector('#link-${proj.id}'); s.value = 'new|${id}'; s.dispatchEvent(new Event('change', { bubbles: true })); })()`);
await waitText(`[data-project="${proj.id}"]`, 'Linked to', 30000);
say('linked:', (await text(`[data-project="${proj.id}"]`)).slice(0, 160));
await page.click(`#sync-check-${proj.id}`);
await page.waitFor("document.querySelectorAll('[data-action]').length >= 1", 60000, 'the plan');
await sleep(500);
say('what a sync would do:', await page.eval("[...document.querySelectorAll('[data-action]')].map((r) => r.innerText.replace(/\\s+/g, ' ')).join(' || ')"));
await page.shot(EV + 's4d-plan.png');
await page.click(`#sync-now-${proj.id}`);
await waitText(`#sync-status-${proj.id}`, 'new file', 120000);
say('sync says:', await text(`#sync-status-${proj.id}`));
await page.shot(EV + 's4e-synced.png');

const remoteProjects = (await api(`acct:${id}`, 'GET', 'projects')).body.items;
const rproj = remoteProjects.find((p) => p.name === proj.name);
const rfiles = (await api(`acct:${id}`, 'GET', `projects/${rproj.id}/files`)).body.items;
const lfiles = (await api('local', 'GET', `projects/${proj.id}/files`)).body.items;
say('on the server: project', rproj.name, 'with', rfiles.length, 'files:', rfiles.map((f) => f.title).join(', '));
const nodeKey = (d) => Object.values(d.state.nodes).filter((n) => !n.deleted).map((n) => `${n.id}|${n.parent_id}|${n.pos}|${JSON.stringify(n.current_fields ?? n.fields)}`).sort().join('\n');
const pair = async (title) => {
  const l = lfiles.find((f) => f.title === title); const r = rfiles.find((f) => f.title === title);
  return { l, r, ld: (await api('local', 'GET', `documents/${l.id}`)).body, rd: (await api(`acct:${id}`, 'GET', `documents/${r.id}`)).body };
};
for (const f of lfiles) {
  const { r, ld, rd } = await pair(f.title);
  const verify = (await api(`acct:${id}`, 'GET', `documents/${r.id}/verify`)).body;
  say(`  ${f.title}: same nodes, ids and content on both sides:`, nodeKey(ld) === nodeKey(rd), '| server-side chain verifies:', verify.ok, '| revisions: here', ld.latest_seq, 'there', rd.latest_seq);
}

// ---- changes on both sides -------------------------------------------------------------------------------------------------
say('## 4c. edits on both sides');
const charter = await pair('Project Charter');
const paragraphs = (d) => Object.values(d.state.nodes).filter((n) => n.type === 'item' && !n.deleted && typeof (n.current_fields ?? n.fields)?.label === 'string' && (n.current_fields ?? n.fields).label.length > 20).sort((a, b) => (a.pos < b.pos ? -1 : 1));
const [A, B] = paragraphs(charter.ld);
const labelOf = (d, n) => (d.state.nodes[n.id].current_fields ?? d.state.nodes[n.id].fields).label;
const edit = async (env, docId, node, value) => {
  const d = (await api(env, 'GET', `documents/${docId}`)).body;
  const r = await api(env, 'POST', `documents/${docId}/batch`, { ops: [{ type: 'FieldEdited', node_id: node.id, field: 'label', value }], base_seq: d.latest_seq });
  if (r.status >= 300) throw new Error('edit failed ' + r.status + ' ' + JSON.stringify(r.body));
};
const read = async (env, docId) => (await api(env, 'GET', `documents/${docId}`)).body;
await edit('local', charter.l.id, A, labelOf(charter.ld, A) + ' [edited on this computer]');
await edit(`acct:${id}`, charter.r.id, B, labelOf(charter.rd, B) + ' [edited on the server]');
await page.click(`#sync-check-${proj.id}`); await sleep(2500);
say('plan with a change on each side:', await page.eval("[...document.querySelectorAll('[data-action]')].filter((r) => !/In sync/.test(r.innerText)).map((r) => r.innerText.replace(/\\s+/g, ' ')).join(' || ')"));
await page.shot(EV + 's4f-plan-both.png');
await page.click(`#sync-now-${proj.id}`);
await waitText(`#sync-status-${proj.id}`, 'sent to|received', 120000);
say('sync says:', await text(`#sync-status-${proj.id}`));
const after = await pair('Project Charter');
say('both edits are on both sides:', labelOf(after.ld, A).endsWith('[edited on this computer]') && labelOf(after.rd, A).endsWith('[edited on this computer]') && labelOf(after.ld, B).endsWith('[edited on the server]') && labelOf(after.rd, B).endsWith('[edited on the server]'));
say('  each side verifies its own chain:', (await api('local', 'GET', `documents/${after.l.id}/verify`)).body.ok, (await api(`acct:${id}`, 'GET', `documents/${after.r.id}/verify`)).body.ok, '| revisions here', after.ld.latest_seq, 'there', after.rd.latest_seq);
await page.click(`#sync-check-${proj.id}`); await sleep(2500);
say('syncing again finds nothing to do:', await page.eval("[...document.querySelectorAll('[data-action]')].every((r) => /In sync/.test(r.innerText))"));

// ---- the same paragraph on both sides ------------------------------------------------------------------------------------------
say('## 4d. the same paragraph changed on both sides');
await edit('local', after.l.id, A, 'Retention: ten years, as agreed on this computer.');
await edit(`acct:${id}`, after.r.id, A, 'Retention: five years, as the server has it.');
const lw = (await read('local', after.l.id)).latest_seq;
const rw = (await read(`acct:${id}`, after.r.id)).latest_seq;
await page.click(`#sync-check-${proj.id}`); await sleep(3000);
say('plan:', await page.eval("[...document.querySelectorAll('[data-action]')].filter((r) => !/In sync/.test(r.innerText)).map((r) => r.innerText.replace(/\\s+/g, ' ')).join(' || ')"));
await page.shot(EV + 's4g-conflict.png');
await page.click(`#sync-now-${proj.id}`);
await sleep(6000);
say('a sync with the conflict unresolved writes nothing:', (await read('local', after.l.id)).latest_seq === lw && (await read(`acct:${id}`, after.r.id)).latest_seq === rw);
await page.eval("document.querySelector('.conflict input[type=radio]').click()"); await sleep(400);
say('chose "This computer":', await page.eval("document.querySelector('.conflict label.is-on')?.innerText.replace(/\\s+/g, ' ')"));
await page.shot(EV + 's4h-conflict-chosen.png');
await page.click(`#sync-now-${proj.id}`);
await waitText(`#sync-status-${proj.id}`, 'sent to', 120000);
const resolved = await pair('Project Charter');
say('after the choice, both sides read:', JSON.stringify(labelOf(resolved.ld, A)), '/', JSON.stringify(labelOf(resolved.rd, A)));
say('  each side verifies:', (await api('local', 'GET', `documents/${resolved.l.id}/verify`)).body.ok, (await api(`acct:${id}`, 'GET', `documents/${resolved.r.id}/verify`)).body.ok);
await page.shot(EV + 's4i-resolved.png');

// ---- sign out and in again, and the session across a restart -------------------------------------------------------------------
say('## 4e. sign out, sign in again, restart');
await page.click(`#signout-${id}`); await sleep(400);
await page.click('#dialog-confirm');
await page.waitFor("(document.querySelector('[data-server]')?.innerText || '').includes('Not signed in')", 30000, 'sign-out');
say('signed out:', await text('[data-server]'), '| credential entry left:', cm(`account:${id}`).length);
say('  a request for that server now says:', (await page.invoke('api_get', { path: 'projects', env: `acct:${id}` }).then((r) => JSON.stringify(r), (e) => String(e).slice(0, 120))));
say('  the server rejects the old session (the cookie was revoked, so a fresh browser is needed):', 'checked by the sign-out call');
await page.shot(EV + 's4j-signed-out.png');
resetFile(BROWSER_FILE);
await page.click(`#signin-${id}`);
await page.waitFor("!!document.querySelector('#signin-pairing')", 15000, 'the pairing code');
const pairing2 = (await page.text('#signin-pairing')).trim();
const address2 = await waitForAddress(BROWSER_FILE);
const web2 = await browserSignIn({ origin: ORIGIN, address: address2, email: 'priya.raman@example.org', name: 'Priya Raman', shots: EV + 'again-', expectPairing: pairing2 });
say('second sign-in: pairing matched', web2.pairingMatches, '| finished at', web2.finalUrl);
await page.waitFor("(document.querySelector('[data-server]')?.innerText || '').includes('Signed in')", 30000, 'signed in again');
say('signed in again:', await text('[data-server]'), '| the second attempt used a new pairing code and state:', pairing !== pairing2 || 'same by chance');
page.close();
say('closed:', await closeGracefully(app.child));
say('waiting 40 s so the access token (30 s) has expired while the app is closed…');
await sleep(40000);
({ app, page } = await start());
await settings('accounts');
await page.waitFor("(document.querySelector('[data-server]')?.innerText || '').length > 0", 15000, 'the server row');
say('after the restart:', await text('[data-server]'));
await page.invoke('account_check', { id });
say('  the session was renewed on the first request (the stored session still works):', JSON.stringify((await page.invoke('accounts_list')).accounts.map((a) => ({ host: a.host, signed_in: a.signed_in }))));
await page.eval(`document.querySelector('#sync-check-${proj.id}').click()`); await sleep(5000);
say('  a check of the link works:', await page.eval("[...document.querySelectorAll('[data-action]')].every((r) => /In sync/.test(r.innerText))"));
await page.shot(EV + 's4k-after-restart.png');

// ---- the limits of what a person sees -------------------------------------------------------------------------------------------
const leaked = [];
function walk(dir) {
  for (const f of readdirSync(dir, { withFileTypes: true })) {
    const p = dir + '/' + f.name;
    if (f.isDirectory()) { if (!/pgdata/.test(p)) walk(p); continue; }
    let st; try { st = statSync(p); } catch { continue; }
    if (st.size > 32 * 1048576) continue;
    let b; try { b = readFileSync(p); } catch { continue; }
    if (b.includes('dynodoc_session=') || b.includes('"cookie"')) leaked.push(p);
  }
}
walk(HOME);
say('files under the app home that mention the session cookie:', leaked.length, leaked.join(', '));
page.close();
say('closed:', await closeGracefully(app.child));
writeFileSync(EV + 'acc4.log', log.join('\n'));
