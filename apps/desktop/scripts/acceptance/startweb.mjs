// Starts the built web tier as a self-hosted server on loopback against the throwaway engine.
//   node startweb.mjs <port>      (stays running; kill it to stop)
import { spawn } from 'node:child_process';
import { readFileSync, openSync } from 'node:fs';

const port = process.argv[2] || '4180';
const key = readFileSync('D:/verne-build/dynodoc/scratch/devstack/service.key', 'utf8').trim();
const log = openSync('D:/verne-build/dynodoc/scratch/web.log', 'w');
const child = spawn(process.execPath, ['build'], {
  cwd: 'D:/verne-build/dynodoc/wt/frontend/researcher',
  env: {
    ...process.env,
    HOST: '127.0.0.1',
    PORT: port,
    ORIGIN: `http://127.0.0.1:${port}`,
    ENGINE_API_URL: 'http://127.0.0.1:18090',
    SESSION_SECRET: 'acceptance-only-session-secret-0123456789abcdef',
    WEB_SERVICE_KEY: key,
    DYNODOC_MODE: 'selfhosted',
    DEV_AUTH: 'true',
    DYNODOC_BRAND_NAME: 'Self-hosted test server',
    DYNODOC_ORG_NAME: 'Acceptance run',
  },
  stdio: ['ignore', log, log],
  detached: true,
});
child.unref();
console.log('web tier pid', child.pid, 'on', port);
