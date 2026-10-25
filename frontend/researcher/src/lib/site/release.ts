// The current pre-alpha release and where to try it. One place for the facts the site shows
// (the Try page, the header and the home page). The sizes are those of the files uploaded to the
// GitHub release; their SHA-256 sums are in the release's SHA256SUMS.txt and its notes.

export const RELEASE = {
  name: 'Dynodoc 0.0.1a (pre-alpha)',
  version: '0.0.1-alpha',
  tag: 'v0.0.1a',
  date: '2026-10-25',
  page: 'https://github.com/ArefinAlter/dynodoc/releases/tag/v0.0.1a',
  sums: 'https://github.com/ArefinAlter/dynodoc/releases/download/v0.0.1a/SHA256SUMS.txt'
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
