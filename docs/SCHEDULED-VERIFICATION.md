# Scheduled integrity verification

`.github/workflows/verification.yml` runs daily at 03:17 UTC on synthetic fixtures.
Operators run `scripts/verify-scheduled.sh` against their own database. It executes
`verify-projection --all` (read-only) and `verify-snapshots --all` (replays under
locks; backfills receipts) using restricted runtime privileges. Negative controls
assert that projection corruption or tampered snapshots exit 1 and invoke the alert
sink; clean fixtures exit 0 silently.

## The GitHub Actions run

The workflow seeds disposable PostgreSQL databases with synthetic documents and runs
the real CLI checks against clean and deliberately corrupted fixtures. It never
connects to a deployment, so it shows that the checks and the wrapper still work, not
that a deployed history is intact. The same job runs on pull requests that touch the
checks and on manual dispatch; only the upstream repository runs the daily schedule.

A failing run turns red and adds an error annotation. It needs no external service or
credential: GitHub's own failed-workflow notification reaches the user who last edited
the workflow's `cron` line (or re-enabled the workflow), so keep Actions notifications
on for that account. GitHub disables scheduled workflows of a public repository after
60 days without repository activity; a missing daily run is a failure to notice, so
check the Actions tab and re-enable the workflow when it happens.

## Operator configuration

Requires `engine-cli`, `scripts/verify-scheduled.sh`, and GNU `timeout`. Set
`DYNODOC_VERIFY_ALERT` to an executable notifier that reads failure summaries on
stdin and exits nonzero on delivery failure.

Environment (`/etc/dynodoc/verification.env`, mode `0600`):
```sh
ENGINE_CLI=/opt/dynodoc/bin/engine-cli
DYNODOC_VERIFY_ALERT=/usr/local/libexec/dynodoc-page-owner
DYNODOC_VERIFY_TIMEOUT=30m
# DATABASE_URL supplied via restricted runtime login credentials.
```

Crontab (runs daily, restricted log):
```cron
17 3 * * * umask 077; set -a; . /etc/dynodoc/verification.env; set +a; /opt/dynodoc/scripts/verify-scheduled.sh >> /var/log/dynodoc/verification.log 2>&1
```

## Failure response

The wrapper executes both checks before exiting. Any non-zero exit code or timeout
invokes `$DYNODOC_VERIFY_ALERT` and exits 1. Never append `|| true`.

On failure: preserve restricted logs and database backup. Investigate the failing
document with independent verification tools before restoring availability. Never
repair canonical history automatically, rewrite applied migrations, or delete receipts.
