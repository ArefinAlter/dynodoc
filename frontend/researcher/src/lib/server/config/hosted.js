// The hosted service's fixed facts: hostnames, the beacon destination, the shared theme
// cookie domain. These are the values the web server used before DYNODOC_MODE existed, kept
// here so hosted behaviour is unchanged and so every other module reads them from the
// configuration instead of naming a hostname. Self-hosted mode never reads this object.
// Server-side only: nothing in the browser bundle imports this file.

const APP = 'https://app.dynodoc.online';

export const HOSTED = Object.freeze({
  /** Public marketing host; where a brand link and the app-host root go. */
  homeUrl: 'https://dynodoc.online',
  helpUrl: 'https://dynodoc.online/help',
  /** Workspace host; admin pages link to its project list. */
  workspaceUrl: APP,
  /** Root of every hosted hostname, for the admin environment label. */
  domain: 'dynodoc.online',
  themeCookieDomain: '.dynodoc.online',
  /** Extra `connect-src` origin: the marketing host posts page metrics here. */
  connectSrc: Object.freeze([APP]),
  /** `app.` has no page of its own at `/`; send it to the marketing host. */
  rootRedirect: Object.freeze({ fromHost: 'app.dynodoc.online', to: 'https://dynodoc.online' }),
  telemetry: Object.freeze({
    endpoint: `${APP}/telemetry`,
    hosts: Object.freeze([
      'dynodoc.online',
      'www.dynodoc.online',
      'app.dynodoc.online',
      'admin.dynodoc.online'
    ]),
    /** Browser origin -> the host label stored with each measurement. */
    origins: Object.freeze([
      ['https://dynodoc.online', 'homepage'],
      ['https://www.dynodoc.online', 'homepage'],
      [APP, 'workspace'],
      ['https://admin.dynodoc.online', 'admin']
    ])
  })
});
