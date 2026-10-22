// Self-signed certificates made at test time, with no openssl and nothing checked in: a small
// DER encoder around node:crypto. For tests only (never imported by the server).
import { generateKeyPairSync, sign } from 'node:crypto';

const len = (n: number) => {
  if (n < 0x80) return Buffer.from([n]);
  const bytes: number[] = [];
  for (let v = n; v > 0; v = Math.floor(v / 256)) bytes.unshift(v & 0xff);
  return Buffer.from([0x80 | bytes.length, ...bytes]);
};
const tlv = (tag: number, ...parts: Buffer[]) => {
  const body = Buffer.concat(parts);
  return Buffer.concat([Buffer.from([tag]), len(body.length), body]);
};
const seq = (...parts: Buffer[]) => tlv(0x30, ...parts);
const set = (...parts: Buffer[]) => tlv(0x31, ...parts);
const oid = (dotted: string) => {
  const [a, b, ...rest] = dotted.split('.').map(Number);
  const out = [a * 40 + b];
  for (const part of rest) {
    const chunk = [part & 0x7f];
    for (let v = part >> 7; v > 0; v >>= 7) chunk.unshift((v & 0x7f) | 0x80);
    out.push(...chunk);
  }
  return tlv(0x06, Buffer.from(out));
};
const integer = (value: Buffer) =>
  tlv(0x02, value[0] & 0x80 ? Buffer.concat([Buffer.from([0]), value]) : value);
const utcTime = (date: Date) => {
  const p = (n: number) => String(n).padStart(2, '0');
  return tlv(
    0x17,
    Buffer.from(
      `${p(date.getUTCFullYear() % 100)}${p(date.getUTCMonth() + 1)}${p(date.getUTCDate())}` +
        `${p(date.getUTCHours())}${p(date.getUTCMinutes())}${p(date.getUTCSeconds())}Z`
    )
  );
};
const name = (cn: string) => seq(set(seq(oid('2.5.4.3'), tlv(0x0c, Buffer.from(cn)))));

export interface TestCertificate {
  key: string;
  cert: string;
}

/**
 * A self-signed ECDSA P-256 certificate for the given names and addresses. Trust it by giving
 * it as the `ca` of the client. `notBefore` / `notAfter` default to a window around now.
 */
export function selfSigned(options: {
  dns?: string[];
  ip?: string[];
  commonName?: string;
  notBefore?: Date;
  notAfter?: Date;
}): TestCertificate {
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const ecdsaSha256 = seq(oid('1.2.840.10045.4.3.2'));
  const names = [
    ...(options.dns ?? []).map((n) => tlv(0x82, Buffer.from(n))),
    ...(options.ip ?? []).map((address) => tlv(0x87, Buffer.from(address.split('.').map(Number))))
  ];
  const extensions = tlv(
    0xa3,
    seq(
      seq(oid('2.5.29.17'), tlv(0x04, seq(...names))),
      seq(oid('2.5.29.19'), tlv(0x01, Buffer.from([0xff])), tlv(0x04, seq(tlv(0x01, Buffer.from([0xff])))))
    )
  );
  const subject = name(options.commonName ?? options.dns?.[0] ?? 'dynodoc test');
  const now = Date.now();
  const tbs = seq(
    tlv(0xa0, integer(Buffer.from([2]))),
    integer(Buffer.from([1, 2, 3, 4, 5, 6, 7, 8])),
    ecdsaSha256,
    subject,
    seq(
      utcTime(options.notBefore ?? new Date(now - 3_600_000)),
      utcTime(options.notAfter ?? new Date(now + 86_400_000))
    ),
    subject,
    Buffer.from(publicKey.export({ type: 'spki', format: 'der' })),
    extensions
  );
  const signature = sign('sha256', tbs, privateKey);
  const der = seq(tbs, ecdsaSha256, tlv(0x03, Buffer.concat([Buffer.from([0]), signature])));
  const pem = (label: string, body: Buffer) =>
    `-----BEGIN ${label}-----\n${body
      .toString('base64')
      .replace(/(.{64})/g, '$1\n')
      .replace(/\n$/, '')}\n-----END ${label}-----\n`;
  return {
    cert: pem('CERTIFICATE', der),
    key: privateKey.export({ type: 'pkcs8', format: 'pem' }).toString()
  };
}
