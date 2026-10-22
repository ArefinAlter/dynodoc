import { fileURLToPath } from 'node:url';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Unit tests cannot import SvelteKit's environment modules; they get an empty stand-in
// unless the test mocks them.
const emptyEnv = (kind: 'private' | 'public') =>
  fileURLToPath(new URL(`./src/lib/testing/env-${kind}.ts`, import.meta.url));

export default defineConfig({
  plugins: [sveltekit()],
  // Mounting Svelte 5 components in jsdom needs the client (`browser`) build of Svelte;
  // without this condition Vite resolves `svelte`'s server entry and `mount()` throws.
  // Scope it to test runs so the production build is unaffected.
  resolve: process.env.VITEST ? { conditions: ['browser'] } : {},
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    environment: 'jsdom',
    alias: {
      '$env/dynamic/private': emptyEnv('private'),
      '$env/dynamic/public': emptyEnv('public')
    }
  }
});
