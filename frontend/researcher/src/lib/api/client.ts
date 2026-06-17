/**
 * The typed API client over the stage-18 REST surface.
 *
 * Built on `openapi-fetch` against the generated {@link paths} (typed request bodies +
 * params). Stage-18 response bodies are `serde_json::Value` in `openapi.yaml`, so the
 * generated responses are `unknown`; the typed helper functions below cast them to the
 * hand-written shapes in {@link ./types}.
 *
 * Auth: a request middleware attaches `Authorization: Bearer <token>`. A response
 * middleware catches a single `401`, attempts one refresh-token rotation, and replays
 * the request; if refresh fails it clears the session and triggers the registered
 * unauthorized handler (the app redirects to `/login`).
 */
import createClient, { type Middleware } from 'openapi-fetch';

import type { paths } from './openapi.d.ts';
import { API_BASE_URL } from './config';
import { refresh as refreshTokens } from './auth';
import type {
  ConflictList,
  Document,
  DocumentDetail,
  EventRow,
  MaterializedNode,
  OpResponse,
  Page,
  ResolveResponse,
  VerifyResult
} from './types';
import { clearSession, session, setAccessToken, setTokens } from '$lib/stores/session.svelte';

let onUnauthorized: (() => void) | null = null;

/** Register the callback fired when auth is unrecoverable (refresh failed). */
export function setUnauthorizedHandler(handler: () => void): void {
  onUnauthorized = handler;
}

const authMiddleware: Middleware = {
  async onRequest({ request }) {
    if (session.accessToken) {
      request.headers.set('Authorization', `Bearer ${session.accessToken}`);
    }
    return request;
  },
  async onResponse({ request, response }) {
    if (response.status !== 401) return response;
    // Don't try to refresh the refresh call itself.
    if (request.headers.get('x-retry') === '1') return response;

    const refreshToken = session.refreshToken;
    if (!refreshToken) {
      clearSession();
      onUnauthorized?.();
      return response;
    }

    try {
      const tokens = await refreshTokens(refreshToken);
      setTokens(tokens.access_token, tokens.refresh_token);
    } catch {
      clearSession();
      onUnauthorized?.();
      return response;
    }

    // Replay the original request once with the new token.
    const retry = new Request(request, { headers: new Headers(request.headers) });
    retry.headers.set('Authorization', `Bearer ${session.accessToken}`);
    retry.headers.set('x-retry', '1');
    return fetch(retry);
  }
};

export const client = createClient<paths>({ baseUrl: API_BASE_URL });
client.use(authMiddleware);

/** Lift a token rotation done elsewhere into the client's view (used by tests). */
export { setAccessToken };

class ApiError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

function unwrap<T>(data: unknown, error: unknown, response: Response): T {
  if (error !== undefined || !response.ok) {
    const detail =
      error && typeof error === 'object' && 'message' in error
        ? String((error as { message: unknown }).message)
        : `request failed (${response.status})`;
    throw new ApiError(response.status, detail);
  }
  return data as T;
}

// --- typed helpers (cast the `unknown` response to the hand-written shapes) -------

/** `GET /documents` — a page of documents. */
export async function listDocuments(params?: {
  status?: string;
  cursor?: string;
  limit?: number;
}): Promise<Page<Document>> {
  const { data, error, response } = await client.GET('/documents', {
    params: { query: params }
  });
  return unwrap<Page<Document>>(data, error, response);
}

/** `POST /documents` — create a document; caller becomes its author. */
export async function createDocument(body: {
  title: string;
  languages?: string[];
}): Promise<Document> {
  const { data, error, response } = await client.POST('/documents', { body });
  return unwrap<Document>(data, error, response);
}

/** `GET /documents/{id}` — metadata + folded current state. */
export async function getDocument(id: string): Promise<DocumentDetail> {
  const { data, error, response } = await client.GET('/documents/{id}', {
    params: { path: { id } }
  });
  return unwrap<DocumentDetail>(data, error, response);
}

/** `GET /documents/{id}/node/{node_id}` — one materialized node. */
export async function getNode(id: string, nodeId: string): Promise<MaterializedNode> {
  const { data, error, response } = await client.GET('/documents/{id}/node/{node_id}', {
    params: { path: { id, node_id: nodeId } }
  });
  return unwrap<MaterializedNode>(data, error, response);
}

/** `GET /documents/{id}/events` — a page of raw event rows. */
export async function listEvents(
  id: string,
  params?: { from_seq?: number; to_seq?: number; cursor?: string; limit?: number }
): Promise<Page<EventRow>> {
  const { data, error, response } = await client.GET('/documents/{id}/events', {
    params: { path: { id }, query: params }
  });
  return unwrap<Page<EventRow>>(data, error, response);
}

/** `GET /documents/{id}/verify` — hash-chain verification result. */
export async function verifyChain(id: string): Promise<VerifyResult> {
  const { data, error, response } = await client.GET('/documents/{id}/verify', {
    params: { path: { id } }
  });
  return unwrap<VerifyResult>(data, error, response);
}

// --- write ops (every op response is `{ event }`) --------------------------------

/** `POST /documents/{id}/ops/node-create`. */
export async function opNodeCreate(
  id: string,
  body: {
    node_id?: string;
    node_type: string;
    parent_id?: string;
    pos: string;
    fields?: unknown;
    var_name?: string;
  }
): Promise<OpResponse> {
  const { data, error, response } = await client.POST('/documents/{id}/ops/node-create', {
    params: { path: { id } },
    body: body as never
  });
  return unwrap<OpResponse>(data, error, response);
}

/** `POST /documents/{id}/ops/field-edit`. */
export async function opFieldEdit(
  id: string,
  body: { node_id: string; field: string; value: unknown }
): Promise<OpResponse> {
  const { data, error, response } = await client.POST('/documents/{id}/ops/field-edit', {
    params: { path: { id } },
    body: body as never
  });
  return unwrap<OpResponse>(data, error, response);
}

/** `POST /documents/{id}/ops/comment`. */
export async function opComment(
  id: string,
  body: { node_id?: string; body: string }
): Promise<OpResponse> {
  const { data, error, response } = await client.POST('/documents/{id}/ops/comment', {
    params: { path: { id } },
    body: body as never
  });
  return unwrap<OpResponse>(data, error, response);
}

/** `POST /documents/{id}/ops/suggest` — `detail` is the wrapped op (tagged JSON). */
export async function opSuggest(
  id: string,
  body: { suggestion_id?: string; target_node_id?: string; detail: unknown }
): Promise<OpResponse> {
  const { data, error, response } = await client.POST('/documents/{id}/ops/suggest', {
    params: { path: { id } },
    body: body as never
  });
  return unwrap<OpResponse>(data, error, response);
}

/** `POST /documents/{id}/proposals/{suggestion_id}/accept`. */
export async function acceptProposal(id: string, suggestionId: string): Promise<OpResponse> {
  const { data, error, response } = await client.POST(
    '/documents/{id}/proposals/{event_id}/accept',
    { params: { path: { id, event_id: suggestionId } } }
  );
  return unwrap<OpResponse>(data, error, response);
}

/** `POST /documents/{id}/proposals/{suggestion_id}/reject`. */
export async function rejectProposal(
  id: string,
  suggestionId: string,
  reason?: string
): Promise<OpResponse> {
  const { data, error, response } = await client.POST(
    '/documents/{id}/proposals/{event_id}/reject',
    { params: { path: { id, event_id: suggestionId } }, body: { reason } as never }
  );
  return unwrap<OpResponse>(data, error, response);
}

// --- conflicts (stage 21) --------------------------------------------------------

/** `GET /documents/{id}/conflicts` — the live four-card conflicts. */
export async function listConflicts(id: string): Promise<ConflictList> {
  const { data, error, response } = await client.GET('/documents/{id}/conflicts', {
    params: { path: { id } }
  });
  return unwrap<ConflictList>(data, error, response);
}

/**
 * `POST /documents/{id}/conflicts/{conflict_id}/resolve`. Set exactly one of
 * `accept_suggestion_id` (accept a competing proposal) or `custom_value` (override all).
 */
export async function resolveConflict(
  id: string,
  conflictId: string,
  body: { accept_suggestion_id?: string; custom_value?: unknown }
): Promise<ResolveResponse> {
  const { data, error, response } = await client.POST(
    '/documents/{id}/conflicts/{conflict_id}/resolve',
    { params: { path: { id, conflict_id: conflictId } }, body: body as never }
  );
  return unwrap<ResolveResponse>(data, error, response);
}

/** `POST /documents/{id}/deploy` — runs the integrity gate, pins a snapshot. */
export async function deploy(id: string): Promise<OpResponse> {
  const { data, error, response } = await client.POST('/documents/{id}/deploy', {
    params: { path: { id } }
  });
  return unwrap<OpResponse>(data, error, response);
}

export { ApiError };
