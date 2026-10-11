/**
 * Roles, review rules, approvals, copy detection and notifications. The server
 * decides every permission; these helpers only shape requests and wording.
 */
import { workspaceRequest } from './api';

export type MemberRole = 'owner' | 'manager' | 'editor' | 'reviewer' | 'contributor' | 'viewer';
export interface Capabilities {
  read: boolean;
  comment: boolean;
  propose: boolean;
  approve: boolean;
  edit_team: boolean;
  merge: boolean;
  share: boolean;
  manage_rules: boolean;
  archive?: boolean;
  edit_settings?: boolean;
  transfer: boolean;
  trash: boolean;
}
export interface Policy {
  protect_team_version: boolean;
  required_approvals: number;
  merge_roles: 'editors' | 'owners';
}
export interface PersonRow {
  id: string;
  email: string | null;
  name: string | null;
  role: MemberRole;
  via: 'direct' | 'workspace';
  space: string | null;
}
export interface People {
  owner: { id: string; email: string | null; name: string | null };
  you: { id: string; role: MemberRole; can: Capabilities };
  members: PersonRow[];
  policy: Policy;
  policy_source?: {
    kind: 'file' | 'project' | 'default';
    id?: string;
    name?: string;
    floor?: Policy;
    file_rules?: Policy | null;
  };
}

/** Plain-language names and what each role adds, lowest first. */
export const ROLES: { role: MemberRole; name: string; summary: string; grantable: boolean }[] = [
  {
    role: 'viewer',
    name: 'Viewer',
    summary: 'Reads and downloads. Can’t comment or change anything.',
    grantable: true
  },
  {
    role: 'contributor',
    name: 'Contributor',
    summary:
      'Comments, suggests, works in personal drafts and sends changes (including edited files) for review.',
    grantable: true
  },
  {
    role: 'reviewer',
    name: 'Reviewer',
    summary:
      'Everything a contributor can do, plus approving or requesting changes on other people’s requests.',
    grantable: true
  },
  {
    role: 'editor',
    name: 'Editor',
    summary:
      'Edits the team version directly and merges or declines change requests, within the review rules.',
    grantable: true
  },
  {
    role: 'manager',
    name: 'Manager',
    summary:
      'Given through a workspace. Shares the document; project review rules and overrides require project management.',
    grantable: false
  },
  {
    role: 'owner',
    name: 'Owner',
    summary:
      'Manages standalone files and is told about copies. Project review authority belongs to project owners and managers.',
    grantable: false
  }
];
export const roleName = (role?: string | null) =>
  ROLES.find((r) => r.role === role)?.name || 'No access';

export const people = (documentId: string) =>
  workspaceRequest<People>(`/documents/${documentId}/people`);
export const setPolicy = (documentId: string, policy: Policy) =>
  workspaceRequest<Policy>(`/documents/${documentId}/policy`, policy);
export const inheritPolicy = (documentId: string) =>
  workspaceRequest(`/documents/${documentId}/policy/inherit`, {});
export const transferOwnership = (documentId: string, email: string) =>
  workspaceRequest(`/documents/${documentId}/owner`, { email });
export const shareDocument = (documentId: string, email: string, role: MemberRole | 'remove') =>
  workspaceRequest(`/documents/${documentId}/members`, { email, role });

export type EmailStatus = 'sent' | 'skipped' | 'not_configured' | 'rate_limited' | 'failed';
/** Share, then email an invitation (sent by the web server; access is granted first). */
export async function inviteByEmail(
  target: { document_id: string } | { space_id: string },
  email: string,
  role: string,
  options: { notify: boolean; note?: string }
): Promise<{ email_status: EmailStatus }> {
  const response = await fetch('/api/invite', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...target, email, role, notify: options.notify, note: options.note })
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = new Error(data.error || `Request failed (${response.status})`);
    Object.assign(error, { status: response.status });
    throw error;
  }
  return data;
}
/** What happened to the invitation email, in plain words. */
export function invitationNotice(status: EmailStatus, email: string) {
  switch (status) {
    case 'sent':
      return `Access updated. We emailed ${email} an invitation with a link.`;
    case 'rate_limited':
      return `Access updated. No email was sent because ${email} was invited a few minutes ago.`;
    case 'failed':
      return `Access updated, but the invitation email could not be sent. Copy the document link and send it to ${email} yourself.`;
    case 'not_configured':
      return `Access updated. Email is not set up on this server, so send ${email} the link yourself.`;
    default:
      return 'Access updated. They will see it in their workspace when they sign in.';
  }
}

export type Relation =
  | 'identical'
  | 'copy'
  | 'edited_copy'
  | 'excerpt'
  | 'extended'
  | 'overlap'
  | 'same_name'
  | 'unrelated';
export interface Comparison {
  relation: Relation;
  score: number;
  jaccard: number;
  coverage_new: number;
  coverage_old: number;
  shared_blocks: number;
  title_match: boolean;
}
export interface SimilarDocument {
  document_id: string;
  title: string;
  kind: string;
  role: MemberRole;
  can_push: boolean;
  owner: { name: string | null; email: string; is_you: boolean };
  embedded_id: boolean;
  comparison: Comparison;
}
export const RELATION_TEXT: Record<Relation, string> = {
  identical: 'Identical content',
  copy: 'A copy',
  edited_copy: 'An edited copy',
  excerpt: 'Part of this document',
  extended: 'This document plus new material',
  overlap: 'Shares some passages',
  same_name: 'Same name, different content',
  unrelated: 'Unrelated'
};
export const percent = (value: number) => `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;

/** Compare a file's text with the documents you can open, before uploading it. */
export const checkSimilarity = (blocks: string[], title: string, originDocumentId?: string) =>
  workspaceRequest<{
    matches: SimilarDocument[];
    file: { blocks: number; words: number; comparable: boolean };
  }>('/similarity/check', {
    blocks: blocks.slice(0, 50000),
    title,
    origin_document_id: originDocumentId || null
  });

export interface CopyRow {
  id: string;
  direction: 'copy_of_this' | 'this_resembles';
  relation: Relation;
  method: string;
  score: number;
  coverage_source: number;
  coverage_target: number;
  shared_blocks: number;
  title_match: boolean;
  status: 'open' | 'invited' | 'linked' | 'dismissed';
  detected_at: string;
  document: {
    id: string;
    title: string;
    kind: string;
    role: MemberRole | null;
    filename: string | null;
  };
  person: { id: string; name: string | null; email: string | null };
}
export const listCopies = (documentId: string) =>
  workspaceRequest<{ items: CopyRow[]; can_act: boolean }>(`/documents/${documentId}/copies`);
export const respondToCopy = (matchId: string, action: 'invite' | 'dismiss' | 'link') =>
  workspaceRequest<{ status: string }>(`/copies/${matchId}`, { action });

export interface NotificationItem {
  id: string;
  kind: string;
  created_at: string;
  read: boolean;
  payload: Record<string, unknown>;
  document: { id: string; title: string; kind: string; role: MemberRole | null } | null;
  actor: { id: string; name: string | null; email: string | null } | null;
  source: {
    id: string;
    title: string;
    kind: string;
    role: MemberRole | null;
    filename: string | null;
  } | null;
  match: { id: string; status: string; relation: Relation; score: number } | null;
  request: { id: string; name: string; status: string } | null;
}
export const listNotifications = (before?: string) =>
  workspaceRequest<{ items: NotificationItem[]; more: boolean; unread: number }>(
    `/notifications${before ? `?before=${encodeURIComponent(before)}` : ''}`
  );
export const unreadCount = () => workspaceRequest<{ unread: number }>('/notifications/count');
export const markRead = (ids: string[] | 'all') =>
  workspaceRequest('/notifications/read', ids === 'all' ? { all: true } : { ids });

const who = (actor: NotificationItem['actor']) =>
  actor?.name || actor?.email?.split('@')[0] || 'Someone';
/** One sentence describing a notification. */
export function describe(n: NotificationItem): string {
  const title = n.document ? `“${n.document.title}”` : 'a document';
  const score = typeof n.payload.score === 'number' ? percent(n.payload.score) : '';
  switch (n.kind) {
    case 'copy_detected':
      return `${who(n.actor)} uploaded ${n.source?.filename ? `“${n.source.filename}”` : 'a file'}, which is ${RELATION_TEXT[(n.payload.relation as Relation) || 'copy'].toLowerCase()} of ${title} (${score} match).`;
    case 'same_name_detected':
      return `${who(n.actor)} uploaded a different file named like ${title}. Its content doesn’t match yours.`;
    case 'copy_invitation':
      return `${who(n.actor)} invited you to send your copy as changes to ${title}.`;
    case 'change_request_submitted':
      return `${who(n.actor)} sent “${n.request?.name || 'changes'}” for review in ${title}.`;
    case 'change_request_reviewed':
      return `${who(n.actor)} ${n.payload.verdict === 'approved' ? 'approved' : n.payload.verdict === 'changes_requested' ? 'requested changes to' : 'commented on'} “${n.request?.name || 'your request'}” in ${title}.`;
    case 'change_request_merged':
      return `${who(n.actor)} merged “${n.request?.name || 'your request'}” into ${title}.`;
    case 'change_request_declined':
      return `${who(n.actor)} declined “${n.request?.name || 'your request'}” in ${title}.`;
    case 'access_granted':
      return `${who(n.actor)} gave you ${roleName(n.payload.role as string).toLowerCase()} access to ${title}.`;
    case 'ownership_transferred':
      return `${who(n.actor)} made you the owner of ${title}.`;
    default:
      return `Activity in ${title}.`;
  }
}

export interface Review {
  id: string;
  verdict: 'approved' | 'changes_requested' | 'commented';
  note: string;
  node_id: string | null;
  created_at: string;
  stale: boolean;
  reviewer: { id: string; name: string | null; email: string | null };
}
export interface ReviewState {
  reviews: Review[];
  rules: Policy;
  approvals: number;
  approved_by: string[];
  changes_requested_by: string[];
  required_approvals: number;
  satisfied: boolean;
  can_merge: boolean;
  can_override: boolean;
  can_approve: boolean;
}
export const submitReview = (
  documentId: string,
  requestId: string,
  body: { verdict: Review['verdict']; note: string; node_id?: string | null }
) =>
  workspaceRequest<{ id: string; review: ReviewState }>(
    `/documents/${documentId}/change-requests/${requestId}/reviews`,
    body
  );
