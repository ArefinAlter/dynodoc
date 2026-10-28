// A linked copy gets new engine-global node IDs. The map belongs to that document pair; existing
// nodes/history are never renamed. Rich-text/content fields remain content, not engine identities.
import type { Nodes } from './remote-map.ts';

export type NodeIds = Record<string, string>;
const own = (o: object, key: string): boolean => Object.prototype.hasOwnProperty.call(o, key);
export function freshNodeId(): string {
  const alphabet = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  let time = Date.now(), out = '';
  for (let i = 0; i < 10; i++) { out = alphabet[time % 32] + out; time = Math.floor(time / 32); }
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  for (const byte of bytes) out += alphabet[byte % 32];
  return out;
}
export function checkedIds(value: NodeIds = {}): NodeIds {
  const map: NodeIds = Object.create(null), reverse = new Set<string>();
  for (const [local, remote] of Object.entries(value)) {
    if (!local || typeof remote !== 'string' || !remote || reverse.has(remote)) throw new Error('The saved node mapping is invalid. Restore the workspace backup before syncing.');
    map[local] = remote; reverse.add(remote);
  }
  return map;
}
export function inverseIds(map: NodeIds): NodeIds {
  const out: NodeIds = Object.create(null);
  for (const [local, remote] of Object.entries(map)) out[remote] = local;
  return out;
}
export function pairedIds(existing: NodeIds | undefined, local: Nodes[], remote: Nodes[]): NodeIds {
  const map = checkedIds(existing), reverse = inverseIds(map);
  // Old links used identical IDs. Preserve that convention for their existing common nodes only.
  const remoteIds = new Set(remote.flatMap(nodes => Object.keys(nodes)));
  for (const id of local.flatMap(nodes => Object.keys(nodes))) {
    if (own(map, id)) continue;
    const target = existing === undefined && remoteIds.has(id) && !own(reverse, id) ? id : freshNodeId();
    map[id] = target; reverse[target] = id;
  }
  for (const id of remoteIds) {
    if (own(reverse, id)) continue;
    const target = freshNodeId(); map[target] = id; reverse[id] = target;
  }
  return map;
}
export function mapNodes(nodes: Nodes, ids: NodeIds): Nodes {
  const out: Nodes = Object.create(null);
  const key = (id: string): string => { if (!own(ids, id)) throw new Error('A node is missing from the saved sync mapping.'); return ids[id]; };
  for (const node of Object.values(nodes)) {
    const id = key(node.id);
    out[id] = { ...node, id, parent_id: node.parent_id ? key(node.parent_id) : null };
  }
  return out;
}
