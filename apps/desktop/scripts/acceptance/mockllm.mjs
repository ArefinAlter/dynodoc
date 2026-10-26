// A mock model provider on loopback for the agent acceptance run. It speaks the three wire styles the app
// speaks (Anthropic Messages, OpenAI chat completions, Gemini streamGenerateContent), checks the key it is
// sent against the one it expects, logs what it received (never the key), and plays a small script keyed on
// what the person asked: it calls real tools of the closed table (and one that is not in it), then answers
// from what the tools returned.
//
//   node mockllm.mjs <port> <expected-key> <log.jsonl>
import http from 'node:http';
import { appendFileSync, writeFileSync } from 'node:fs';

const [, , PORT = '9741', EXPECT = 'DYNODOC-CANARY-key-ok-0123456789', LOG = 'mockllm.jsonl'] = process.argv;
writeFileSync(LOG, '');
const log = (o) => appendFileSync(LOG, JSON.stringify({ at: Date.now(), ...o }) + '\n');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let n = 0;

/** What to do next, from the person's last prompt and the tool answers so far. */
function decide(prompt, results) {
  const p = prompt.toLowerCase();
  const k = results.length;
  const number = (s) => Number(/#(\d+)/.exec(s)?.[1] ?? 1);
  if (p.includes('long')) return { slow: true };
  if (p.includes('summarise')) {
    if (k === 0) return { text: 'Let me read the open change requests.', calls: [{ name: 'requests.list', args: { state: 'open' } }] };
    if (k === 1) return { calls: [{ name: 'requests.summarise', args: { number: number(results[0]) } }] };
    return { text: `Here is what I read.\n\n- The list said: ${results[0].split('\n')[1] ?? ''}\n- The summary said: ${results[1].replace(/\s+/g, ' ').slice(0, 260)}\n\nI can read and summarise these; approving and merging are yours.` };
  }
  if (p.includes('propose a review comment')) {
    if (k === 0) return { calls: [{ name: 'review.propose_comment', args: { number: number(prompt), text: 'Could you add a line on why the retention period changes?' } }] };
    return { text: `I drafted one comment on the request. ${results[0]}` };
  }
  if (p.includes('merge')) {
    if (k === 0) return { text: 'I will merge it.', calls: [{ name: 'requests.merge', args: { number: number(prompt) } }] };
    return { text: `That did not work: ${results[0]} Merging is a person's button.` };
  }
  if (p.includes('commit summary')) {
    if (k === 0) return { calls: [{ name: 'commit.draft_summary', args: { summary: 'Update the project charter', description: 'Project Charter.docx: retention wording' } }] };
    return { text: `Drafted. ${results[0]}` };
  }
  return { text: 'Hello from the mock model. I have no tools to call for that.' };
}

function body(req) {
  return new Promise((res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => res(Buffer.concat(chunks).toString('utf8')));
  });
}

const sse = (res, event, data) => res.write((event ? `event: ${event}\n` : '') + `data: ${JSON.stringify(data)}\n\n`);
const approx = (s) => Math.ceil(s.length / 4);
const wire = (name) => name.replace(/\./g, '__');
const unwire = (name) => name.replace(/__/g, '.');

async function slowText(res, emit, closed) {
  for (let i = 0; i < 80 && !closed(); i++) {
    emit(`word${i} `);
    await sleep(150);
  }
}

// ---- Anthropic ------------------------------------------------------------------------------------------
function anthropicTurn(b) {
  const msgs = b.messages ?? [];
  let lastUser = -1;
  msgs.forEach((m, i) => {
    if (m.role === 'user' && (typeof m.content === 'string' || m.content.some((c) => c.type === 'text'))) lastUser = i;
  });
  const first = msgs[lastUser];
  const prompt = typeof first.content === 'string' ? first.content : first.content.filter((c) => c.type === 'text').map((c) => c.text).join('\n');
  const results = [];
  for (const m of msgs.slice(lastUser + 1)) {
    if (m.role === 'user' && Array.isArray(m.content)) for (const c of m.content) if (c.type === 'tool_result') results.push(typeof c.content === 'string' ? c.content : (c.content ?? []).map((x) => x.text).join(''));
  }
  return { prompt, results, tools: (b.tools ?? []).map((t) => unwire(t.name)), system: b.system };
}

async function anthropic(req, res, raw) {
  const b = JSON.parse(raw);
  const key = req.headers['x-api-key'] ?? '';
  if (key !== EXPECT) {
    log({ route: 'anthropic', rejected: true, path: req.url });
    res.writeHead(401, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ type: 'error', error: { type: 'authentication_error', message: `invalid x-api-key: ${key}` } }));
    return;
  }
  const t = anthropicTurn(b);
  const plan = decide(t.prompt, t.results);
  log({ route: 'anthropic', path: req.url, model: b.model, key_ok: true, tools: t.tools, system_len: String(t.system ?? '').length, prompt: t.prompt.slice(0, 120), results: t.results.length, body_bytes: raw.length });
  if (++n === 1) writeFileSync(LOG.replace(/\.jsonl$/, '') + '-first-request.json', JSON.stringify({ headers_present: Object.keys(req.headers).filter((h) => h !== 'x-api-key'), key_header_present: true, body: b }, null, 2));
  res.writeHead(200, { 'content-type': 'text/event-stream' });
  let closed = false;
  res.on('close', () => { closed = true; });
  sse(res, 'message_start', { type: 'message_start', message: { usage: { input_tokens: approx(raw), output_tokens: 1 } } });
  let index = 0;
  if (plan.slow) {
    sse(res, 'content_block_start', { type: 'content_block_start', index, content_block: { type: 'text', text: '' } });
    await slowText(res, (w) => sse(res, 'content_block_delta', { type: 'content_block_delta', index, delta: { type: 'text_delta', text: w } }), () => closed);
    if (closed) { log({ route: 'anthropic', client_closed_early: true }); return res.end(); }
  }
  if (plan.text) {
    sse(res, 'content_block_start', { type: 'content_block_start', index, content_block: { type: 'text', text: '' } });
    for (const part of plan.text.match(/.{1,24}/gs)) { sse(res, 'content_block_delta', { type: 'content_block_delta', index, delta: { type: 'text_delta', text: part } }); await sleep(25); }
    sse(res, 'content_block_stop', { type: 'content_block_stop', index });
    index += 1;
  }
  for (const c of plan.calls ?? []) {
    sse(res, 'content_block_start', { type: 'content_block_start', index, content_block: { type: 'tool_use', id: `toolu_${n}_${index}`, name: wire(c.name), input: {} } });
    const json = JSON.stringify(c.args);
    sse(res, 'content_block_delta', { type: 'content_block_delta', index, delta: { type: 'input_json_delta', partial_json: json.slice(0, 5) } });
    sse(res, 'content_block_delta', { type: 'content_block_delta', index, delta: { type: 'input_json_delta', partial_json: json.slice(5) } });
    sse(res, 'content_block_stop', { type: 'content_block_stop', index });
    index += 1;
  }
  sse(res, 'message_delta', { type: 'message_delta', delta: { stop_reason: plan.calls?.length ? 'tool_use' : 'end_turn' }, usage: { output_tokens: 40 + (plan.text?.length ?? 0) / 4 | 0 } });
  sse(res, 'message_stop', { type: 'message_stop' });
  res.end();
}

// ---- OpenAI chat completions (what Ollama and OpenAI-compatible servers speak) -------------------------------
async function openai(req, res, raw) {
  const b = JSON.parse(raw);
  const auth = req.headers.authorization ?? '';
  const msgs = b.messages ?? [];
  let lastUser = -1;
  msgs.forEach((m, i) => { if (m.role === 'user') lastUser = i; });
  const prompt = String(msgs[lastUser].content);
  const results = msgs.slice(lastUser + 1).filter((m) => m.role === 'tool').map((m) => String(m.content));
  const plan = decide(prompt, results);
  log({ route: 'openai', path: req.url, model: b.model, bearer_present: auth.startsWith('Bearer '), tools: (b.tools ?? []).map((t) => unwire(t.function.name)), prompt: prompt.slice(0, 120), results: results.length });
  res.writeHead(200, { 'content-type': 'text/event-stream' });
  const chunk = (delta, finish = null, extra = {}) => sse(res, null, { choices: [{ index: 0, delta, finish_reason: finish }], ...extra });
  if (plan.text) for (const part of plan.text.match(/.{1,24}/gs)) { chunk({ content: part }); await sleep(25); }
  let i = 0;
  for (const c of plan.calls ?? []) {
    const json = JSON.stringify(c.args);
    chunk({ tool_calls: [{ index: i, id: `call_${n}_${i}`, type: 'function', function: { name: wire(c.name), arguments: json.slice(0, 5) } }] });
    chunk({ tool_calls: [{ index: i, function: { arguments: json.slice(5) } }] });
    i += 1;
  }
  chunk({}, plan.calls?.length ? 'tool_calls' : 'stop');
  sse(res, null, { choices: [], usage: { prompt_tokens: approx(raw), completion_tokens: 30 } });
  res.write('data: [DONE]\n\n');
  res.end();
}

// ---- Gemini ---------------------------------------------------------------------------------------------------
async function gemini(req, res, raw) {
  const b = JSON.parse(raw);
  const keyOk = (req.headers['x-goog-api-key'] ?? '') === EXPECT;
  if (!keyOk) {
    log({ route: 'gemini', rejected: true });
    res.writeHead(403, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ error: { code: 403, message: 'API key not valid.', status: 'PERMISSION_DENIED' } }));
    return;
  }
  const contents = b.contents ?? [];
  let lastUser = -1;
  contents.forEach((c, i) => { if (c.role === 'user' && c.parts.some((p) => p.text !== undefined)) lastUser = i; });
  const prompt = contents[lastUser].parts.filter((p) => p.text !== undefined).map((p) => p.text).join('\n');
  const results = [];
  for (const c of contents.slice(lastUser + 1)) for (const p of c.parts) if (p.functionResponse) results.push(String(p.functionResponse.response?.output ?? p.functionResponse.response?.result ?? JSON.stringify(p.functionResponse.response)));
  const plan = decide(prompt, results);
  const decls = (b.tools ?? []).flatMap((t) => t.functionDeclarations ?? []);
  log({ route: 'gemini', path: req.url.split('?')[0], key_ok: true, key_in_url: /key=/.test(req.url), tools: decls.map((d) => unwire(d.name)), prompt: prompt.slice(0, 120), results: results.length });
  res.writeHead(200, { 'content-type': 'text/event-stream' });
  const parts = [];
  if (plan.text) for (const part of plan.text.match(/.{1,24}/gs)) { sse(res, null, { candidates: [{ content: { role: 'model', parts: [{ text: part }] } }] }); await sleep(25); }
  for (const c of plan.calls ?? []) parts.push({ functionCall: { name: wire(c.name), args: c.args }, thoughtSignature: 'sig-' + c.name });
  sse(res, null, { candidates: [{ content: { role: 'model', parts }, finishReason: 'STOP' }], usageMetadata: { promptTokenCount: approx(raw), candidatesTokenCount: 30 } });
  res.end();
}

http
  .createServer(async (req, res) => {
    try {
      const raw = await body(req);
      if (req.method === 'POST' && req.url.startsWith('/v1/messages')) return await anthropic(req, res, raw);
      if (req.method === 'POST' && /\/chat\/completions/.test(req.url)) return await openai(req, res, raw);
      if (req.method === 'POST' && /:streamGenerateContent/.test(req.url)) return await gemini(req, res, raw);
      log({ route: 'unknown', method: req.method, path: req.url });
      res.writeHead(404);
      res.end('not found');
    } catch (e) {
      log({ error: String(e).slice(0, 200) });
      try { res.writeHead(500); res.end('error'); } catch {}
    }
  })
  .listen(Number(PORT), '127.0.0.1', () => console.log('mock model on', PORT));
