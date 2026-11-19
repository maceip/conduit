#!/usr/bin/env bash
# the whole system end to end => see README.md
set -euo pipefail
cd "$(dirname "$0")"
npm ci --silent
npx playwright install chromium ${E2E_INSTALL_DEPS:+--with-deps}
exec node run.mjs
