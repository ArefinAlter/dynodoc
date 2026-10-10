import { afterEach, expect, it, vi } from 'vitest';
import { beginPairing, finishPairing } from './pairing';
afterEach(() => vi.unstubAllGlobals());
it('puts only a hash in the browser link and authenticates completion with the editor secret', async () => {
  const p = await beginPairing('https://app.test', 'word');
  expect(p.token).toMatch(/^[0-9a-f]{64}$/);
  expect(p.url).not.toContain(p.token);
  expect(new URL(p.url).searchParams.get('challenge')).toBe(p.challenge);
  expect(p.expires).toBeGreaterThan(Date.now() / 1000);
  const request = vi.fn(
    async () => new Response(JSON.stringify({ host: 'word', document_id: 'the-file' }))
  );
  vi.stubGlobal('fetch', request);
  expect(await finishPairing('https://app.test', 'word', p)).toEqual({
    documentId: 'the-file',
    token: p.token
  });
  expect(request.mock.calls[0]).toEqual([
    'https://app.test/api/connectors/connection',
    expect.objectContaining({
      headers: { authorization: `Bearer ${p.token}` },
      credentials: 'omit',
      redirect: 'error'
    })
  ]);
});
it('rejects insecure origins, expired links, incomplete approval and the wrong host', async () => {
  await expect(beginPairing('http://app.test', 'excel')).rejects.toThrow('HTTPS');
  const p = await beginPairing('https://app.test', 'excel');
  const fetcher = vi.fn(async () => new Response('{}', { status: 401 }));
  vi.stubGlobal('fetch', fetcher);
  await expect(finishPairing('https://app.test', 'excel', { ...p, expires: 0 })).rejects.toThrow(
    'expired'
  );
  expect(fetcher).not.toHaveBeenCalled();
  await expect(finishPairing('https://app.test', 'excel', p)).rejects.toThrow('Approve');
  fetcher.mockImplementation(async () => new Response('{"host":"word"}'));
  await expect(finishPairing('https://app.test', 'excel', p)).rejects.toThrow('different editor');
});
