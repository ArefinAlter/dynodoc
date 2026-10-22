#!/bin/sh
# One-shot job `init`. Runs the API image's engine-cli with the database OWNER credential:
# applies any pending migrations, grants the restricted runtime login exactly what it needs, and
# checks that it has nothing more. Idempotent: with nothing pending it changes nothing. A failure
# here stops `docker compose up` before the API starts.
#
# The owner credential exists only in this job (read from a file, kept in this process). The api
# and web containers never receive it.
set -eu

OWNER_PW=$(cat /run/secrets/postgres_password)
RUNTIME_PW=$(cat /run/secrets/runtime_database_password)
export MIGRATION_DATABASE_URL="postgres://dynodoc:${OWNER_PW}@postgres:5432/dynodoc"
export DATABASE_URL="postgres://dynodoc_runtime:${RUNTIME_PW}@postgres:5432/dynodoc"

echo "init: applying migrations as the database owner"
engine-cli migrate
echo "init: granting the runtime login"
engine-cli grant-runtime dynodoc_runtime
echo "init: checking the runtime login"
engine-cli check-runtime
echo "init: database ready"
