#!/usr/bin/env python3
"""Write a bounded, non-secret host overview. Run on the host, never expose Docker."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path('/opt/dynodoc-demo')
output = root / 'ops'
output.mkdir(mode=0o755, exist_ok=True)
names = ['dynodoc-demo-' + name + '-1' for name in ('web','admin','api','postgres','converter')]
services = []
for name in names:
    reply = subprocess.run(['docker','inspect','--format','{{json .State}}',name], capture_output=True, text=True, timeout=10)
    if reply.returncode:
        services.append({'name':name,'running':False,'health':'Unavailable'})
        continue
    state = json.loads(reply.stdout)
    services.append({'name':name,'running':state['Running'],'health':state.get('Health',{}).get('Status'),'started_at':state['StartedAt']})
backup = None
files = sorted((root / 'backups').glob('dynodoc-*.dump'), key=lambda p:p.stat().st_mtime)
if files:
    path = files[-1]
    checksum = path.with_suffix(path.suffix + '.sha256')
    expected = checksum.read_text().split()[0] if checksum.exists() else None
    with path.open('rb') as stream:
        actual = hashlib.file_digest(stream,'sha256').hexdigest()
    backup = {'created_at':datetime.datetime.fromtimestamp(path.stat().st_mtime,datetime.timezone.utc).isoformat(),'bytes':path.stat().st_size,'checksum_ok':actual == expected}
status = {'checked_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'services':services,'disk_free_bytes':shutil.disk_usage(root).free,'backup':backup}
with tempfile.NamedTemporaryFile(mode='w',dir=output,delete=False) as file:
    json.dump(status,file)
    tmp = file.name
os.chmod(tmp,0o644)
os.replace(tmp,output/'status.json')
# Operational samples expire independently of the immutable document history.
subprocess.run(['docker','exec','dynodoc-demo-postgres-1','psql','-U','dynodoc','-d','dynodoc','-c',"delete from web_metric where recorded_at < now()-interval '30 days'"],check=True,stdout=subprocess.DEVNULL,timeout=30)
