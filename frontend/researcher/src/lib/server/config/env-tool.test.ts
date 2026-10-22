// scripts/dynodoc-env.mjs: the environment-file generator and the offline configuration check.
import { afterAll, describe, expect, it } from 'vitest';
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
// @ts-expect-error the tool is a plain script
import {
  SECRET_FILES,
  checkConfig,
  generateEnv,
  parseEnvFile
} from '../../../../scripts/dynodoc-env.mjs';

const root = mkdtempSync(join(tmpdir(), 'dynodoc-env-'));
afterAll(() => rmSync(root, { recursive: true, force: true }));
let counter = 0;
const fresh = () => join(root, `install-${counter++}`);
const unix = process.platform !== 'win32';
const ORIGIN = 'https://dynodoc.example.test';

const secretValues = (dir: string) =>
  Object.fromEntries(
    readdirSync(join(dir, 'secrets')).map((file) => [
      file,
      readFileSync(join(dir, 'secrets', file), 'utf8').trim()
    ])
  );
const secretsOf = (value: Record<string, string>) => Object.values(value);

describe('generate-env', () => {
  it('writes a commented .env and one random file per secret, and prints no secret', () => {
    const dir = fresh();
    const { written, lines } = generateEnv({ dir, origin: ORIGIN, orgName: 'Acme Research Ltd.' });
    expect(written).toHaveLength(7);
    const values = secretValues(dir);
    expect(Object.keys(values).sort()).toEqual(
      [
        ...SECRET_FILES.map(([file]: string[]) => file).filter(
          (f: string) => f !== 'converter_key'
        ),
        'runtime_database_password'
      ].sort()
    );
    for (const file of ['session_secret', 'web_service_key', 'paseto_local_key'])
      expect(values[file]).toMatch(/^[0-9a-f]{64}$/);
    expect(values.paseto_local_key).not.toMatch(/^0+$/);
    expect(values.database_url).toBe(
      `postgres://dynodoc_runtime:${values.runtime_database_password}@postgres:5432/dynodoc`
    );
    // Every secret is different from every other.
    expect(new Set(secretsOf(values)).size).toBe(secretsOf(values).length);
    // The report names the files and what to back up, and holds no secret value.
    const report = lines.join('\n');
    for (const value of secretsOf(values)) expect(report).not.toContain(value);
    expect(report).toContain('Back up');
    expect(report).toContain(join(dir, 'secrets', 'session_secret'));
    const env = readFileSync(join(dir, '.env'), 'utf8');
    for (const value of secretsOf(values)) expect(env).not.toContain(value);
    expect(env).toContain('DYNODOC_MODE=selfhosted');
    expect(env).toContain(`ORIGIN=${ORIGIN}`);
    expect(env).toContain('DYNODOC_ORG_NAME=Acme Research Ltd.');
    expect(env).toContain('# AUTH_EMAIL_LINK=on');
    expect(env).toMatch(/^SESSION_SECRET_FILE=.*secrets\/session_secret$/m);
    expect(env).not.toMatch(/^POSTGRES_PASSWORD/m);
  });
  it.skipIf(!unix)('creates private files and directories on Unix', () => {
    const dir = fresh();
    generateEnv({ dir, origin: ORIGIN });
    for (const path of [
      join(dir, '.env'),
      ...readdirSync(join(dir, 'secrets')).map((f) => join(dir, 'secrets', f))
    ])
      expect(statSync(path).mode & 0o077, path).toBe(0);
    expect(statSync(join(dir, 'secrets')).mode & 0o077).toBe(0);
  });
  it('gives different secrets every time', () => {
    const a = fresh();
    const b = fresh();
    generateEnv({ dir: a, origin: ORIGIN });
    generateEnv({ dir: b, origin: ORIGIN });
    const first = secretValues(a);
    const second = secretValues(b);
    for (const file of ['session_secret', 'web_service_key', 'paseto_local_key'])
      expect(first[file]).not.toBe(second[file]);
  });
  it('refuses to overwrite, and leaves what exists untouched', () => {
    const dir = fresh();
    generateEnv({ dir, origin: ORIGIN });
    const before = readFileSync(join(dir, 'secrets', 'session_secret'), 'utf8');
    expect(() => generateEnv({ dir, origin: ORIGIN })).toThrow(/Refusing to overwrite/);
    expect(readFileSync(join(dir, 'secrets', 'session_secret'), 'utf8')).toBe(before);
    // One existing file is enough, and nothing else is written.
    const partial = fresh();
    generateEnv({ dir: partial, origin: ORIGIN });
    rmSync(join(partial, 'secrets', 'postgres_password'));
    rmSync(join(partial, '.env'));
    const keep = readFileSync(join(partial, 'secrets', 'web_service_key'), 'utf8');
    expect(() => generateEnv({ dir: partial, origin: ORIGIN })).toThrow(/web_service_key/);
    expect(existsSync(join(partial, '.env'))).toBe(false);
    expect(existsSync(join(partial, 'secrets', 'postgres_password'))).toBe(false);
    expect(readFileSync(join(partial, 'secrets', 'web_service_key'), 'utf8')).toBe(keep);
  });
  it('rejects a missing or malformed directory, origin and database name before writing', () => {
    expect(() => generateEnv({ origin: ORIGIN })).toThrow(/--dir is required/);
    expect(() => generateEnv({ dir: fresh() })).toThrow(/--origin is required/);
    for (const origin of ['dynodoc.example.test', `${ORIGIN}/`, 'ftp://x.test'])
      expect(() => generateEnv({ dir: fresh(), origin })).toThrow(/ORIGIN/);
    const dir = fresh();
    expect(() => generateEnv({ dir, origin: ORIGIN, dbHost: 'a b' })).toThrow(/--db-host/);
    expect(existsSync(dir)).toBe(false);
  });
  it('adds the converter key and address only when asked', () => {
    const dir = fresh();
    generateEnv({ dir, origin: ORIGIN, withConverter: true });
    expect(secretValues(dir).converter_key).toMatch(/^[0-9a-f]{64}$/);
    const env = readFileSync(join(dir, '.env'), 'utf8');
    expect(env).toMatch(/^CONVERTER_KEY_FILE=.*converter_key$/m);
    expect(env).toContain('CONVERTER_URL=http://converter:8090');
    expect(checkConfig({ dir })).toMatchObject({ ok: true, warnings: [] });
  });
  it('can name the secrets where a container will mount them', () => {
    const dir = fresh();
    generateEnv({
      dir,
      origin: ORIGIN,
      containerSecretsDir: '/run/secrets',
      dbHost: 'db',
      dbName: 'docs'
    });
    const env = readFileSync(join(dir, '.env'), 'utf8');
    expect(env).toContain('SESSION_SECRET_FILE=/run/secrets/session_secret');
    expect(env).toContain('DATABASE_URL_FILE=/run/secrets/database_url');
    expect(secretValues(dir).database_url).toMatch(/@db:5432\/docs$/);
  });
});

describe('check-config', () => {
  const generated = () => {
    const dir = fresh();
    generateEnv({ dir, origin: ORIGIN });
    return dir;
  };
  it('accepts what generate-env wrote, with no warning', () => {
    const result = checkConfig({ dir: generated() });
    expect(result).toEqual({ ok: true, problems: [], warnings: [], mode: 'selfhosted' });
  });
  it('lists every problem at once', () => {
    const dir = generated();
    writeFileSync(
      join(dir, 'secrets', 'session_secret'),
      'change-me-to-a-long-random-value-please\n'
    );
    rmSync(join(dir, 'secrets', 'web_service_key'));
    writeFileSync(join(dir, 'secrets', 'paseto_local_key'), 'not-hex\n');
    const env = readFileSync(join(dir, '.env'), 'utf8').replace(
      /^ORIGIN=.*$/m,
      'ORIGIN=https://x.test/'
    );
    writeFileSync(join(dir, '.env'), `${env}\nDYNODOC_TELEMTRY=on\n`);
    const result = checkConfig({ dir });
    const text = result.problems.join('\n');
    expect(result.ok).toBe(false);
    expect(text).toContain('SESSION_SECRET looks like a placeholder');
    expect(text).toContain('WEB_SERVICE_KEY_FILE');
    expect(text).toContain('PASETO_LOCAL_KEY must be hex-encoded');
    expect(text).toContain('ORIGIN must be an origin');
    expect(text).not.toContain('change-me-to-a-long-random');
    expect(result.warnings.join('\n')).toContain('did you mean DYNODOC_TELEMETRY?');
  });
  it('catches both forms of a secret, an owner database URL and bad lines', () => {
    const dir = generated();
    const env = readFileSync(join(dir, '.env'), 'utf8');
    writeFileSync(
      join(dir, '.env'),
      `${env}\nSESSION_SECRET=${'a1b2c3d4'.repeat(8)}\nMIGRATION_DATABASE_URL=postgres://owner:x@db/d\nthis is not a setting\n`
    );
    const text = checkConfig({ dir }).problems.join('\n');
    expect(text).toContain('SESSION_SECRET and SESSION_SECRET_FILE are both set');
    expect(text).toContain('MIGRATION_DATABASE_URL must not be present');
    expect(text).toContain('is not KEY=VALUE');
    expect(text).not.toContain('a1b2c3d4a1b2c3d4');
  });
  it.skipIf(!unix)('refuses a secret file others can read, unless allowed', () => {
    const dir = generated();
    chmodSync(join(dir, 'secrets', 'session_secret'), 0o644);
    expect(checkConfig({ dir }).problems.join('\n')).toContain('accessible by group or others');
    const env = readFileSync(join(dir, '.env'), 'utf8');
    writeFileSync(join(dir, '.env'), `${env}\nDYNODOC_ALLOW_OPEN_SECRET_FILES=true\n`);
    expect(checkConfig({ dir }).ok).toBe(true);
  });
  it('reads quoted values and can add the process environment', () => {
    const dir = generated();
    expect(parseEnvFile('A="x y"\nB=\'z\'\n# c\n\nC=d=e').values).toEqual({
      A: 'x y',
      B: 'z',
      C: 'd=e'
    });
    const before = process.env.DYNODOC_TELEMTRY;
    process.env.DYNODOC_TELEMTRY = 'on';
    try {
      expect(checkConfig({ dir }).warnings).toEqual([]);
      expect(checkConfig({ dir, useProcessEnv: true }).warnings.join('')).toContain(
        'DYNODOC_TELEMETRY'
      );
    } finally {
      if (before === undefined) delete process.env.DYNODOC_TELEMTRY;
      else process.env.DYNODOC_TELEMTRY = before;
    }
  });
  it('needs a directory with a .env', () => {
    expect(() => checkConfig({})).toThrow(/--dir is required/);
    expect(() => checkConfig({ dir: fresh() })).toThrow(/No \.env/);
  });
});
