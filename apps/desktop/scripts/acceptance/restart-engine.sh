#!/bin/bash
# Restarts the throwaway engine of the devstack with a given access-token life (seconds).
# usage: restart-engine.sh 30
TTL=${1:-3600}
ENGINE=/d/verne-build/dynodoc/programs
BASE=/d/verne-build/dynodoc/scratch/devstack
taskkill //IM engine-api.exe //F > /dev/null 2>&1
sleep 1
DATABASE_URL='postgres://dynodoc_runtime:rt-dev-456@127.0.0.1:54330/dynodoc' PASETO_LOCAL_KEY=$(cat $BASE/paseto.key) WEB_SERVICE_KEY=$(cat $BASE/service.key) API_HOST=127.0.0.1 API_PORT=18090 APP_ENV=production TOKEN_TTL_SECONDS=$TTL RUST_LOG=warn nohup $ENGINE/engine-api.exe > $BASE/api.log 2>&1 < /dev/null &
for i in $(seq 1 40); do curl -s http://127.0.0.1:18090/healthz > /dev/null && break; sleep 0.5; done
curl -s http://127.0.0.1:18090/healthz; echo " ttl=$TTL"
