// What the hosted workspace (app.dynodoc.online) runs right now. The marketing pages state it in
// several places (features, roadmap, changelog, home page); they all read it from here. Change
// these three values in the release commit, after the release is live and verified, and nowhere
// else. The 14 October entry in the changelog is a record of that earlier release, so it keeps
// its own text. Record: docs/operations/2026-10-23-schema47-release.md.
export const production = {
  /** Application revision on the VPS. */
  app: '3fa06fd',
  /** PostgreSQL schema version of that release. */
  schema: 47,
  /** Day the release was activated (UTC). */
  date: '23 October 2026'
} as const;
