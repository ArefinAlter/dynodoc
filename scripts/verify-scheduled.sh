#!/bin/sh
# Linux operator entry point. Logs may contain document fields; keep them private.
# Run with a provisioned database URL and an executable failure notifier.
set -u
: "${DATABASE_URL:?DATABASE_URL is required}"
: "${DYNODOC_VERIFY_ALERT:?Set the absolute path of an executable failure notifier}"
case "$DYNODOC_VERIFY_ALERT" in
  /*) ;;
  *) echo 'DYNODOC_VERIFY_ALERT must be an absolute executable path' >&2; exit 2 ;;
esac
if [ ! -x "$DYNODOC_VERIFY_ALERT" ]; then
  echo 'DYNODOC_VERIFY_ALERT is not executable' >&2
  exit 2
fi
engine_binary=${ENGINE_CLI:-engine-cli}
check_timeout=${DYNODOC_VERIFY_TIMEOUT:-30m}
projection_status=0
snapshot_status=0
timeout --signal=TERM --kill-after=30s "$check_timeout" "$engine_binary" verify-projection --all || projection_status=$?
timeout --signal=TERM --kill-after=30s "$check_timeout" "$engine_binary" verify-snapshots --all || snapshot_status=$?
if [ "$projection_status" -ne 0 ] || [ "$snapshot_status" -ne 0 ]; then
  # No credentials, document IDs or content are sent to the notifier.
  if ! "$DYNODOC_VERIFY_ALERT" <<EOF
Dynodoc scheduled verification failed: verify-projection=$projection_status verify-snapshots=$snapshot_status. Investigate the restricted host log; do not automatically repair history.
EOF
  then
    echo 'ERROR: the verification failure notifier also failed' >&2
  fi
  exit 1
fi
