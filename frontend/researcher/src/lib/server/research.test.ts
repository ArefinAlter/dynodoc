import { describe, expect, it, vi } from 'vitest';
import {
  publicIPv4,
  recordsToCsv,
  researchOrigin,
  researchRead,
  type ResearchTransport
} from './research';
const base = {
  provider: 'kobo' as const,
  origin: 'https://kf.kobotoolbox.org',
  token: 'test-token',
  action: 'import' as const,
  form: 'abc',
  kind: 'submissions' as const
};
const reply = (value: unknown) => ({ status: 200, body: Buffer.from(JSON.stringify(value)) });
describe('research connections', () => {
  it('rejects local, reserved and alternate IP representations before a connection', () => {
    for (const ip of [
      '127.0.0.1',
      '10.0.0.1',
      '172.16.1.1',
      '192.168.1.1',
      '169.254.169.254',
      '100.100.100.200',
      '0.0.0.0',
      '198.18.0.1',
      '224.0.0.1',
      '::1',
      '::ffff:127.0.0.1'
    ])
      expect(publicIPv4(ip)).toBe(false);
    expect(publicIPv4('1.1.1.1')).toBe(true);
    for (const origin of [
      'http://example.org',
      'https://127.0.0.1',
      'https://2130706433',
      'https://user:password@example.org',
      'https://example.org:8443',
      'https://example.org/api/',
      'https://example.org?token=x'
    ])
      expect(() => researchOrigin(origin)).toThrow();
  });
  it('keeps response fields, nested values and arrays while escaping formula injection', () => {
    const csv = recordsToCsv([
      {
        id: '1',
        note: '=HYPERLINK("https://example.org")',
        person: { age: 42 },
        children: [{ age: 3 }]
      },
      { id: '2', extra: 'new' }
    ]);
    expect(csv).toContain('person/age');
    expect(csv).toContain('children');
    expect(csv).toContain("'=HYPERLINK");
    expect(csv).toContain('extra');
    expect(csv).toContain('42');
  });
  it('follows same-host Kobo pages and excludes the token from the imported record', async () => {
    const transport = vi
      .fn<ResearchTransport>()
      .mockResolvedValueOnce(
        reply({
          results: [{ _id: 1, answer: 'A' }],
          next: 'https://kf.kobotoolbox.org/api/v2/assets/abc/data/?start=1'
        })
      )
      .mockResolvedValueOnce(reply({ results: [{ _id: 2, answer: 'B' }], next: null }));
    const result = await researchRead(base, transport);
    expect(transport).toHaveBeenCalledTimes(2);
    expect(transport.mock.calls[0][1]).toBe('Token test-token');
    expect(JSON.stringify(result)).not.toContain('test-token');
    if (!('content_base64' in result)) throw new Error('Expected a file');
    expect(Buffer.from(result.content_base64, 'base64').toString()).toContain('2,B');
  });
  it('never forwards credentials to a cross-host pagination link', async () => {
    const transport = vi
      .fn<ResearchTransport>()
      .mockResolvedValue(
        reply({ results: [{ id: 1 }], next: 'https://attacker.example/api/v2/assets/abc/data/' })
      );
    await expect(researchRead(base, transport)).rejects.toThrow('different host');
    expect(transport).toHaveBeenCalledTimes(1);
  });
  it('uses Central bearer auth and returns the preserved XLSForm source', async () => {
    const bytes = Buffer.from([0x50, 0x4b, 3, 4]);
    const transport = vi.fn<ResearchTransport>().mockResolvedValue({ status: 200, body: bytes });
    const result = await researchRead(
      {
        ...base,
        provider: 'odk',
        origin: 'https://study.example.org',
        project: '12',
        form: 'household',
        kind: 'questionnaire'
      },
      transport
    );
    expect(transport.mock.calls[0][0].pathname).toBe('/v1/projects/12/forms/household.xlsx');
    expect(transport.mock.calls[0][1]).toBe('Bearer test-token');
    expect('content_base64' in result && result.content_base64).toBe(bytes.toString('base64'));
  });
  it('refuses truncation and reports revoked credentials without upstream details', async () => {
    await expect(
      researchRead(base, async () => reply({ count: 3000, results: [{ id: 1 }], next: null }))
    ).rejects.toThrow('2,999');
    await expect(
      researchRead(base, async () => ({ status: 403, body: Buffer.from('private server details') }))
    ).rejects.toThrow('permission');
  });
  it('lists Central projects and paginates Kobo survey discovery', async () => {
    const odk = await researchRead({ ...base, provider: 'odk', action: 'projects' }, async () =>
      reply([{ id: 4, name: 'Study' }])
    );
    expect(odk).toEqual({ items: [{ id: '4', name: 'Study' }] });
    const kobo = await researchRead({ ...base, action: 'projects' }, async () =>
      reply({
        results: [
          { uid: 'a1', name: 'Questions', asset_type: 'survey' },
          { uid: 'a2', asset_type: 'collection' }
        ],
        next: null
      })
    );
    expect(kobo).toEqual({ items: [{ id: 'a1', name: 'Questions' }] });
  });
});
