#!/usr/bin/env python3
"""Rehearse a schema upgrade (e.g. 41 -> 47) on an isolated copy of production.

Reads production only through pg_dump. Everything else runs in throwaway containers
on an internal Docker network that is removed at the end. Writes a JSON report to
REPORT. Exit code 0 only when every check passed."""
import hashlib
import json
import os
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

R = Path(os.environ['RELDIR'])
TO_SCHEMA = int(os.environ['TO_SCHEMA'])
TAG = os.environ.get('TAG', 'rehearsal')
IMAGE_API = os.environ['API_IMAGE']
IMAGE_WEB = os.environ['WEB_IMAGE']
PROD_PG = 'dynodoc-demo-postgres-1'
NET = 'dynodoc-rehearsal-' + TAG
PG = 'dynodoc-rehearsal-pg-' + TAG
APIC = 'dynodoc-rehearsal-api-' + TAG
WEBC = 'dynodoc-rehearsal-web-' + TAG
ADMINC = 'dynodoc-rehearsal-admin-' + TAG
REPORT = R / 'rehearsal-report.json'
os.umask(0o077)
report = {'steps': [], 'ok': False}


def run(args, **kw):
    try:
        return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw).stdout
    except subprocess.CalledProcessError as e:
        raise RuntimeError('%s -> %s | %s' % (' '.join(args[-3:]), e.returncode,
                                              (e.stderr or b'').decode()[-1200:])) from None


def step(name, **data):
    report['steps'].append({'step': name, **data})
    print(name, json.dumps(data)[:300], flush=True)


def sql(query, container=PG):
    return run(['docker', 'exec', '-i', container, 'psql', '-X', '-A', '-t', '-v', 'ON_ERROR_STOP=1',
                '-U', 'dynodoc', '-d', 'dynodoc'], input=query.encode()).decode().strip()


FINGERPRINT = (
    "select json_build_object('schema',(select max(version) from _sqlx_migrations),"
    "'documents',(select count(*) from document),'events',(select count(*) from event),"
    "'snapshots',(select count(*) from snapshot),'identities',(select count(*) from identity),"
    "'event_digest',(select md5(coalesce(string_agg(row_to_json(e)::text,'' order by document_id,seq),'')) from event e),"
    "'snapshot_digest',(select md5(coalesce(string_agg(row_to_json(s)::text,'' order by id),'')) from snapshot s))"
)


def fingerprint(container):
    return json.loads(sql(FINGERPRINT, container=container))


def http(url, data=None, method='GET', timeout=10):
    req = urllib.request.Request(url, data=data, method=method,
                                 headers={'content-type': 'application/json'} if data else {})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read()[:2000]
    except urllib.error.HTTPError as e:
        return e.code, e.read()[:2000]


def container_logs(name, tail=2000):
    """stdout and stderr of a container (never the environment)."""
    proc = subprocess.run(['docker', 'logs', '--tail', str(tail), name], stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)
    return proc.stdout.decode(errors='replace')


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def http_no_redirect(url, timeout=10):
    try:
        with urllib.request.build_opener(_NoRedirect).open(url, timeout=timeout) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code


def wait_health(url, tries=60):
    for _ in range(tries):
        try:
            if http(url)[0] == 200:
                return True
        except Exception:
            pass
        time.sleep(1)
    return False


LIVE_ENV_PATH = Path('/opt/dynodoc-demo/infra/demo/.env')


def live_env():
    """The production .env as a dict (values are secrets: never print or log them)."""
    out = {}
    for line in LIVE_ENV_PATH.read_text().splitlines():
        if '=' in line and not line.startswith('#'):
            key, value = line.split('=', 1)
            out[key] = value
    return out


def write_env_file(name, values):
    """A 0600 env file in the release directory; removed again by cleanup()."""
    path = R / name
    path.write_text(''.join('%s=%s\n' % (k, v) for k, v in values.items()))
    path.chmod(0o600)
    return path


def container_ip(name):
    return json.loads(run(['docker', 'inspect', name]))[0]['NetworkSettings']['Networks'][NET]['IPAddress']


def cleanup():
    for c in [ADMINC, WEBC, APIC, PG]:
        subprocess.run(['docker', 'rm', '-f', c], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run(['docker', 'network', 'rm', NET], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for name in ['rehearsal-api.env', 'rehearsal-web.env', 'rehearsal-admin.env']:
        (R / name).unlink(missing_ok=True)


def main():
    cleanup()
    started = time.time()
    # 1. read-only production dump
    dump = R / 'rehearsal-before.dump'
    with dump.open('wb') as out:
        subprocess.run(['docker', 'exec', PROD_PG, 'pg_dump', '-U', 'dynodoc', '-d', 'dynodoc', '-Fc'],
                       stdout=out, check=True)
    with dump.open('rb') as f:
        run(['docker', 'exec', '-i', PROD_PG, 'pg_restore', '--list'], stdin=f)
    step('dump', bytes=dump.stat().st_size, sha256=hashlib.sha256(dump.read_bytes()).hexdigest())
    prod_fp = fingerprint(PROD_PG)
    step('production-fingerprint', **prod_fp)
    # 2. isolated copy
    password = secrets.token_hex(24)
    run(['docker', 'network', 'create', '--internal', NET])
    run(['docker', 'run', '-d', '--name', PG, '--network', NET, '-e', 'POSTGRES_USER=dynodoc',
         '-e', 'POSTGRES_DB=dynodoc', '-e', 'POSTGRES_PASSWORD=' + password, 'postgres:16'])
    for _ in range(60):
        if subprocess.run(['docker', 'exec', PG, 'pg_isready', '-U', 'dynodoc', '-d', 'dynodoc'],
                          stdout=subprocess.DEVNULL).returncode == 0:
            break
        time.sleep(1)
    time.sleep(3)
    # The production dump carries GRANTs to the live restricted login; create that role name first.
    live_env = Path('/opt/dynodoc-demo/infra/demo/.env').read_text().splitlines()
    runtime_url = [l.split('=', 1)[1] for l in live_env if l.startswith('RUNTIME_DATABASE_URL=')][0]
    role = urllib.parse.urlparse(runtime_url).username
    runtime_pw = secrets.token_hex(24)
    sql("CREATE ROLE %s LOGIN PASSWORD '%s' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS"
        % (role, runtime_pw))
    with dump.open('rb') as f:
        run(['docker', 'exec', '-i', PG, 'pg_restore', '--exit-on-error', '-U', 'dynodoc', '-d', 'dynodoc',
             '--no-owner'], stdin=f)
    # Database-level ACLs are not part of a pg_dump; mirror production's (PUBLIC keeps CONNECT only).
    sql("REVOKE ALL ON DATABASE dynodoc FROM PUBLIC; GRANT CONNECT ON DATABASE dynodoc TO PUBLIC;")
    sql("ALTER DEFAULT PRIVILEGES FOR ROLE dynodoc REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;")
    before = fingerprint(PG)
    assert before == prod_fp, ('restored copy differs from production', before, prod_fp)
    step('restore', **before)
    # 3. operator migration with the candidate image
    envfile = R / 'rehearsal-operator.env'
    envfile.write_text('MIGRATION_DATABASE_URL=postgres://dynodoc:%s@%s:5432/dynodoc\n'
                       'DATABASE_URL=postgres://%s:%s@%s:5432/dynodoc\n' % (password, PG, role, runtime_pw, PG))
    envfile.chmod(0o600)

    def op(*args):
        return run(['docker', 'run', '--rm', '--network', NET, '--env-file', str(envfile), '--entrypoint',
                    'engine-cli', IMAGE_API, *args]).decode()

    t0 = time.time()
    op('migrate')
    step('migrate', seconds=round(time.time() - t0, 2))
    op('grant-runtime', role)
    step('grant-runtime', role=role)
    op('verify-snapshots', '--all')
    docs = sql('select id from document order by id').splitlines()
    for d in docs:
        op('verify', d)
    step('verify', documents=len(docs))
    op('check-runtime')
    step('check-runtime')
    after = fingerprint(PG)
    assert after == {**before, 'schema': TO_SCHEMA}, ('fingerprint changed', before, after)
    step('fingerprint-after', **after)
    # 4. candidate API with the restricted login and the PRODUCTION secrets (the new start-up checks
    #    must accept the real values; the database is the throwaway copy, never production).
    live = live_env()
    api_env = write_env_file('rehearsal-api.env', {
        'PASETO_LOCAL_KEY': live['PASETO_LOCAL_KEY'], 'WEB_SERVICE_KEY': live['WEB_SERVICE_KEY'],
        'TOKEN_TTL_SECONDS': live.get('TOKEN_TTL_SECONDS') or '3600',
        'DATABASE_URL': 'postgres://%s:%s@%s:5432/dynodoc' % (role, runtime_pw, PG),
        'API_HOST': '0.0.0.0', 'API_PORT': '8080', 'APP_ENV': 'production',
        'ADMIN_EMAILS': live.get('ADMIN_EMAILS', ''), 'PUBLIC_API_BASE_URL': 'https://rehearsal.invalid',
        'RUST_LOG': 'info', 'RETENTION_JOBS': 'off'})
    run(['docker', 'run', '-d', '--name', APIC, '--network', NET, '--read-only', '--cap-drop', 'ALL',
         '--tmpfs', '/tmp:size=64m', '--env-file', str(api_env), IMAGE_API])
    ip = container_ip(APIC)
    assert wait_health('http://%s:8080/healthz' % ip), 'candidate API did not become healthy: ' + \
        container_logs(APIC, 40)[-1500:]
    checks = {}
    for path in ['/healthz', '/projects', '/profile', '/admin/overview']:
        checks[path] = http('http://%s:8080%s' % (ip, path))[0]
    checks['device-challenge'] = http(
        'http://%s:8080/projects/00000000-0000-0000-0000-000000000000/device-sessions/challenge' % ip,
        data=b'{}', method='POST')[0]
    assert checks['/healthz'] == 200 and checks['/projects'] == 401 and checks['/admin/overview'] == 401, checks
    # New in this release: the agent data plane is mounted only when AGENT_GATEWAY=on (it is not set), and the
    # native-app hand-over routes refuse a caller that has no web service key.
    checks['agent-files-absent'] = http(
        'http://%s:8080/projects/00000000-0000-0000-0000-000000000000/agent/files' % ip)[0]
    checks['app-handover-no-key'] = http('http://%s:8080/auth/app-handover' % ip, data=b'{}', method='POST')[0]
    checks['app-handover-redeem-no-key'] = http(
        'http://%s:8080/auth/app-handover/redeem' % ip, data=b'{}', method='POST')[0]
    assert checks['agent-files-absent'] == 404, checks
    assert checks['app-handover-no-key'] in (401, 403) and checks['app-handover-redeem-no-key'] in (401, 403), checks
    assert 'retention jobs are off' in container_logs(APIC), 'RETENTION_JOBS=off was not honoured'
    step('api', checks=checks)
    # 5. candidate web (workspace host variant) against that API, with the production web secrets
    web_values = {
        'ORIGIN': 'https://rehearsal.invalid', 'HOST': '0.0.0.0', 'PORT': '3000',
        'SESSION_SECRET': live['SESSION_SECRET'], 'WEB_SERVICE_KEY': live['WEB_SERVICE_KEY'],
        'CONVERTER_KEY': live.get('CONVERTER_KEY', ''), 'RESEND_API_KEY': live.get('RESEND_API_KEY', ''),
        'EMAIL_FROM': live.get('EMAIL_FROM', ''), 'GOOGLE_CLIENT_ID': live.get('GOOGLE_CLIENT_ID', ''),
        'GOOGLE_CLIENT_SECRET': live.get('GOOGLE_CLIENT_SECRET', ''),
        'CONNECTOR_ORIGINS': live.get('CONNECTOR_ORIGINS', ''),
        'ENGINE_API_URL': 'http://%s:8080' % ip, 'CONVERTER_URL': 'http://converter.invalid:8090',
        'DEV_AUTH': 'false'}
    web_env = write_env_file('rehearsal-web.env', web_values)
    run(['docker', 'run', '-d', '--name', WEBC, '--network', NET, '--read-only', '--cap-drop', 'ALL',
         '--tmpfs', '/tmp:size=64m', '--env-file', str(web_env), IMAGE_WEB])
    wip = container_ip(WEBC)
    assert wait_health('http://%s:3000/healthz' % wip), 'candidate web did not become healthy: ' + \
        container_logs(WEBC, 40)[-1500:]
    pages = {}
    for path in ['/', '/login', '/features', '/privacy', '/security', '/add-ins', '/support',
                 '/.well-known/security.txt', '/service-worker.js', '/offline.html', '/site.webmanifest']:
        pages[path] = http('http://%s:3000%s' % (wip, path))[0]
    assert all(v == 200 for v in pages.values()), pages
    # The native-app sign-in page sends a signed-out visitor on; the self-hosted-only add-in generator
    # routes do not exist in hosted mode.
    extra = {path: http_no_redirect('http://%s:3000%s' % (wip, path)) for path in ['/app-signin', '/connectors/addins']}
    assert extra['/app-signin'] in (200, 303) and extra['/connectors/addins'] == 404, extra
    step('web', pages=pages, extra=extra)
    # 6. the admin site variant of the same image must start with the same secrets
    admin_values = dict(web_values, ORIGIN='https://admin.dynodoc.online', ADMIN_SITE='true',
                        PUBLIC_WORKSPACE_URL='https://app.dynodoc.online', OPS_STATUS_PATH='/tmp/status.json')
    admin_env = write_env_file('rehearsal-admin.env', admin_values)
    run(['docker', 'run', '-d', '--name', ADMINC, '--network', NET, '--read-only', '--cap-drop', 'ALL',
         '--tmpfs', '/tmp:size=64m', '--env-file', str(admin_env), IMAGE_WEB])
    aip = container_ip(ADMINC)
    healthy = wait_health('http://%s:3000/healthz' % aip)
    assert healthy, 'candidate admin site did not become healthy: ' + container_logs(ADMINC, 40)[-1500:]
    step('admin-web', healthy=True)
    report['ok'] = True
    report['seconds'] = round(time.time() - started, 1)


if __name__ == '__main__':
    failed = False
    try:
        main()
    except Exception as exc:  # report, clean up, exit non-zero
        report['error'] = repr(exc)[:2000]
        failed = True
        print('FAILED', report['error'], flush=True)
    finally:
        REPORT.write_text(json.dumps(report, indent=2))
        cleanup()
    sys.exit(1 if failed else 0)
