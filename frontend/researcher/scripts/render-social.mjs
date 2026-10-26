// Renders static/brand/social.svg to static/brand/social.png (1200 x 630) with the site's own
// self-hosted Geist fonts, so the image never depends on fonts installed on the machine.
//
//   node scripts/render-social.mjs            write static/brand/social.png
//   node scripts/render-social.mjs --measure  print the width of each headline line (to size the
//                                             highlighter rectangles hl1 to hl3 in the SVG)
//
// Uses the Playwright that is already a dev dependency (run `npx playwright install chromium` once).
import { chromium } from '@playwright/test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const font = (name) => pathToFileURL(resolve(root, 'static/fonts', name)).href;
const svg = readFileSync(resolve(root, 'static/brand/social.svg'), 'utf8');

const html = `<!doctype html><meta charset="utf-8"><style>
@font-face{font-family:Geist;font-weight:100 900;src:url(${font('geist-latin.woff2')}) format('woff2');unicode-range:U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD}
@font-face{font-family:'Geist Mono';font-weight:100 900;src:url(${font('geist-mono-latin.woff2')}) format('woff2');unicode-range:U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD}
html,body{margin:0;background:#f6f3ea}svg{display:block}</style>${svg}`;

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1200, height: 630 },
  deviceScaleFactor: 1
});
// A file: page may load file: fonts; about:blank (setContent) may not.
const scratch = mkdtempSync(resolve(tmpdir(), 'dynodoc-social-'));
const file = resolve(scratch, 'social.html');
writeFileSync(file, html);
await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
await page.evaluate(() => document.fonts.ready);
await page.evaluate(() =>
  Promise.all(
    ['500 17px "Geist Mono"', '520 54px Geist', '600 32px Geist'].map((f) => document.fonts.load(f))
  )
);
if (process.argv.includes('--measure')) {
  const widths = await page.evaluate(() =>
    ['t1', 't2', 't3'].map((id) => {
      const b = document.getElementById(id).getBBox();
      return { id, x: b.x, width: Math.round(b.width) };
    })
  );
  console.log(JSON.stringify(widths));
} else {
  const png = await page.screenshot({
    type: 'png',
    clip: { x: 0, y: 0, width: 1200, height: 630 }
  });
  writeFileSync(resolve(root, 'static/brand/social.png'), png);
  console.log('wrote static/brand/social.png', png.length, 'bytes');
}
await browser.close();
rmSync(scratch, { recursive: true, force: true });
