// The web server's configuration, read once and frozen. Everything that used to read
// `$env/dynamic/private` for a setting or a secret reads it here instead, so a bad
// deployment stops at start (the `init` hook in hooks.server.ts) with one message listing
// every problem, and `*_FILE` secrets work for every secret. See ./schema.js for the rules.
// A development server (`vite dev`) reports problems as warnings instead, so the public
// pages can be worked on without secrets; production and `vite preview` stop.
import { dev } from '$app/environment';
import { env as privateEnv } from '$env/dynamic/private';
import { formatProblems, loadConfig, type LoadResult, type WebConfig } from './schema.js';

export type { PublicSite, WebConfig } from './schema.js';

let loaded: LoadResult | undefined;
// `$env/dynamic/private` never holds PUBLIC_* variables; the hook hands them over at start.
let publicVariables: Record<string, string | undefined> = {};
const load = () => (loaded ??= loadConfig({ ...publicVariables, ...privateEnv }));

/** Forget the configuration read so far. Unit tests that change a mocked environment only. */
export const resetConfig = () => {
  loaded = undefined;
};

/** The validated configuration. Throws one aggregated error when it is invalid (not in dev). */
export function getConfig(): Readonly<WebConfig> {
  const result = load();
  if (!result.ok && !dev) throw new Error(formatProblems(result.problems));
  return result.config;
}

/** Called from the `init` hook: validate now, report warnings, fail before the server listens. */
export function startupCheck(publicEnv: Record<string, string | undefined> = {}) {
  publicVariables = publicEnv;
  loaded = undefined;
  const result = load();
  for (const warning of result.warnings)
    console.warn(`[dynodoc] configuration warning: ${warning}`);
  if (result.ok) return;
  if (!dev) throw new Error(formatProblems(result.problems));
  for (const problem of result.problems)
    console.warn(`[dynodoc] configuration problem: ${problem}`);
}
