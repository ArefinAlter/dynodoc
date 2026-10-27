// Scenario 6: settings persist for real (a file, applied again after a restart), and a backup of the
// whole workspace restores it. Run twice around a restart: node run.mjs <home> acc6.mjs --pick pick-acc6.json
import { readFileSync, existsSync, rmSync, mkdirSync, writeFileSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { launch, connect, sleep, closeGracefully } from './cdp.mjs';

const HOME = process.argv[2] || 'D:/verne-build/dynodoc/scratch/home-acc2';
const EV = 'D:/verne-build/dynodoc/evidence/acc6/';
mkdirSync(EV, { recursive: true });
const BACKUP = 'D:/verne-build/dynodoc/scratch/backups/ws1.zip';
mkdirSync('D:/verne-build/dynodoc/scratch/backups', { recursive: true });
rmSync(BACKUP, { force: true });
writeFileSync('D:/verne-build/dynodoc/scratch/drive/pick-acc6.json', JSON.stringify({ backup_save: [BACKUP], backup_open: [BACKUP] }));
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };
const port = 9455;

async function start() {
  const app = launch({ home: HOME, port, pickFile: 'D:/verne-build/dynodoc/scratch/drive/pick-acc6.json' });
  const page = await connect(port);
  await page.waitFor("(!!document.querySelector('.sidebar') && !document.querySelector('.splash'))", 300000, 'the workspace');
  return { app, page };
}
const text = async (page, sel = '.content') => (await page.text(sel)).replace(/\s+/g, ' ');
const section = async (page, id) => { await page.click('[data-view=settings]'); await sleep(400); await page.click(`[data-section=${id}]`); await sleep(500); };
const settingsFile = HOME + '/config/client-settings.json';

// ---- first run: change settings --------------------------------------------------------------------------------
let { app, page } = await start();
say('## 6a. settings');
await section(page, 'appearance');
await page.eval("[...document.querySelectorAll('.theme-card')].find((b) => /Dark/.test(b.textContent)).click()"); await sleep(600);
await page.eval("document.querySelector('.accent-swatch, [data-accent]')?.click()").catch(() => {});
await section(page, 'versioncontrol');
say('version control options:', (await text(page)).slice(0, 220));
await section(page, 'general');
await sleep(1500);
say('theme applied now:', await page.eval("document.documentElement.getAttribute('data-scheme') || 'system'"));
await page.shot(EV + 'settings-dark.png');
await sleep(1200);
say('settings file written:', existsSync(settingsFile), existsSync(settingsFile) ? statSync(settingsFile).size + ' bytes' : '');
page.close();
say('closed:', await closeGracefully(app.child));

// ---- second run: the settings are still there ---------------------------------------------------------------------
({ app, page } = await start());
await sleep(800);
say('after the restart, theme:', await page.eval("document.documentElement.getAttribute('data-scheme') || 'system'"), '| file:', JSON.parse(readFileSync(settingsFile, 'utf8')).theme);
const prefs = readFileSync(settingsFile, 'utf8');
say('the settings file holds no key:', !/sk-|api_key|apikey|password|secret/i.test(prefs));
await page.shot(EV + 'settings-after-restart.png');

// ---- backup and restore -----------------------------------------------------------------------------------------------
say('## 6b. backup and restore');
const before = (await page.invoke('api_get', { path: 'projects', env: 'local' })).body.items.map((p) => p.name);
say('projects before the backup:', JSON.stringify(before));
await section(page, 'workspace');
await shotOf('workspace');
async function shotOf(n) { await page.shot(EV + n + '.png'); }
await page.click('#backup-now');
await page.waitFor("!document.querySelector('.splash') && document.body.innerText.includes('Backup written')", 240000, 'the backup');
const bytes = existsSync(BACKUP) ? statSync(BACKUP).size : 0;
say('backup written:', existsSync(BACKUP), (bytes / 1048576).toFixed(1) + ' MB');
const z = spawnSync('D:/verne-build/dynodoc/scratch/venv/Scripts/python.exe', ['-I', '-c', "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); n=z.namelist(); print(len(n),'entries;', z.read('manifest.json').decode().replace(chr(10),' ')); print('secret-like entries:', [x for x in n if 'secret' in x.lower() or x.endswith('.key')])", BACKUP], { encoding: 'utf8' });
say(z.stdout.trim());
await sleep(1500);
// A change after the backup: a new project.
const LATER = 'After the backup ' + Date.now();
await page.invoke('api_send', { method: 'POST', path: 'projects', body: { name: LATER, description: '' }, ifMatch: null, env: 'local' });
const mid = (await page.invoke('api_get', { path: 'projects', env: 'local' })).body.items.map((p) => p.name);
say('projects after a new one:', JSON.stringify(mid));
await page.eval("document.querySelector('#restore-now')?.click()"); await sleep(600);
await page.click('#dialog-confirm');
await page.waitFor("!document.querySelector('.splash') && document.body.innerText.includes('Backup restored')", 300000, 'the restore');
await sleep(1500);
const after = (await page.invoke('api_get', { path: 'projects', env: 'local' })).body.items.map((p) => p.name);
say('projects after the restore:', JSON.stringify(after), '| the later project is gone:', !after.includes(LATER));
const mine = (await page.invoke('api_get', { path: 'projects', env: 'local' })).body.items.find((p) => p.name === 'Riverside survey');
const files = (await page.invoke('api_get', { path: `projects/${mine.id}/files`, env: 'local' })).body.items;
for (const f of files) {
  const v = await page.invoke('api_get', { path: `documents/${f.id}/verify`, env: 'local' });
  say('  verify', f.title, JSON.stringify(v.body));
}
await page.shot(EV + 'restored.png');
const kept = spawnSync('cmd', ['/c', 'dir', '/b', HOME.replaceAll('/', String.fromCharCode(92)) + String.fromCharCode(92) + 'data' + String.fromCharCode(92) + 'workspace'], { encoding: 'utf8' }).stdout;
say('workspace folder now:', kept.trim().split(/\r?\n/).join(' | '));
page.close();
say('closed:', await closeGracefully(app.child));
writeFileSync(EV + 'acc6.log', log.join('\n'));
