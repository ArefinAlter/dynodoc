// Acceptance, scenarios 1-3 (and the export half of 2), through the real window and the real
// Word, Excel and PowerPoint. Run on a fresh home:  node run.mjs <home> acc2.mjs --fresh --pick pick-acc.json
import { mkdirSync, readdirSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { officePids, closeNewOffice } from './cdp.mjs';

const FOLDER = 'D:/verne-build/dynodoc/scratch/watched';
const SAMPLES = 'D:/verne-build/dynodoc/scratch/samples/office/';
const PY = 'D:/verne-build/dynodoc/scratch/venv/Scripts/python.exe';
const EDIT = 'D:/verne-build/dynodoc/scratch/office-edit/';
const win = (p) => p.replaceAll('/', '\\');
const log = [];
const say = (...a) => { const l = a.join(' '); console.log(l); log.push(l); };

function ps(mode, path, ...rest) {
  const r = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', win(EDIT + 'office-edit.ps1'), mode, win(path), ...rest], { encoding: 'utf8', timeout: 120000 });
  say('   ', (r.stdout || '').trim().split('\n').pop(), (r.stderr || '').trim().slice(0, 160));
}

export default async function (page, { evidence, sleep, home }) {
  const before = officePids();
  const shot = (n) => page.shot(evidence + n + '.png');
  const text = async (sel = '.content') => (await page.text(sel)).replace(/\s+/g, ' ');
  const btn = (label, scope = '.content, .dialog, .topbar') => page.eval(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(scope.split(',').map((s) => s.trim() + ' button').join(','))})].find((b) => b.textContent.trim().startsWith(${JSON.stringify(label)}) && !b.disabled); if (!b) return false; b.click(); return true; })()`);
  const need = async (label, scope) => { if (!(await btn(label, scope))) throw new Error('no enabled button: ' + label); };
  const menuItem = (re) => page.eval(`(() => { const e = [...document.querySelectorAll('.menu-item')].find((e) => ${re}.test(e.querySelector('.menu-label > span')?.textContent || '')); if (!e) return false; e.click(); return true; })()`);
  const lastToast = async () => (await page.eval("[...document.querySelectorAll('.toast')].map((t) => t.innerText.replace(/\\s+/g, ' '))")).join(' | ');
  const setText = (sel, v) => page.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); const set = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(e), 'value').set; set.call(e, ${JSON.stringify(v)}); e.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  const pickDraft = async (re) => { await page.click('#branch-picker'); await sleep(500); const ok = await menuItem(re); await sleep(900); return ok; };
  const toMain = async () => { await page.click('#branch-picker'); await sleep(500); await menuItem(/^main$/); await sleep(800); };
  const openDoc = async (name) => { await page.click('[data-view=documents]'); await sleep(400); await page.eval("document.querySelector('.crumbs .link')?.click()"); await sleep(300); await page.eval(`[...document.querySelectorAll('table.table tbody tr')].find((r) => r.textContent.includes(${JSON.stringify(name)})).click()`); await sleep(600); await page.eval(`[...document.querySelectorAll('.tabs button, [role=tab]')].find((b) => /^Overview/.test(b.textContent.trim()))?.click()`); await sleep(300); };

  // ---- scenario 1: first launch, profile, project ------------------------------------------------------
  say('## 1. first launch');
  if (await page.has('#profile-name')) {
    await shot('s1-welcome');
    await page.type('#profile-name', 'Priya Raman');
    await page.type('#profile-project', 'Riverside survey');
    await page.click('#profile-go');
    await page.waitFor("!!document.querySelector('.sidebar') && !document.querySelector('.splash')", 90000, 'the workspace');
  }
  await sleep(1200);
  say('topbar:', await text('.topbar'));
  await shot('s1-workspace');

  // ---- scenario 2a: import real files ---------------------------------------------------------------------
  say('## 2a. import');
  await page.click('[data-view=documents]'); await sleep(500);
  await page.click('#add-files');
  await page.waitFor("document.querySelectorAll('table.table tbody tr').length === 3", 120000, 'three files');
  await sleep(1000);
  say('files:', await text('table.table'));
  await shot('s2a-imported');
  for (const [name, hint] of [['Project Charter', 'Riverside Community Health Survey'], ['Budget 2027', 'Interviewer wages'], ['Quarterly Review', 'Pilot interviews']]) {
    await openDoc(name);
    const body = await text();
    say(`  ${name}: preview has "${hint}":`, body.includes(hint), '| blocks listed:', (await page.eval("document.querySelectorAll('.pv-block').length")));
    await shot('s2a-preview-' + name.split(' ')[0].toLowerCase());
  }
  await page.click('[data-view=history]'); await sleep(700);
  await need('Verify every file'); await sleep(2500);
  say('verify:', await lastToast());
  await shot('s2a-verify');

  // ---- scenario 3: folder watch ---------------------------------------------------------------------------
  say('## 3. folder watch');
  rmSync(FOLDER, { recursive: true, force: true });
  mkdirSync(FOLDER, { recursive: true });
  await page.click('[data-view=projects]'); await sleep(500);
  await page.eval("document.querySelector('.project-card').click()"); await sleep(600);
  await page.click('#folder-choose'); await sleep(1500);
  say('project folder:', (await text()).match(/Project folder.{0,120}/)?.[0]);
  await page.click('[data-view=documents]'); await sleep(500);
  for (const name of ['Project Charter', 'Budget 2027', 'Quarterly Review']) {
    await openDoc(name);
    await page.eval(`[...document.querySelectorAll('button')].find((b) => /^Open in /.test(b.textContent.trim())).click()`); await sleep(3000);
  }
  say('written to the folder:', readdirSync(FOLDER).filter((f) => !f.startsWith('~$')).join(' | '));
  say('lock files (the programs hold the copies open):', readdirSync(FOLDER).filter((f) => f.startsWith('~$')).length);
  closeNewOffice(before);
  await sleep(1500);
  const docx = FOLDER + '/Project Charter (v14).docx';
  mkdirSync('D:/verne-build/dynodoc/scratch/keep', { recursive: true });
  copyFileSync(docx, 'D:/verne-build/dynodoc/scratch/keep/charter-r14.docx');
  const r = spawnSync(PY, [EDIT + 'edit-docx.py', docx, 'seven years', 'ten years', 'Amended by the steering group on 3 March 2027.'], { encoding: 'utf8' });
  say('python-docx:', r.stdout.trim());
  ps('word-resave', docx);
  ps('excel-edit', FOLDER + '/Budget 2027 (v21).xlsx', 'B4', '19500');
  ps('ppt-edit', FOLDER + '/Quarterly Review (v12).pptx', '2', 'Pilot interviews: 52 of 60 complete');
  closeNewOffice(before);
  await page.click('[data-view=documents]'); await sleep(500);
  await page.waitFor("document.querySelectorAll('.watch-notice').length >= 3 && document.body.innerText.includes('was saved in its editor')", 120000, 'three saved files');
  say('banners:', await text('.watch-notices'));
  await shot('s3-saved');

  // ---- scenario 2b: drafts, diffs, change requests, comments, merge ----------------------------------------
  say('## 2b. review and merge');
  for (const [name, re] of [['Project Charter', /Edited in Word/], ['Budget 2027', /Edited in Excel/], ['Quarterly Review', /Edited in PowerPoint/]]) {
    const tag = name.split(' ')[0].toLowerCase();
    await page.click('[data-view=documents]'); await sleep(400);
    await page.eval("document.querySelector('.crumbs .link')?.click()"); await sleep(300);
    if (!(await pickDraft(re))) throw new Error('no draft for ' + name);
    await page.click('[data-view=history]'); await sleep(900);
    await shot('s2b-diff-' + tag);
    const diffText = await page.eval("[...document.querySelectorAll('.diff-row.is-add, .diff-row.is-del, .row-add, .row-del, [class*=\"add\"], [class*=\"del\"]')].slice(0, 6).map((e) => e.innerText.replace(/\\s+/g, ' ')).join(' || ')");
    say(`  ${name} diff:`, (diffText || '').slice(0, 300));
    await page.click('.branch-menu-btn'); await sleep(400);
    if (!(await menuItem(/^Send for review/i))) throw new Error('no send-for-review item');
    await sleep(500);
    await setText('#cr-title', `Edits to ${name} from its editor`);
    await setText('#cr-note', 'Saved from the program that opened the copy.');
    await need('Open change request', '.dialog'); await sleep(2500);
    if (name === 'Project Charter') {
      await setText('.content textarea', 'Please confirm the ten years with the data protection officer.');
      await need('Comment'); await sleep(1500);
      say('  review comment shown:', (await text()).includes('data protection officer'));
    }
    await shot('s2b-request-' + tag);
    await need('Merge into main'); await sleep(700);
    await page.click('#dialog-confirm'); await sleep(3000);
    say('  merged:', (await text()).slice(0, 120), '| toast:', await lastToast());
  }
  await toMain();
  await page.click('[data-view=history]'); await sleep(900);
  await need('Verify every file'); await sleep(3000);
  say('verify after merges:', await lastToast());
  await shot('s2b-history-merged');

  // ---- scenario 2d: versions and restore ---------------------------------------------------------------------
  say('## 2d. versions (named version before the conflict)');
  await openDoc('Project Charter');
  await page.eval(`[...document.querySelectorAll('.tabs button, [role=tab]')].find((b) => /^Versions/.test(b.textContent.trim())).click()`); await sleep(600);
  await need('Save named version'); await sleep(500);
  await setText('#ver-name', 'Steering group copy');
  await need('Save version', '.dialog'); await sleep(2500);
  say('  versions:', (await text()).slice(0, 300));
  await shot('s2d-versions');
  // ---- scenario 2c: merge with conflicts ---------------------------------------------------------------------
  say('## 2c. conflicts');
  // A second person's copy of revision 14 (kept before the first edit), edited at the same sentence.
  const copy = FOLDER + '/Project Charter (v14) second copy.docx';
  copyFileSync('D:/verne-build/dynodoc/scratch/keep/charter-r14.docx', copy);
  spawnSync(PY, [EDIT + 'edit-docx.py', copy, 'seven years', 'five years', ''], { encoding: 'utf8' });
  ps('word-resave', copy);
  closeNewOffice(before);
  await page.click('[data-view=documents]'); await sleep(500);
  await page.eval("document.querySelector('.crumbs .link')?.click()"); await sleep(300);
  await page.waitFor("document.body.innerText.includes('second copy.docx was saved in its editor')", 90000, 'the second copy');
  // The notice of the second copy leads to its own draft (the first Word draft carries the same name).
  await page.eval("[...document.querySelectorAll('.watch-notice')].find((n) => n.textContent.includes('second copy.docx')).querySelector('button').click()"); await sleep(1200);
  await page.click('.branch-menu-btn'); await sleep(400);
  await menuItem(/^Send for review/i); await sleep(500);
  await setText('#cr-title', 'Five years of retention');
  await setText('#cr-note', 'A second reviewer changed the same sentence.');
  await need('Open change request', '.dialog'); await sleep(2500);
  say('  request text:', (await text()).slice(0, 400));
  await shot('s2c-conflict');
  say('  conflicts card:', await page.eval("(document.querySelector('.conflicts-card')?.innerText || 'none').replace(/\\s+/g, ' ').slice(0, 600)"));
  await need('Use all of theirs'); await sleep(500);
  await need('Merge into main'); await sleep(700);
  await page.click('#dialog-confirm'); await sleep(3000);
  say('  after the conflict merge:', (await text()).slice(0, 160), '| toast:', await lastToast());
  await toMain();
  await openDoc('Project Charter');
  say('  main now reads (their version chosen):', (await text()).match(/retained for [a-z]+ years/)?.[0]);
  await shot('s2c-after');

  // ---- scenario 2d (second half): restore the saved version as new changes, review, merge ------------------------
  say('## 2d. restore');
  await openDoc('Project Charter');
  await page.eval(`[...document.querySelectorAll('.tabs button, [role=tab]')].find((b) => /^Versions/.test(b.textContent.trim())).click()`); await sleep(600);
  await need('Restore as new changes'); await sleep(2500);
  say('  restore toast:', await lastToast());
  if (!(await pickDraft(/restore-steering/))) throw new Error('no restore draft');
  await page.click('[data-view=history]'); await sleep(900);
  await shot('s2d-restore-diff');
  await page.click('.branch-menu-btn'); await sleep(400);
  await menuItem(/^Send for review/i); await sleep(500);
  await setText('#cr-title', 'Restore the steering group copy');
  await setText('#cr-note', 'Back to ten years.');
  await need('Open change request', '.dialog'); await sleep(2500);
  await need('Merge into main'); await sleep(700);
  await page.click('#dialog-confirm'); await sleep(3000);
  say('  restore merged:', await lastToast());
  await toMain();
  await openDoc('Project Charter');
  say('  main reads after the restore:', (await text()).match(/retained for [a-z]+ years/)?.[0]);
  await page.click('[data-view=history]'); await sleep(900);
  await need('Verify every file'); await sleep(3000);
  say('  verify:', await lastToast());
  await shot('s2d-history');

  // ---- scenario 2e: export the merged files and open them ------------------------------------------------------
  say('## 2e. export');
  for (const name of ['Project Charter', 'Budget 2027', 'Quarterly Review']) {
    await openDoc(name);
    await need('Save a copy'); await sleep(3000);
    say('  ', name, '->', await lastToast());
  }
  say('folder now:', readdirSync(FOLDER).filter((f) => !f.startsWith('~$')).join(' | '));
  await openDoc('Project Charter');
  await page.eval(`[...document.querySelectorAll('button')].find((b) => /^Open in /.test(b.textContent.trim())).click()`); await sleep(3500);
  const opened = [...officePids()].filter((p) => !before.has(p)).length;
  say('merged file opened by the system default program (new Office processes):', opened);
  closeNewOffice(before);
  say('## done');
  writeFileSync(evidence + 'acc2.log', log.join('\n'));
}
