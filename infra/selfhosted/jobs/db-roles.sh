#!/bin/sh
# One-shot job `db-roles`. Runs in the PostgreSQL image (it has psql) as the database owner.
#
# Creates the restricted runtime login the API uses, or sets its password to the current secret,
# and closes the database to everyone else. `engine-cli grant-runtime` (the next job) creates no
# role and handles no password, so this is the one place the login is made. Idempotent: running
# it again only re-applies the same state, which is how a changed runtime password is rolled out.
#
# The owner password and the runtime password are read from /run/secrets inside this process only.
set -eu

PGPASSWORD=$(cat /run/secrets/postgres_password)
export PGPASSWORD
export PGHOST=postgres PGUSER=dynodoc PGDATABASE=dynodoc PGCONNECT_TIMEOUT=10

# The statements that carry the password are not logged, even if one fails.
psql -v ON_ERROR_STOP=1 --no-psqlrc -q <<'SQL'
SET log_statement = 'none';
SET log_min_error_statement = 'panic';
\set rtpw `cat /run/secrets/runtime_database_password`
SELECT format('CREATE ROLE dynodoc_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD %L', :'rtpw')
 WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'dynodoc_runtime') \gexec
SELECT format('ALTER ROLE dynodoc_runtime WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD %L', :'rtpw') \gexec
REVOKE ALL ON DATABASE dynodoc FROM PUBLIC;
GRANT CONNECT ON DATABASE dynodoc TO dynodoc_runtime;
SQL
echo "db-roles: login dynodoc_runtime is in place; the database is closed to PUBLIC."
