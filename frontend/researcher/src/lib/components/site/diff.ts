/** One rendered line of a <DiffCard>. */
export type DiffLine = {
  kind: 'add' | 'del' | 'ctx' | 'hunk';
  text: string;
};

/**
 * Parse unified-diff style text: `@@ ... @@` hunk headers, `+` additions, `-` deletions;
 * anything else is context (a single leading space is dropped). File headers
 * (`---`, `+++`) are skipped, so a diff can be pasted in as it is.
 */
export function parsePatch(patch: string): DiffLine[] {
  const lines: DiffLine[] = [];
  for (const raw of patch.replace(/\r\n?/g, '\n').split('\n')) {
    if (raw.startsWith('+++') || raw.startsWith('---')) continue;
    if (raw.startsWith('@@')) lines.push({ kind: 'hunk', text: raw });
    else if (raw.startsWith('+')) lines.push({ kind: 'add', text: raw.slice(1) });
    else if (raw.startsWith('-')) lines.push({ kind: 'del', text: raw.slice(1) });
    else lines.push({ kind: 'ctx', text: raw.startsWith(' ') ? raw.slice(1) : raw });
  }
  while (lines.length && lines[lines.length - 1].kind === 'ctx' && !lines[lines.length - 1].text)
    lines.pop();
  return lines;
}
