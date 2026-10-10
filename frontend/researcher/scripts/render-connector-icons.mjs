// Rasterize our existing vector favicon at Office's required resource sizes.
import { chromium } from '@playwright/test';
import { readFile, mkdir } from 'node:fs/promises';
const directory = new URL('../static/connectors/assets/', import.meta.url);
await mkdir(directory, { recursive: true });
const svg = await readFile(new URL('../static/favicon.svg', import.meta.url), 'utf8');
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  for (const size of [16, 32, 80]) {
    await page.setViewportSize({ width: size, height: size });
    await page.setContent(
      `<style>html,body{margin:0}svg{width:100vw;height:100vh;display:block}</style>${svg}`
    );
    await page.screenshot({
      path: new URL(`icon-${size}.png`, directory).pathname.replace(/^\/(.:)/, '$1'),
      omitBackground: true
    });
  }
} finally {
  await browser.close();
}
