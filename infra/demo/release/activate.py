#!/usr/bin/env python3
"""Activate a schema release (FROM_SCHEMA -> TO_SCHEMA, e.g. 41 -> 47) on the production VPS.

Order: preflight -> nginx maintenance (503) -> stop api/web/admin -> verified backup
-> source copy -> operator migration + grants + verification -> start new containers
-> live checks -> reopen nginx. Traffic stays closed if anything fails after the
migration starts; the database is never rolled back automatically."""
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

REV = os.environ['REV']
R = Path(os.environ['RELDIR'])
FROM_SCHEMA = int(os.environ['FROM_SCHEMA'])
TO_SCHEMA = int(os.environ['TO_SCHEMA'])
LIVE = Path('/opt/dynodoc-demo')
SRC = R / 'candidate'
API_IMAGE = 'dynodoc-demo-api:release-' + REV
WEB_IMAGE = 'dynodoc-demo-web:release-' + REV
NETWORK = 'dynodoc-demo_engine'
PG = 'dynodoc-demo-postgres-1'
WRITERS = ['dynodoc-demo-web-1', 'dynodoc-demo-admin-1', 'dynodoc-demo-api-1']
STATUS = R / 'activation-status'
os.umask(0o077)


def run(args, **kw):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw).stdout


def status(value):
    STATUS.write_text(value)
    print('STATUS', value, flush=True)


def sql(query):
    return run(['docker', 'exec', '-i', PG, 'psql', '-X', '-A', '-t', '-v', 'ON_ERROR_STOP=1', '-U', 'dynodoc',
                '-d', 'dynodoc'], input=query.encode()).decode().strip()


def fingerprint():
    return json.loads(sql(
        "select json_build_object('schema',(select max(version) from _sqlx_migrations),"
        "'documents',(select count(*) from document),'events',(select count(*) from event),"
        "'snapshots',(select count(*) from snapshot),'identities',(select count(*) from identity),"
        "'event_digest',(select md5(coalesce(string_agg(row_to_json(e)::text,'' order by document_id,seq),'')) from event e),"
        "'snapshot_digest',(select md5(coalesce(string_agg(row_to_json(s)::text,'' order by id),'')) from snapshot s))"))


def containers():
    ids = run(['docker', 'ps', '-q']).decode().split()
    return {c['Name'].lstrip('/'): {'id': c['Id'], 'started': c['State']['StartedAt'], 'image': c['Image']}
            for c in json.loads(run(['docker', 'inspect', *ids]))}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


fetcher = urllib.request.build_opener(NoRedirect)


def fetch(url):
    try:
        with fetcher.open(url, timeout=20) as r:
            return r.status, dict(r.headers), r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()


def wait_health(url):
    for _ in range(90):
        try:
            code, _, body = fetch(url)
            if code == 200 and json.loads(body).get('status') == 'ok':
                return
        except Exception:
            pass
        time.sleep(1)
    raise RuntimeError('Health gate failed: ' + url)


SITES = {name: Path('/etc/nginx/sites-available/dynodoc-' + name) for name in ['app', 'admin']}
MAINT = 'listen 443 ssl;\n    return 503;'


def close_proxy(saved):
    for name, path in SITES.items():
        assert 'listen 443 ssl;' in saved[name]
        path.write_text(saved[name].replace('listen 443 ssl;', MAINT, 1))
    run(['nginx', '-t'])
    run(['systemctl', 'reload', 'nginx'])


def open_proxy(saved):
    for name, path in SITES.items():
        path.write_text(saved[name])
    run(['nginx', '-t'])
    run(['systemctl', 'reload', 'nginx'])


def activate():
    # ---- preflight (no production change) -------------------------------------------------
    assert (SRC / 'SOURCE_REVISION').read_text().strip().startswith(REV)
    assert (R / 'build-status').read_text().strip() == 'PASSED'
    rehearsal = json.loads((R / 'rehearsal-report.json').read_text())
    assert rehearsal['ok'] is True
    for row in (SRC / 'SOURCE_MANIFEST.sha256').read_text().splitlines():
        digest, name = row.split('  ', 1)
        assert hashlib.sha256((SRC / name).read_bytes()).hexdigest() == digest, name
    before_containers = containers()
    (R / 'activation-containers-before.json').write_text(json.dumps(before_containers, indent=2))
    pre = fingerprint()
    assert pre['schema'] == FROM_SCHEMA, pre
    (R / 'production-before.json').write_text(json.dumps(pre))
    env_path = LIVE / 'infra/demo/.env'
    env_before = env_path.read_bytes()
    (R / 'production-env-before').write_bytes(env_before)
    env = dict(line.split('=', 1) for line in env_before.decode().splitlines() if '=' in line and not line.startswith('#'))
    runtime_url = env['RUNTIME_DATABASE_URL']
    role = urllib.parse.urlparse(runtime_url).username
    assert role and role.startswith('dynodoc_runtime'), role
    pg = json.loads(run(['docker', 'inspect', PG]))[0]
    pg_env = dict(p.split('=', 1) for p in pg['Config']['Env'] if '=' in p)
    for kind, current in [('api', 'dynodoc-demo-api-1'), ('web', 'dynodoc-demo-web-1')]:
        image = before_containers[current]['image']
        run(['docker', 'tag', image, 'dynodoc-demo-%s:before-schema%d-%s' % (kind, TO_SCHEMA, REV)])
    saved = {name: path.read_text() for name, path in SITES.items()}
    (R / 'nginx-before.json').write_text(json.dumps(saved))
    operator_env = R / 'production-operator.env'
    operator_env.write_text('MIGRATION_DATABASE_URL=postgres://dynodoc:%s@postgres:5432/dynodoc\nDATABASE_URL=%s\n'
                            % (urllib.parse.quote(pg_env['POSTGRES_PASSWORD'], safe=''), runtime_url))
    operator_env.chmod(0o600)
    (R / 'release.compose.yml').write_text(
        'services:\n  api:\n    image: %s\n  database-admin:\n    image: %s\n  web:\n    image: %s\n  admin:\n    image: %s\n'
        % (API_IMAGE, API_IMAGE, WEB_IMAGE, WEB_IMAGE))
    compose = ['docker', 'compose', '--project-name', 'dynodoc-demo', '--env-file', str(env_path),
               '-f', str(LIVE / 'infra/demo/compose.yml'), '-f', str(R / 'release.compose.yml')]

    def operator(*args):
        out = run(['docker', 'run', '--rm', '--network', NETWORK, '--env-file', str(operator_env), '--entrypoint',
                   'engine-cli', API_IMAGE, *args])
        with (R / 'production-verification.log').open('ab') as log:
            log.write(b'$ engine-cli ' + ' '.join(args).encode() + b'\n' + out)

    # ---- maintenance + stop writers ---------------------------------------------------------
    phase = 'PREFLIGHT'
    try:
        close_proxy(saved)
        status('MAINTENANCE')
        phase = 'MAINTENANCE'
        run(['docker', 'stop', *WRITERS])
        status('WRITERS_STOPPED')
        stable = fingerprint()
        assert stable == pre, ('database changed during preflight', pre, stable)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        backup = LIVE / 'backups' / ('dynodoc-before-schema%d-' % TO_SCHEMA + stamp + '.dump')
        with backup.open('wb') as out:
            subprocess.run(['docker', 'exec', PG, 'pg_dump', '-U', 'dynodoc', '-d', 'dynodoc', '-Fc'], stdout=out,
                           check=True)
        with backup.open('rb') as f:
            run(['docker', 'exec', '-i', PG, 'pg_restore', '--list'], stdin=f)
        (R / 'production-backup.json').write_text(json.dumps(
            {'path': str(backup), 'bytes': backup.stat().st_size,
             'sha256': hashlib.sha256(backup.read_bytes()).hexdigest()}))
        status('BACKED_UP')
        phase = 'BACKED_UP'
    except Exception:
        # Nothing irreversible happened yet: put the previous release back exactly as it was.
        subprocess.run(['docker', 'start', *WRITERS])
        open_proxy(saved)
        status('FAILED_RESTORED:' + phase)
        raise
    # ---- point of no return: migration starts -------------------------------------------------
    phase = 'MIGRATING'
    status('MIGRATING')
    for row in (SRC / 'SOURCE_MANIFEST.sha256').read_text().splitlines():
        _, name = row.split('  ', 1)
        target = (LIVE / name).resolve()
        assert target.is_relative_to(LIVE.resolve()), name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(SRC / name, target)
    for name in ['SOURCE_REVISION', 'SOURCE_MANIFEST.sha256']:
        shutil.copy2(SRC / name, LIVE / name)
    assert env_path.read_bytes() == env_before, 'live env changed by source copy'
    operator('migrate')
    operator('grant-runtime', role)
    operator('verify-snapshots', '--all')
    for document in sql('select id from document order by id').splitlines():
        operator('verify', document)
    operator('check-runtime')
    after = fingerprint()
    assert after == {**pre, 'schema': TO_SCHEMA}, ('history fingerprint changed', pre, after)
    (R / 'production-after.json').write_text(json.dumps(after))
    status('DATABASE_VERIFIED')
    phase = 'DATABASE_VERIFIED'
    run(compose + ['up', '-d', '--no-deps', '--no-build', '--pull', 'never', 'api', 'web', 'admin'])
    api = json.loads(run(['docker', 'inspect', 'dynodoc-demo-api-1']))[0]
    address = api['NetworkSettings']['Networks'][NETWORK]['IPAddress']
    wait_health('http://%s:8080/healthz' % address)
    wait_health('http://127.0.0.1:3088/healthz')
    wait_health('http://127.0.0.1:3089/healthz')
    api_env = dict(p.split('=', 1) for p in api['Config']['Env'] if '=' in p)
    assert 'MIGRATION_DATABASE_URL' not in api_env and role + ':' in api_env['DATABASE_URL']
    assert api_env.get('DYNODOC_SHARED_CHECKPOINT_WRITES', '').lower() not in ['true', '1']
    assert 'DYNODOC_REF_SIGNING_KEY_FILE' not in api_env  # receipt authority is intentionally not installed
    status('SERVICES_UP')
    open_proxy(saved)
    checks = []
    for host in ['app', 'admin']:
        wait_health('https://%s.dynodoc.online/healthz' % host)
    for host in ['app', 'admin']:
        for path, expected in [('/healthz', 200), ('/login', 200), ('/theme-init.js', 200), ('/missing-rollout-check', 404)]:
            code, headers, _ = fetch('https://%s.dynodoc.online%s' % (host, path))
            lower = {k.lower(): v for k, v in headers.items()}
            assert code == expected, (host, path, code)
            assert lower.get('strict-transport-security') == 'max-age=31536000', (host, path)
            assert "object-src 'none'" in lower.get('content-security-policy', ''), (host, path)
            checks.append({'host': host, 'path': path, 'status': code})
    code, headers, _ = fetch('https://app.dynodoc.online/projects')
    assert code == 303 and '/login' in {k.lower(): v for k, v in headers.items()}.get('location', '')
    code, headers, _ = fetch('https://admin.dynodoc.online/admin')
    assert code in (303, 200), code
    for path in ['/', '/features', '/privacy', '/security', '/add-ins', '/support', '/connectors',
                 '/.well-known/security.txt', '/connectors/assets/icon-64.png']:
        code, headers, body = fetch('https://app.dynodoc.online' + path)
        assert code < 500, (path, code)  # soft: redirects are fine, server errors are not
        checks.append({'host': 'app', 'path': path, 'status': code})
    for pane in ['word', 'excel', 'powerpoint', 'commands.html']:
        code, headers, _ = fetch('https://app.dynodoc.online/connectors/' + pane)
        lower = {k.lower(): v for k, v in headers.items()}
        assert code == 200 and 'x-frame-options' not in lower, pane
        assert 'https://appsforoffice.microsoft.com' in lower['content-security-policy'], pane
        checks.append({'pane': pane, 'status': code})
    code, _, body = fetch('https://app.dynodoc.online/connectors/packages/word.xml')
    assert code == 200 and b'Dynodoc for Word' in body
    # New in the 41 -> 47 release: the installable web app files, the native-app sign-in page (a signed-out
    # visitor is sent on, never a server error) and the self-hosted-only add-in routes that must stay absent.
    for path in ['/service-worker.js', '/offline.html', '/site.webmanifest']:
        code, headers, _ = fetch('https://app.dynodoc.online' + path)
        assert code == 200, (path, code)
        checks.append({'host': 'app', 'path': path, 'status': code})
    code, headers, _ = fetch('https://app.dynodoc.online/app-signin')
    assert code in (200, 303), ('/app-signin', code)
    checks.append({'host': 'app', 'path': '/app-signin', 'status': code})
    code, _, _ = fetch('https://app.dynodoc.online/connectors/addins')
    assert code == 404, ('/connectors/addins', code)
    checks.append({'host': 'app', 'path': '/connectors/addins', 'status': code})
    code, headers, _ = fetch('https://app.dynodoc.online/source')
    loc = {k.lower(): v for k, v in headers.items()}.get('location', '')
    assert code == 302 and 'dynodoc-engine' in loc, ('source must redirect to the engine repo', code, loc)
    checks.append({'path': '/source', 'status': code, 'location': loc})
    code, _, body = fetch('https://app.dynodoc.online/.well-known/security.txt')
    assert code == 200 and b'info@vernesystems.com' in body
    checks.append({'path': '/.well-known/security.txt', 'status': code})
    after_containers = containers()
    (R / 'activation-containers-after.json').write_text(json.dumps(after_containers, indent=2))
    for name in [PG, 'dynodoc-demo-converter-1']:
        assert after_containers.get(name) == before_containers[name], ('protected container changed', name)
    unrelated = [n for n, old in before_containers.items() if n not in WRITERS and after_containers.get(n) != old]
    (R / 'live-checks.json').write_text(json.dumps(
        {'checks': checks, 'unrelated_containers_unchanged': not unrelated, 'unrelated_changes': unrelated,
         'runtime_role': role, 'revision': REV}, indent=2))
    status('ACTIVATED')
    print('Release activated; database content verified; live checks passed.', flush=True)


if __name__ == '__main__':
    try:
        activate()
    except Exception as exc:
        phase = STATUS.read_text() if STATUS.exists() else 'PREFLIGHT'
        if phase in ['MIGRATING', 'DATABASE_VERIFIED', 'SERVICES_UP']:
            # Keep traffic closed; never reopen an old API on a migrated schema.
            for path in SITES.values():
                content = path.read_text()
                if 'return 503;' not in content:
                    path.write_text(content.replace('listen 443 ssl;', MAINT, 1))
            subprocess.run(['nginx', '-t'])
            subprocess.run(['systemctl', 'reload', 'nginx'])
            STATUS.write_text('FAILED_NEEDS_REVIEW:' + phase)
        elif not phase.startswith('FAILED_RESTORED'):
            STATUS.write_text('FAILED_PREFLIGHT:' + phase)
        print('FAILED', repr(exc)[:1500], flush=True)
        raise
