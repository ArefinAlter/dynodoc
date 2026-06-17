/**
 * Component tests for the four editor modes. Each view is fed real adapter output
 * (DocumentState → EditorDocument) and mounted in jsdom; we assert it renders the
 * questionnaire content without throwing. This is the PoC stand-in for the doc's
 * "Vitest component tests for the four editor modes".
 */
import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { toEditorDocument } from '$lib/adapter';
import type { DocumentDetail } from '$lib/api/types';
import DocumentView from './DocumentView.svelte';
import OutlineView from './OutlineView.svelte';
import SpecView from './SpecView.svelte';
import PreviewView from './PreviewView.svelte';

const detail: DocumentDetail = {
  id: 'doc-1',
  title: 'Wellbeing Instrument',
  languages: ['en'],
  status: 'draft',
  settings: {},
  deployed_snapshot_id: null,
  created_by: null,
  created_at: '2026-06-17T00:00:00Z',
  updated_at: '2026-06-17T00:00:00Z',
  state: {
    nodes: {
      sec1: {
        id: 'sec1',
        parent_id: null,
        type: 'section',
        pos: 'a0',
        current_fields: { title: 'Screening' },
        var_name: null,
        deleted: false
      },
      q1: {
        id: 'q1',
        parent_id: 'sec1',
        type: 'item',
        pos: 'b0',
        current_fields: { label: 'How are you feeling?', type: 'select_one' },
        var_name: 'mood',
        deleted: false
      },
      c1: {
        id: 'c1',
        parent_id: 'q1',
        type: 'choice',
        pos: 'c0',
        current_fields: { label: 'Good' },
        var_name: null,
        deleted: false
      }
    },
    comments: [],
    suggestions: {},
    removed_choices: []
  },
  latest_seq: 0
};

const document = toEditorDocument(detail);

let target: HTMLElement;
let component: Record<string, unknown> | null = null;

afterEach(() => {
  if (component) {
    unmount(component);
    component = null;
  }
  target?.remove();
});

function render(Component: unknown, props: Record<string, unknown>) {
  target = window.document.createElement('div');
  window.document.body.appendChild(target);
  component = mount(Component as never, { target, props }) as Record<string, unknown>;
  flushSync();
  return target;
}

describe('editor mode views render real adapter data', () => {
  it('Document mode shows the question text and choices', () => {
    const el = render(DocumentView, {
      document,
      selectedId: 'q1',
      suggesting: false,
      onSelect: () => {}
    });
    expect(el.textContent).toContain('Wellbeing Instrument');
    expect(el.textContent).toContain('How are you feeling?');
    expect(el.textContent).toContain('Good');
  });

  it('Outline mode renders the tree of sections and items', () => {
    const el = render(OutlineView, { document, selectedId: 'q1', onSelect: () => {} });
    expect(el.textContent).toContain('Screening');
  });

  it('Spec mode renders typed fields', () => {
    const el = render(SpecView, { document });
    expect(el.textContent).toContain('mood');
  });

  it('Preview mode renders the questionnaire as a respondent sees it', () => {
    const el = render(PreviewView, { document });
    expect(el.textContent).toContain('How are you feeling?');
  });
});
