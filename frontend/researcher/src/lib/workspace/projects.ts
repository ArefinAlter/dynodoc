import type { Policy } from './collaboration';
/** Project membership preserves historical storage names. */
export const projectRoleName = (role: string) =>
  ({
    owner: 'Owner',
    manager: 'Manager',
    editor: 'Editor',
    approver: 'Reviewer',
    reviewer: 'Contributor',
    viewer: 'Viewer'
  })[role] || role;
export interface Project {
  id: string;
  name: string;
  description: string;
  revision: number;
  role: string;
  policy: Policy;
  watch: 'requests' | 'ignore';
}
export interface ProjectFile {
  id: string;
  title: string;
  kind: string;
  role: string;
  latest_seq: number;
  updated_at: string;
}
export interface ProjectRequest {
  id: string;
  name: string;
  status: string;
  revision: number;
  source: { bundle_id?: string };
  document: { id: string; title: string; kind: string };
  author?: { id: string; name: string | null; email: string };
  created_at: string;
  /** Present when an AI agent wrote the request or draft. */
  agent?: { id: string; client_kind: string; display_name: string | null };
}
export function downloadJson(value: unknown, filename: string) {
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' })
  );
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
