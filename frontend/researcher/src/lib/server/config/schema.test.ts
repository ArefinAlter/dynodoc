// The web configuration: every setting, every error, aggregation and the hosted defaults.
import { afterAll, describe, expect, it } from 'vitest';
import { chmodSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspect } from 'node:util';
import {
  KNOWN_DYNODOC_NAMES,
  NOTICE_MAX_CHARS,
  NOTICE_MAX_LINES,
  formatProblems,
  likelyTypoOf,
  loadConfig,
  weakSecretReason
} from './schema.js';

const SESSION = 'a3f1c9d27b6e4058912ce7d4b0a6f358a3f1c9d27b6e4058912ce7d4b0a6f358';
const SERVICE = '0e8b5d3c9a71f26480bd7e1c35a9f2640e8b5d3c9a71f26480bd7e1c35a9f264';
const dir = mkdtempSync(join(tmpdir(), 'dynodoc-config-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
let counter = 0;
const file = (content: string, mode = 0o600) => {
  const path = join(dir, `f-${counter++}`);
  writeFileSync(path, content);
  if (process.platform !== 'win32') chmodSync(path, mode);
  return path;
};

type Env = Record<string, string | undefined>;
const hosted = (extra: Env = {}): Env => ({
  SESSION_SECRET: SESSION,
  WEB_SERVICE_KEY: SERVICE,
  ...extra
});
const selfhosted = (extra: Env = {}): Env => ({
  DYNODOC_MODE: 'selfhosted',
  ORIGIN: 'https://dynodoc.example.test',
  SESSION_SECRET: SESSION,
  WEB_SERVICE_KEY: SERVICE,
  ...extra
});
/** Load and demand exactly these problems (as substrings, in any order). */
function problems(env: Env, ...expected: string[]) {
  const result = loadConfig(env);
  expect(result.ok, result.problems.join('\n')).toBe(expected.length === 0);
  expect(result.problems).toHaveLength(expected.length);
  for (const part of expected)
    expect(
      result.problems.filter((p) => p.includes(part)),
      `${part}\n${result.problems.join('\n')}`
    ).toHaveLength(1);
  return result;
}
const warned = (env: Env, part: string) =>
  expect(
    loadConfig(env).warnings.some((w) => w.includes(part)),
    part
  ).toBe(true);
const quiet = (env: Env, part: string) =>
  expect(
    loadConfig(env).warnings.some((w) => w.includes(part)),
    part
  ).toBe(false);

describe('hosted mode (the default)', () => {
  it('keeps the hosted service exactly as it was', () => {
    const { config, warnings, ok } = loadConfig(hosted({ ORIGIN: 'https://app.dynodoc.online' }));
    expect(ok).toBe(true);
    expect(warnings).toEqual([]);
    expect(config.mode).toBe('hosted');
    expect(config.marketing).toBe(true);
    expect(config.indexable).toBe(true);
    expect(config.engineApiUrl).toBe('http://127.0.0.1:8080');
    expect(config.connectSrc).toEqual(['https://app.dynodoc.online']);
    expect(config.rootRedirect).toEqual({
      fromHost: 'app.dynodoc.online',
      to: 'https://dynodoc.online'
    });
    expect(config.hostedDomain).toBe('dynodoc.online');
    expect(config.telemetry).toEqual({
      enabled: true,
      origins: [
        ['https://dynodoc.online', 'homepage'],
        ['https://www.dynodoc.online', 'homepage'],
        ['https://app.dynodoc.online', 'workspace'],
        ['https://admin.dynodoc.online', 'admin']
      ]
    });
    expect(config.site).toEqual({
      mode: 'hosted',
      brand: 'Dynodoc',
      notice: null,
      indexable: true,
      homeUrl: 'https://dynodoc.online',
      helpUrl: 'https://dynodoc.online/help',
      workspaceUrl: 'https://app.dynodoc.online',
      themeCookieDomain: '.dynodoc.online',
      telemetry: {
        endpoint: 'https://app.dynodoc.online/telemetry',
        hosts: [
          'dynodoc.online',
          'www.dynodoc.online',
          'app.dynodoc.online',
          'admin.dynodoc.online'
        ]
      },
      emailSignIn: true
    });
  });
  it('does not require ORIGIN or WEB_SERVICE_KEY (it only reports the missing key)', () => {
    const result = problems({ SESSION_SECRET: SESSION });
    expect(result.warnings.some((w) => w.startsWith('WEB_SERVICE_KEY is not set'))).toBe(true);
  });
  it('is the mode when DYNODOC_MODE is blank or says hosted', () => {
    for (const mode of [undefined, '', '  ', 'hosted', 'HOSTED'])
      expect(loadConfig(hosted({ DYNODOC_MODE: mode })).config.mode).toBe('hosted');
  });
  it('lets the operator turn page metrics off, and reports self-hosted settings it ignores', () => {
    const off = loadConfig(hosted({ DYNODOC_TELEMETRY: 'off' })).config;
    expect(off.telemetry.enabled).toBe(false);
    expect(off.site.telemetry).toBeNull();
    const ignored = hosted({ DYNODOC_BRAND_NAME: 'Acme Docs', DYNODOC_ORG_NAME: 'Acme' });
    const result = loadConfig(ignored);
    expect(result.config.site.brand).toBe('Dynodoc');
    expect(result.config.site.notice).toBeNull();
    warned(ignored, 'DYNODOC_BRAND_NAME is only used when DYNODOC_MODE=selfhosted');
  });
  it('treats ORIGIN, ENGINE_API_URL slashes as warnings, not failures', () => {
    expect(loadConfig(hosted({ ORIGIN: 'https://app.test/' })).ok).toBe(true);
    warned(hosted({ ORIGIN: 'https://app.test/' }), 'ORIGIN must be an origin');
    expect(loadConfig(hosted({ ENGINE_API_URL: 'http://api:8080/' })).ok).toBe(true);
    warned(hosted({ ENGINE_API_URL: 'http://api:8080/' }), 'must not end with a slash');
  });
});

describe('DYNODOC_MODE', () => {
  it('accepts only hosted and selfhosted', () => {
    problems(hosted({ DYNODOC_MODE: 'self-hosted' }), 'DYNODOC_MODE must be hosted or selfhosted');
    problems(hosted({ DYNODOC_MODE: 'cloud' }), 'DYNODOC_MODE must be hosted or selfhosted');
    expect(loadConfig(selfhosted({ DYNODOC_MODE: ' SelfHosted ' })).config.mode).toBe('selfhosted');
  });
  it('turns the hosted-only parts off in selfhosted mode', () => {
    const { config, warnings } = loadConfig(selfhosted());
    expect(warnings).toEqual([]);
    expect(config.marketing).toBe(false);
    expect(config.indexable).toBe(false);
    expect(config.rootRedirect).toBeNull();
    expect(config.hostedDomain).toBeNull();
    expect(config.workspaceUrl).toBeUndefined();
    expect(config.connectSrc).toEqual(['https://dynodoc.example.test']);
    expect(config.telemetry.enabled).toBe(false);
    expect(config.site).toEqual({
      mode: 'selfhosted',
      brand: 'Dynodoc',
      notice: null,
      indexable: false,
      homeUrl: '/',
      helpUrl: null,
      workspaceUrl: '',
      themeCookieDomain: null,
      telemetry: null,
      emailSignIn: true
    });
  });
  it('names no hosted hostname anywhere in a selfhosted configuration', () => {
    const text = JSON.stringify(loadConfig(selfhosted({ DYNODOC_TELEMETRY: 'on' })).config);
    expect(text).not.toMatch(/dynodoc\.online|vernesystems|Verne/i);
  });
});

describe('SESSION_SECRET', () => {
  it.each([
    ['hosted', hosted],
    ['selfhosted', selfhosted]
  ] as const)('is validated at start in %s mode', (_name, make) => {
    problems(make({ SESSION_SECRET: undefined }), 'SESSION_SECRET is not set');
    problems(make({ SESSION_SECRET: '' }), 'SESSION_SECRET is not set');
    problems(make({ SESSION_SECRET: 'x7Kp2mQ9vL4nR8sT' }), 'SESSION_SECRET must be at least 32');
    problems(make({ SESSION_SECRET: ' '.repeat(40) }), 'SESSION_SECRET must not be blank');
    problems(make({ SESSION_SECRET: 'a'.repeat(64) }), 'SESSION_SECRET uses too few distinct');
    problems(make({ SESSION_SECRET: 'abcd'.repeat(10) }), 'SESSION_SECRET uses too few distinct');
    problems(make({ SESSION_SECRET: 'abcdefgh'.repeat(5) }), 'SESSION_SECRET repeats a short');
  });
  it('rejects documented placeholders', () => {
    for (const text of [
      'change-me-to-a-long-random-value-please',
      'CHANGEME-CHANGEME-CHANGEME-CHANGEME-1234',
      'REPLACE_WITH_RUNTIME_PASSWORD_0123456789',
      'replace-with-a-random-32-byte-secret-value',
      '<paste a random value of 32 chars here>',
      'this-is-a-placeholder-for-the-real-secret'
    ])
      problems(selfhosted({ SESSION_SECRET: text }), 'SESSION_SECRET looks like a placeholder');
  });
  it('accepts random values and the readable keys the test stacks use', () => {
    for (const text of [
      SESSION,
      'isolated-ci-session-key-never-for-production',
      'Zk3Wq8Lm1Xv5Rt9Bn2Hy7Cd4Fg6Js0Pa'
    ])
      expect(loadConfig(selfhosted({ SESSION_SECRET: text })).ok, text).toBe(true);
  });
  it('is read from SESSION_SECRET_FILE with the API rules', () => {
    const path = file(`${SESSION}\n`);
    const { config, ok } = loadConfig(
      selfhosted({ SESSION_SECRET: undefined, SESSION_SECRET_FILE: path })
    );
    expect(ok).toBe(true);
    expect(config.secrets.session?.expose()).toBe(SESSION);
    problems(
      selfhosted({ SESSION_SECRET_FILE: file(SESSION) }),
      'SESSION_SECRET and SESSION_SECRET_FILE are both set'
    );
    problems(
      selfhosted({ SESSION_SECRET: undefined, SESSION_SECRET_FILE: join(dir, 'missing') }),
      'SESSION_SECRET_FILE'
    );
    // A file that holds a placeholder is as weak as the same text in the environment.
    problems(
      selfhosted({ SESSION_SECRET: undefined, SESSION_SECRET_FILE: file('changeme'.repeat(6)) }),
      'SESSION_SECRET looks like a placeholder'
    );
  });
  it('keeps the exact text for the cookie key (no trimming of a plain value)', () => {
    const padded = ` ${SESSION} `;
    expect(loadConfig(hosted({ SESSION_SECRET: padded })).config.secrets.session?.expose()).toBe(
      padded
    );
  });
  it('is judged on its own text, whatever its length in UTF-16 units', () => {
    expect(weakSecretReason('é'.repeat(20))).toContain('too few distinct');
    expect(weakSecretReason('ab'.repeat(5))).toBe('must be at least 32 characters');
    expect(weakSecretReason(SESSION)).toBeUndefined();
  });
});

describe('WEB_SERVICE_KEY and the other secrets', () => {
  it('is required in selfhosted mode and checked like the session secret', () => {
    problems(selfhosted({ WEB_SERVICE_KEY: undefined }), 'WEB_SERVICE_KEY is not set');
    problems(selfhosted({ WEB_SERVICE_KEY: 'short' }), 'WEB_SERVICE_KEY must be at least 32');
    problems(hosted({ WEB_SERVICE_KEY: 'short' }), 'WEB_SERVICE_KEY must be at least 32');
  });
  it('may be the same value as the session secret in hosted mode only as a warning', () => {
    warned(hosted({ WEB_SERVICE_KEY: SESSION }), 'same value');
    problems(selfhosted({ WEB_SERVICE_KEY: SESSION }), 'same value');
  });
  it('reads every secret from NAME_FILE', () => {
    const env = selfhosted({
      SESSION_SECRET: undefined,
      WEB_SERVICE_KEY: undefined,
      SESSION_SECRET_FILE: file(SESSION),
      WEB_SERVICE_KEY_FILE: file(`${SERVICE}\r\n`),
      RESEND_API_KEY_FILE: file('re_test_key_0123456789\n'),
      GOOGLE_CLIENT_SECRET_FILE: file('google-secret-0123456789\n'),
      CONVERTER_KEY_FILE: file('converter-key-0123456789\n'),
      EMAIL_FROM: 'Docs <docs@example.test>',
      GOOGLE_CLIENT_ID: 'id',
      CONVERTER_URL: 'http://converter:8090'
    });
    const { config, ok, warnings } = loadConfig(env);
    expect(ok).toBe(true);
    expect(warnings).toEqual([]);
    expect(config.secrets.webServiceKey?.expose()).toBe(SERVICE);
    expect(config.secrets.resendApiKey?.expose()).toBe('re_test_key_0123456789');
    expect(config.secrets.googleClientSecret?.expose()).toBe('google-secret-0123456789');
    expect(config.secrets.converterKey?.expose()).toBe('converter-key-0123456789');
  });
  it('reports a bad file for any secret, and both forms set', () => {
    problems(selfhosted({ RESEND_API_KEY_FILE: join(dir, 'nope') }), 'RESEND_API_KEY_FILE');
    problems(
      selfhosted({ GOOGLE_CLIENT_SECRET: 'a', GOOGLE_CLIENT_SECRET_FILE: file('b') }),
      'GOOGLE_CLIENT_SECRET and GOOGLE_CLIENT_SECRET_FILE are both set'
    );
    problems(
      selfhosted({ CONVERTER_KEY_FILE: file('a'.repeat(16 * 1024 + 1)) }),
      'is larger than 16384 bytes'
    );
    problems(selfhosted({ CONVERTER_KEY_FILE: file('') }), 'is empty');
  });
  it('explains a half-set pair without failing', () => {
    warned(selfhosted({ RESEND_API_KEY: 'k' }), 'RESEND_API_KEY and EMAIL_FROM need both');
    warned(selfhosted({ EMAIL_FROM: 'a@b.test' }), 'RESEND_API_KEY and EMAIL_FROM need both');
    warned(selfhosted({ GOOGLE_CLIENT_ID: 'id' }), 'GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET');
    warned(selfhosted({ CONVERTER_URL: 'http://c' }), 'CONVERTER_URL and CONVERTER_KEY');
    quiet(selfhosted(), 'need both');
  });
});

describe('origins and addresses', () => {
  it('requires ORIGIN in selfhosted mode and accepts only a bare origin', () => {
    problems(selfhosted({ ORIGIN: undefined }), 'ORIGIN is not set');
    for (const bad of [
      'dynodoc.example.test',
      'https://dynodoc.example.test/',
      'https://dynodoc.example.test/app',
      'https://dynodoc.example.test?x=1',
      'ftp://dynodoc.example.test',
      'https://user:pass@dynodoc.example.test'
    ])
      problems(selfhosted({ ORIGIN: bad }), 'ORIGIN');
    expect(
      loadConfig(selfhosted({ ORIGIN: 'https://dynodoc.example.test:8443' })).config.origin
    ).toBe('https://dynodoc.example.test:8443');
  });
  it('warns about plain HTTP except on loopback', () => {
    warned(selfhosted({ ORIGIN: 'http://dynodoc.corp.test' }), 'uses plain HTTP');
    quiet(selfhosted({ ORIGIN: 'http://127.0.0.1:4173' }), 'uses plain HTTP');
    quiet(selfhosted({ ORIGIN: 'http://localhost:3000' }), 'uses plain HTTP');
  });
  it('lists alternative origins, drops duplicates and checks each', () => {
    const { config } = loadConfig(
      selfhosted({
        DYNODOC_ALT_ORIGINS:
          'https://docs.example.test, https://dynodoc.example.test,https://docs.example.test'
      })
    );
    expect(config.origins).toEqual(['https://dynodoc.example.test', 'https://docs.example.test']);
    expect(config.connectSrc).toEqual(config.origins);
    problems(
      selfhosted({ DYNODOC_ALT_ORIGINS: 'https://ok.test, nope' }),
      'DYNODOC_ALT_ORIGINS entry "nope"'
    );
    problems(
      selfhosted({
        DYNODOC_ALT_ORIGINS: Array.from({ length: 9 }, (_, i) => `https://h${i}.test`).join(',')
      }),
      'more than 8 origins'
    );
  });
  it('checks the engine address', () => {
    expect(loadConfig(selfhosted()).config.engineApiUrl).toBe('http://127.0.0.1:8080');
    expect(loadConfig(selfhosted({ ENGINE_API_URL: 'http://api:8080' })).config.engineApiUrl).toBe(
      'http://api:8080'
    );
    problems(selfhosted({ ENGINE_API_URL: 'api:8080' }), 'ENGINE_API_URL');
    problems(selfhosted({ ENGINE_API_URL: 'ftp://api' }), 'ENGINE_API_URL must start with http');
    problems(selfhosted({ ENGINE_API_URL: 'http://u:p@api:8080' }), 'must not contain a user name');
    problems(selfhosted({ ENGINE_API_URL: 'http://api:8080/' }), 'must not end with a slash');
    problems(selfhosted({ ENGINE_API_URL: 'not a url' }), 'ENGINE_API_URL is not a URL');
  });
  it('leaves the workspace-host redirect off unless configured', () => {
    expect(loadConfig(selfhosted()).config.workspaceUrl).toBeUndefined();
    expect(loadConfig(hosted()).config.workspaceUrl).toBeUndefined();
    const set = selfhosted({ PUBLIC_WORKSPACE_URL: 'https://app.example.test/' });
    expect(loadConfig(set).config.workspaceUrl).toBe('https://app.example.test');
    warned(set, 'PUBLIC_WORKSPACE_URL redirects workspace pages');
    warned(hosted({ PUBLIC_WORKSPACE_URL: 'nonsense' }), 'is not a URL; no redirect is applied');
    expect(
      loadConfig(hosted({ PUBLIC_WORKSPACE_URL: 'nonsense' })).config.workspaceUrl
    ).toBeUndefined();
  });
});

describe('switches', () => {
  it('turns page metrics on in hosted mode and off in selfhosted mode, and obeys DYNODOC_TELEMETRY', () => {
    expect(loadConfig(hosted()).config.telemetry.enabled).toBe(true);
    expect(loadConfig(selfhosted()).config.telemetry.enabled).toBe(false);
    expect(loadConfig(selfhosted()).config.site.telemetry).toBeNull();
    problems(selfhosted({ DYNODOC_TELEMETRY: 'yes' }), 'DYNODOC_TELEMETRY must be on or off');
    problems(hosted({ DYNODOC_TELEMETRY: 'maybe' }), 'DYNODOC_TELEMETRY must be on or off');
    expect(loadConfig(selfhosted({ DYNODOC_TELEMETRY: ' ON ' })).config.telemetry.enabled).toBe(
      true
    );
    expect(loadConfig(selfhosted({ DYNODOC_TELEMETRY: '' })).config.telemetry.enabled).toBe(false);
  });
  it('sends self-hosted metrics only to the installation itself', () => {
    const { config } = loadConfig(
      selfhosted({ DYNODOC_TELEMETRY: 'on', DYNODOC_ALT_ORIGINS: 'https://docs.example.test:8443' })
    );
    expect(config.site.telemetry).toEqual({
      endpoint: '/telemetry',
      hosts: ['dynodoc.example.test', 'docs.example.test']
    });
    expect(config.telemetry.origins).toEqual([
      ['https://dynodoc.example.test', 'workspace'],
      ['https://docs.example.test:8443', 'workspace']
    ]);
    const admin = loadConfig(selfhosted({ DYNODOC_TELEMETRY: 'on', ADMIN_SITE: 'true' })).config;
    expect(admin.telemetry.origins).toEqual([['https://dynodoc.example.test', 'admin']]);
  });
  it('lets the operator turn email sign-in off', () => {
    expect(loadConfig(selfhosted()).config.emailSignIn).toBe(true);
    const off = loadConfig(selfhosted({ AUTH_EMAIL_LINK: 'off' })).config;
    expect(off.emailSignIn).toBe(false);
    expect(off.site.emailSignIn).toBe(false);
    problems(selfhosted({ AUTH_EMAIL_LINK: 'never' }), 'AUTH_EMAIL_LINK must be on or off');
  });
  it('keeps ADMIN_SITE and DEV_AUTH exactly as before: only the text true turns them on', () => {
    for (const [value, expected] of [
      ['true', true],
      ['TRUE', false],
      ['1', false],
      ['false', false],
      [undefined, false]
    ] as const) {
      const { config } = loadConfig(hosted({ ADMIN_SITE: value, DEV_AUTH: value }));
      expect(config.adminSite, String(value)).toBe(expected);
      expect(config.devAuth, String(value)).toBe(expected);
    }
    warned(hosted({ DEV_AUTH: 'yes' }), 'DEV_AUTH should be true or false');
    warned(selfhosted({ DEV_AUTH: 'true' }), 'DEV_AUTH=true returns sign-in tokens');
    quiet(hosted({ DEV_AUTH: 'true' }), 'DEV_AUTH=true returns sign-in tokens');
  });
});

describe('brand and organisation notice (selfhosted)', () => {
  it('builds the notice from the name, the text and the support link', () => {
    const { config } = loadConfig(
      selfhosted({
        DYNODOC_BRAND_NAME: ' Acme Docs ',
        DYNODOC_ORG_NAME: 'Acme Research Ltd.',
        DYNODOC_ORG_NOTICE: 'Run by the records office.\r\nQuestions: ask us.\r\n\r\n',
        DYNODOC_SUPPORT_URL: 'mailto:records@acme.example.test'
      })
    );
    expect(config.site.brand).toBe('Acme Docs');
    expect(config.site.notice).toEqual({
      organisation: 'Acme Research Ltd.',
      text: 'Run by the records office.\nQuestions: ask us.',
      supportUrl: 'mailto:records@acme.example.test'
    });
    // The support link is for contact; "Full guide" has no target in a self-hosted installation.
    expect(config.site.helpUrl).toBeNull();
  });
  it('reads the notice from a file, once', () => {
    const path = file('From the file.\nSecond line.\n', 0o644);
    expect(loadConfig(selfhosted({ DYNODOC_NOTICE_FILE: path })).config.site.notice?.text).toBe(
      'From the file.\nSecond line.'
    );
    problems(
      selfhosted({ DYNODOC_NOTICE_FILE: path, DYNODOC_ORG_NOTICE: 'inline' }),
      'DYNODOC_ORG_NOTICE and DYNODOC_NOTICE_FILE are both set'
    );
    problems(selfhosted({ DYNODOC_NOTICE_FILE: join(dir, 'gone') }), 'DYNODOC_NOTICE_FILE');
    problems(selfhosted({ DYNODOC_NOTICE_FILE: file('x'.repeat(16 * 1024 + 1)) }), 'larger than');
  });
  it('is bounded plain text', () => {
    const tooLong = 'x'.repeat(NOTICE_MAX_CHARS + 1);
    const tooManyLines = Array.from({ length: NOTICE_MAX_LINES + 1 }, () => 'line').join('\n');
    problems(selfhosted({ DYNODOC_ORG_NOTICE: tooLong }), 'organisation notice is longer than');
    problems(
      selfhosted({ DYNODOC_ORG_NOTICE: tooManyLines }),
      'organisation notice is longer than'
    );
    expect(loadConfig(selfhosted({ DYNODOC_ORG_NOTICE: 'x'.repeat(NOTICE_MAX_CHARS) })).ok).toBe(
      true
    );
    problems(selfhosted({ DYNODOC_ORG_NOTICE: 'bell\u0007here' }), 'control or direction-override');
    problems(selfhosted({ DYNODOC_ORG_NOTICE: 'evil ‮txt.exe' }), 'control or direction-override');
    problems(
      selfhosted({ DYNODOC_BRAND_NAME: 'x'.repeat(61) }),
      'DYNODOC_BRAND_NAME must be one line'
    );
    problems(
      selfhosted({ DYNODOC_BRAND_NAME: 'two\nlines' }),
      'DYNODOC_BRAND_NAME must be one line'
    );
    problems(
      selfhosted({ DYNODOC_ORG_NAME: 'x'.repeat(121) }),
      'DYNODOC_ORG_NAME must be one line'
    );
    // Markup is plain text: it is kept as typed and the page escapes it.
    const markup = loadConfig(selfhosted({ DYNODOC_ORG_NOTICE: '<b>hi</b> & <script>' })).config;
    expect(markup.site.notice?.text).toBe('<b>hi</b> & <script>');
  });
  it('accepts only web and mail links for support', () => {
    for (const ok of ['https://help.example.test/portal', 'http://help.local/', 'mailto:a@b.test'])
      expect(loadConfig(selfhosted({ DYNODOC_SUPPORT_URL: ok })).ok, ok).toBe(true);
    for (const bad of [
      'javascript:alert(1)',
      'data:text/html,hi',
      'file:///etc/passwd',
      'not a url',
      'https://user:pw@help.example.test/',
      `https://help.example.test/${'a'.repeat(300)}`
    ])
      problems(selfhosted({ DYNODOC_SUPPORT_URL: bad }), 'DYNODOC_SUPPORT_URL');
  });
  it('shows no notice when none is configured', () => {
    expect(loadConfig(selfhosted()).config.site.notice).toBeNull();
    expect(loadConfig(selfhosted({ DYNODOC_ORG_NOTICE: '   ' })).config.site.notice).toBeNull();
  });
});

describe('warnings', () => {
  it('flag a setting that looks like a typo of a known one', () => {
    warned(
      hosted({ DYNODOC_TELEMTRY: 'on' }),
      'DYNODOC_TELEMTRY is not a known setting; did you mean DYNODOC_TELEMETRY?'
    );
    warned(hosted({ DYNODOC_MODEE: 'selfhosted' }), 'did you mean DYNODOC_MODE?');
    warned(hosted({ DYNODOC_BRAND_NAMES: 'x' }), 'did you mean DYNODOC_BRAND_NAME?');
    warned(
      hosted({ DYNODOC_ALLOW_OPEN_SECRET_FILE: 'true' }),
      'did you mean DYNODOC_ALLOW_OPEN_SECRET_FILES?'
    );
    warned(hosted({ DYNODOC_ALT_ORIGIN: 'x' }), 'did you mean DYNODOC_ALT_ORIGINS?');
  });
  it('stay quiet about known names and unrelated ones', () => {
    for (const name of KNOWN_DYNODOC_NAMES) expect(likelyTypoOf(name)).toEqual([]);
    expect(likelyTypoOf('DYNODOC_COMPLETELY_UNRELATED_SETTING')).toEqual([]);
    const env = hosted({ DYNODOC_COMPLETELY_UNRELATED_SETTING: '1', PATH: '/bin' });
    expect(loadConfig(env).warnings).toEqual([]);
  });
  it('know every DYNODOC_ name the Rust crates read', () => {
    const root = join(process.cwd(), '..', '..', 'crates');
    let names = new Set<string>();
    try {
      const walk = (path: string) => {
        for (const entry of readdirSync(path, { withFileTypes: true })) {
          const next = join(path, entry.name);
          if (entry.isDirectory()) walk(next);
          else if (entry.name.endsWith('.rs'))
            for (const m of readFileSync(next, 'utf8').matchAll(/"(DYNODOC_[A-Z0-9_]+)"/g))
              names.add(m[1]);
        }
      };
      walk(root);
    } catch {
      names = new Set(); // The crates are not part of every checkout of this package.
    }
    // Benchmarks and tests set their own variables; the settings an operator can set are listed.
    const ignored = /^DYNODOC_(CHECKPOINT_BENCH|SIDECAR_TEST|MAIN_CHECKOUT|DESKTOP_HOME)/;
    for (const name of names)
      if (!ignored.test(name)) expect(KNOWN_DYNODOC_NAMES, name).toContain(name);
  });
});

describe('aggregation and secrecy', () => {
  it('lists every problem once, in one message, and no secret', () => {
    const env = {
      DYNODOC_MODE: 'selfhosted',
      SESSION_SECRET: 'tooShortSecretValue',
      WEB_SERVICE_KEY: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      ORIGIN: 'https://x.test/path',
      ENGINE_API_URL: 'ftp://nope',
      DYNODOC_TELEMETRY: 'sure',
      AUTH_EMAIL_LINK: 'perhaps',
      DYNODOC_BRAND_NAME: 'x'.repeat(100),
      DYNODOC_SUPPORT_URL: 'javascript:alert(1)',
      RESEND_API_KEY: 'unused-but-present-0123456789',
      RESEND_API_KEY_FILE: file('also-present')
    };
    const result = loadConfig(env);
    expect(result.ok).toBe(false);
    expect(result.problems).toHaveLength(9);
    const message = formatProblems(result.problems);
    expect(message.startsWith('invalid configuration (9 problems):\n  - ')).toBe(true);
    expect(message.split('\n')).toHaveLength(10);
    for (const secret of [
      env.SESSION_SECRET,
      env.WEB_SERVICE_KEY,
      env.RESEND_API_KEY,
      'also-present'
    ]) {
      expect(message).not.toContain(secret);
      expect(result.warnings.join('\n')).not.toContain(secret);
    }
    expect(formatProblems(['one'])).toBe('invalid configuration (1 problem):\n  - one');
  });
  it('is frozen and prints no secret however it is shown', () => {
    const { config } = loadConfig(
      selfhosted({ RESEND_API_KEY: 'resend-secret-0123456789', EMAIL_FROM: 'a@b.test' })
    );
    expect(() => {
      (config as { mode: string }).mode = 'hosted';
    }).toThrow();
    expect(() => {
      (config.site as { brand: string }).brand = 'x';
    }).toThrow();
    expect(() => {
      (config.connectSrc as string[]).push('https://evil.test');
    }).toThrow();
    for (const text of [
      JSON.stringify(config),
      inspect(config, { depth: 6 }),
      String(config.secrets.session)
    ])
      for (const secret of [SESSION, SERVICE, 'resend-secret-0123456789'])
        expect(text).not.toContain(secret);
    expect(config.secrets.session?.expose()).toBe(SESSION);
  });
});
