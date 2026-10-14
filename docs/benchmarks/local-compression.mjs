// Synthetic CLI experiment; never point this at an existing repository or key.
// node local-compression.mjs NEW_ENGINE_CLI SCHEMA2_ENGINE_CLI NEW_DIRECTORY [NODES=128] [EDITS=32] [FIXTURE=repetitive|docs]
//
// Builds the same synthetic history twice: with a schema-2 CLI (identity rows, the
// baseline) and with the current schema-3 CLI (compressed rows). Then compacts
// the baseline in place and compares sizes, addresses and timings.
//
// Fixtures: `repetitive` is the earlier growth benchmark's label ("Paragraph N: "
// plus 256 repeated letters), an upper bound that compresses unrealistically
// well. `docs` uses real English paragraphs from this repository's own
// documentation, one per node, so ratios look like prose rather than padding.
import assert from 'node:assert/strict';
import { createPrivateKey, createHash } from 'node:crypto';
import { mkdirSync, writeFileSync, readFileSync, readdirSync, statSync, existsSync, unlinkSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { cpus, platform, arch } from 'node:os';
import { performance } from 'node:perf_hooks';

const [newExe, oldExe, directory, nodesArg = '128', editsArg = '32', fixture = 'repetitive'] = process.argv.slice(2);
assert(newExe && oldExe && directory, 'Supply NEW_ENGINE_CLI, SCHEMA2_ENGINE_CLI and a NEW_DIRECTORY');
assert(['repetitive', 'docs'].includes(fixture), 'FIXTURE is repetitive or docs');
const nodeCount = Number(nodesArg);
const edits = Number(editsArg);
assert(Number.isInteger(nodeCount) && nodeCount >= 1 && nodeCount <= 1024);
assert(Number.isInteger(edits) && edits >= 1 && edits <= 1000);
const scratch = resolve(directory);
mkdirSync(scratch); // refuses an existing directory
const here = dirname(fileURLToPath(import.meta.url));
const project = '11111111-1111-4111-8111-111111111111';
const document = '22222222-2222-4222-8222-222222222222';
const keyPath = join(scratch, 'synthetic-key.der');
// Publicly known synthetic seed, never a production credential.
const key = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 37)]),
  format: 'der', type: 'pkcs8'
});
writeFileSync(keyPath, key.export({ format: 'der', type: 'pkcs8' }), { flag: 'wx', mode: 0o600 });

// Paragraph pool for the `docs` fixture: prose paragraphs (blank-line separated,
// lines joined, 160 to 900 characters) of every markdown file in this
// repository's docs folder, in file-name order. The pool depends on the
// repository state, so the report records its size and SHA-256.
function pool() {
  const found = [];
  for (const name of readdirSync(join(here, '..')).filter((n) => n.endsWith('.md')).sort()) {
    for (const block of readFileSync(join(here, '..', name), 'utf8').split(/\r?\n\s*\r?\n/)) {
      const text = block.replace(/\s+/g, ' ').trim();
      if (text.length >= 160 && text.length <= 900 && !/^[|#`>*0-9-]/.test(text)) found.push(text);
    }
  }
  // De-duplicate so identical paragraphs cannot make the history look smaller.
  const unique = [...new Set(found)];
  assert(unique.length >= 2 * nodeCount + 256, 'documentation paragraph pool is too small');
  return unique;
}
const paragraphs = fixture === 'docs' ? pool() : null;
const label = (i, edit) => fixture === 'docs'
  ? paragraphs[(edit === 0 ? i : nodeCount + edit * 7 + i) % paragraphs.length]
  : (edit === 0 ? `Paragraph ${i}: ${'x'.repeat(256)}` : `Edit ${edit}: ${'y'.repeat(256)}`);

function runner(binary, database) {
  return (...args) => {
    const start = performance.now();
    const result = spawnSync(binary, ['repository', database, project, ...args], {
      encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 600_000
    });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    return { value: JSON.parse(result.stdout), ms: performance.now() - start };
  };
}
const metrics = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return { min: sorted[0], median: sorted[Math.floor(sorted.length / 2)],
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1], max: sorted.at(-1) };
};
const files = (database) => ['', '-wal', '-shm'].reduce((sum, suffix) =>
  sum + (existsSync(database + suffix) ? statSync(database + suffix).size : 0), 0);

function build(name, binary, expectedSchema) {
  const dir = join(scratch, name);
  mkdirSync(dir);
  const database = join(dir, 'project.sqlite');
  const run = runner(resolve(binary), database);
  const statePath = join(dir, 'state.json');
  const manifestPath = join(dir, 'commit.json');
  const nodes = {};
  for (let i = 0; i < nodeCount; i++) {
    const id = String(i + 1).padStart(26, '0');
    nodes[id] = { id, parent_id: null, type: 'paragraph', pos: `p${String(i).padStart(6, '0')}`,
      var_name: null, deleted: false, current_fields: { label: label(i, 0) } };
  }
  const state = { nodes, comments: [], suggestions: {}, removed_choices: [] };
  const ids = Object.keys(nodes);
  assert.equal(run('init').value.local_schema, expectedSchema);
  const writes = [];
  const commits = [];
  let previous = null;
  let firstRoot;
  let fullJson = 0;
  for (let revision = 0; revision <= edits; revision++) {
    if (revision > 0) {
      const index = (revision - 1) % nodeCount;
      nodes[ids[index]].current_fields.label = label(index, revision);
    }
    const json = JSON.stringify(state);
    fullJson += Buffer.byteLength(json);
    writeFileSync(statePath, json);
    const stored = run('store-state', document, statePath);
    if (revision === 0) firstRoot = stored.value.state;
    const manifest = { version: 1, project, parents: previous ? [previous] : [],
      files: { 'Report.docx': { document, state: stored.value.state } }, author_key: '',
      authored_at_ms: revision, message: `Synthetic revision ${revision}` };
    writeFileSync(manifestPath, JSON.stringify(manifest));
    const committed = run('commit', manifestPath, keyPath);
    previous = committed.value.commit;
    writes.push(stored.ms);
    commits.push(committed.ms);
  }
  run('set-head', 'main', previous);
  return { dir, database, run, firstRoot, previous, writes, commits, fullJson };
}

function inventory(run) {
  const keys = [];
  let after = [];
  for (;;) {
    const page = run('objects', '--limit', '256', ...after).value.objects;
    if (page.length === 0) return keys;
    keys.push(...page.map((o) => `${o.document}/${o.address}`));
    const last = page.at(-1);
    after = ['--after-document', last.document, '--after-address', last.address];
  }
}

try {
  const baseline = build('schema2', oldExe, 2);
  const packed = build('schema3', newExe, 3);
  const packedFilesBefore = files(packed.database);
  const baselineKeys = inventory(runner(resolve(newExe), join(baseline.dir, 'project.sqlite')));
  // The first open with the new CLI above upgraded the baseline file to schema 3.
  const beforeCompact = runner(resolve(newExe), baseline.database)('storage');
  const baselineFilesBefore = files(baseline.database);
  const compact = runner(resolve(newExe), baseline.database)('compact', '--vacuum');
  const again = runner(resolve(newExe), baseline.database)('compact');
  const afterCompact = runner(resolve(newExe), baseline.database)('storage');
  const packedRun = runner(resolve(newExe), packed.database);
  const packedUsage = packedRun('storage').value;
  const packedKeys = inventory(packedRun);
  const packedCompactNoop = packedRun('compact');
  const packedVacuum = runner(resolve(newExe), packed.database)('compact', '--vacuum');
  const packedPlan = packedRun('gc-plan', '--list', '0', '--top', '1').value;
  const firstOld = runner(resolve(newExe), baseline.database)('read-state', document, baseline.firstRoot);
  const firstNew = packedRun('read-state', document, packed.firstRoot);
  assert.deepEqual(firstOld.value, firstNew.value, 'first historical root reads identically');
  assert.equal(baseline.firstRoot, packed.firstRoot, 'addresses are identical');
  assert.deepEqual(baselineKeys, packedKeys, 'object inventories are identical');
  assert.equal(compact.value.complete, true);
  assert.equal(again.value.compacted, 0, 'second compaction rewrites nothing');
  assert.equal(afterCompact.value.bytes_used, beforeCompact.value.bytes_used);
  assert.equal(packedUsage.bytes_used, beforeCompact.value.bytes_used, 'logical bytes equal the baseline');
  const report = {
    kind: 'Synthetic local CLI compression at rest; one changed paragraph per commit',
    environment: { platform: platform(), arch: arch(), cpu: cpus()[0]?.model, node: process.version,
      build: 'debug builds; process startup and database reopen included in every timing' },
    fixture: { name: fixture, nodes: nodeCount, edits, commits: edits + 1,
      paragraph_pool: paragraphs ? { paragraphs: paragraphs.length,
        sha256: createHash('sha256').update(paragraphs.join('\n')).digest('hex') } : null },
    logical_bytes_used: beforeCompact.value.bytes_used,
    objects: beforeCompact.value.objects_used,
    commit_bytes_note: 'bytes_used includes signed-commit bodies, which are never compressed',
    sum_full_state_json_bytes: baseline.fullJson,
    schema2_identity_rows: {
      stored_bytes: beforeCompact.value.stored_bytes,
      sqlite_files_bytes: baselineFilesBefore,
      state_write_ms: metrics(baseline.writes),
      note: 'written by a schema-2 CLI, then opened (upgraded) by the schema-3 CLI'
    },
    schema3_written_compressed: {
      stored_bytes: packedUsage.stored_bytes,
      compressed_objects: packedPlan.physical.compressed_objects,
      sqlite_files_bytes: packedFilesBefore,
      state_write_ms: metrics(packed.writes),
      compact_rewrites_after_the_fact: packedCompactNoop.value.compacted,
      files_after_compact_vacuum: files(packed.database),
      vacuum: packedVacuum.value.vacuum
    },
    schema2_compacted: {
      stored_bytes: afterCompact.value.stored_bytes,
      compact_ms: compact.ms,
      rewritten_objects: compact.value.compacted,
      kept_identity_objects: compact.value.kept_identity,
      stored_bytes_saved: compact.value.stored_bytes_saved,
      sqlite_files_bytes_after_vacuum: files(baseline.database),
      vacuum: compact.value.vacuum,
      second_run_rewrites: again.value.compacted
    },
    ratios: {
      stored_over_logical_payload: afterCompact.value.stored_bytes / beforeCompact.value.bytes_used,
      sqlite_files_after_over_before: files(baseline.database) / baselineFilesBefore
    },
    assertions: ['identical object inventory and addresses for both build paths',
      'logical bytes_used equal between identity and compressed rows', 'second compaction rewrites nothing',
      'first historical root reads identically from both repositories'],
    limitations: ['debug CLI builds; codec speed is measured separately in release mode',
      'one document, paragraph nodes only; no assets or formatting runs',
      'SQLite file sizes include indexes, free pages, WAL and rollback state; payload counters do not',
      'not a PostgreSQL, HTTP transfer or peak-memory benchmark']
  };
  writeFileSync(join(scratch, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  process.stdout.write(`${fixture} ${nodeCount}x${edits}: logical ${report.logical_bytes_used}, stored ${afterCompact.value.stored_bytes}, ` +
    `files ${baselineFilesBefore} -> ${files(baseline.database)}, compact ${Math.round(compact.ms)} ms\n`);
} finally {
  unlinkSync(keyPath);
}
