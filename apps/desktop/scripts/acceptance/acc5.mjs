// Scenario 5 (and the agent half of 6): the BYOK agent through the real window, against a mock provider on
// loopback that speaks the three wire styles. Uses the scratch home of the earlier runs (the project, three
// files and their history are there); makes one open change request from a real edit, then drives Settings,
// Providers and the Agent screen the way a person does.
//   node acc5.mjs <home>
import { readFileSync, existsSync, mkdirSync, readdirSync, statSync, writeFileSync, copyFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { launch, connect, sleep, closeGracefully } from './cdp.mjs';

const HOME = process.argv[2] || 'D:/verne-build/dynodoc/scratch/home-acc2';
const EV = 'D:/verne-build/dynodoc/evidence/acc5/';
mkdirSync(EV, { recursive: true });
const FOLDER = 'D:/verne-build/dynodoc/scratch/watched';
const PY = 'D:/verne-build/dynodoc/scratch/venv/Scripts/python.exe';
const EDIT = 'D:/verne-build/dynodoc/scratch/office-edit/';
const KEY = 'DYNODOC-CANARY-key-ok-0123456789';
const WRONG = 'DYNODOC-CANARY-wrong-key-9876543210';
const MPORT = 9741;
const MLOG = EV + 'mockllm.jsonl';
const port = 9456;
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };

// ---- the mock provider ---------------------------------------------------------------------------------------
const mock = spawn('node', ['D:/verne-build/dynodoc/scratch/drive/mockllm.mjs', String(MPORT), KEY, MLOG], { stdio: 'ignore' });
await sleep(800);
const mockLog = () => (existsSync(MLOG) ? readFileSync(MLOG, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)) : []);
const calls = () => mockLog().filter((e) => e.route && e.route !== 'unknown');

async function start() {
  const app = launch({ home: HOME, port });
  const page = await connect(port);
  await page.waitFor("(!!document.querySelector('.sidebar') && !document.querySelector('.splash'))", 300000, 'the workspace');
  await sleep(800);
  return { app, page };
}

let { app, page } = await start();
const text = async (sel = '.content') => (await page.text(sel)).replace(/\s+/g, ' ');
const setText = (sel, v) => page.type(sel, v);
const toast = async () => (await page.eval("[...document.querySelectorAll('.toast')].map((t) => t.innerText.replace(/\\s+/g, ' '))")).join(' | ');
const menuItem = (re) => page.eval(`(() => { const e = [...document.querySelectorAll('.menu-item')].find((e) => ${re}.test(e.querySelector('.menu-label > span')?.textContent || '')); if (!e) return false; e.click(); return true; })()`);
const btn = (label, scope = '.content, .dialog, .topbar') => page.eval(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(scope.split(',').map((s) => s.trim() + ' button').join(','))})].find((b) => b.textContent.trim().startsWith(${JSON.stringify(label)}) && !b.disabled); if (!b) return false; b.click(); return true; })()`);
const need = async (label, scope) => { if (!(await btn(label, scope))) throw new Error('no enabled button: ' + label); };
const settings = async (id) => { await page.click('[data-view=settings]'); await sleep(300); await page.click(`[data-section=${id}]`); await sleep(500); };
const agentView = async () => { await page.click('[data-view=agent]'); await sleep(500); };
const idle = async () => { await sleep(600); await page.waitFor("(() => { const t = document.querySelector('.timeline'); const last = t && t.lastElementChild; return !!document.querySelector('#agent-send:not([disabled])') && !!last && last.classList.contains('msg-agent') && !last.classList.contains('is-streaming') && !last.classList.contains('working'); })()", 60000, 'the agent to finish'); };
const switchOn = (sel) => page.eval(`document.querySelector(${JSON.stringify(sel)}).getAttribute('aria-checked') === 'true'`);
const newThread = async () => { await page.eval("document.querySelector('.agent-threads-head .icon-btn').click()"); await sleep(400); };
const ask = async (q) => { await setText('#composer', q); await page.click('#agent-send'); };
const lastAgent = () => page.eval("(() => { const m = [...document.querySelectorAll('.msg-agent')].pop(); return m ? m.innerText.replace(/\\s+/g, ' ') : ''; })()");
const callsOfLast = () => page.eval("(() => { const m = [...document.querySelectorAll('.msg-agent')].pop(); return m ? [...m.querySelectorAll('.call')].map((c) => (c.querySelector('code')?.textContent || '') + (c.classList.contains('is-refused') ? ' [refused]' : '') + ' :: ' + c.querySelector('.call-out')?.innerText.replace(/\\s+/g, ' ').slice(0, 120)) : []; })()");
const setAccess = async (label) => { await page.eval(`[...document.querySelectorAll('.composer-row .picker')].find((b) => /Read only|Propose/.test(b.textContent)).click()`); await sleep(300); await menuItem(new RegExp('^' + label)); await sleep(300); };

say('## 5. the agent');
// ---- material: one open change request from a real edit of a file in the project folder --------------------
const openNumber = async () => { await page.click('[data-view=requests]'); await sleep(1000); const m = /#(\d+)/.exec(await text()); return m ? Number(m[1]) : null; };
let reqNumber = await openNumber();
say('open request in the project before:', reqNumber);
if (!reqNumber) {
  const docx = FOLDER + '/' + readdirSync(FOLDER).filter((f) => /^Project Charter \(v\d+\)\.docx$/.test(f)).sort().pop();
  const r = spawnSync(PY, [EDIT + 'edit-docx.py', docx, 'ten years', 'eight years', 'Retention reviewed by the steering group.'], { encoding: 'utf8' });
  say('python-docx:', r.stdout.trim());
  await page.click('[data-view=documents]'); await sleep(500);
  await page.waitFor("document.querySelectorAll('.watch-notice').length >= 1 && document.body.innerText.includes('was saved in its editor')", 120000, 'the saved file');
  await page.eval("document.querySelector('.crumbs .link')?.click()"); await sleep(300);
  await page.click('#branch-picker'); await sleep(500);
  if (!(await menuItem(/Edited in Word/))) throw new Error('no draft');
  await sleep(900);
  await page.click('[data-view=history]'); await sleep(700);
  await page.click('.branch-menu-btn'); await sleep(400);
  if (!(await menuItem(/^Send for review/i))) throw new Error('no send-for-review item');
  await sleep(500);
  await setText('#cr-title', 'Retention period in the charter');
  await setText('#cr-note', 'Eight years, as agreed.');
  await need('Open change request', '.dialog'); await sleep(2500);
  reqNumber = await openNumber();
}
const REQ = { number: reqNumber };
say('open request to read: #' + REQ.number);

// ---- Settings, Providers: a key goes to the credential store -----------------------------------------------------------
say('## 5a. providers');
await settings('providers');
await page.eval("[...document.querySelectorAll('[data-entry=lp1] button')].filter((b) => ['Withdraw', 'Forget'].includes(b.textContent.trim())).forEach((b) => b.click())"); await sleep(1000);
await page.shot(EV + 'p0-providers-empty.png');
say('where keys are kept:', (await text()).match(/Keys are kept in [^.]*\./)?.[0]);
await setText('#lb-lp1', `http://127.0.0.1:${MPORT}`);
await setText('#lm-lp1', 'mock-claude');
await setText('#lk-lp1', KEY);
await page.click('#lks-lp1'); await sleep(900);
say('entry 1 after storing:', await page.eval("document.querySelector('[data-entry=lp1] .key-stored')?.innerText.replace(/\\s+/g, ' ')"));
say('the key field is gone from the page, the key is nowhere in it:', !(await page.eval('document.documentElement.outerHTML')).includes(KEY), '| input value cleared:', !(await page.has('#lk-lp1')));
if (!(await switchOn('[data-entry=lp1] button[role=switch]'))) { await page.click('[data-entry=lp1] button[role=switch]'); await sleep(400); }
say('enabled; "in use":', await page.eval("!!document.querySelector('[data-entry=lp1] .pill')"), '| consent line:', await page.eval("document.querySelector('[data-entry=lp1] .ladder-consent')?.innerText.replace(/\\s+/g, ' ')"));
const cm = spawnSync('cmd', ['/c', 'cmdkey /list'], { encoding: 'utf8' }).stdout.split(/\r?\n/).filter((l) => /provider:lp1/i.test(l)).map((l) => l.replace(/^\s+/, ''));
say('Windows Credential Manager target for the key:', cm.join(' | ') || '(none found)');
await page.shot(EV + 'p1-provider-stored.png');
await sleep(1500);
const prefs = readFileSync(HOME + '/config/client-settings.json', 'utf8');
say('preferences file holds no key:', !prefs.includes(KEY) && !prefs.includes('CANARY'), '| holds the last four characters only:', /"keyHint":\s*"6789"/.test(prefs));

// ---- caps and prices are set in the app --------------------------------------------------------------------------------
await setText('#price-model', 'mock-claude'); await setText('#price-in', '3'); await setText('#price-out', '15');
await page.click('#price-save'); await sleep(900);
say('price table:', (await text()).match(/mock-claude\s*\$[\d.]+\s*\$[\d.]+/)?.[0]);

// ---- scenario 5b: a thread with the model: consent, tools, streaming -----------------------------------------------
say('## 5b. a run');
await agentView();
await newThread();
say('model picker:', await text('#model-picker'));
await ask('Summarise the open change requests');
await page.waitFor("!!document.querySelector('#consent-allow')", 30000, 'the consent dialog');
const dlg = await text('.dialog');
await page.shot(EV + 'a1-consent.png');
say('consent dialog says:', dlg.slice(0, 420));
say('  names the host and the tools it sends:', dlg.includes(`127.0.0.1:${MPORT}`), dlg.includes('requests__list'), '| the key is not in it:', !dlg.includes(KEY));
say('  nothing sent before the person allows:', calls().length === 0);
await page.click('#consent-allow');
await idle();
await sleep(500);
say('answer:', (await lastAgent()).slice(0, 600));
say('tool calls shown:', JSON.stringify(await callsOfLast()));
await page.eval("document.querySelector('.msg-agent .calls')?.setAttribute('open', '')");
await page.shot(EV + 'a2-answer.png');
const first = calls();
say('requests the provider received:', first.length, '| first request tools offered (read only):', JSON.stringify(first[0].tools), '| key matched:', first[0].key_ok);
say('  second and third requests carried tool results:', first.map((c) => c.results).join(','));
say('the page cannot reach the provider itself (blocked by the page policy):', await page.eval(`fetch('http://127.0.0.1:${MPORT}/v1/messages', { method: 'POST', body: '{}' }).then(() => 'REACHED', (e) => 'blocked: ' + String(e).slice(0, 60))`), '| provider saw it:', mockLog().filter((e) => e.route === 'unknown' || (e.route === 'anthropic' && e.body_bytes === 2)).length > 0);

// ---- propose (a card a person presses) -----------------------------------------------------------------------------------
say('## 5c. propose, refuse, press');
await newThread();
await ask(`Propose a review comment on #${REQ.number}`);
await idle();
say('read only thread, answer:', (await lastAgent()).slice(0, 260), '| tools it was offered:', JSON.stringify(calls().pop().tools));
await newThread();
await setAccess('Propose');
await ask(`Propose a review comment on #${REQ.number}`);
await idle();
say('propose thread, tools offered:', JSON.stringify(calls().filter((c) => c.prompt?.startsWith('Propose')).pop().tools.slice(-3)));
say('card:', await page.eval("document.querySelector('.agent-card')?.innerText.replace(/\\s+/g, ' ')"));
await page.shot(EV + 'a3-card.png');
await need('Post as you'); await sleep(1800);
say('after pressing Post as you:', await page.eval("document.querySelector('.card-done')?.innerText"), '| toast:', await toast());
await page.click('[data-view=requests]'); await sleep(900);
await page.eval(`[...document.querySelectorAll('.list-row, tr, button')].find((e) => e.innerText.includes('#${REQ.number}'))?.click()`); await sleep(900);
say('the comment is on the request:', (await text()).includes('why the retention period changes'), '| marked as drafted by the agent:', /drafted by the agent|agent/i.test(await text()));
await page.shot(EV + 'a3b-comment-posted.png');
await agentView(); await sleep(300);

await newThread();
await ask(`Merge #${REQ.number} for me`);
await idle();
say('merge ask, tool calls:', JSON.stringify(await callsOfLast()));
say('merge ask, answer:', (await lastAgent()).slice(0, 240));
const stillOpen = (await openNumber()) === REQ.number;
await agentView();
say('the request is still open (the agent could not merge it):', stillOpen);
await page.shot(EV + 'a4-refused.png');

// ---- stop ----------------------------------------------------------------------------------------------------------------------------
say('## 5d. stop');
await newThread();
const n0 = calls().length;
await ask('Write a long answer');
await page.waitFor("(document.querySelector('.msg-agent.is-streaming')?.innerText || '').includes('word3')", 20000, 'streaming text');
say('streaming text arrives as it is produced:', (await lastAgent()).slice(0, 60));
await page.click('#agent-stop');
await idle();
say('after Stop:', (await lastAgent()).slice(-80), '| provider saw the connection close:', await (async () => { await sleep(600); return mockLog().some((e) => e.client_closed_early); })());
await page.shot(EV + 'a5-stopped.png');

// ---- offline, limits, consent ----------------------------------------------------------------------------------------------------
say('## 5e. offline, limits, consent');
const n1 = calls().length;
await settings('providers');
await page.click('button[role=switch][aria-label=Offline]'); await sleep(600);
say('offline on:', await switchOn('button[role=switch][aria-label=Offline]'));
await agentView(); await newThread();
await ask('Summarise the open change requests'); await idle();
say('offline answer:', (await lastAgent()).slice(0, 160), '| provider calls since:', calls().length - n1);
await settings('providers');
await page.click('button[role=switch][aria-label=Offline]'); await sleep(600);
await setText('#cap-thread', '0'); await page.click('#caps-save'); await sleep(900);
await agentView(); await newThread();
await ask('Summarise the open change requests'); await idle();
say('cap 0 answer:', (await lastAgent()).slice(0, 200), '| provider calls since:', calls().length - n1);
await settings('providers');
await setText('#cap-thread', '200000'); await setText('#cap-day', '1'); await page.click('#caps-save'); await sleep(900);
await agentView(); await newThread();
await ask('Summarise the open change requests'); await idle();
say('day cap reached answer:', (await lastAgent()).slice(0, 200), '| provider calls since:', calls().length - n1);
await settings('providers');
say('used in the last 24 hours:', (await page.eval("document.querySelector('#used-tokens')?.innerText")), '| cost:', (await text()).match(/Estimated cost\s*(\$[\d.]+|not estimated)/)?.[1]);
await page.shot(EV + 'p2-limits-usage.png');
await setText('#cap-day', '1000000'); await page.click('#caps-save'); await sleep(900);
await page.eval("[...document.querySelectorAll('[data-entry=lp1] button')].find((b) => b.textContent.trim() === 'Withdraw').click()"); await sleep(700);
say('consent withdrawn:', await page.eval("document.querySelector('[data-entry=lp1] .ladder-consent')?.innerText.replace(/\\s+/g, ' ')"));
await agentView(); await newThread();
await ask('Summarise the open change requests');
await page.waitFor("!!document.querySelector('#consent-allow')", 30000, 'the consent dialog again');
await page.eval("[...document.querySelectorAll('.dialog button')].find((b) => b.textContent.trim() === 'Cancel').click()"); await idle();
say('asked again; cancelled; answer:', (await lastAgent()).slice(0, 120), '| provider calls since:', calls().length - n1);

// ---- a wrong key never shows in an error -----------------------------------------------------------------------------------------------------
say('## 5f. a wrong key that the provider echoes back');
await settings('providers');
await page.eval("[...document.querySelectorAll('[data-entry=lp1] button')].find((b) => b.textContent.trim() === 'Forget').click()"); await sleep(700);
await setText('#lk-lp1', WRONG); await page.click('#lks-lp1'); await sleep(900);
await agentView(); await newThread();
await ask('Summarise the open change requests');
await page.waitFor("!!document.querySelector('#consent-allow')", 30000, 'consent');
await page.click('#consent-allow'); await idle();
const wrongAns = await lastAgent();
say('error shown:', wrongAns.slice(0, 260));
say('the key the provider echoed is not on the screen:', !(await page.eval('document.documentElement.outerHTML')).includes(WRONG), '| provider got a request and rejected it:', mockLog().some((e) => e.rejected));
await page.shot(EV + 'a6-wrong-key.png');
await settings('providers');
await page.eval("[...document.querySelectorAll('[data-entry=lp1] button')].find((b) => b.textContent.trim() === 'Forget').click()"); await sleep(700);
await setText('#lk-lp1', KEY); await page.click('#lks-lp1'); await sleep(900);

// ---- the other two wire styles --------------------------------------------------------------------------------------------------------------
say('## 5g. OpenAI-style (Ollama kind) and Gemini');
await setText('#lb-lp3', `http://127.0.0.1:${MPORT}`); await setText('#lm-lp3', 'mock-llama');
await page.eval("document.querySelector('[data-entry=lp3] button[role=switch]').click()"); await sleep(400);
await page.eval("[...document.querySelectorAll('select')].find((s) => s.getAttribute('aria-label') === 'Add a provider').value = 'google'; [...document.querySelectorAll('select')].find((s) => s.getAttribute('aria-label') === 'Add a provider').dispatchEvent(new Event('change', { bubbles: true }))"); await sleep(600);
const gid = await page.eval("[...document.querySelectorAll('[data-entry]')].map((e) => e.getAttribute('data-entry')).pop()");
await setText(`#lb-${gid}`, `http://127.0.0.1:${MPORT}/v1beta`); await setText(`#lm-${gid}`, 'mock-gemini'); await setText(`#lk-${gid}`, KEY);
await page.click(`#lks-${gid}`); await sleep(900);
await page.eval(`document.querySelector('[data-entry=${gid}] button[role=switch]').click()`); await sleep(400);
await page.shot(EV + 'p3-three-providers.png');
for (const [label, id] of [['Ollama', 'lp3'], ['Google', gid]]) {
  await agentView(); await newThread();
  await page.click('#model-picker'); await sleep(300);
  const picked = await page.eval(`(() => { const e = [...document.querySelectorAll('.menu-item')].find((e) => ${label === 'Ollama' ? '/mock-llama/' : '/mock-gemini/'}.test(e.innerText)); if (!e) return false; e.click(); return true; })()`);
  await sleep(300);
  await ask('Summarise the open change requests');
  await page.waitFor("!!document.querySelector('#consent-allow')", 30000, 'consent');
  await page.click('#consent-allow'); await idle();
  const calls2 = calls().filter((c) => c.route === (label === 'Ollama' ? 'openai' : 'gemini'));
  say(`${label}: picked`, picked, '| requests:', calls2.length, '| tools carried results:', calls2.map((c) => c.results).join(','), '| answer:', (await lastAgent()).slice(0, 180));
  say('  ', label === 'Ollama' ? `bearer header not needed for Ollama: ${!calls2[0].bearer_present}` : `key in header, not in the address: ${calls2[0].key_ok && !calls2[0].key_in_url}`);
}
await page.shot(EV + 'a7-gemini.png');
await agentView();
await settings('providers');
say('used now:', await page.eval("document.querySelector('#used-tokens')?.innerText"));

// ---- persistence, then the leak scan -------------------------------------------------------------------------------------------------------------
say('## 6x. persistence across a restart');
await sleep(1200);
page.close();
say('closed:', await closeGracefully(app.child));
({ app, page } = await start());
await settings('providers');
say('after the restart, entry 1:', await page.eval("document.querySelector('[data-entry=lp1] .key-stored')?.innerText.replace(/\\s+/g, ' ')"), '| consent:', await page.eval("document.querySelector('[data-entry=lp1] .ladder-consent')?.innerText.replace(/\\s+/g, ' ')"));
say('  base and model kept:', await page.eval("document.querySelector('#lb-lp1').value + ' ' + document.querySelector('#lm-lp1').value"), '| price kept:', (await text()).match(/mock-claude\s*\$[\d.]+\s*\$[\d.]+/)?.[0], '| used in 24 h:', await page.eval("document.querySelector('#used-tokens')?.innerText"));
await page.shot(EV + 'p4-after-restart.png');
await agentView(); await newThread();
await ask('Summarise the open change requests'); await idle();
say('a run after the restart works without asking again:', !(await page.has('#consent-allow')), '|', (await lastAgent()).slice(0, 100));
page.close();
say('closed:', await closeGracefully(app.child));

say('## leak scan');
const skip = /pgdata[\\/]/;
let scanned = 0;
const hits = [];
function walk(dir) {
  for (const f of readdirSync(dir, { withFileTypes: true })) {
    const p = dir + '/' + f.name;
    if (f.isDirectory()) { if (!skip.test(p + '/')) walk(p); continue; }
    let st; try { st = statSync(p); } catch { continue; }
    if (st.size > 64 * 1048576) continue;
    let b; try { b = readFileSync(p); } catch { continue; }
    scanned += 1;
    if (b.includes(KEY) || b.includes(WRONG) || b.includes('CANARY')) hits.push(p);
  }
}
walk(HOME);
say('files scanned under the app home (config, logs, web view storage, workspace folders except the database files):', scanned, '| files holding the key:', hits.length, hits.join(', '));
const mockRaw = readFileSync(MLOG, 'utf8');
say('the provider mock logged the key anywhere:', mockRaw.includes(KEY) || mockRaw.includes(WRONG));
const sent = readFileSync(EV + 'mockllm-first-request.json', 'utf8');
say('first request, as the provider saw it: system text begins:', JSON.parse(sent).body.system.slice(0, 120).replace(/\s+/g, ' '), '| tools sent:', JSON.parse(sent).body.tools.length);
mock.kill();
writeFileSync(EV + 'acc5.log', log.join('\n'));
