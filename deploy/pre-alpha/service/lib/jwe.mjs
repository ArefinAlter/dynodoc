// Reads the web tier's session cookie (`dynodoc_session`): a compact JWE with `alg: dir` and
// `enc: A256GCM` whose key is SHA-256 of SESSION_SECRET (frontend lib/server/session.ts). The
// service only reads it, to learn whose request this is; it never writes or renews a session.
import { createDecipheriv, createHash } from 'node:crypto';

export function sessionKey(secret) {
  if (!secret || secret.length < 32) throw new Error('SESSION_SECRET must contain at least 32 characters');
  return createHash('sha256').update(secret).digest();
}

const b64 = (s) => Buffer.from(s, 'base64url');

/** The decrypted claims, or null for anything that is not a valid, unexpired session. */
export function readSession(value, key, now = Date.now()) {
  if (typeof value !== 'string' || value.length > 8192) return null;
  const parts = value.split('.');
  if (parts.length !== 5) return null;
  const [protectedHeader, encryptedKey, iv, ciphertext, tag] = parts;
  try {
    const header = JSON.parse(b64(protectedHeader).toString('utf8'));
    if (header.alg !== 'dir' || header.enc !== 'A256GCM' || encryptedKey !== '') return null;
    const decipher = createDecipheriv('aes-256-gcm', key, b64(iv));
    decipher.setAAD(Buffer.from(protectedHeader, 'ascii'));
    decipher.setAuthTag(b64(tag));
    const plain = Buffer.concat([decipher.update(b64(ciphertext)), decipher.final()]);
    const claims = JSON.parse(plain.toString('utf8'));
    if (typeof claims.exp === 'number' && claims.exp * 1000 < now) return null;
    if (typeof claims.access_token !== 'string') return null;
    return claims;
  } catch {
    return null;
  }
}

/** One cookie's value from a Cookie header. */
export function cookieValue(header, name) {
  if (typeof header !== 'string') return undefined;
  for (const part of header.split(';')) {
    const i = part.indexOf('=');
    if (i < 0) continue;
    if (part.slice(0, i).trim() === name) return part.slice(i + 1).trim();
  }
  return undefined;
}
