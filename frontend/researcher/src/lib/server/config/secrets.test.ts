// Secret files: the same rules as crates/engine-api/src/secrets.rs.
import { afterAll, describe, expect, it } from 'vitest';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspect } from 'node:util';
import {
  ALLOW_OPEN_FILES,
  MAX_SECRET_FILE_BYTES,
  Secret,
  ownerOnly,
  readBoundedFile,
  readSecret
} from './secrets.js';

// The byte order mark, written as a code so no invisible character sits in this file.
const BOM = String.fromCharCode(0xfeff);
const VALUE = 's3cr3t-value-that-must-not-leak-0123456789';
const unix = process.platform !== 'win32';
const dir = mkdtempSync(join(tmpdir(), 'dynodoc-secrets-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

let counter = 0;
/** A private temporary file (mode 0600 where modes exist). */
function file(content: string | Uint8Array, mode = 0o600) {
  const path = join(dir, `secret-${counter++}`);
  writeFileSync(path, content);
  if (unix) chmodSync(path, mode);
  return path;
}
const lookup = (values: Record<string, string>) => (name: string) =>
  Object.hasOwn(values, name) ? values[name] : undefined;
const value = (values: Record<string, string>, name = 'KEY') => {
  const result = readSecret(lookup(values), name);
  expect(result.problem).toBeUndefined();
  return result.secret?.expose();
};
const problem = (values: Record<string, string>, name = 'KEY') => {
  const result = readSecret(lookup(values), name);
  expect(result.secret).toBeUndefined();
  return result.problem as string;
};

describe('plain variables', () => {
  it('are returned byte for byte, with no trimming', () => {
    for (const text of [VALUE, ' padded with spaces ', 'ends with newline\n'])
      expect(value({ KEY: text })).toBe(text);
  });
  it('count as unset when empty or absent', () => {
    expect(readSecret(lookup({}), 'KEY')).toEqual({});
    expect(readSecret(lookup({ KEY: '' }), 'KEY')).toEqual({});
    // An empty NAME_FILE is ignored, as if unset.
    expect(readSecret(lookup({ KEY_FILE: '' }), 'KEY')).toEqual({});
  });
});

describe('secret files', () => {
  it('are read and lose only trailing line breaks', () => {
    for (const [content, expected] of [
      [VALUE, VALUE],
      [`${VALUE}\n`, VALUE],
      [`${VALUE}\r\n`, VALUE],
      [`${VALUE}\n\r\n\n`, VALUE],
      [` ${VALUE} \n`, ` ${VALUE} `],
      [`\n${VALUE}`, `\n${VALUE}`]
    ])
      expect(value({ KEY_FILE: file(content) })).toBe(expected);
  });
  it('keep a byte order mark, as the API does', () => {
    expect(value({ KEY_FILE: file(`${BOM}${VALUE}\n`) })).toBe(`${BOM}${VALUE}`);
  });
  it('conflict with a non-empty plain variable but not with an empty one', () => {
    const path = file(VALUE);
    const message = problem({ KEY: VALUE, KEY_FILE: path });
    expect(message).toBe('KEY and KEY_FILE are both set; set exactly one');
    expect(value({ KEY: '', KEY_FILE: path })).toBe(VALUE);
  });
  it('name the variable and the path when missing', () => {
    const path = join(dir, 'absent');
    const message = problem({ KEY_FILE: path });
    expect(message).toBe(`KEY_FILE (${path}): cannot open: no such file or directory`);
  });
  it('refuse a directory', () => {
    expect(problem({ KEY_FILE: dir })).toMatch(
      /^KEY_FILE \(.*\): (cannot open|is not a regular file)/
    );
  });
  it('refuse oversized files and accept one at the limit', () => {
    expect(problem({ KEY_FILE: file('a'.repeat(MAX_SECRET_FILE_BYTES + 1)) })).toContain(
      `is larger than ${MAX_SECRET_FILE_BYTES} bytes`
    );
    expect(value({ KEY_FILE: file('a'.repeat(MAX_SECRET_FILE_BYTES)) })).toHaveLength(
      MAX_SECRET_FILE_BYTES
    );
  });
  it('refuse empty files, including one holding only a line break', () => {
    for (const content of ['', '\n', '\r\n\r\n'])
      expect(problem({ KEY_FILE: file(content) })).toContain('is empty');
  });
  it('refuse content that is not UTF-8 without echoing it', () => {
    const message = problem({ KEY_FILE: file(new Uint8Array([0xff, 0xfe, 0x78, 0x79])) });
    expect(message).toContain('is not valid UTF-8 text');
  });
  it('never put the value, or a piece of it, in a message', () => {
    const big = file(VALUE + 'a'.repeat(MAX_SECRET_FILE_BYTES));
    const ok = file(VALUE);
    for (const message of [
      problem({ KEY_FILE: big }),
      problem({ KEY: VALUE, KEY_FILE: ok }),
      problem({ KEY_FILE: file(new Uint8Array([...Buffer.from(VALUE), 0xff])) })
    ]) {
      expect(message).not.toContain(VALUE);
      expect(message).not.toContain(VALUE.slice(0, 8));
    }
  });
});

describe('the Secret wrapper', () => {
  it('shows *** when printed, logged or serialised', () => {
    const secret = new Secret(VALUE);
    expect(secret.expose()).toBe(VALUE);
    for (const text of [String(secret), JSON.stringify({ secret }), inspect(secret), `${secret}`])
      expect(text).not.toContain(VALUE);
    expect(inspect({ nested: { secret } })).toContain('Secret(***)');
  });
});

describe('modes', () => {
  it('treat only owner-only modes as private', () => {
    for (const mode of [0o400, 0o600, 0o100600, 0o200, 0o700]) expect(ownerOnly(mode)).toBe(true);
    for (const mode of [0o440, 0o640, 0o644, 0o604, 0o666, 0o444, 0o070, 0o007])
      expect(ownerOnly(mode)).toBe(false);
  });
  it('are not checked on Windows', () => {
    const path = file(VALUE, 0o666);
    const result = readBoundedFile('KEY_FILE', path, {
      maxBytes: 100,
      ownerOnly: true,
      allowOpen: false,
      platform: 'win32'
    });
    expect(result).toEqual({ text: VALUE });
  });
  it.skipIf(!unix)('refuse a file group or others can access, unless allowed', () => {
    for (const mode of [0o644, 0o640, 0o604, 0o660, 0o666]) {
      const path = file(VALUE, mode);
      const message = problem({ KEY_FILE: path });
      expect(message).toContain('accessible by group or others');
      expect(message).toContain('KEY_FILE');
      expect(message).not.toContain(VALUE);
      expect(value({ KEY_FILE: path, [ALLOW_OPEN_FILES]: 'true' })).toBe(VALUE);
      // Only the exact text "true" opens the check.
      expect(problem({ KEY_FILE: path, [ALLOW_OPEN_FILES]: 'yes' })).toContain('accessible');
    }
    expect(value({ KEY_FILE: file(VALUE, 0o400) })).toBe(VALUE);
  });
  it.skipIf(!unix || process.getuid?.() === 0)('report an unreadable file', () => {
    const message = problem({ KEY_FILE: file(VALUE, 0o000) });
    expect(message).toContain('cannot open: permission denied');
  });
});
