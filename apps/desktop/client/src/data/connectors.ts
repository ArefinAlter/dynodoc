// The connector catalogue: what Dynodoc reaches and how. The editor add-ins are where editing
// happens (this client has no editor; it links out to them and shows their connection state).
// The imports are the web workspace's "Import from research tool" and "Import from Google Forms".
// Statuses are stated as they are in the repository: "Built" means the code exists in Dynodoc;
// no add-in has been tried in an installed Office or Google editor, and none is in a store.
import type { AddinHost } from './types.ts';

export type ConnectorStatus = 'Built' | 'Manifest only';

export interface Connector {
  id: string;
  name: string;
  category: 'Editor add-in' | 'Import' | 'History' | 'Agent access';
  summary: string;
  auth: string;
  hosts: string[];
  status: ConnectorStatus;
  /** Shown under the status: what has and has not been tried. */
  caveat: string;
  /** For add-ins: the host id used by connection keys. */
  host?: AddinHost;
  where: string;
}

const ADDIN_CAVEAT = 'Not yet tried in an installed editor. No store listing exists.';

export const CONNECTORS: Connector[] = [
  { id: 'word', host: 'word', name: 'Word add-in', category: 'Editor add-in', summary: 'Captures paragraph edits in Word and pushes them for review; previews team changes in the document.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/word (Office manifest)' },
  { id: 'excel', host: 'excel', name: 'Excel add-in', category: 'Editor add-in', summary: 'Captures cell values and formulas; pushes a change bundle; previews the team version.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/excel (Office manifest)' },
  { id: 'powerpoint', host: 'powerpoint', name: 'PowerPoint add-in', category: 'Editor add-in', summary: 'Captures slide text and notes; pushes changes for review.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/powerpoint (Office manifest)' },
  { id: 'google-docs', host: 'google-docs', name: 'Google Docs add-on', category: 'Editor add-in', summary: 'A sidebar in Google Docs that captures paragraph edits and pushes them for review.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/google-docs (Apps Script)' },
  { id: 'google-sheets', host: 'google-sheets', name: 'Google Sheets add-on', category: 'Editor add-in', summary: 'A sidebar in Google Sheets that captures cell edits.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/google-sheets (Apps Script)' },
  { id: 'google-slides', host: 'google-slides', name: 'Google Slides add-on', category: 'Editor add-in', summary: 'A sidebar in Google Slides that captures slide text.', auth: 'Pairing code, then a connection key for one file (7 days)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: ADDIN_CAVEAT, where: 'extensions/google-slides (Apps Script)' },
  { id: 'kobo', name: 'KoboToolbox', category: 'Import', summary: 'Imports a form or its current responses as a spreadsheet.', auth: 'API token (used once, not saved)', hosts: ['kf.kobotoolbox.org', 'your own server'], status: 'Built', caveat: 'Runs on the server; the token is used for one import and not saved.', where: 'web workspace, Import from research tool' },
  { id: 'odk', name: 'ODK Central', category: 'Import', summary: 'Imports a form or its submissions as a spreadsheet.', auth: 'Web User session token', hosts: ['your-project.getodk.cloud', 'your own server'], status: 'Built', caveat: 'App User tokens do not give this access.', where: 'web workspace, Import from research tool' },
  { id: 'redcap', name: 'REDCap', category: 'Import', summary: 'Imports a project’s instruments or records.', auth: 'API token', hosts: ["your institution's REDCap"], status: 'Built', caveat: 'The token is used for one import and not saved.', where: 'web workspace, Import from research tool' },
  { id: 'surveycto', name: 'SurveyCTO', category: 'Import', summary: 'Imports a form definition or its data.', auth: 'API user and password', hosts: ['your-server.surveycto.com'], status: 'Built', caveat: 'Used for one import and not saved.', where: 'web workspace, Import from research tool' },
  { id: 'surveymonkey', name: 'SurveyMonkey', category: 'Import', summary: 'Imports a survey or its responses.', auth: 'API access token', hosts: ['api.surveymonkey.com'], status: 'Built', caveat: 'Used for one import and not saved.', where: 'web workspace, Import from research tool' },
  { id: 'gforms', name: 'Google Forms', category: 'Import', summary: 'Imports a form as a questionnaire.', auth: 'Google sign-in (when the server is configured for it)', hosts: ['Google'], status: 'Built', caveat: 'Available only on servers with Google sign-in configured.', where: 'web workspace, Import from Google Forms' },
  { id: 'cli', name: 'engine-cli sync', category: 'History', summary: 'Publishes signed commits of a project to its shared history; verifies receipts.', auth: 'Device signing key (this computer’s key)', hosts: ['Your Dynodoc server'], status: 'Built', caveat: 'Command line; the desktop app bundles engine-cli for its key only.', where: 'crates/engine-cli' },
  { id: 'agent-api', name: 'Agent API', category: 'Agent access', summary: 'Lets an AI agent read files, outlines and diffs and propose changes under the project’s agent policy.', auth: 'Agent key enrolled under Profile, Signing keys', hosts: ['Your Dynodoc server'], status: 'Built', caveat: 'Proposals wait for a person; an agent cannot merge.', where: '/projects/{id}/agent/*' },
];

export const HOST_LABEL: Record<AddinHost, string> = {
  word: 'Microsoft Word',
  excel: 'Microsoft Excel',
  powerpoint: 'Microsoft PowerPoint',
  'google-docs': 'Google Docs',
  'google-sheets': 'Google Sheets',
  'google-slides': 'Google Slides',
};
