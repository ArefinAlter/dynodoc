// Smoke test of an INSTALLED release build (the NSIS installer's files), with its data kept in a
// `dynodoc-data` folder beside the program so that nothing touches the real per-user folders. Drives the
// real window through WebView2's remote debugging port; the file dialog is the real Windows dialog,
// answered with UI Automation (the release build has no test seams).
//   node smoke-install.mjs <install folder> <evidence folder>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, existsSync, readdirSync, writeFileSync } from 'node:fs';
import { connect, sleep, closeGracefully } from './cdp.mjs';

const INST = process.argv[2] || 'D:/verne-build/dynodoc/install-test/Dynodoc';
const EV = (process.argv[3] || 'D:/verne-build/dynodoc/evidence/install/').replace(/\\/g, '/');
const SAMPLES = 'D:\\verne-build\\dynodoc\\scratch\\samples\\office';
const here = new URL('.', import.meta.url).pathname.replace(/^\//, '');
mkdirSync(EV, { recursive: true });
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };
const port = 9460;

rmSync(INST + '/dynodoc-data', { recursive: true, force: true });
mkdirSync(INST + '/dynodoc-data', { recursive: true });
const child = spawn(INST + '/dynodoc-desktop.exe', [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: 'ignore',
});
const procs = () => spawnSync('powershell', ['-NoProfile', '-Command', `Get-Process | Where-Object { $_.Path -and $_.Path -like '${INST.replace(/\//g, '\\')}*' } | ForEach-Object { $_.ProcessName }`], { encoding: 'utf8' }).stdout.split(/\r?\n/).filter(Boolean).sort();
let page;
try {
  page = await connect(port);
  await page.waitFor("!!document.querySelector('#profile-name') || (!!document.querySelector('.sidebar') && !document.querySelector('.splash'))", 300000, 'the first screen');
  say('first launch shows:', (await page.text('body')).replace(/\s+/g, ' ').slice(0, 160));
  await page.shot(EV + 'i1-welcome.png');
  await page.type('#profile-name', 'Priya Raman');
  await page.type('#profile-project', 'Riverside survey');
  await page.click('#profile-go');
  await page.waitFor("!!document.querySelector('.sidebar') && !document.querySelector('.splash')", 120000, 'the workspace');
  await sleep(1500);
  say('programs running from the install folder:', procs().join(', '));
  say('topbar:', (await page.text('.topbar')).replace(/\s+/g, ' ').slice(0, 100));
  // Settings, This computer: where the secrets are.
  await page.click('[data-view=settings]'); await sleep(300);
  await page.click('[data-section=workspace]'); await sleep(700);
  say('this computer:', (await page.text('.content')).replace(/\s+/g, ' ').match(/Secrets[^.]*\./)?.[0]);
  await page.shot(EV + 'i2-this-computer.png');
  // Add the three sample files through the real dialog.
  await page.click('[data-view=documents]'); await sleep(500);
  await page.click('#add-files');
  const r = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', here + 'file-dialog.ps1', '-ProcessId', String(child.pid), '-Names', '"Project Charter.docx" "Budget 2027.xlsx" "Quarterly Review.pptx"', '-Folder', SAMPLES], { encoding: 'utf8', timeout: 90000 });
  say('the file dialog:', r.stdout.replace(/\s+/g, ' ').trim(), r.stderr.trim().slice(0, 120));
  await page.waitFor("document.querySelectorAll('table.table tbody tr').length === 3", 120000, 'three files');
  await sleep(1000);
  say('files in the project:', (await page.text('table.table')).replace(/\s+/g, ' ').slice(0, 200));
  await page.shot(EV + 'i3-imported.png');
  const projects = (await page.invoke('api_get', { path: 'projects', env: 'local' })).body.items;
  const files = (await page.invoke('api_get', { path: `projects/${projects[0].id}/files`, env: 'local' })).body.items;
  for (const f of files) {
    const v = (await page.invoke('api_get', { path: `documents/${f.id}/verify`, env: 'local' })).body;
    say('  verify', f.title, JSON.stringify({ ok: v.ok }));
  }
  page.close();
  page = null;
  say('closed the way a person does:', await closeGracefully(child, 90000));
  await sleep(2500);
  say('programs left running from the install folder after closing:', procs().join(', ') || 'none');
  say('data kept beside the program:', readdirSync(INST + '/dynodoc-data').join(', '), '| workspace:', existsSync(INST + '/dynodoc-data/data/workspace/pgdata/PG_VERSION'));
} catch (e) {
  say('FAILED:', e.message);
  if (page) await page.shot(EV + 'i-failed.png').catch(() => {});
  try { child.kill(); } catch {}
  process.exitCode = 1;
} finally {
  writeFileSync(EV + 'install-smoke.log', log.join('\n'));
}
