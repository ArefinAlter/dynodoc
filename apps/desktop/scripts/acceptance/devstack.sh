#!/bin/bash
# A throwaway engine for the Node-level checks: PostgreSQL from the pruned tree, the freshly built
# engine-api, on loopback ports. usage: devstack.sh start|stop|reset
PG=/d/verne-build/dynodoc/programs/pg
ENGINE=/d/verne-build/dynodoc/programs
BASE=/d/verne-build/dynodoc/scratch/devstack
DATA=$BASE/data
mkdir -p $BASE
case "$1" in
start)
  if [ ! -d $DATA ]; then
    echo "pw-dev-123" > $BASE/pw.txt
    $PG/bin/initdb.exe -D $DATA -U dynodoc_owner --pwfile=$BASE/pw.txt --auth=scram-sha-256 -E UTF8 --locale=C --no-instructions --no-sync > $BASE/initdb.log 2>&1 < /dev/null
  fi
  $PG/bin/pg_ctl.exe start -D $DATA -w -t 60 -s -l $BASE/pg.log -o "-c port=54330 -c listen_addresses=127.0.0.1 -c max_connections=40 -c shared_buffers=64MB" > /dev/null 2>&1 < /dev/null
  /d/verne-build/dynodoc/scratch/venv/Scripts/python /d/verne-build/dynodoc/scratch/devprov.py
  export MIGRATION_DATABASE_URL='postgres://dynodoc_owner:pw-dev-123@127.0.0.1:54330/dynodoc'
  $ENGINE/engine-cli.exe migrate > $BASE/migrate.log 2>&1 < /dev/null
  $ENGINE/engine-cli.exe grant-runtime dynodoc_runtime >> $BASE/migrate.log 2>&1 < /dev/null
  unset MIGRATION_DATABASE_URL
  [ -f $BASE/paseto.key ] || python -c "import os;print(os.urandom(32).hex())" > $BASE/paseto.key
  [ -f $BASE/service.key ] || python -c "import os;print(os.urandom(24).hex())" > $BASE/service.key
  DATABASE_URL='postgres://dynodoc_runtime:rt-dev-456@127.0.0.1:54330/dynodoc' PASETO_LOCAL_KEY=$(cat $BASE/paseto.key) WEB_SERVICE_KEY=$(cat $BASE/service.key) API_HOST=127.0.0.1 API_PORT=18090 APP_ENV=production TOKEN_TTL_SECONDS=3600 RUST_LOG=warn nohup $ENGINE/engine-api.exe > $BASE/api.log 2>&1 < /dev/null &
  for i in $(seq 1 40); do curl -s http://127.0.0.1:18090/healthz > /dev/null && break; sleep 0.5; done
  curl -s http://127.0.0.1:18090/healthz; echo
  ;;
stop)
  taskkill //IM engine-api.exe //F > /dev/null 2>&1
  $PG/bin/pg_ctl.exe stop -D $DATA -m fast -s < /dev/null > /dev/null 2>&1
  echo stopped
  ;;
reset)
  $0 stop; rm -rf $BASE; echo reset
  ;;
esac
