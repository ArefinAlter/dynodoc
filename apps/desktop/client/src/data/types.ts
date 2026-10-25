// The client's data model. Names follow Dynodoc's API (openapi.yaml and the web workspace's
// types) where the concept exists there: projects, documents (files), revisions with chain
// hashes, named versions, drafts (a private branch of one file), change requests with reviews,
// conflicts and merges, people and roles, connection keys for the editor add-ins, notifications.
import type { Block, Edit } from '../lib/diff.ts';
import type { DraftState } from '../lib/branchMenu.ts';

export type FileKind = 'document' | 'spreadsheet' | 'presentation';
/** Stored project and document roles; the interface labels approver "Reviewer" and reviewer "Contributor". */
export type Role = 'owner' | 'manager' | 'editor' | 'approver' | 'reviewer' | 'viewer';

export interface Person {
  id: string;
  name: string;
  email: string;
  /** An AI agent's key, not a person. */
  agent?: boolean;
}

export interface Member {
  personId: string;
  role: Role;
  via: 'direct' | 'workspace';
}

export interface ReviewRules {
  /** "Protect the team version": editors send changes through review. */
  protectMain: boolean;
  requiredApprovals: number;
  mergeRoles: 'editors' | 'owners';
}

export interface Label {
  id: string;
  name: string;
  color: string;
}

export interface Issue {
  number: number;
  title: string;
  body: string;
  state: 'open' | 'closed';
  authorId: string;
  assigneeId?: string;
  labels: string[];
  comments: { authorId: string; at: number; text: string }[];
  createdAt: number;
  closedAt?: number;
}

export interface AgentPolicy {
  enabled: boolean;
  minRole: Role;
  callsPerMinute: number;
  openProposals: number;
  proposalsPerDay: number;
  submitRequiresPerson: boolean;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  ownerId: string;
  role: Role;
  private: boolean;
  starred: boolean;
  stars: number;
  watch: 'ignore' | 'requests';
  updatedAt: number;
  rules: ReviewRules;
  members: Member[];
  issues: Issue[];
  labels: Label[];
  agentPolicy: AgentPolicy;
  /** The workspace the project lives in. */
  space: string;
}

export interface Version {
  id: string;
  name: string;
  note: string;
  throughSeq: number;
  at: number;
  authorId: string;
}

export interface ConnectionKey {
  id: string;
  host: AddinHost;
  createdAt: number;
  expiresAt: number;
  revoked: boolean;
  /** Last time the add-in pushed or read with this key. */
  lastUsed?: number;
}

export type AddinHost = 'word' | 'excel' | 'powerpoint' | 'google-docs' | 'google-sheets' | 'google-slides';

export interface Doc {
  id: string;
  projectId: string;
  title: string;
  filename: string;
  kind: FileKind;
  /** The blocks of main at its latest revision. */
  blocks: Block[];
  versions: Version[];
  connections: ConnectionKey[];
  createdAt: number;
  /** Discussion threads (comments in the document). */
  discussions: { id: string; blockId: string; authorId: string; text: string; at: number; resolved: boolean; replies: { authorId: string; text: string; at: number }[] }[];
  /** Copies and similar files Dynodoc found. */
  copies: { id: string; title: string; relation: 'copy' | 'edited_copy' | 'excerpt' | 'same_name'; score: number; status: 'open' | 'invited' | 'linked' | 'dismissed' }[];
  publicLinks: { id: string; createdAt: number; expiresAt?: number; revoked: boolean }[];
}

export type CommitKind = 'revision' | 'merge' | 'revert' | 'cherry_pick' | 'restore' | 'rebase';

/** One revision in the hash-chained history (Dynodoc's events, grouped as one commit). */
export interface Commit {
  id: string;
  docId: string;
  /** "main" or a draft id. */
  branch: string;
  /** Revision number on its branch's file history (r12). */
  seq: number;
  summary: string;
  description: string;
  authorId: string;
  at: number;
  edits: Edit[];
  kind: CommitKind;
  /** For a merge: the draft merged. */
  mergedFrom?: string;
  /** SHA-256 of the canonical edits. */
  contentHash: string;
  prevChainHash: string;
  chainHash: string;
  /** On this computer only, not yet on the server. */
  local: boolean;
}

export interface Draft {
  id: string;
  docId: string;
  projectId: string;
  name: string;
  authorId: string;
  baseSeq: number;
  createdAt: number;
  state: DraftState;
  /** The draft's own blocks (main at baseSeq plus its commits). */
  blocks: Block[];
}

export type Verdict = 'approved' | 'changes_requested' | 'commented';

export interface Review {
  reviewerId: string;
  verdict: Verdict;
  note: string;
  at: number;
  /** Superseded by new commits on the draft. */
  stale: boolean;
}

export interface ReviewComment {
  id: string;
  authorId: string;
  blockId?: string;
  text: string;
  at: number;
  /** Proposed by an agent and posted by this person. */
  proposedByAgent?: boolean;
}

export interface Conflict {
  blockId: string;
  label: string;
  ancestor: string;
  team: string;
  draft: string;
  choice?: 'team' | 'draft';
}

export interface ChangeRequest {
  id: string;
  number: number;
  projectId: string;
  docId: string;
  draftId: string;
  name: string;
  note: string;
  authorId: string;
  status: 'open' | 'merged' | 'declined' | 'withdrawn';
  createdAt: number;
  mergedAt?: number;
  baseSeq: number;
  /** Changes in the request; `excluded` holds block ids a reviewer left out of the merge. */
  edits: Edit[];
  excluded: string[];
  conflicts: Conflict[];
  reviews: Review[];
  comments: ReviewComment[];
  /** Opened by an agent's key (AI proposal). */
  byAgent?: boolean;
  /** The agent's own description, untrusted text. */
  agentStatement?: string;
}

/** A change captured by an add-in on this computer and not yet committed (GitHub Desktop's
 *  working-directory change). */
export interface WorkingChange {
  id: string;
  docId: string;
  /** "main" or a draft id: where the add-in was capturing. */
  branch: string;
  edit: Edit;
  staged: boolean;
  source: AddinHost;
  capturedAt: number;
}

export type NotificationKind =
  | 'change_request_submitted'
  | 'change_request_reviewed'
  | 'change_request_merged'
  | 'change_request_declined'
  | 'copy_detected'
  | 'access_granted'
  | 'mention';

export interface Notification {
  id: string;
  kind: NotificationKind;
  at: number;
  read: boolean;
  actorId: string;
  projectId: string;
  docId?: string;
  requestId?: string;
  text: string;
}

export interface ActivityItem {
  id: string;
  at: number;
  actorId: string;
  projectId: string;
  docId?: string;
  kind: 'commit' | 'merge' | 'request' | 'review' | 'version' | 'draft' | 'member' | 'issue' | 'verify';
  summary: string;
  byAgent?: boolean;
}

export interface Space {
  id: string;
  name: string;
  kind: 'organization' | 'team' | 'folder';
  parentId?: string;
  members: Member[];
}

export interface DeviceKey {
  id: string;
  name: string;
  kind: 'person' | 'agent';
  publicKey: string;
  createdAt: number;
  revoked: boolean;
  scopes: string[];
}

export interface Db {
  me: string;
  people: Person[];
  spaces: Space[];
  projects: Project[];
  docs: Doc[];
  commits: Commit[];
  drafts: Draft[];
  requests: ChangeRequest[];
  working: WorkingChange[];
  notifications: Notification[];
  activity: ActivityItem[];
  deviceKeys: DeviceKey[];
  /** When the project list was last fetched from "the server" (the script). */
  lastFetched: number;
  /** Scripted events that a fetch brings in, in order. */
  incoming: number;
}
