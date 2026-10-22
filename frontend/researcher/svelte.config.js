import node from '@sveltejs/adapter-node';
import vercel from '@sveltejs/adapter-vercel';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: process.env.VERCEL ? vercel() : node(),
    // The page registers /service-worker.js itself ($lib/pwa/client.svelte.ts) so it can
    // skip private areas and hold updates until the person accepts them.
    serviceWorker: { register: false },
    csp: {
      mode: 'auto',
      directives: {
        'default-src': ['none'],
        'script-src': ['self'],
        // Svelte transitions and document formatting require dynamic styles.
        'style-src': ['self', 'unsafe-inline'],
        'img-src': ['self', 'data:', 'blob:', 'https:'],
        'font-src': ['self', 'data:', 'blob:'],
        // Further origins come from the configuration, added in $lib/server/security.ts.
        'connect-src': ['self'],
        'worker-src': ['self', 'blob:'],
        // Without this the browser refuses to read /site.webmanifest under default-src 'none',
        // so the app could never be installed. Same-origin only.
        'manifest-src': ['self'],
        'frame-src': ['self'],
        'frame-ancestors': ['self'],
        'object-src': ['none'],
        'base-uri': ['none'],
        'form-action': ['self']
      }
    }
  }
};
