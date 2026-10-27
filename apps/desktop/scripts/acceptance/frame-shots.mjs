// The frameless window: checks and screenshots, light and dark. Run through run.mjs:
//   CDP_PORT=9471 node run.mjs D:/verne-build/dynodoc/evidence/frame/home frame-shots.mjs --fresh --no-wait
// (ACC_EXE=<path> to drive an installed build; its portable data folder is then the home.) Pass --no-wait to run.mjs.
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const PY = process.env.PYTHON || 'E:/Python310/python.exe';
const OUT = process.env.FRAME_OUT || 'D:/verne-build/dynodoc/evidence/frame';
const EXE = process.env.FRAME_EXE_NAME || 'dynodoc-desktop';

export default async function (page, { sleep }) {
  await page.waitFor("!!document.querySelector('#profile-name') || (!!document.querySelector('.topbar') && !document.querySelector('.splash'))", 300000, 'the app to start');
  if (await page.has('#profile-name')) {
    await page.type('#profile-name', 'Priya Raman');
    await page.click('#profile-go');
  }
  await page.waitFor("!!document.querySelector('.topbar') && !document.querySelector('.splash')", 300000, 'the workspace');
  // The sidebar's state is a saved preference: start with it open.
  if (!(await page.has('.sidebar'))) await page.click('button[aria-label="Show sidebar"]');
  await sleep(1500);
  mkdirSync(OUT, { recursive: true });
  const info = await page.eval(`JSON.stringify({ frameless: document.documentElement.classList.contains('frameless'), controls: !!document.querySelector('#frame'), brand: !!document.querySelector('#app-menu'), ua: navigator.userAgent })`);
  console.log('page', info);
  for (const scheme of ['light', 'dark']) {
    await page.eval(`document.documentElement.setAttribute('data-scheme', ${JSON.stringify(scheme)})`);
    await sleep(500);
    const r = spawnSync(PY, [join(here, 'frame_shots.py'), '--out', OUT, '--scheme', scheme, '--exe', EXE, ...(scheme === 'dark' ? ['--skip-checks'] : [])], { encoding: 'utf8' });
    console.log(r.stdout);
    if (r.status !== 0) console.log('python failed', r.status, r.stderr);
  }
  await page.eval(`document.documentElement.removeAttribute('data-scheme')`);
}
