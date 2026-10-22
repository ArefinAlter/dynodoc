#!/bin/sh
# Service `database-admin` (profile `maintenance`): runs engine-cli with the database OWNER
# credential (MIGRATION_DATABASE_URL) and the runtime login (DATABASE_URL), both built here from
# the secret files and kept in this process only. Used for operator tasks that the guide calls
# `database-admin`:
#
#   docker compose run --rm database-admin verify-snapshots --all
#   docker compose run --rm database-admin verify <document-uuid>
#   docker compose run --rm database-admin check-runtime
#   docker compose run --rm database-admin auth list-links      # operator sign-in links (decision 018)
#
# Never attached to the api or web, and not started by `docker compose up`.
set -eu

OWNER_PW=$(cat /run/secrets/postgres_password)
RUNTIME_PW=$(cat /run/secrets/runtime_database_password)
export MIGRATION_DATABASE_URL="postgres://dynodoc:${OWNER_PW}@postgres:5432/dynodoc"
export DATABASE_URL="postgres://dynodoc_runtime:${RUNTIME_PW}@postgres:5432/dynodoc"
exec engine-cli "$@"
