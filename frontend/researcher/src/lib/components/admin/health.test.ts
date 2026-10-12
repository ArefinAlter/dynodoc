import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  hostIsStale,
  needsAttention,
  relativeTime,
  serviceLabel,
  type HostStatus
} from './health';

const now = Date.parse('2026-10-12T12:00:00Z');
const ago = (ms: number) => new Date(now - ms).toISOString();
const GiB = 1024 ** 3;
const healthy: HostStatus = {
  checked_at: ago(30_000),
  services: [
    { name: 'dynodoc-demo-web-1', running: true },
    { name: 'dynodoc-demo-api-1', running: true, health: 'healthy' }
  ],
  disk_free_bytes: 20 * GiB,
  backup: { created_at: ago(3_600_000), checksum_ok: true }
};
const ok = { google: true, email: true };

describe('administration health guidance', () => {
  it('reports nothing for a fresh, healthy host', () => {
    expect(needsAttention(healthy, ok, now)).toEqual([]);
  });
  it('never presents a missing host report as healthy', () => {
    const items = needsAttention(null, ok, now);
    expect(items.map((i) => i.id)).toEqual(['host-missing']);
    expect(hostIsStale(null, now)).toBe(true);
  });
  it('flags stale reports, stopped and unhealthy services', () => {
    const host: HostStatus = {
      ...healthy,
      checked_at: ago(10 * 60_000),
      services: [
        { name: 'dynodoc-demo-web-1', running: false, health: 'Unavailable' },
        { name: 'dynodoc-demo-api-1', running: true, health: 'unhealthy' }
      ]
    };
    const items = needsAttention(host, ok, now);
    expect(items.map((i) => i.id)).toEqual([
      'service-dynodoc-demo-web-1',
      'service-dynodoc-demo-api-1',
      'host-stale'
    ]);
    expect(items[0].level).toBe('danger');
    expect(items[2].level).toBe('warning');
  });
  it('flags low disk, old backups, failed checksums and missing credentials', () => {
    const host: HostStatus = {
      ...healthy,
      disk_free_bytes: GiB / 2,
      backup: { created_at: ago(72 * 3_600_000), checksum_ok: false }
    };
    const items = needsAttention(host, { google: false, email: false }, now);
    const byId = Object.fromEntries(items.map((i) => [i.id, i.level]));
    expect(byId).toMatchObject({
      'disk-low': 'danger',
      'backup-stale': 'warning',
      'backup-checksum': 'danger',
      'google-missing': 'warning',
      'email-missing': 'warning'
    });
    // Most urgent first.
    expect(items.findIndex((i) => i.level === 'warning')).toBeGreaterThan(
      items.map((i) => i.level).lastIndexOf('danger')
    );
  });
  it('flags a host that has never taken a backup', () => {
    const items = needsAttention({ ...healthy, backup: null }, ok, now);
    expect(items.map((i) => i.id)).toEqual(['backup-missing']);
  });
});

describe('formatting', () => {
  it('chooses a readable byte unit', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1536)).toBe('1.5 KiB');
    expect(formatBytes(268_435_456)).toBe('256 MiB');
    expect(formatBytes(undefined)).toBe('Unavailable');
  });
  it('shortens container names', () => {
    expect(serviceLabel('dynodoc-demo-postgres-1')).toBe('postgres');
    expect(serviceLabel('other')).toBe('other');
  });
  it('words relative times', () => {
    expect(relativeTime(now - 10_000, now)).toBe('just now');
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe('3 hours ago');
    expect(relativeTime(now - 2 * 86_400_000, now)).toBe('2 days ago');
  });
});
