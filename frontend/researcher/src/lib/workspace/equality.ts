/** JSON object key order is irrelevant; PostgreSQL JSONB reorders object keys. */
export function stableJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value) ?? 'null';
  if (Array.isArray(value)) return '[' + value.map(stableJson).join(',') + ']';
  const record = value as Record<string, unknown>;
  return (
    '{' +
    Object.keys(record)
      .filter((k) => record[k] !== undefined)
      .sort()
      .map((k) => JSON.stringify(k) + ':' + stableJson(record[k]))
      .join(',') +
    '}'
  );
}
