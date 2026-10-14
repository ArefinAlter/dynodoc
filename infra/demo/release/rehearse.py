#!/usr/bin/env python3
"""Rehearse the schema 29 -> 38 upgrade on an isolated copy of production.

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


def wait_health(url, tries=60):
    for _ in range(tries):
        try:
            if http(url)[0] == 200:
                return True
        except Exception:
            pass
        time.sleep(1)
    return False


def container_ip(name):
    return json.loads(run(['docker', 'inspect', name]))[0]['NetworkSettings']['Networks'][NET]['IPAddress']


def cleanup():
    for c in [WEBC, APIC, PG]:
        subprocess.run(['docker', 'rm', '-f', c], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run(['docker', 'network', 'rm', NET], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


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
    # 4. candidate API with the restricted login
    run(['docker', 'run', '-d', '--name', APIC, '--network', NET, '--read-only', '--cap-drop', 'ALL',
         '--tmpfs', '/tmp:size=64m',
         '-e', 'PASETO_LOCAL_KEY=' + secrets.token_hex(32), '-e', 'WEB_SERVICE_KEY=' + secrets.token_hex(24),
         '-e', 'DATABASE_URL=postgres://%s:%s@%s:5432/dynodoc' % (role, runtime_pw, PG),
         '-e', 'API_HOST=0.0.0.0', '-e', 'API_PORT=8080', '-e', 'APP_ENV=production', '-e', 'ADMIN_EMAILS=',
         '-e', 'PUBLIC_API_BASE_URL=https://rehearsal.invalid', IMAGE_API])
    ip = container_ip(APIC)
    assert wait_health('http://%s:8080/healthz' % ip), 'candidate API did not become healthy'
    checks = {}
    for path in ['/healthz', '/projects', '/profile', '/admin/overview']:
        checks[path] = http('http://%s:8080%s' % (ip, path))[0]
    checks['device-challenge'] = http(
        'http://%s:8080/projects/00000000-0000-0000-0000-000000000000/device-sessions/challenge' % ip,
        data=b'{}', method='POST')[0]
    assert checks['/healthz'] == 200 and checks['/projects'] == 401 and checks['/admin/overview'] == 401, checks
    step('api', checks=checks)
    # 5. candidate web against that API (public pages only; no sign-in)
    run(['docker', 'run', '-d', '--name', WEBC, '--network', NET, '--read-only', '--cap-drop', 'ALL',
         '--tmpfs', '/tmp:size=64m',
         '-e', 'ORIGIN=https://rehearsal.invalid', '-e', 'HOST=0.0.0.0', '-e', 'PORT=3000',
         '-e', 'SESSION_SECRET=' + secrets.token_hex(24), '-e', 'WEB_SERVICE_KEY=' + secrets.token_hex(24),
         '-e', 'ENGINE_API_URL=http://%s:8080' % ip, '-e', 'DEV_AUTH=false', IMAGE_WEB])
    wip = container_ip(WEBC)
    assert wait_health('http://%s:3000/healthz' % wip), 'candidate web did not become healthy'
    pages = {}
    for path in ['/', '/login', '/features', '/privacy', '/security', '/add-ins', '/support',
                 '/.well-known/security.txt']:
        pages[path] = http('http://%s:3000%s' % (wip, path))[0]
    assert all(v == 200 for v in pages.values()), pages
    step('web', pages=pages)
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
