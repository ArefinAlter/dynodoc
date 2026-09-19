import { stableJson } from './equality';
import { ulid } from 'ulid';
import type { JSONContent } from '@tiptap/core';
import type { DocumentState, EventPayload } from '$lib/api/types';
import { documentNodes } from './api';
import { textOf, type TextBlock } from './files';
export function cloneState(state: DocumentState): DocumentState {
  return JSON.parse(JSON.stringify(state));
}
export function richContent(state: DocumentState): JSONContent {
  return {
    type: 'doc',
    content: documentNodes(state).map((n) => ({
      ...((n.current_fields.content as JSONContent) || {
        type: 'paragraph',
        content: n.current_fields.label
          ? [{ type: 'text', text: String(n.current_fields.label) }]
          : []
      }),
      attrs: { ...((n.current_fields.content as JSONContent)?.attrs || {}), blockId: n.id }
    }))
  };
}
export function richChanges(state: DocumentState, content: JSONContent): EventPayload[] {
  const ops: EventPayload[] = [];
  const prior = documentNodes(state);
  const root = Object.values(state.nodes).find((n) => !n.deleted && n.type === 'form');
  if (!root) return ops;
  const seen = new Set<string>();
  let left = '';
  for (let block of content.content || []) {
    let id = String(block.attrs?.blockId || ulid());
    if (seen.has(id)) id = ulid();
    seen.add(id);
    block = { ...block, attrs: { ...block.attrs, blockId: id } };
    const node = state.nodes[id];
    let pos = node?.pos;
    // Keep existing keys when their relative order is unchanged. New blocks get a
    // dense lexical key between their neighbors without moving unrelated blocks.
    const index = (content.content || []).findIndex((b) => b === block || b.attrs?.blockId === id);
    const next = (content.content || [])
      .slice(index + 1)
      .map((b) => state.nodes[String(b.attrs?.blockId)])
      .find((n) => n && !n.deleted && n.pos > left)?.pos;
    if (!pos || pos <= left || (next && pos >= next)) pos = between(left, next);
    const label = textOf(block as TextBlock);
    if (!node)
      ops.push({
        type: 'NodeCreated',
        node_id: id,
        node_type: 'item',
        parent_id: root.id,
        pos,
        fields: { content: block, label }
      });
    else {
      if (node.deleted) ops.push({ type: 'NodeRestored', node_id: id });
      if (stableJson(node.current_fields.content) !== stableJson(block)) {
        ops.push({ type: 'FieldEdited', node_id: id, field: 'content', value: block });
        ops.push({ type: 'FieldEdited', node_id: id, field: 'label', value: label });
      }
      if (node.pos !== pos)
        ops.push({ type: 'NodeMoved', node_id: id, new_parent_id: root.id, new_pos: pos });
    }
    left = pos;
  }
  for (const node of prior)
    if (!seen.has(node.id)) ops.push({ type: 'NodeDeleted', node_id: node.id });
  return ops;
}
// Fixed alphabet midpoint, compatible with the original numeric import keys.
export function between(left: string, right?: string): string {
  let prefix = '';
  for (let i = 0; i < 200; i++) {
    const a = left.charCodeAt(i) || 32;
    const b = right?.charCodeAt(i) || 126;
    if (b - a > 1) return prefix + String.fromCharCode(Math.floor((a + b) / 2));
    prefix += String.fromCharCode(a);
  }
  return left + 'P';
}
export function stateChanges(from: DocumentState, to: DocumentState): EventPayload[] {
  const ops: EventPayload[] = [];
  const live = Object.values(to.nodes).filter((n) => !n.deleted);
  const depth = (id: string): number => {
    let n = to.nodes[id],
      d = 0;
    while (n?.parent_id && d++ < live.length) n = to.nodes[n.parent_id];
    return d;
  };
  live.sort((a, b) => depth(a.id) - depth(b.id));
  for (const node of live) {
    const prev = from.nodes[node.id];
    if (!prev) {
      ops.push({
        type: 'NodeCreated',
        node_id: node.id,
        node_type: node.type,
        parent_id: node.parent_id,
        pos: node.pos,
        fields: node.current_fields,
        var_name: node.var_name
      });
      continue;
    }
    if (prev.deleted) ops.push({ type: 'NodeRestored', node_id: node.id });
    if (prev.parent_id !== node.parent_id || prev.pos !== node.pos)
      ops.push({
        type: 'NodeMoved',
        node_id: node.id,
        new_parent_id: node.parent_id,
        new_pos: node.pos
      });
    for (const field of new Set([
      ...Object.keys(prev.current_fields),
      ...Object.keys(node.current_fields)
    ])) {
      if (
        stableJson(prev.current_fields[field] ?? null) !==
        stableJson(node.current_fields[field] ?? null)
      )
        ops.push({
          type: 'FieldEdited',
          node_id: node.id,
          field,
          value: node.current_fields[field] ?? null
        });
    }
    if (prev.var_name !== node.var_name)
      ops.push({
        type: 'FieldEdited',
        node_id: node.id,
        field: 'var_name',
        value: node.var_name || ''
      });
  }
  for (const node of Object.values(from.nodes)
    .filter((n) => !n.deleted)
    .sort((a, b) => depth(b.id) - depth(a.id)))
    if (!to.nodes[node.id] || to.nodes[node.id].deleted)
      ops.push({ type: 'NodeDeleted', node_id: node.id });
  return ops;
}
