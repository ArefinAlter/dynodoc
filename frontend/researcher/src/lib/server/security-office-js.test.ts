// The Office panes' content security policy in both states: with the Microsoft CDN (the default,
// byte for byte what it always was) and with a local copy of Office.js (no Microsoft host).
import { describe, expect, it } from 'vitest';
import { secureResponse } from './security';

const MICROSOFT = [
  'https://appsforoffice.microsoft.com',
  'https://officeapis.public.onecdn.static.microsoft',
  'https://ajax.aspnetcdn.com'
];
const BASE =
  "default-src 'none'; script-src 'self' 'nonce-n'; connect-src 'self'; object-src 'none'";
const ANCESTORS =
  "'self' https://*.officeapps.live.com https://*.office.com https://*.office365.com https://*.microsoft365.com https://*.cloud.microsoft https://office.live.com https://onedrive.live.com https://*.sharepoint.com";
const response = () => new Response('x', { headers: { 'content-security-policy': BASE } });
const at = (path: string) => new URL(path, 'https://dynodoc.corp.example');
// The call as it was before the mode existed: no fifth argument.
const omitted = (path: string) =>
  secureResponse(response(), at(path)).headers.get('content-security-policy')!;
const policyOf = (
  path: string,
  connectorOrigins = '',
  connectSrc: string[] = [],
  officeJs: 'cdn' | 'local' = 'cdn'
) =>
  secureResponse(response(), at(path), connectorOrigins, connectSrc, officeJs).headers.get(
    'content-security-policy'
  )!;
const PANES = ['/connectors/word', '/connectors/excel', '/connectors/powerpoint/'];

describe('the pane policy with the Microsoft CDN', () => {
  it('is exactly the policy the panes always had, whether the mode is omitted or named', () => {
    const golden = `default-src 'none'; script-src 'self' 'nonce-n' ${MICROSOFT.join(' ')}; connect-src 'self' ${MICROSOFT.join(' ')}; object-src 'none'; frame-ancestors ${ANCESTORS}`;
    for (const path of PANES) {
      expect(omitted(path), path).toBe(golden);
      expect(policyOf(path, '', [], 'cdn'), path).toBe(golden);
    }
  });
});

describe('the pane policy with a local copy of Office.js', () => {
  it('lists no Microsoft host to load scripts or connect to, and keeps the rest', () => {
    for (const path of PANES) {
      const policy = policyOf(path, '', [], 'local');
      expect(policy, path).toBe(
        `default-src 'none'; script-src 'self' 'nonce-n'; connect-src 'self'; object-src 'none'; frame-ancestors ${ANCESTORS}`
      );
      for (const host of MICROSOFT) expect(policy).not.toContain(host);
      expect(policy).toContain("script-src 'self' 'nonce-n';");
    }
  });
  it('still allows Office to frame the pane, and still adds configured connector origins', () => {
    const policy = policyOf(
      '/connectors/word',
      'https://other.corp.example',
      ['https://dynodoc.corp.example'],
      'local'
    );
    expect(policy).toContain('https://*.officeapps.live.com');
    expect(policy).toContain(
      "connect-src 'self' https://dynodoc.corp.example https://other.corp.example;"
    );
    expect(policy).not.toContain('microsoft.com');
  });
  it('changes nothing for a page that is not a pane, in either state', () => {
    for (const path of [
      '/login',
      '/projects',
      '/connectors',
      '/connectors/addins',
      '/connectors/authorize'
    ])
      expect(policyOf(path, '', [], 'local'), path).toBe(policyOf(path, '', [], 'cdn'));
  });
});
