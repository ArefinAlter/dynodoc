#!/bin/sh
# Smoke test of the self-hosted bundle on Linux and macOS hosts.
#
#   infra/selfhosted/scripts/smoke.sh [options]       (options: see smoke.mjs)
#
# This is a launcher. The checks live in smoke.mjs, one implementation that Windows, macOS and
# Linux all run, so the three cannot drift apart. It needs Docker with Compose v2 and Node 20 or
# newer on the machine that runs the test (Node is not needed to RUN Dynodoc, only to run this
# test and to generate the settings with dynodoc-env.mjs if you prefer not to use the tools image).
# The browser checks also need Playwright with a Chromium: pass --playwright-from DIR where DIR's
# node_modules has @playwright/test (the repository's frontend/researcher after `pnpm install`
# and `pnpm exec playwright install chromium`), or pass --skip-browser.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

command -v docker >/dev/null 2>&1 || { echo "smoke.sh: docker is not installed or not on PATH" >&2; exit 2; }
docker compose version >/dev/null 2>&1 || { echo "smoke.sh: Docker Compose v2 ('docker compose') is required" >&2; exit 2; }
command -v node >/dev/null 2>&1 || { echo "smoke.sh: Node 20 or newer is required to run the test" >&2; exit 2; }
major=$(node -p 'process.versions.node.split(".")[0]')
[ "$major" -ge 20 ] || { echo "smoke.sh: Node 20 or newer is required (found $(node -v))" >&2; exit 2; }

exec node "$here/smoke.mjs" "$@"
