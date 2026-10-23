// What the hosted workspace (app.dynodoc.online) runs right now. The marketing pages state it in
// several places (features, roadmap, changelog, home page); they all read it from here. Change
// these three values in the release commit, after the release is live and verified, and nowhere
// else. The 14 October entry in the changelog is a record of that release, so it keeps its own
// text.
export const production = {
  /** Application revision on the VPS. */
  app: 'f62d62c',
  /** PostgreSQL schema version of that release. */
  schema: 41,
  /** Day the release was activated (UTC). */
  date: '14 October 2026'
} as const;
