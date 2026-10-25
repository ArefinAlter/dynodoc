// The provider ladder for the agent (bring your own key). The client tries the entries in order
// and uses the first that is enabled and has what it needs. Keys typed here stay in this window's
// memory: they are never written to storage, never sent anywhere by this release, and shown only
// as "stored" with the last four characters. Pure parts are tested in client/test/ladder.test.mjs.

export type ProviderKind = 'anthropic' | 'openai' | 'google' | 'mistral' | 'deepseek' | 'qwen' | 'kimi' | 'glm' | 'ollama';

export interface ProviderInfo {
  kind: ProviderKind;
  label: string;
  base: string;
  model: string;
  needsKey: boolean;
}

export const PROVIDERS: ProviderInfo[] = [
  { kind: 'anthropic', label: 'Anthropic', base: 'https://api.anthropic.com', model: 'claude-sonnet-4-5', needsKey: true },
  { kind: 'openai', label: 'OpenAI', base: 'https://api.openai.com/v1', model: 'gpt-5', needsKey: true },
  { kind: 'google', label: 'Google Gemini', base: 'https://generativelanguage.googleapis.com/v1beta', model: 'gemini-2.5-pro', needsKey: true },
  { kind: 'mistral', label: 'Mistral', base: 'https://api.mistral.ai/v1', model: 'mistral-large-latest', needsKey: true },
  { kind: 'deepseek', label: 'DeepSeek', base: 'https://api.deepseek.com', model: 'deepseek-chat', needsKey: true },
  { kind: 'qwen', label: 'Qwen (Alibaba Cloud)', base: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1', model: 'qwen-max', needsKey: true },
  { kind: 'kimi', label: 'Kimi (Moonshot)', base: 'https://api.moonshot.ai/v1', model: 'kimi-k2', needsKey: true },
  { kind: 'glm', label: 'GLM (Zhipu)', base: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4.6', needsKey: true },
  { kind: 'ollama', label: 'Ollama (this computer)', base: 'http://127.0.0.1:11434', model: 'llama3.1', needsKey: false },
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
