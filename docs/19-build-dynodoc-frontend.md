# 08 · Dynodoc Frontend (the Phase-0 deliverable)

### Claude Code prompt to start this stage

> Build Dynodoc frontend, the Phase-0 product. This is the surface a 5-person expert team uses to author a questionnaire. Reference the existing static prototype at `docs/dynodoc_interactive.html` — port its visual design, four-mode editor (Document / Outline / Spec / Preview), and suggesting interaction model into a real SvelteKit app. In `frontend/researcher/`:
>
> 1. **Routing structure** (file-based, SvelteKit):
>    - `/` — list of documents the user has access to.
>    - `/d/[document_id]` — the four-mode editor, default to Document mode.
>    - `/d/[document_id]/outline` — Outline mode (tree view of nodes).
>    - `/d/[document_id]/spec` — Spec mode (the typed-tree view with field-level editing).
>    - `/d/[document_id]/preview` — Preview mode (renders as a respondent would see it).
>    - `/d/[document_id]/history` — full event log, audit view.
>    - `/d/[document_id]/proposals` — proposals queue for the author to accept/reject.
>
> 2. **Visual style**: warm parchment background (#F4EFE6), Fraunces for display, Source Serif 4 for body, IBM Plex Sans for UI chrome, terracotta accent (#B14B2C). Match the prototype exactly. Components use design tokens in `src/lib/tokens.ts`.
>
> 3. **State management**: use Svelte 5 runes (`$state`, `$derived`, `$effect`). One store per document (`documentStore.ts`) holding the materialized state + subscription to the SSE stream. The store applies incoming events incrementally without re-fetching the full state.
>
> 4. **The suggesting interaction**: when in suggesting mode, edits to a text field create a `SuggestionProposed{wrapped: FieldEdited}` rather than applying directly. The UI shows the suggestion inline with a strikethrough/highlight overlay, plus an accept/reject button bar.
>
> 5. **API client**: generate a typed TypeScript client from the OpenAPI spec using `openapi-typescript` and `openapi-fetch`. The client lives in `frontend/shared/api`. Both frontends share it. Don't hand-roll API calls.
>
> 6. **Optimistic updates**: write operations apply optimistically to the local store, then reconcile with the server's response. On error, roll back and show a toast. On success, the SSE stream delivers the canonical event and the optimistic state is reconciled.
>
> 7. **Offline support**: use a service worker to cache the document state and queue write operations. When offline, edits are stored locally; when online again, the queue is drained to the API with idempotency keys.
>
> 8. **Accessibility**: WCAG 2.2 AA. Keyboard navigation through nodes (arrow keys + Tab), screen reader announcements for state changes, focus management on mode switch, color contrast ≥4.5:1 on body text and ≥3:1 on UI.
>
> Tests:
>    - Vitest component tests for the four editor modes.
>    - Playwright end-to-end: log in → open document → add a section → add an item → save → reload → state persists.
>    - Playwright: suggesting mode → propose an edit → another user accepts → both users see the canonical state.

### Why ship this before the public layer

Dynodoc is its own product. A small team — your icddr,b or a2i colleagues — can use it for real work right now. Their feedback shapes the rest of the build. Crucially, *the engine that runs this product is the same engine that runs the national consultation*. By proving it works on a five-user load with structured documents, you de-risk the public-scale work.

### The four-mode editor, in summary

This is the conceptual heart of the prototype (see `dynodoc_uiux_theory.md`).

**Document mode**: WYSIWYG-style editing. The author sees the questionnaire roughly as a respondent would; they edit prose, item wording, choice labels. This is where 80% of work happens. The mode hides node IDs, varNames, expressions — those are surfaced in Spec mode.

**Outline mode**: a tree view, like a file browser. Drag-and-drop sections, items, choices. Quick restructuring. Each node shows a small "spec preview" inline.

**Spec mode**: the technical view. Each field of each node is visible and editable: `varName`, `type`, `constraint`, `relevance`, `calculation`, `media`, `hint`. This is where ODK/Kobo-specific power-user work happens.

**Preview mode**: renders the questionnaire as ODK Collect would render it. Tab through the form as a respondent would. Catch logic errors.

Mode switches are cheap (no save needed; state is shared) and preserve cursor focus where possible.

### The state store, sketched

```typescript
// frontend/researcher/src/lib/documentStore.ts
import { SSE } from '$lib/sse';
import { apiClient } from '@dynodoc/shared/api';

export class DocumentStore {
    state = $state<DocumentState>({ nodes: new Map(), proposals: new Map() });
    seq = $state(0);
    connection = $state<'connected' | 'reconnecting' | 'offline'>('connected');

    constructor(public documentId: string) {}

    async load() {
        const doc = await apiClient.GET('/documents/{id}', { params: { id: this.documentId } });
        this.state = doc.data.materialized;
        this.seq = doc.data.latest_seq;
        this.subscribe();
    }

    private subscribe() {
        const sse = new SSE(`/documents/${this.documentId}/stream?since_seq=${this.seq}`);
        sse.onEvent((event) => {
            this.applyEvent(event);
            this.seq = event.seq;
        });
        sse.onReconnect(() => this.connection = 'reconnecting');
        sse.onConnect(() => this.connection = 'connected');
    }

    applyEvent(event: Event) {
        // delegate to a reducer that matches the engine's materialization rules
        // this MUST stay in sync with the Rust materializer; integration tests check it
        materializerReducer(this.state, event);
    }

    async editField(nodeId: string, field: string, value: any, mode: 'direct' | 'suggest') {
        // optimistic update
        const undo = optimisticEdit(this.state, nodeId, field, value);
        try {
            if (mode === 'direct') {
                await apiClient.POST('/documents/{id}/ops/field-edit', {
                    params: { id: this.documentId },
                    body: { node_id: nodeId, field, value },
                });
            } else {
                await apiClient.POST('/documents/{id}/ops/suggest', { /* wrap as suggestion */ });
            }
        } catch (e) {
            undo();
            throw e;
        }
    }
}
```

### Watch out for

**Materializer drift.** The frontend's reducer must apply events the same way the Rust materializer does. Integration test: write a fuzz test that generates random event sequences, runs them through both, and asserts equal output. If they ever diverge, the optimistic UI lies to the user.

**Service worker complexity.** Offline support is valuable but the service worker is a major surface area. Consider deferring it to a follow-up stage if the team is slim.

**Bundle size.** SvelteKit produces small bundles by default; keep it that way. No moment.js (use Temporal or date-fns). No lodash (use the small handful of functions you need directly). No emoji libraries.

### Verification checklist

- [ ] All four editor modes render and switch cleanly.
- [ ] Two browsers logged in as different users see live edits via SSE.
- [ ] Suggesting mode produces a proposal in the proposals queue; accepting it applies to the canonical state.
- [ ] Lighthouse score: Performance 95+, Accessibility 100, Best Practices 100.
- [ ] Playwright E2E suite passes.
- [ ] The Phase-0 deliverable is genuinely usable by a friendly research team.
