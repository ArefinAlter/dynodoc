/** Pure helpers that turn the host report and overview counts into administrator guidance. */
export type HostService = {
  name: string;
  running: boolean;
  health?: string | null;
  started_at?: string | null;
};
export type HostStatus = {
  checked_at: string;
  services?: HostService[];
  disk_free_bytes?: number;
  backup?: { created_at?: string; bytes?: number; checksum_ok?: boolean } | null;
};
export type Providers = { google: boolean; email: boolean };
export type Attention = {
  id: string;
  level: 'danger' | 'warning';
  title: string;
  detail: string;
  href: string;
};

export const HOST_STALE_MS = 3 * 60_000;
export const BACKUP_STALE_MS = 48 * 3_600_000;
export const DISK_WARN_BYTES = 5 * 1024 ** 3;
export const DISK_CRITICAL_BYTES = 1024 ** 3;

const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
export function formatBytes(value: number | null | undefined) {
  let n = Number(value);
  if (!Number.isFinite(n) || n < 0) return 'Unavailable';
  let unit = 0;
  while (n >= 1024 && unit < units.length - 1) {
    n /= 1024;
    unit++;
  }
  return `${new Intl.NumberFormat('en', { maximumFractionDigits: unit === 0 ? 0 : 1 }).format(n)} ${units[unit]}`;
}

/** "dynodoc-demo-web-1" -> "web". Unknown names are kept as reported. */
export function serviceLabel(name: string) {
  return name.replace(/^dynodoc-demo-/, '').replace(/-\d+$/, '') || name;
}
export function serviceState(service: HostService) {
  return service.running ? 'Running' : 'Stopped';
}
export function serviceOk(service: HostService) {
  return service.running && service.health !== 'unhealthy';
}

/** Coarse "3 hours" / "2 days" wording for server-rendered guidance text. */
export function span(ms: number) {
  const minutes = Math.max(1, Math.round(ms / 60_000));
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'}`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours} hour${hours === 1 ? '' : 's'}`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? '' : 's'}`;
}

export function hostIsStale(host: HostStatus | null, now: number) {
  const checked = host ? Date.parse(host.checked_at) : NaN;
  return !host || !Number.isFinite(checked) || now - checked > HOST_STALE_MS;
}

/** Everything an administrator should look at, most urgent first. Never invents a healthy state. */
export function needsAttention(
  host: HostStatus | null,
  providers: Providers,
  now: number
): Attention[] {
  const items: Attention[] = [];
  if (!host) {
    items.push({
      id: 'host-missing',
      level: 'warning',
      title: 'Host monitoring is unavailable',
      detail:
        'No host report could be read, so service, disk and backup checks cannot be shown. Check the status collector on the server.',
      href: '#services'
    });
  } else {
    if (hostIsStale(host, now)) {
      const checked = Date.parse(host.checked_at);
      items.push({
        id: 'host-stale',
        level: 'warning',
        title: 'Host report is out of date',
        detail: Number.isFinite(checked)
          ? `The last report is ${span(now - checked)} old. Refresh after checking the collector.`
          : 'The host report has no valid timestamp. Check the status collector.',
        href: '#services'
      });
    }
    for (const service of host.services ?? []) {
      if (!service.running)
        items.push({
          id: `service-${service.name}`,
          level: 'danger',
          title: `${serviceLabel(service.name)} is not running`,
          detail: `The host reports this container as stopped${service.health ? ` (${service.health})` : ''}.`,
          href: '#services'
        });
      else if (service.health === 'unhealthy')
        items.push({
          id: `service-${service.name}`,
          level: 'danger',
          title: `${serviceLabel(service.name)} is unhealthy`,
          detail: 'The container is running but its health check is failing.',
          href: '#services'
        });
    }
    if (typeof host.disk_free_bytes === 'number' && host.disk_free_bytes < DISK_WARN_BYTES)
      items.push({
        id: 'disk-low',
        level: host.disk_free_bytes < DISK_CRITICAL_BYTES ? 'danger' : 'warning',
        title: 'Server disk space is low',
        detail: `${formatBytes(host.disk_free_bytes)} free. Backups and the database grow on this disk.`,
        href: '#storage'
      });
    if (!host.backup)
      items.push({
        id: 'backup-missing',
        level: 'warning',
        title: 'No local backup found',
        detail: 'The host has not reported any database backup file.',
        href: '#storage'
      });
    else {
      const created = Date.parse(host.backup.created_at ?? '');
      if (Number.isFinite(created) && now - created > BACKUP_STALE_MS)
        items.push({
          id: 'backup-stale',
          level: 'warning',
          title: 'Latest backup is more than 48 hours old',
          detail: `The newest local backup was written ${span(now - created)} ago.`,
          href: '#storage'
        });
      if (host.backup.checksum_ok === false)
        items.push({
          id: 'backup-checksum',
          level: 'danger',
          title: 'Backup checksum is not verified',
          detail: 'The newest backup does not match its recorded checksum, or has none.',
          href: '#storage'
        });
    }
  }
  if (!providers.google)
    items.push({
      id: 'google-missing',
      level: 'warning',
      title: 'Google credentials are missing',
      detail: 'Google client credentials are not configured on this server.',
      href: '#services'
    });
  if (!providers.email)
    items.push({
      id: 'email-missing',
      level: 'warning',
      title: 'Email credentials are missing',
      detail: 'Sign-in link emails cannot be sent until email credentials are configured.',
      href: '#services'
    });
  return items.sort((a, b) => Number(b.level === 'danger') - Number(a.level === 'danger'));
}

/** "3 hours ago" / "in 2 days" wording for the browser, where the current time is known. */
export function relativeTime(then: number, now: number) {
  const seconds = Math.round((then - now) / 1000);
  const abs = Math.abs(seconds);
  const format = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
  if (abs < 45) return 'just now';
  if (abs < 3600) return format.format(Math.round(seconds / 60), 'minute');
  if (abs < 86_400) return format.format(Math.round(seconds / 3600), 'hour');
  if (abs < 86_400 * 30) return format.format(Math.round(seconds / 86_400), 'day');
  if (abs < 86_400 * 365) return format.format(Math.round(seconds / (86_400 * 30)), 'month');
  return format.format(Math.round(seconds / (86_400 * 365)), 'year');
}
