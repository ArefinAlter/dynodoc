// DYNODOC_OFFICE_JS_DIR: the directory that holds an operator-supplied copy of Office.js.
import { afterAll, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadConfig } from './schema.js';

const SESSION = 'a3f1c9d27b6e4058912ce7d4b0a6f358a3f1c9d27b6e4058912ce7d4b0a6f358';
const SERVICE = '0e8b5d3c9a71f26480bd7e1c35a9f2640e8b5d3c9a71f26480bd7e1c35a9f264';
const root = mkdtempSync(join(tmpdir(), 'dynodoc-officejs-config-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));

const selfhosted = (extra: Record<string, string> = {}) => ({
  DYNODOC_MODE: 'selfhosted',
  ORIGIN: 'https://dynodoc.example.test',
  SESSION_SECRET: SESSION,
  WEB_SERVICE_KEY: SERVICE,
  DYNODOC_MAIL_TRANSPORT: 'smtp',
  SMTP_HOST: 'mail.example.test',
  SMTP_FROM: 'docs@dynodoc.example.test',
  ...extra
});
function directory(
  name: string,
  files: Record<string, string> = { 'office.js': '/* stand-in */' }
) {
  const path = join(root, name);
  mkdirSync(path, { recursive: true });
  for (const [file, text] of Object.entries(files)) writeFileSync(join(path, file), text);
  return path;
}

describe('DYNODOC_OFFICE_JS_DIR', () => {
  it('is off by default: the Microsoft CDN stays in use', () => {
    const result = loadConfig(selfhosted());
    expect(result.ok, result.problems.join('\n')).toBe(true);
    expect(result.config.officeJsDir).toBeNull();
  });
  it('is accepted when the directory holds office.js', () => {
    const dir = directory('good');
    const result = loadConfig(selfhosted({ DYNODOC_OFFICE_JS_DIR: ` ${dir} ` }));
    expect(result.ok, result.problems.join('\n')).toBe(true);
    expect(result.config.officeJsDir).toBe(dir);
  });
  it.each([
    ['a relative path', () => 'office-js', /must be an absolute path/],
    [
      'a path that does not exist',
      () => join(root, 'nowhere'),
      /not a directory this process can read/
    ],
    [
      'a file',
      () => {
        const path = join(root, 'a-file');
        writeFileSync(path, 'x');
        return path;
      },
      /is not a directory/
    ],
    [
      'a directory without office.js',
      () => directory('empty', { 'other.js': 'x' }),
      /holds no office\.js/
    ],
    ['an empty office.js', () => directory('blank', { 'office.js': '' }), /holds no office\.js/],
    [
      'an office.js that is a directory',
      () => {
        const dir = directory('odd', {});
        mkdirSync(join(dir, 'office.js'));
        return dir;
      },
      /holds no office\.js/
    ]
  ])('stops the server for %s, saying what to do', (_name, make, reason) => {
    const result = loadConfig(selfhosted({ DYNODOC_OFFICE_JS_DIR: make() }));
    expect(result.ok).toBe(false);
    const problems = result.problems.filter((p) => p.startsWith('DYNODOC_OFFICE_JS_DIR'));
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatch(reason);
    expect(result.config.officeJsDir).toBeNull();
  });
  it('is reported and ignored in hosted mode', () => {
    const dir = directory('hosted-ignored');
    const result = loadConfig({
      SESSION_SECRET: SESSION,
      WEB_SERVICE_KEY: SERVICE,
      DYNODOC_OFFICE_JS_DIR: dir
    });
    expect(result.config.officeJsDir).toBeNull();
    expect(result.warnings.some((w) => w.startsWith('DYNODOC_OFFICE_JS_DIR is only used'))).toBe(
      true
    );
  });
  it('is not mistaken for a typo of another setting', () => {
    const dir = directory('typo');
    const result = loadConfig(selfhosted({ DYNODOC_OFFICE_JS_DIR: dir }));
    expect(result.warnings.filter((w) => w.includes('DYNODOC_OFFICE_JS_DIR'))).toEqual([]);
  });
});
