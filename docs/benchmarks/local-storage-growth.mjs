// Synthetic CLI experiment; never point this at an existing repository or key.
// node local-storage-growth.mjs ENGINE_CLI NEW_DIRECTORY [NODES=128] [EDITS=32]
import assert from 'node:assert/strict';
import { createPrivateKey } from 'node:crypto';
import { mkdirSync, writeFileSync, statSync, existsSync, unlinkSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { cpus, platform, arch } from 'node:os';
import { performance } from 'node:perf_hooks';

const [executable, directory, nodesArg = '128', editsArg = '32'] = process.argv.slice(2);
assert(executable && directory, 'Supply ENGINE_CLI and a NEW_DIRECTORY');
const binary = resolve(executable);
const scratch = resolve(directory);
const nodeCount = Number(nodesArg);
const edits = Number(editsArg);
assert(Number.isInteger(nodeCount) && nodeCount >= 1 && nodeCount <= 1024);
assert(Number.isInteger(edits) && edits >= 1 && edits <= 1000);
// Refuse an existing directory; every created file belongs only to this run.
mkdirSync(scratch);
const database = join(scratch, 'project.sqlite');
const keyPath = join(scratch, 'synthetic-key.der');
const statePath = join(scratch, 'state.json');
const manifestPath = join(scratch, 'commit.json');
const project = '11111111-1111-4111-8111-111111111111';
const document = '22222222-2222-4222-8222-222222222222';
const key = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 37)]),
  format: 'der', type: 'pkcs8'
});
// Publicly known synthetic seed, never a production credential.
writeFileSync(keyPath, key.export({ format: 'der', type: 'pkcs8' }), { flag: 'wx', mode: 0o600 });

function run(...args) {
  const start = performance.now();
  const result = spawnSync(binary, ['repository', database, project, ...args], {
    encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 120_000
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  return { value: JSON.parse(result.stdout), ms: performance.now() - start };
}
function metrics(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return { min: sorted[0], median: sorted[Math.floor(sorted.length / 2)],
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1], max: sorted.at(-1) };
}
const nodes = {};
for (let i = 0; i < nodeCount; i++) {
  const id = String(i + 1).padStart(26, '0');
  nodes[id] = { id, parent_id: null, type: 'paragraph', pos: `p${String(i).padStart(6, '0')}`,
    var_name: null, deleted: false, current_fields: { label: `Paragraph ${i}: ${'x'.repeat(256)}` } };
}
const state = { nodes, comments: [], suggestions: {}, removed_choices: [] };
const ids = Object.keys(nodes);
const rounds = [];
let copiedJsonBytes = 0;
let previous = null;
let firstRoot;
let firstState;
let expectedBytes = 0;
let expectedObjects = 0;
try {
  assert.equal(run('init').value.local_schema, 2);
  for (let revision = 0; revision <= edits; revision++) {
    if (revision > 0) nodes[ids[(revision - 1) % nodeCount]].current_fields.label = `Edit ${revision}: ${'y'.repeat(256)}`;
    const json = JSON.stringify(state);
    copiedJsonBytes += Buffer.byteLength(json);
    writeFileSync(statePath, json);
    const stored = run('store-state', document, statePath);
    if (revision === 0) { firstRoot = stored.value.state; firstState = JSON.parse(json); }
    const manifest = { version: 1, project, parents: previous ? [previous] : [],
      files: { 'Report.docx': { document, state: stored.value.state } }, author_key: '',
      authored_at_ms: revision, message: `Synthetic revision ${revision}` };
    writeFileSync(manifestPath, JSON.stringify(manifest));
    const committed = run('commit', manifestPath, keyPath);
    previous = committed.value.commit;
    const signed = run('show', previous).value;
    expectedBytes += stored.value.new_bytes + Buffer.byteLength(JSON.stringify(signed));
    expectedObjects += stored.value.new_objects;
    rounds.push({ revision, input_json_bytes: Buffer.byteLength(json), new_objects: stored.value.new_objects,
      new_object_bytes: stored.value.new_bytes, state_write_ms: stored.ms, commit_import_ms: committed.ms });
    if (revision % 8 === 0) process.stdout.write(`Retained revision ${revision}/${edits}\n`);
  }
  run('set-head', 'main', previous);
  const usage = run('storage').value;
  assert.equal(usage.bytes_used, expectedBytes);
  assert.equal(usage.objects_used, expectedObjects);
  assert.equal(usage.commits_used, edits + 1);
  assert.equal(usage.branches_used, 1);
  const historical = run('read-state', document, firstRoot);
  assert.deepEqual(historical.value, firstState);
  const retry = run('store-state', document, statePath);
  assert.equal(retry.value.new_objects, 0);
  assert.equal(retry.value.new_bytes, 0);
  assert.deepEqual(run('storage').value, usage);
  const dbBytes = ['', '-wal', '-shm'].reduce((sum, suffix) =>
    sum + (existsSync(database + suffix) ? statSync(database + suffix).size : 0), 0);
  const report = {
    kind: 'Synthetic local CLI storage growth; one changed paragraph per commit',
    environment: { platform: platform(), arch: arch(), cpu: cpus()[0]?.model, node: process.version,
      binary_sha256_note: 'Record the source revision and build profile alongside this report.' },
    fixture: { nodes: nodeCount, edits, commits: edits + 1, label_padding_characters: 256 },
    usage, sum_full_state_json_bytes: copiedJsonBytes,
    retained_payload_fraction_of_full_json_history: usage.bytes_used / copiedJsonBytes,
    sqlite_database_wal_shm_bytes_after_closed_cli: dbBytes,
    state_write_ms: metrics(rounds.map(r => r.state_write_ms)),
    commit_import_ms: metrics(rounds.map(r => r.commit_import_ms)),
    historical_first_root_read_ms: historical.ms, unchanged_retry_ms: retry.ms,
    assertions: ['exact object and signed-commit accounting', 'first historical root still exact',
      'unchanged retry stores zero objects and bytes'], rounds,
    limitations: ['debug/release CLI startup and reopen cost are included', 'full JSON baseline is uncompressed and excludes commit metadata',
      'native binaries/assets and installed editors are not exercised', 'not a PostgreSQL, HTTP transfer, peak memory or service-scale benchmark',
      'storage deduplication does not establish incremental publication complexity']
  };
  writeFileSync(join(scratch, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  process.stdout.write(`Payload bytes: ${usage.bytes_used}; full JSON history: ${copiedJsonBytes}; SQLite files: ${dbBytes}\n`);
} finally {
  unlinkSync(keyPath);
}
