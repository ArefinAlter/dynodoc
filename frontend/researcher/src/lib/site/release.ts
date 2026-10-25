// The current pre-alpha release and where to try it. One place for the facts the site shows
// (the Try page, the header and the home page). The sizes are those of the files uploaded to the
// GitHub release; their SHA-256 sums are in the release's SHA256SUMS.txt and its notes.

export const RELEASE = {
  name: 'Dynodoc 0.0.1a (pre-alpha)',
  version: '0.0.1-alpha',
  tag: 'v0.0.1a',
  date: '2026-10-25',
  page: 'https://github.com/ArefinAlter/dynodoc/releases/tag/v0.0.1a',
  sums: 'https://github.com/ArefinAlter/dynodoc/releases/download/v0.0.1a/SHA256SUMS.txt',
  sumsFile: 'SHA256SUMS.txt'
} as const;

/** A visitor's own sample workspace on the hosted pre-alpha server (no account needed). */
export const SAMPLE_WORKSPACE_URL = 'https://app.dynodoc.online/svc/visitor';

export type Asset = {
  platform: 'Windows x64' | 'Linux x64';
  kind: string;
  file: string;
  bytes: number;
  note: string;
};

const download = (file: string) =>
  `https://github.com/ArefinAlter/dynodoc/releases/download/${RELEASE.tag}/${file}`;

export const ASSETS: (Asset & { url: string })[] = (
  [
    {
      platform: 'Windows x64',
      kind: 'Installer',
      file: 'Dynodoc_0.0.1-alpha_x64-setup.exe',
      bytes: 7004111,
      note: 'Windows 10 or 11. Unsigned: SmartScreen warns; choose More info, then Run anyway.'
    },
    {
      platform: 'Windows x64',
      kind: 'Portable zip',
      file: 'Dynodoc_0.0.1a_windows-x64-portable.zip',
      bytes: 9439514,
      note: 'No installer: unzip and run Dynodoc.exe (keep engine-cli.exe beside it). Needs WebView2, which Windows 11 has.'
    },
    {
      platform: 'Linux x64',
      kind: 'AppImage',
      file: 'Dynodoc_0.0.1-alpha_amd64.AppImage',
      bytes: 88185336,
      note: 'Ubuntu 22.04 or newer, or a similar distribution. Make it executable (chmod +x), then run it.'
    },
    {
      platform: 'Linux x64',
      kind: 'Debian package',
      file: 'Dynodoc_0.0.1-alpha_amd64.deb',
      bytes: 10544586,
      note: 'Debian 12, Ubuntu 22.04 or newer: sudo apt install ./Dynodoc_0.0.1-alpha_amd64.deb'
    }
  ] satisfies Asset[]
).map((a) => ({ ...a, url: download(a.file) }));

/** "12.3 MB" */
export function size(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export type DownloadFile = { kind: string; file: string; url: string; bytes: number };
export type PlatformDownloads = {
  os: 'windows' | 'linux';
  name: string;
  /** One line: what the system needs. */
  requirement: string;
  primary: DownloadFile;
  more: DownloadFile[];
};

const file = (kind: string, name: string, bytes: number): DownloadFile => ({
  kind,
  file: name,
  url: download(name),
  bytes
});

/** The download cards on the Try page: one primary file per platform, the rest as secondary links. */
export const PLATFORMS: PlatformDownloads[] = [
  {
    os: 'windows',
    name: 'Windows',
    requirement:
      'Windows 10 or 11, x64. The installer adds WebView2 only if Windows does not have it (Windows 11 does).',
    primary: file('Installer', 'Dynodoc_0.0.1-alpha_x64-setup.exe', 7004111),
    more: [
      file(
        'Portable .zip (keep engine-cli.exe beside Dynodoc.exe)',
        'Dynodoc_0.0.1a_windows-x64-portable.zip',
        9439514
      ),
      file('engine-cli on its own, .exe', 'engine-cli_0.0.1a_windows-x64.exe', 19295744)
    ]
  },
  {
    os: 'linux',
    name: 'Linux',
    requirement:
      'x64, Ubuntu 22.04 or newer, Debian 12 or similar. AppImage on most distributions (needs FUSE 2), .deb for Debian and Ubuntu.',
    primary: file('Desktop app', 'Dynodoc_0.0.1-alpha_amd64.AppImage', 88185336),
    more: [
      file('Debian package, .deb', 'Dynodoc_0.0.1-alpha_amd64.deb', 10544586),
      file('engine-cli on its own', 'engine-cli_0.0.1a_linux-x64', 18830328)
    ]
  }
];
