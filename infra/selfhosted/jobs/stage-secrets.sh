#!/bin/sh
# One-shot job `secrets-init`. Copies the operator's secret files, the settings file and (in
# `provided` TLS mode) the certificate and key from the host directory into small named volumes,
# one per consumer, owned by the user that consumer runs as, mode 0400.
#
# Why: Compose bind-mounts file secrets with the host file's owner and mode and ignores `uid`,
# `gid` and `mode` (it says so), so a secret that is private to the operator on the host (0600)
# is unreadable by the container's user, and making it readable makes the API refuse it. Copying
# once, as root with three capabilities and no network, gives every service exactly the files it
# needs, readable only by it, the same on Linux, macOS and Windows hosts.
#
# Runs on every `docker compose up`. Idempotent. Prints no secret.
#
# Host (read-only):  /host/env  /host/secrets/*  /host/tls/*
# Volumes (writable): /stage/api /stage/web /stage/db /stage/check /stage/converter /stage/tls
#   api        10001  paseto_local_key database_url web_service_key
#   web        10001  every other secret file (session_secret, web_service_key, smtp_password, ...)
#   db         999    postgres_password runtime_database_password
#   check      10001  everything except the db files, plus .env  (for the `preflight` job)
#   converter  10001  converter_key
#   tls        101    fullchain.pem privkey.pem (provided mode only)
set -eu
umask 077

HOST_ENV=/host/env
SRC=/host/secrets
TLS_SRC=/host/tls
APP_UID=10001
DB_UID=999
PROXY_UID=101
problems=0

say() { printf '%s\n' "$*"; }
problem() { printf 'ERROR: %s\n' "$*" >&2; problems=$((problems + 1)); }

# --- checks ------------------------------------------------------------------------------------

[ -f "$HOST_ENV" ] || { say "ERROR: .env is missing. Create it with: node frontend/researcher/scripts/dynodoc-env.mjs generate-env (see README)." >&2; exit 1; }
[ -d "$SRC" ] || { say "ERROR: the secrets directory is missing." >&2; exit 1; }

for f in session_secret web_service_key paseto_local_key database_url postgres_password runtime_database_password; do
  if [ ! -f "$SRC/$f" ]; then
    problem "secrets/$f is missing. Create the settings and secrets with generate-env (see README); never copy a secret from another installation."
  elif [ ! -s "$SRC/$f" ]; then
    problem "secrets/$f is empty."
  fi
done
[ "$problems" -eq 0 ] || exit 1

for f in postgres_password runtime_database_password; do
  value=$(tr -d '\r\n' < "$SRC/$f")
  if ! printf '%s' "$value" | grep -Eq '^[A-Za-z0-9._~-]{16,}$'; then
    problem "secrets/$f must be at least 16 characters from A-Z a-z 0-9 . _ ~ - (the generated value is hex). Other characters would need escaping inside the database URL."
  fi
done
if [ "$problems" -eq 0 ]; then
  rt_pw=$(tr -d '\r\n' < "$SRC/runtime_database_password")
  expected="postgres://dynodoc_runtime:${rt_pw}@postgres:5432/dynodoc"
  actual=$(tr -d '\r\n' < "$SRC/database_url")
  if [ "$actual" != "$expected" ]; then
    problem "secrets/database_url does not match secrets/runtime_database_password, or names another login, host or database. The bundle fixes them as login dynodoc_runtime, host postgres, database dynodoc; generate-env writes exactly that."
  fi
fi

if ! grep -Eq '^DYNODOC_MODE=selfhosted[[:space:]]*$' "$HOST_ENV"; then
  problem ".env must contain the line DYNODOC_MODE=selfhosted (generate-env writes it). The bundle runs the self-hosted edition only."
fi

mode=${DYNODOC_TLS_MODE:-}
case "$mode" in
  provided)
    for f in fullchain.pem privkey.pem; do
      if [ ! -s "$TLS_SRC/$f" ]; then
        problem "DYNODOC_TLS_MODE=provided but tls/$f is missing or empty. Put your certificate chain (server certificate first) in tls/fullchain.pem and its private key in tls/privkey.pem."
      fi
    done
    ;;
  evaluation|acme) ;;
  *) problem "DYNODOC_TLS_MODE is '$mode'. Choose one in .env: provided (your own certificate), evaluation (self-signed, for trying it out) or acme (automatic public certificate)." ;;
esac
[ "$problems" -eq 0 ] || exit 1

# On a real Linux host a group- or world-readable secret is worth a warning. Docker Desktop shows
# every bind-mounted file as root-owned and 777, which says nothing about the real file.
for f in "$SRC"/* "$HOST_ENV"; do
  [ -f "$f" ] || continue
  info=$(stat -c '%a %u' "$f")
  perm=${info% *}
  owner=${info#* }
  if [ "$owner" != 0 ] && [ "$(printf '%s' "$perm" | tail -c 2)" != "00" ]; then
    say "WARNING: $(basename "$f") is readable by others on the host (mode $perm). Use chmod 600." >&2
  fi
done

# --- staging -----------------------------------------------------------------------------------

# A previous run left the volumes' top directories read-only for everyone (0555) once filled.
for d in /stage/*; do chmod 0755 "$d"; done

# stage DIR UID GID MODE SRCFILE NAME : copy one file in, replacing any previous version.
stage() {
  dir=$1 uid=$2 gid=$3 fmode=$4 srcfile=$5 name=$6
  install -o "$uid" -g "$gid" -m "$fmode" "$srcfile" "$dir/.new.$name"
  mv -f "$dir/.new.$name" "$dir/$name"
}

# prune DIR NAMES... : remove everything in DIR that is not one of NAMES (a removed secret must go).
prune() {
  dir=$1; shift
  keep=" $* "
  for existing in "$dir"/* "$dir"/.[!.]*; do
    [ -e "$existing" ] || continue
    base=$(basename "$existing")
    case "$keep" in *" $base "*) ;; *) rm -rf "$existing" ;; esac
  done
  chown 0:0 "$dir"
  chmod 0555 "$dir"
}

api_only="paseto_local_key database_url"
api_files="$api_only web_service_key"
db_files="postgres_password runtime_database_password"

# web and check: every file in secrets/ that is not reserved for the api or the database.
web_files=""
for f in "$SRC"/*; do
  [ -f "$f" ] || continue
  n=$(basename "$f")
  case "$n" in
    *[!a-z0-9_]*) say "NOTE: secrets/$n is not staged (names may use only a-z, 0-9 and _)." >&2; continue ;;
  esac
  case " $api_only $db_files " in *" $n "*) continue ;; esac
  web_files="$web_files $n"
done
web_files=${web_files# }

for n in $api_files; do stage /stage/api $APP_UID $APP_UID 0400 "$SRC/$n" "$n"; done
prune /stage/api $api_files

for n in $web_files; do stage /stage/web $APP_UID $APP_UID 0400 "$SRC/$n" "$n"; done
prune /stage/web $web_files

for n in $db_files; do stage /stage/db $DB_UID $DB_UID 0400 "$SRC/$n" "$n"; done
prune /stage/db $db_files

for n in $api_only $web_files; do stage /stage/check $APP_UID $APP_UID 0400 "$SRC/$n" "$n"; done
stage /stage/check $APP_UID $APP_UID 0400 "$HOST_ENV" .env
prune /stage/check $api_only $web_files .env

if [ -f "$SRC/converter_key" ]; then
  stage /stage/converter $APP_UID $APP_UID 0400 "$SRC/converter_key" converter_key
  prune /stage/converter converter_key
else
  prune /stage/converter
fi

if [ "$mode" = provided ]; then
  stage /stage/tls $PROXY_UID $PROXY_UID 0444 "$TLS_SRC/fullchain.pem" fullchain.pem
  stage /stage/tls $PROXY_UID $PROXY_UID 0400 "$TLS_SRC/privkey.pem" privkey.pem
  prune /stage/tls fullchain.pem privkey.pem
else
  prune /stage/tls
fi

say "secrets staged: api $(echo $api_files | wc -w), web $(echo $web_files | wc -w), database 2, converter $([ -f "$SRC/converter_key" ] && echo 1 || echo 0), tls $([ "$mode" = provided ] && echo 2 || echo 0)."
