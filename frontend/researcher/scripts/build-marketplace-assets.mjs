// Rasterises static/brand/mark.svg onto the canvases the Microsoft and Google
// marketplaces ask for. No dependencies beyond the installed Playwright Chromium.
//
//   pnpm build:marketplace-assets
//
// Output: extensions/marketplace/assets/*.png (committed), plus the Office sizes
// copied into static/connectors/assets/icon-<size>.png, which the manifests link.
// Inputs are fixed (the SVG, one font file, constant colours and sizes), so
// re-running on the same machine reproduces the same bytes. Store screenshots are
// NOT generated here: they must be real captures from installed hosts.
import { chromium } from '@playwright/test';
import { readFile, writeFile, mkdir, copyFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const path = (relative) => fileURLToPath(new URL(relative, import.meta.url));
const assets = path('../../../extensions/marketplace/assets/');
const served = path('../static/connectors/assets/');

const PAPER = '#f6f3ea';
const INK = '#1b1a17';
const markSvg = await readFile(path('../static/brand/mark.svg'), 'utf8');
const markUrl = 'data:image/svg+xml;base64,' + Buffer.from(markSvg).toString('base64');
const fontFile = ['geist-latin.woff2', 'public-sans-latin.woff2']
  .map((name) => path('../static/fonts/' + name))
  .find((file) => existsSync(file));
const fontUrl = 'data:font/woff2;base64,' + (await readFile(fontFile)).toString('base64');

function page(width, height, body, background = 'transparent') {
  return `<!doctype html><meta charset="utf-8"><style>
@font-face{font-family:"Wordmark";src:url(${fontUrl}) format("woff2");font-weight:100 900}
html,body{margin:0;padding:0;width:${width}px;height:${height}px;overflow:hidden;background:${background}}
body{display:flex;align-items:center;justify-content:center}
img{display:block}
.word{font:600 ${Math.round(height * 0.18)}px/1 "Wordmark",sans-serif;letter-spacing:-.02em;color:${INK};margin-left:${Math.round(height * 0.08)}px}
</style><body>${body}</body>`;
}
const mark = (size) => `<img alt="" width="${size}" height="${size}" src="${markUrl}">`;
const tile = (size, fill, background) =>
  page(size, size, mark(Math.round(size * fill)), background);
const banner = (w, h) =>
  page(w, h, `${mark(Math.round(h * 0.46))}<span class="word">Dynodoc</span>`, PAPER);

const files = [
  // Office: ribbon (16/32/80), IconUrl (32), HighResolutionIconUrl (64). Transparent.
  ...[16, 32, 64, 80].map((s) => ({ name: `office-icon-${s}.png`, w: s, h: s, html: tile(s, 1) })),
  // Partner Center large logo (216-350 px square allowed; 300 recommended).
  { name: 'office-store-logo-300.png', w: 300, h: 300, html: tile(300, 0.76, PAPER) },
  // Google Workspace Marketplace: icons (32/128 required, 96 for web apps) + card banner.
  ...[32, 96, 128].map((s) => ({
    name: `google-icon-${s}.png`,
    w: s,
    h: s,
    html: tile(s, 1)
  })),
  { name: 'google-card-banner-220x140.png', w: 220, h: 140, html: banner(220, 140) },
  { name: 'google-card-banner-440x280.png', w: 440, h: 280, html: banner(440, 280) }
];

await mkdir(assets, { recursive: true });
await mkdir(served, { recursive: true });
const browser = await chromium.launch();
try {
  const context = await browser.newContext({ deviceScaleFactor: 1 });
  for (const file of files) {
    const view = await context.newPage();
    await view.setViewportSize({ width: file.w, height: file.h });
    await view.setContent(file.html);
    await view.waitForFunction(() => [...document.images].every((image) => image.complete));
    await view.evaluate(() => document.fonts.load('600 20px Wordmark'));
    await view.evaluate(() => document.fonts.ready);
    const png = await view.screenshot({
      type: 'png',
      omitBackground: true,
      clip: { x: 0, y: 0, width: file.w, height: file.h }
    });
    await writeFile(assets + file.name, png);
    await view.close();
    console.log(`${file.name} ${file.w}x${file.h} ${png.length} bytes`);
  }
} finally {
  await browser.close();
}
// The manifests reference these exact sizes over HTTPS from the app origin.
for (const size of [16, 32, 64, 80])
  await copyFile(assets + `office-icon-${size}.png`, served + `icon-${size}.png`);
