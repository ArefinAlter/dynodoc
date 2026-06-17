import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  // Mounting Svelte 5 components in jsdom needs the client (`browser`) build of Svelte;
  // without this condition Vite resolves `svelte`'s server entry and `mount()` throws.
  // Scope it to test runs so the production build is unaffected.
  resolve: process.env.VITEST ? { conditions: ['browser'] } : {},
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    environment: 'jsdom'
  }
});
