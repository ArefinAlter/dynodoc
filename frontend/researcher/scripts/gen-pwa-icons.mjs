// Rasterises static/brand/mark.svg into the icons the web app manifest and iOS ask for.
// No dependencies beyond the installed Playwright Chromium.
//
//   node scripts/gen-pwa-icons.mjs            writes only the files that are missing
//   node scripts/gen-pwa-icons.mjs --force    rewrites all of them
//
// Output (committed): static/icon-192.png, icon-512.png, apple-touch-icon.png (the mark
// on the paper colour) and icon-maskable-512.png. A maskable icon is cropped by the
// launcher to a circle or rounded square, so the platform guarantees only the inner 80%
// circle: the mark is drawn smaller there and the background runs to every edge.
import { chromium } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const path = (relative) => fileURLToPath(new URL(relative, import.meta.url));
const PAPER = '#faf9f6';
const force = process.argv.includes('--force');
const markSvg = await readFile(path('../static/brand/mark.svg'), 'utf8');
const markUrl = 'data:image/svg+xml;base64,' + Buffer.from(markSvg).toString('base64');

// `fill` is the mark's share of the canvas. The mark's drawn shapes reach about 39.6 of
// the 64 units from its centre, so 0.64 keeps them inside the 40% safe radius.
const icons = [
  { name: 'icon-192.png', size: 192, fill: 0.85 },
  { name: 'icon-512.png', size: 512, fill: 0.85 },
  { name: 'apple-touch-icon.png', size: 180, fill: 0.85 },
  { name: 'icon-maskable-512.png', size: 512, fill: 0.64 }
];

const todo = icons.filter((icon) => force || !existsSync(path(`../static/${icon.name}`)));
if (!todo.length) console.log('All icons exist. Use --force to rewrite them.');
else {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({ deviceScaleFactor: 1 });
    for (const { name, size, fill } of todo) {
      const view = await context.newPage();
      await view.setViewportSize({ width: size, height: size });
      await view.setContent(`<!doctype html><meta charset="utf-8"><style>
html,body{margin:0;width:${size}px;height:${size}px;overflow:hidden;background:${PAPER}}
body{display:flex;align-items:center;justify-content:center}
img{display:block}
</style><img alt="" width="${Math.round(size * fill)}" height="${Math.round(size * fill)}" src="${markUrl}">`);
      await view.waitForFunction(() => [...document.images].every((image) => image.complete));
      const png = await view.screenshot({
        type: 'png',
        clip: { x: 0, y: 0, width: size, height: size }
      });
      await writeFile(path(`../static/${name}`), png);
      await view.close();
      console.log(`${name} ${size}x${size} ${png.length} bytes`);
    }
  } finally {
    await browser.close();
  }
}
