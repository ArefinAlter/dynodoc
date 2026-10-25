// ULIDs for new nodes (the engine's node ids are ULIDs): 48 bits of milliseconds and 80 random
// bits, Crockford base32. No dependency.
import { randomBytes } from 'node:crypto';

const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

export function ulid(now = Date.now(), random = randomBytes(10)) {
  let time = '';
  let t = now;
  for (let i = 0; i < 10; i++) {
    time = ALPHABET[t % 32] + time;
    t = Math.floor(t / 32);
  }
  let bits = 0n;
  for (const b of random) bits = (bits << 8n) | BigInt(b);
  let rand = '';
  for (let i = 0; i < 16; i++) {
    rand = ALPHABET[Number(bits & 31n)] + rand;
    bits >>= 5n;
  }
  return time + rand;
}

/** The web editor's midpoint between two position keys (frontend lib/workspace/changes.ts). */
export function between(left, right) {
  let prefix = '';
  for (let i = 0; i < 200; i++) {
    const a = left.charCodeAt(i) || 32;
    const b = (right ?? '').charCodeAt(i) || 126;
    if (b - a > 1) return prefix + String.fromCharCode(Math.floor((a + b) / 2));
    prefix += String.fromCharCode(a);
  }
  return left + 'P';
}

export const seqPos = (i, width = 8) => String(i).padStart(width, '0');
