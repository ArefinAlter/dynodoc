// Stand-in for SvelteKit's `$env/dynamic/private` in unit tests (see vite.config.ts). The real
// module reads the running server's environment and cannot be imported by a test. A test that
// needs particular values mocks the module: vi.mock('$env/dynamic/private', () => ({ env })).
// Private and public have separate files because a mock is keyed by the file it resolves to.
export const env: Record<string, string | undefined> = {};
