// The current pre-alpha release and where to try it. One place for the facts the site shows
// (the Try page, the header and the home page). The sizes are those of the files uploaded to the
// GitHub release; their SHA-256 sums are in the release's SHA256SUMS.txt and its notes.

export const RELEASE = {
  name: 'Dynodoc 0.0.1c (pre-alpha)',
  version: '0.0.1-beta.1',
  tag: 'v0.0.1c',
  date: '2026-10-28',
  page: 'https://github.com/ArefinAlter/dynodoc/releases/tag/v0.0.1c',
  sums: 'https://github.com/ArefinAlter/dynodoc/releases/download/v0.0.1c/SHA256SUMS.txt',
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
      file: 'Dynodoc_0.0.1-beta.1_x64-setup.exe',
      bytes: 29819381,
      note: 'Windows 10 or 11. Unsigned: SmartScreen warns; choose More info, then Run anyway. It carries its own database, so it is larger than 0.0.1a.'
    },
    {
      platform: 'Windows x64',
      kind: 'Portable zip',
      file: 'Dynodoc_0.0.1c_windows-x64-portable.zip',
      bytes: 42975982,
      note: 'No installer: unzip and run Dynodoc.exe (keep the other files beside it; everything it keeps is in the dynodoc-data folder). Needs WebView2, which Windows 11 has.'
    },
    {
      platform: 'Linux x64',
      kind: 'AppImage',
      file: 'Dynodoc_0.0.1-beta.1_amd64.AppImage',
      bytes: 115853816,
      note: 'Ubuntu 22.04 or newer, or a similar distribution. Make it executable (chmod +x), then run it.'
    },
    {
      platform: 'Linux x64',
      kind: 'Debian package',
      file: 'Dynodoc_0.0.1-beta.1_amd64.deb',
      bytes: 41865518,
      note: 'Debian 12, Ubuntu 22.04 or newer: sudo apt install ./Dynodoc_0.0.1-beta.1_amd64.deb'
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
      'Windows 10 or 11, x64. The installer adds WebView2 only if Windows does not have it (Windows 11 does). It starts its own database, so allow about 110 MB of disk.',
    primary: file('Installer', 'Dynodoc_0.0.1-beta.1_x64-setup.exe', 29819381),
    more: [
      file(
        'Portable .zip (keep the files beside Dynodoc.exe)',
        'Dynodoc_0.0.1c_windows-x64-portable.zip',
        42975982
      ),
      file('engine-cli on its own, .exe', 'engine-cli_0.0.1c_windows-x64.exe', 19295744)
    ]
  },
  {
    os: 'linux',
    name: 'Linux',
    requirement:
      'x64, Ubuntu 22.04 or newer, Debian 12 or similar. AppImage on most distributions (needs FUSE 2), .deb for Debian and Ubuntu.',
    primary: file('Desktop app', 'Dynodoc_0.0.1-beta.1_amd64.AppImage', 115853816),
    more: [
      file('Debian package, .deb', 'Dynodoc_0.0.1-beta.1_amd64.deb', 41865518),
      file('engine-cli on its own', 'engine-cli_0.0.1c_linux-x64', 18826064)
    ]
  }
];
