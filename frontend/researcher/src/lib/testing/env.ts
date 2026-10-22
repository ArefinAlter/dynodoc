// Stand-in for SvelteKit's `$env/dynamic/*` modules in unit tests (see vite.config.ts). The
// real modules read the running server's environment and cannot be imported by a test.
// A test that needs particular values still mocks the module: vi.mock('$env/dynamic/private').
export const env: Record<string, string | undefined> = {};
