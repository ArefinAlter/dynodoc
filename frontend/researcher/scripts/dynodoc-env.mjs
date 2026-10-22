// Environment files for a self-hosted installation.
//
//   generate-env --dir <dir> --origin <https://...>   write a commented .env and a secrets/
//                                                     directory of random secret files
//   check-config --dir <dir>                          validate an existing directory with the
//                                                     same validator the web server runs at
//                                                     start, without starting anything
//
// Nothing here prints a secret. generate-env refuses to overwrite anything. The secrets are
// 32 random bytes each (crypto.randomBytes), one file per secret, mode 0600 on Unix, and the
// .env names them with the `*_FILE` variables (src/lib/server/config/secrets.js has the rules).
// No dependencies beyond Node. `dynodoctl` is meant to call these two commands.
//
// usage: node scripts/dynodoc-env.mjs generate-env --dir ./dynodoc-config --origin https://dynodoc.example.org
//          [--brand-name "Acme Docs"] [--org-name "Acme Ltd."] [--support-url mailto:docs@acme.example]
//          [--container-secrets-dir /run/secrets] [--db-host postgres] [--db-name dynodoc]
//          [--db-port 5432] [--db-user dynodoc_runtime] [--with-converter]
//        node scripts/dynodoc-env.mjs check-config --dir ./dynodoc-config [--use-process-env] [--json]
import { randomBytes } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadConfig } from '../src/lib/server/config/schema.js';
import { readSecret } from '../src/lib/server/config/secrets.js';

/** Secret files, in the order they are written: name, what uses it, what losing it costs. */
export const SECRET_FILES = [
  [
    'session_secret',
    'SESSION_SECRET',
    'web',
    'Signs every web session cookie. Lost or changed: everyone must sign in again.'
  ],
  [
    'web_service_key',
    'WEB_SERVICE_KEY',
    'web+api',
    'Shared by the web server and the API. Both must hold the same value.'
  ],
  [
    'paseto_local_key',
    'PASETO_LOCAL_KEY',
    'api',
    'Seals API access tokens. Lost or changed: every session ends.'
  ],
  [
    'converter_key',
    'CONVERTER_KEY',
    'web+converter',
    'Shared by the web server and the legacy .doc converter, if you run one.'
  ],
  [
    'postgres_password',
    'POSTGRES_PASSWORD',
    'database',
    'The database owner password (migrations, backups).'
  ],
  [
    'database_url',
    'DATABASE_URL',
    'api',
    "The API's restricted runtime login, as a connection URL."
  ]
];
const RUNTIME_PASSWORD_FILE = 'runtime_database_password';

const flag = (args, name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const hex = (bytes) => randomBytes(bytes).toString('hex');
const unix = () => process.platform !== 'win32';

/** Write a new file, never over an existing one. */
function createFile(path, text, mode) {
  writeFileSync(path, text, { flag: 'wx', mode });
  if (unix()) chmodSync(path, mode); // the process umask may have narrowed the mode, never widened it
}
function ensureDir(path) {
  mkdirSync(path, { recursive: true, mode: 0o700 });
  if (unix()) chmodSync(path, 0o700);
}

/**
 * Create the directory. Returns what was written and the lines to show; throws before writing
 * anything when a target exists or an option is wrong.
 */
export function generateEnv(options) {
  const dir = resolve(options.dir ?? '');
  if (!options.dir) throw new Error('--dir is required (a new or empty directory).');
  if (!options.origin)
    throw new Error('--origin is required, for example https://dynodoc.example.org');
  const probe = loadConfig({ DYNODOC_MODE: 'selfhosted', ORIGIN: options.origin });
  const originProblem = probe.problems.find((p) => p.startsWith('ORIGIN'));
  if (originProblem) throw new Error(originProblem);
  const secretsDir = join(dir, 'secrets');
  const mountDir = options.containerSecretsDir ?? secretsDir;
  const dbHost = options.dbHost ?? 'postgres';
  const dbPort = String(options.dbPort ?? '5432');
  const dbName = options.dbName ?? 'dynodoc';
  const dbUser = options.dbUser ?? 'dynodoc_runtime';
  for (const [label, value] of [
    ['--db-host', dbHost],
    ['--db-name', dbName],
    ['--db-user', dbUser],
    ['--db-port', dbPort]
  ])
    if (!/^[A-Za-z0-9_.-]+$/.test(value))
      throw new Error(`${label} may contain only letters, digits, . _ and -`);

  // The converter (legacy .doc import) is optional: its key exists only when asked for.
  const files = SECRET_FILES.filter(([file]) => file !== 'converter_key' || options.withConverter);
  const targets = [
    join(dir, '.env'),
    ...files.map(([file]) => join(secretsDir, file)),
    join(secretsDir, RUNTIME_PASSWORD_FILE)
  ];
  const existing = targets.filter((path) => existsSync(path));
  if (existing.length)
    throw new Error(
      `Refusing to overwrite. These already exist:\n  - ${existing.join('\n  - ')}\nChoose another --dir, or move them away.`
    );

  const runtimePassword = hex(24);
  const values = {
    session_secret: hex(32),
    web_service_key: hex(32),
    paseto_local_key: hex(32),
    converter_key: hex(32),
    postgres_password: hex(24),
    database_url: `postgres://${dbUser}:${runtimePassword}@${dbHost}:${dbPort}/${dbName}`
  };
  ensureDir(dir);
  ensureDir(secretsDir);
  for (const [file] of files) createFile(join(secretsDir, file), `${values[file]}\n`, 0o600);
  createFile(join(secretsDir, RUNTIME_PASSWORD_FILE), `${runtimePassword}\n`, 0o600);

  const optional = (name, value, example) => (value ? `${name}=${value}` : `# ${name}=${example}`);
  const env = [
    '# Dynodoc configuration for a self-hosted installation.',
    `# Generated ${new Date().toISOString().slice(0, 10)} by scripts/dynodoc-env.mjs generate-env.`,
    '# This file holds settings and the paths of the secret files. It holds no secret value.',
    '# Check it at any time: node scripts/dynodoc-env.mjs check-config --dir <this directory>',
    '',
    '# --- Mode and address ---',
    '# selfhosted turns off the marketing pages, search indexing and page metrics.',
    'DYNODOC_MODE=selfhosted',
    '# The exact address people type: scheme, host, optional port, no trailing slash.',
    `ORIGIN=${new URL(options.origin).origin}`,
    '# More names the same installation answers to (comma-separated origins, at most 8).',
    '# DYNODOC_ALT_ORIGINS=',
    '# Where the web server reaches the API (inside the private network).',
    'ENGINE_API_URL=http://api:8080',
    '',
    '# --- Your organisation, shown on the sign-in page and in the workspace footer ---',
    optional('DYNODOC_BRAND_NAME', options.brandName, 'Dynodoc'),
    optional('DYNODOC_ORG_NAME', options.orgName, 'Your organisation'),
    '# Plain text, up to 2000 characters. Use one of the next two lines, not both.',
    '# DYNODOC_ORG_NOTICE=',
    '# DYNODOC_NOTICE_FILE=/etc/dynodoc/notice.txt',
    optional('DYNODOC_SUPPORT_URL', options.supportUrl, 'mailto:helpdesk@example.org'),
    '',
    '# --- Sign-in ---',
    '# on | off. Off hides email sign-in and refuses requests for new emailed links.',
    '# AUTH_EMAIL_LINK=on',
    '# Mail delivery (optional; SMTP arrives in a later release). Set both or neither.',
    '# RESEND_API_KEY_FILE=',
    '# EMAIL_FROM=',
    '# Comma-separated emails allowed to use /admin.',
    '# ADMIN_EMAILS=',
    '',
    '# --- Page metrics ---',
    '# on | off. Off by default in self-hosted mode: the page sends nothing and the route answers 404.',
    '# DYNODOC_TELEMETRY=off',
    '',
    '# --- Secrets: each one is a file holding only the value ---',
    '# The files are in the secrets directory, mode 0600. Where a platform fixes the mode (Docker',
    '# and Kubernetes secrets), set DYNODOC_ALLOW_OPEN_SECRET_FILES=true.',
    '# DYNODOC_ALLOW_OPEN_SECRET_FILES=true',
    ...files
      .filter(([, name]) => name !== 'POSTGRES_PASSWORD')
      .map(([file, name]) => `${name}_FILE=${join(mountDir, file).split('\\').join('/')}`),
    ...(options.withConverter ? ['CONVERTER_URL=http://converter:8090'] : []),
    ''
  ].join('\n');
  createFile(join(dir, '.env'), env, 0o600);

  const written = [
    join(dir, '.env'),
    ...files.map(([file]) => join(secretsDir, file)),
    join(secretsDir, RUNTIME_PASSWORD_FILE)
  ];
  const lines = [
    `Wrote ${written.length} files under ${dir}:`,
    ...written.map((path) => `  ${path}`),
    '',
    unix()
      ? 'The secret files are readable by you only (mode 0600).'
      : 'This system has no Unix file modes: restrict the directory to your account yourself.',
    mountDir !== secretsDir
      ? `The .env names the secrets under ${mountDir}; mount or copy them there.`
      : '',
    '',
    'Back up the whole directory somewhere safe and separate from the server. Without it:',
    ...files.map(([file, name, , cost]) => `  - ${file} (${name}): ${cost}`),
    `  - ${RUNTIME_PASSWORD_FILE}: the password inside database_url; needed when you create the database login.`,
    'No secret is shown here. Do not commit this directory, paste its files into a ticket, or mail them.',
    '',
    `Next: node scripts/dynodoc-env.mjs check-config --dir ${dir}`
  ].filter((line, i, all) => line !== '' || all[i - 1] !== '');
  return { dir, written, lines };
}

/** Parse KEY=VALUE lines. No expansion, no export keyword; a matching pair of quotes is removed. */
export function parseEnvFile(text) {
  const values = {};
  const problems = [];
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim();
    if (!line || line.startsWith('#')) return;
    const eq = line.indexOf('=');
    if (eq <= 0 || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(line.slice(0, eq).trim())) {
      problems.push(`line ${index + 1} is not KEY=VALUE`);
      return;
    }
    let value = line.slice(eq + 1).trim();
    if (value.length >= 2 && /^(["']).*\1$/.test(value)) value = value.slice(1, -1);
    values[line.slice(0, eq).trim()] = value;
  });
  return { values, problems };
}

/**
 * Validate a directory (its .env) with the web server's validator, plus the shape of the two
 * secrets only the API reads. The API still checks its own at start.
 */
export function checkConfig(options) {
  const dir = resolve(options.dir ?? '');
  if (!options.dir) throw new Error('--dir is required.');
  const envPath = join(dir, '.env');
  if (!existsSync(envPath)) throw new Error(`No .env in ${dir}`);
  const { values, problems: syntax } = parseEnvFile(readFileSync(envPath, 'utf8'));
  const env = { ...(options.useProcessEnv ? process.env : {}), ...values };
  const result = loadConfig(env);
  const problems = [...syntax, ...result.problems];
  const warnings = [...result.warnings];
  const lookup = (name) => env[name];
  for (const name of Object.keys(values))
    if (name.endsWith('_FILE') && values[name] && !isAbsolute(values[name]))
      warnings.push(
        `${name} is a relative path (${values[name]}); the server resolves it from its own working directory`
      );

  // The API's own secrets: same reader, shape only.
  const paseto = readSecret(lookup, 'PASETO_LOCAL_KEY');
  if (paseto.problem) problems.push(paseto.problem);
  else if (!paseto.secret)
    problems.push(
      'PASETO_LOCAL_KEY is not set (32 bytes, hex-encoded; or set PASETO_LOCAL_KEY_FILE)'
    );
  else {
    const text = paseto.secret.expose().trim();
    if (!/^[0-9a-fA-F]+$/.test(text)) problems.push('PASETO_LOCAL_KEY must be hex-encoded');
    else if (text.length !== 64) problems.push('PASETO_LOCAL_KEY must be 32 bytes (64 hex chars)');
    else if (/^0+$/.test(text))
      problems.push('A random PASETO_LOCAL_KEY is required in every environment');
  }
  const database = readSecret(lookup, 'DATABASE_URL');
  if (database.problem) problems.push(database.problem);
  else if (!database.secret) problems.push('DATABASE_URL is not set (or set DATABASE_URL_FILE)');
  else if (!/^postgres(?:ql)?:\/\/[^\s]+$/.test(database.secret.expose()))
    problems.push('DATABASE_URL is not a valid PostgreSQL connection URL');
  if (lookup('MIGRATION_DATABASE_URL') || lookup('MIGRATION_DATABASE_URL_FILE'))
    problems.push(
      'MIGRATION_DATABASE_URL must not be present in the API environment (nor MIGRATION_DATABASE_URL_FILE)'
    );
  // The web server and the API must hold the same service key; both read it from this directory.
  return { ok: problems.length === 0, problems, warnings, mode: result.config.mode };
}

function render(result) {
  const lines = [];
  if (result.problems.length)
    lines.push(
      `invalid configuration (${result.problems.length} problem${result.problems.length === 1 ? '' : 's'}):`,
      ...result.problems.map((p) => `  - ${p}`)
    );
  else lines.push(`configuration is valid (${result.mode} mode)`);
  if (result.warnings.length)
    lines.push('', 'warnings:', ...result.warnings.map((w) => `  - ${w}`));
  return lines.join('\n');
}

function main(argv) {
  const [command, ...args] = argv;
  try {
    if (command === 'generate-env') {
      const { lines } = generateEnv({
        dir: flag(args, '--dir'),
        origin: flag(args, '--origin'),
        brandName: flag(args, '--brand-name'),
        orgName: flag(args, '--org-name'),
        supportUrl: flag(args, '--support-url'),
        containerSecretsDir: flag(args, '--container-secrets-dir'),
        dbHost: flag(args, '--db-host'),
        dbName: flag(args, '--db-name'),
        dbPort: flag(args, '--db-port'),
        dbUser: flag(args, '--db-user'),
        withConverter: args.includes('--with-converter')
      });
      console.log(lines.join('\n'));
      return 0;
    }
    if (command === 'check-config') {
      const result = checkConfig({
        dir: flag(args, '--dir'),
        useProcessEnv: args.includes('--use-process-env')
      });
      console.log(args.includes('--json') ? JSON.stringify(result) : render(result));
      return result.ok ? 0 : 1;
    }
  } catch (error) {
    console.error(error.message);
    return 2;
  }
  console.error(
    'usage: dynodoc-env.mjs generate-env|check-config --dir <dir> [options]; see the top of this file'
  );
  return 2;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2));
}
