import { checkFileSize } from './file-limits';
import { Unzip, UnzipInflate, strFromU8 } from 'fflate';

/** Streaming expansion enforces limits on actual output, not attacker supplied ZIP sizes. */
export function readOfficeZip(bytes: Uint8Array, want: (name: string) => boolean) {
  checkFileSize(bytes.byteLength);
  const entries = new Map<string, Uint8Array>();
  let count = 0,
    expanded = 0,
    failure: Error | null = null;
  const unzip = new Unzip((file) => {
    if (++count > 10000) throw new Error('The archive contains too many entries.');
    if (/\.(bin|vba)$/i.test(file.name) && file.name.includes('vba'))
      throw new Error('Macro-enabled files are not supported.');
    if (!want(file.name)) return;
    if (entries.has(file.name)) throw new Error('The archive contains duplicate parts.');
    entries.set(file.name, new Uint8Array());
    const chunks: Uint8Array[] = [];
    let size = 0;
    file.ondata = (error, chunk, final) => {
      if (error) {
        failure = error;
        throw error;
      }
      size += chunk.length;
      expanded += chunk.length;
      if (size > 32 * 1024 * 1024 || expanded > 64 * 1024 * 1024) {
        file.terminate();
        throw new Error('The expanded document exceeds the 64 MB safety limit.');
      }
      chunks.push(chunk);
      if (final) {
        const result = new Uint8Array(size);
        let at = 0;
        for (const c of chunks) {
          result.set(c, at);
          at += c.length;
        }
        entries.set(file.name, result);
      }
    };
    file.start();
  });
  unzip.register(UnzipInflate);
  for (let at = 0; at < bytes.length; at += 65536)
    unzip.push(bytes.subarray(at, at + 65536), at + 65536 >= bytes.length);
  if (failure) throw failure;
  return entries;
}
export function xmlText(bytes: Uint8Array | undefined) {
  if (!bytes) return '';
  const text = strFromU8(bytes);
  if (/<!DOCTYPE|<!ENTITY/i.test(text))
    throw new Error('XML entity declarations are not supported.');
  // Fail before DOM allocation when a malicious tree exceeds the supported nesting depth.
  let depth = 0;
  for (const tag of text.matchAll(/<\/?[A-Za-z_][^>]*>/g)) {
    if (tag[0].startsWith('</')) depth--;
    else if (!tag[0].endsWith('/>') && ++depth > 128)
      throw new Error('Document XML is nested too deeply.');
  }
  return text;
}
