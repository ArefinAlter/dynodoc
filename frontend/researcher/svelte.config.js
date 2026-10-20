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
        'connect-src': ['self', 'https://app.dynodoc.online'],
        'worker-src': ['self', 'blob:'],
        'frame-src': ['self'],
        'frame-ancestors': ['self'],
        'object-src': ['none'],
        'base-uri': ['none'],
        'form-action': ['self']
      }
    }
  }
};
