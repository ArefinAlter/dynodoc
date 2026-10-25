// The agent's tools and what each access level allows. Plain data with no imports, so the rules
// can be tested with `node --test` and read by the interface and the scripted agent alike.
//
// The set is closed. The agent reads history, diffs and requests, and PROPOSES: a review comment
// or a commit summary comes back as a card, and a person presses the button that posts or uses it.
// What is not in the set matters as much: no tool approves, merges, declines, withdraws, deletes
// history, changes roles or rules, or reads a key, so no access level and no instruction text can
// add one.

export type AgentAccess = 'read' | 'propose';

export interface AgentTool {
  name: string;
  access: AgentAccess;
  summary: string;
  /** The Dynodoc endpoint the tool reads (for the inspector); never called by the scripted backend. */
  endpoint: string;
}

export const ACCESS_ORDER: AgentAccess[] = ['read', 'propose'];

export const ACCESS_INFO: Record<AgentAccess, { label: string; blurb: string }> = {
  read: { label: 'Read only', blurb: 'Reads history, diffs, documents and change requests. Changes nothing and proposes nothing.' },
  propose: { label: 'Propose', blurb: 'Also drafts review comments and commit summaries as cards. A person posts or uses them.' },
};

export const TOOLS: AgentTool[] = [
  { name: 'history.read', access: 'read', summary: 'Revisions of a file or project: author, time, chain hash', endpoint: 'GET /documents/{id}/events' },
  { name: 'history.verify', access: 'read', summary: 'Recomputes the hash chain of a file', endpoint: 'GET /documents/{id}/verify' },
  { name: 'diff.get', access: 'read', summary: 'What changed between two revisions, a draft and main, or two versions', endpoint: 'GET /projects/{id}/agent/files/{file}/diff' },
  { name: 'documents.list', access: 'read', summary: 'Files in the project with their latest revision', endpoint: 'GET /projects/{id}/files' },
  { name: 'documents.outline', access: 'read', summary: 'Headings, sheets or slides of one file', endpoint: 'GET /projects/{id}/agent/files/{file}/outline' },
  { name: 'requests.list', access: 'read', summary: 'Change requests and their states', endpoint: 'GET /projects/{id}/change-requests' },
  { name: 'requests.summarise', access: 'read', summary: 'A summary of one change request: what it changes and its review state', endpoint: 'GET /documents/{id}/change-requests/{draft}' },
  { name: 'review.propose_comment', access: 'propose', summary: 'Drafts a review comment on one change; a person posts it', endpoint: 'none: returns a card' },
  { name: 'commit.draft_summary', access: 'propose', summary: 'Drafts a commit summary and description from the staged changes; a person uses it', endpoint: 'none: returns a card' },
];

/** What no tool does, whatever the access level or the instructions say. Shown as fixed rules. */
export const NEVER: string[] = [
  'Approve, request changes, merge or decline a change request. A person presses those buttons.',
  'Commit, push, withdraw a draft or rewrite history.',
  'Change roles, review rules or who may merge.',
  'Read a key, a token or a connection secret.',
];

/** Actions that exist in Dynodoc and that only a person may take. A test keeps them out of TOOLS. */
export const PERSON_ONLY = ['approve', 'merge', 'decline', 'withdraw', 'commit', 'push', 'request_changes', 'set_role', 'set_rules', 'revoke'];

export function accessRank(a: AgentAccess): number {
  return ACCESS_ORDER.indexOf(a);
}

export type Decision = { ok: true } | { ok: false; reason: string };

/** Whether a thread at `access` may call `tool`, given the tools enabled in Settings. */
export function mayCall(tool: string, access: AgentAccess, enabled: readonly string[]): Decision {
  const t = TOOLS.find((x) => x.name === tool);
  if (!t) return { ok: false, reason: 'there is no such tool' };
  if (!enabled.includes(tool)) return { ok: false, reason: 'it is switched off in Settings, Agent' };
  if (accessRank(access) < accessRank(t.access)) {
    return { ok: false, reason: `it needs ${ACCESS_INFO[t.access].label} access and this thread has ${ACCESS_INFO[access].label}` };
  }
  return { ok: true };
}

export const DEFAULT_INSTRUCTIONS = [
  'You are helping a team keep its documents under version control in Dynodoc.',
  '- Quote revisions by number (r12) and chain hash prefix.',
  '- Never approve or merge; say what a person should check before they do.',
  '- Keep review comments short, specific and kind.',
  '- Report what you read and what you did not.',
].join('\n');
