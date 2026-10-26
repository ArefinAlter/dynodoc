// node run.mjs <home> <script.mjs> [--fresh] [--pick file.json]
// Starts the app on a scratch home (kept between runs unless --fresh), waits for the workspace,
// runs the script's default export with (page, ctx), then closes the app the way a person does.
import { pathToFileURL } from 'node:url';
import { mkdirSync } from 'node:fs';
import { launch, connect, clean, sleep, closeGracefully, stopOrphanDatabase } from './cdp.mjs';

const args = process.argv.slice(2);
const home = args[0];
const script = args[1];
const fresh = args.includes('--fresh');
const pickAt = args.indexOf('--pick');
const pick = pickAt >= 0 ? args[pickAt + 1] : undefined;
const noWait = args.includes('--no-wait');
const port = Number(process.env.CDP_PORT || 9333);
if (fresh) { stopOrphanDatabase(home); clean(home); }
const evidence = process.env.EVIDENCE || 'D:/verne-build/dynodoc/evidence/misc/';
mkdirSync(evidence, { recursive: true });

const app = launch({ home, pickFile: pick, port });
let page;
let code = 0;
try {
  page = await connect(port);
  if (!noWait) await page.waitFor("(!!document.querySelector('.sidebar') && !document.querySelector('.splash')) || !!document.querySelector('#profile-name') || !!document.querySelector('#local-retry')", 300000, 'the app to start');
  const mod = await import(pathToFileURL(script).href);
  await mod.default(page, { app, home, evidence, sleep });
} catch (e) {
  console.log('FAILED:', e.message);
  code = 1;
  if (page) await page.shot(evidence + 'failed.png').catch(() => {});
} finally {
  page?.close();
  await sleep(500);
  console.log('closed gracefully:', await closeGracefully(app.child));
  process.exit(code);
}
