import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { expect, it, vi } from 'vitest';
for (const host of ['google-docs', 'google-sheets', 'google-slides'])
  it(`${host} pairs through the private server bridge and keeps the current file scope`, () => {
    const token = 'd'.repeat(64),
      connected = vi.fn(() => ({ scope: 'local-file', checkpoint: {} })),
      fetcher = vi.fn(() => ({
        getResponseCode: () => 200,
        getContentText: () => JSON.stringify({ host, document_id: 'remote-file' })
      }));
    const context = {
      DYNODOC_HOST: host,
      DYNODOC_ORIGIN: 'https://app.test',
      scope_: (scope: string) => {
        if (scope !== 'local-file') throw new Error('Working copy changed');
        return { key: scope };
      },
      connectDynodoc: connected,
      UrlFetchApp: { fetch: fetcher }
    } as unknown as Record<string, unknown> & {
      connectApprovedDynodoc: (token: string, scope: string) => unknown;
    };
    runInNewContext(readFileSync(`../../extensions/${host}/Pairing.gs`, 'utf8'), context);
    context.connectApprovedDynodoc(token, 'local-file');
    expect(fetcher.mock.calls[0]).toEqual([
      'https://app.test/api/connectors/connection',
      {
        headers: { Authorization: `Bearer ${token}` },
        muteHttpExceptions: true,
        followRedirects: false
      }
    ]);
    expect(connected).toHaveBeenCalledWith('remote-file', token, 'local-file');
    connected.mockClear();
    expect(() => context.connectApprovedDynodoc(token, 'other-file')).toThrow(
      'Working copy changed'
    );
    fetcher.mockImplementation(() => ({
      getResponseCode: () => 200,
      getContentText: () => '{"host":"word"}'
    }));
    expect(() => context.connectApprovedDynodoc(token, 'local-file')).toThrow('different editor');
    expect(connected).not.toHaveBeenCalled();
  });
