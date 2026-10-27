// The provider ladder for the agent (bring your own key). The client tries the entries in order
// and uses the first that is enabled and has what it needs. In the desktop app a key goes to the
// operating system's credential store through the app (agent_key_set) and is never read back: the
// page keeps only "stored" and the last four characters. In a hosted copy there is no model call
// and a typed key stays in the tab's memory. Pure parts are tested in client/test/keys-ladder-hash.test.mjs.

export type ProviderKind = 'codex' | 'claude' | 'cursor' | 'antigravity' | 'anthropic' | 'openai' | 'google' | 'mistral' | 'deepseek' | 'qwen' | 'kimi' | 'glm' | 'ollama' | 'compat';

export interface ProviderInfo {
  kind: ProviderKind;
  label: string;
  base: string;
  model: string;
  needsKey: boolean;
}

export const PROVIDERS: ProviderInfo[] = [
  { kind: 'codex', label: 'Codex (existing sign-in)', base: 'https://codex.local-agent.invalid/v1', model: 'default', needsKey: false },
  { kind: 'claude', label: 'Claude Code (existing sign-in)', base: 'https://claude.local-agent.invalid/v1', model: 'default', needsKey: false },
  { kind: 'cursor', label: 'Cursor (existing sign-in)', base: 'https://cursor.local-agent.invalid/v1', model: 'default', needsKey: false },
  { kind: 'antigravity', label: 'Antigravity (existing sign-in)', base: 'https://antigravity.local-agent.invalid/v1', model: 'default', needsKey: false },

  { kind: 'anthropic', label: 'Anthropic', base: 'https://api.anthropic.com', model: 'claude-sonnet-4-5', needsKey: true },
  { kind: 'openai', label: 'OpenAI', base: 'https://api.openai.com/v1', model: 'gpt-5', needsKey: true },
  { kind: 'google', label: 'Google Gemini', base: 'https://generativelanguage.googleapis.com/v1beta', model: 'gemini-2.5-pro', needsKey: true },
  { kind: 'mistral', label: 'Mistral', base: 'https://api.mistral.ai/v1', model: 'mistral-large-latest', needsKey: true },
  { kind: 'deepseek', label: 'DeepSeek', base: 'https://api.deepseek.com', model: 'deepseek-chat', needsKey: true },
  { kind: 'qwen', label: 'Qwen (Alibaba Cloud)', base: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1', model: 'qwen-max', needsKey: true },
  { kind: 'kimi', label: 'Kimi (Moonshot)', base: 'https://api.moonshot.ai/v1', model: 'kimi-k2', needsKey: true },
  { kind: 'glm', label: 'GLM (Zhipu)', base: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4.6', needsKey: true },
  { kind: 'ollama', label: 'Ollama (this computer)', base: 'http://127.0.0.1:11434', model: 'llama3.1', needsKey: false },
  { kind: 'compat', label: 'OpenAI-compatible server', base: 'http://127.0.0.1:1234/v1', model: 'local-model', needsKey: true },
];

export interface LadderEntry {
  id: string;
  kind: ProviderKind;
  enabled: boolean;
  model: string;
  base: string;
  /** Last four characters of a key held in memory, or "" when none. Never the key. */
  keyHint: string;
}

export function isLoopback(host: string): boolean {
  return host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '::1';
}

export interface Issue {
  id: string;
  message: string;
}

const MODEL = /^[A-Za-z0-9._:/@+-]{1,200}$/;

export function validate(list: LadderEntry[]): Issue[] {
  const out: Issue[] = [];
  for (const e of list) {
    const info = PROVIDERS.find((p) => p.kind === e.kind);
    if (!info) {
      out.push({ id: e.id, message: 'Unknown provider.' });
      continue;
    }
    if (e.enabled && !MODEL.test(e.model)) out.push({ id: e.id, message: 'A model id uses letters, digits and . _ : / @ + -.' });
    let url: URL | null = null;
    try {
      url = new URL(e.base);
    } catch {
      out.push({ id: e.id, message: 'The base address is not a URL.' });
    }
    if (url) {
      if (url.username || url.password || url.search || url.hash) out.push({ id: e.id, message: 'No user name, password, query or fragment in a base address.' });
      if (url.protocol === 'http:' && !isLoopback(url.hostname)) out.push({ id: e.id, message: 'Plain http only to this computer (localhost).' });
      else if (url.protocol !== 'http:' && url.protocol !== 'https:') out.push({ id: e.id, message: 'Use https, or http to this computer.' });
    }
    if (e.enabled && info.needsKey && !e.keyHint) out.push({ id: e.id, message: `${info.label} needs a key.` });
  }
  return out;
}

/** The entry the agent would use: the first enabled one without issues. */
export function active(list: LadderEntry[]): LadderEntry | null {
  const bad = new Set(validate(list).map((i) => i.id));
  return list.find((e) => e.enabled && !bad.has(e.id)) ?? null;
}

/** What may be shown of a key: its last four characters, and only when it is long enough that
 *  four characters do not give most of it away. */
export function keyHint(key: string): string {
  const k = key.trim();
  return k.length >= 12 ? k.slice(-4) : k.length > 0 ? '····' : '';
}
