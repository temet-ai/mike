#!/usr/bin/env bash
# dev-watcher-ignore.sh — executable form of the
# .claude/rules/frontend-development.md self-check.
# Verifies frontend/vite.config.ts still ignores the non-imported artifact globs
# (stories / snapshots / screenshots) so the SPA dev server does not full-reload.
# Prints "CLEAN: ..." or "MISSING ignore glob: ...". Exit 0 = clean, 1 = missing.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
node -e "const s=require('fs').readFileSync('frontend/vite.config.ts','utf8'); \
  for (const g of ['*.stories.svelte','__snapshots__','__screenshots__']) \
    if (!s.includes(g)) { console.error('MISSING ignore glob:', g); process.exit(1) } \
  console.log('CLEAN: dev-watcher ignore list intact')"
