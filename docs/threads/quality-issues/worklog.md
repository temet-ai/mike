---
title: "Thread: quality-issues"
summary: Thread started.
type: Thread
status: parked
created: 2026-06-19
updated: 2026-09-03
thread: quality-issues
last_session: 2026-06-19 (session unknown) @ 766041c1
---

## State of play
Where we are: Code review (2026-06-19) surfaced 2 criticals in `load.rs`, 9 warnings, 11 suggestions. No work started yet — thread created to track resolution.

Critical issues to fix first:
1. `load_for_tests_no_toml` (`load.rs:190`) is byte-identical to `load()` — vacuous contract
2. `missing_toml_file_is_not_an_error` + `malformed_toml_is_ignored` (`load.rs:612,625`) are stub tests that test nothing

Key warnings requiring attention:
- `clear_proxy_env` omits 7+ env vars it should clear (load.rs:463-485)
- `MIKE_STORYBOOK_PORT` env var has no effect — never emitted by print-config (start-proxy.sh:209)
- `ACTIVE_WINDOW_MS` in SessionsZone duplicates store logic with a different value (30min vs 60s)
- `--health-timeout` injected unsanitized into awk in deploy-proxy.sh
- `DB_HOST`/`DB_PORT` from .env used unvalidated in /dev/tcp (start-proxy.sh:89-106)

Next: (1) Fix the two critical stub tests in load.rs, (2) fix clear_proxy_env completeness, (3) fix ACTIVE_WINDOW_MS duplication in SessionsZone, (4) add input validation to shell scripts
Files: /Users/dawiddutoit/projects/temet/temet-proxy/crates/claude-proxy/src/config/load.rs, /Users/dawiddutoit/projects/temet/temet-proxy/frontend/src/lib/components/home/SessionsZone.svelte, /Users/dawiddutoit/projects/temet/temet-proxy/scripts/deploy-proxy.sh, /Users/dawiddutoit/projects/temet/temet-proxy/scripts/start-proxy.sh
Open questions: none
Decisions: none
Blockers: none

## Session log
- 2026-06-19 (unknown) @ 766041c1 — thread created; code review findings captured (2 critical, 9 warning, 11 suggestion across 7 files)
- 2026-09-03 (thread sweep) — status active -> parked: no activity since 2026-06-19; no work was ever started
