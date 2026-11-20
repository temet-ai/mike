---
thread: ast-grep-adoption
status: parked
last_session: 2026-07-06 (15150187-e9c0-4206-a97d-fcdac6668389) @ aa371c17
updated: 2026-09-03
---

## State of play
Where we are: Implementation DONE and adversarially ACCEPTED 9/9 (2026-07-07). Landing via a clean, small PR: branch `feat/reminder-on-token-phase-c`, based on `origin/main` @ ca5981ee (PR #124 workspace-phase-1, already merged — phase B resolvers/hook/toggle + migration 20260706120001 came in through that PR, so only phase C (66c1ee0a) needed cherry-picking). Cherry-pick landed as f1e3560c after resolving 3 conflicts: (1) migration rename — no-op, main already has 20260706120001; (2) handler.rs — auto-merged cleanly, wiring order verified (filter L372 → reminder_on_token::apply L409 → lazy_sections L438); (3) resolvers/manifest.rs — main independently fixed the same expect_used clippy issue under a different name (SIGNATURE_RE vs my SIG_RE), kept main's name. Gates re-run by me on the rebased tree: cargo check --tests clean, reminder_on_token 9/9, resolvers 12/12, feature_toggles 14/14, hook suites 12/12+27/27, fidelity-check CLEAN, reminder_token_hook.py confirmed NOT registered in .claude/settings.json. Full `make check` in flight. BLOCKED on push/PR-open: coordinator (bb8247f0) flagged PR #126 (rg-in-CI) must land first — main's CI is currently red on rg-dependent resolver tests without it.
Units: implementation U0-U9 + U-ACCEPT all verified (see prior session log entries for the full ledger). Current unit: U-LAND (cherry-pick + gate + PR), tier=orchestrator, status=gates-green-pending-push, blocked on PR #126.
Next: (1) wait for coordinator confirmation PR #126 landed; (2) push feat/reminder-on-token-phase-c, open PR against main (title: reminder_on_token phase C); (3) reply to coordinator with PR URL; (4) after merge: deploy new binary to :9988, THEN register reminder_token_hook.py in .claude/settings.json (never before deploy — raw-token leak otherwise); (5) 4th-arm measurement (DECISIONS.md D6); (6) cosmetic resolver follow-ups (kind-tag misclassification, page-1-of-1 truncation marker, rg kill-on-timeout).
Files: crates/claude-proxy/src/reminder_on_token/{mod.rs,scan.rs,token.rs}, crates/claude-proxy/src/resolvers/{literal.rs,manifest.rs}, crates/claude-proxy/src/proxy/handler.rs, crates/claude-proxy/tests/reminder_on_token.rs, crates/claude-proxy/src/store/persistence/migrations/20260706120001_reminder_on_token_toggle.sql, artifacts/2026-07-06/ast-grep-resolver/{DECISIONS.md,spike-posttooluse-placement.md,e2e-runbook.md}, ROADMAP.md Phase 24.
Open questions: none blocking — only the external PR #126 timing dependency.
Decisions: BUNDLE-vs-peel resolved as PEEL (coordinator verified 1aa3051a + migration already on main via PR #124; only 66c1ee0a needed a standalone cherry-pick, not a 3-repo bundle). Standing: proxy is the delivery vehicle; serve-resolved-answer not enforce-tool; gate net-negative, disabled in main; reminder_token_hook.py unregistered until :9988 cutover.
Blockers: PR #126 (rg-in-CI) must land before push/PR-open — external dependency, not mine to resolve.
## Acceptance criteria
<acceptance-criteria>
Implementation run 2026-07-06 (/delegate: "full implementation of the above, mark okf as ignored"). Spec = artifacts/2026-07-06/ast-grep-resolver/{DECISIONS.md,injection-design-DRAFT.md,resolver-spec-DRAFT.md}. Target = temet-proxy @ feature/system-prompt-store-all (unrelated uncommitted changes from other threads present — never staged).
- C-SPIKE: PostToolUse additionalContext placement determined EMPIRICALLY (captured JSON from a real session showing where hook output lands relative to the tool_result), documented in artifacts/2026-07-06/ast-grep-resolver/spike-posttooluse-placement.md.
- C-HOOK: observe-only classifier reusing ast_grep_gate.py split_command_segments/parse_grep; emits PUA-delimited base64 token+payload for R1/R2 intents; NEVER denies; scripts/hook-eval suite green incl. new classifier cases.
- C-R2: literal where-used resolver — rg + standing noise-dir exclusions (target/, node_modules/, .git/, .sqlx/, artifacts/), hits grouped by file with 3-line context, kind tags, pagination past ~50 hits; unit-tested.
- C-R1: subsystem-manifest resolver — on-demand ast-grep, {file, one_line_role, key_excerpt} manifest, ≤8 files inline else file-list fallback, live-disk (no nightly cache); unit-tested.
- C-INJ: reminder_on_token proxy stage — scans user-turn tool_result content, new stage after request filter / before lazy sections in proxy/handler.rs; ALWAYS strips the token; injects case-specific <system-reminder> via reminders::appender primitive on resolver hit; fail-open (strip-only) on miss/timeout; capture stores pre-mutation inbound bytes; integration-tested.
- C-TOGGLE: reminder_on_token toggle defaults ON, settable from /settings, strip still happens when OFF; no-default-off self-check grep clean.
- C-GATE: make check green; scripts/hook-eval suite green; scripts/fidelity-check CLEAN. okf checks IGNORED (operator instruction 2026-07-06).
- C-LIVE: real-time E2E — live Claude Code session through the rebuilt proxy issues a grep-shaped command; captured next request shows injected case-specific reminder AND stripped token. Restarting the operator's running stack requires asking first; isolated alt-port instance is the fallback.
- C-CHECKPOINT: per-phase scoped commits (other threads' files never staged), worklog unit ledger maintained, no push.
Non-goals: R3/R4 resolvers, LSP substrate lane, 4th-arm measurement, worktree settings changes.
</acceptance-criteria>

## Work units
| id | what | owner | tier | status | allowlist | verify | evidence | rounds | handle |
|----|------|-------|------|--------|-----------|--------|----------|--------|--------|
| U1 | SPIKE PostToolUse token placement | agent | R4 | verified | artifacts/2026-07-06/ast-grep-resolver/spike-*, scratch | orchestrator re-pulled request 104733 messages[3] from DB — byte-identical to doc fragment | sibling text block, pre-wrapped system-reminder, cache_control attached | 0 | spike-posttooluse (Agent tool) |
| U2 | hook classifier (observe-only, token emit) | agent | R2 | verified | .claude/hooks/reminder_token_hook.py + hooks/tests/** | hook-eval suite + review | orch re-ran: 12/12 + 27/27 PASS (post-Pyright-fix) | 0 | wf_77f184a0-b4c |
| U3 | R2 literal where-used resolver | agent | R2 | verified | crates/claude-proxy/src/resolvers/literal.rs | verifier re-runs scoped nextest | orch re-ran combined nextest: 26/26 passed | 0 | wf_77f184a0-b4c |
| U4 | R1 subsystem-manifest resolver | agent | R3 | verified | crates/claude-proxy/src/resolvers/manifest.rs | verifier re-runs scoped nextest | orch re-ran combined nextest: 26/26 passed | 0 | wf_77f184a0-b4c |
| U5 | reminder_on_token toggle plumbing | agent | R2 | verified | crates/claude-proxy/src/feature_toggles/** | verifier + no-default-off grep | orch re-ran nextest + no-default-off grep CLEAN | 0 | wf_77f184a0-b4c |
| U5b-fe | settings FEATURE_COPY entry for toggle | agent | R2 | verified | frontend/src/routes/settings/+page.svelte | verifier re-runs sb-test | orch re-ran sb-test routes/settings: PASS | 0 | wf_55551931-262 |
| U5b-db | CHECK-constraint migration + seed | agent | R2 | verified | migrations/20260706120001_reminder_on_token_toggle.sql (renamed from 20260706000001, collision fix) | verifier re-runs DB-backed nextest | orch re-ran: 14/14; CHECK list = 9 prior keys + reminder_on_token; Perf-Exempt (DDL, db-perf-check exit 2 verified) | 0 | wf_55551931-262 |
| U6 | reminder_on_token stage wiring | agent | R4 | verified | reminder_on_token/** (new), proxy/handler.rs, lib.rs 1 line, tests/reminder_on_token.rs | verifier: 7/7 functional PASS w/ live hook contract check; clippy gate initially FAIL | round 1: orch fixed mod.rs:139 lifetime + manifest.rs SIG_RE→LazyLock; round 2: U6-lint worker converted 54 test unwrap/expect to anyhow. Orch re-ran post-fix: clippy -p claude-proxy --all-targets CLEAN, feature_toggles 14/14, hook suites 12/12+27/27, fidelity CLEAN | 2 | wf_911c5620-610 → Agent-tool verify + U6-lint fix |
| U7-prep | E2E harness + baseline capture + runbook | agent | R4 | verified | throwaway dir + artifacts/.../e2e-runbook.md only | orchestrator re-ran baseline SQL: raw_token=t, no_strip=t (req 105210) | e2e-runbook.md; harness /private/tmp/spike-u7.gHlVbo; baseline sess e350d2e6/req 105210 | 0 | spike-posttooluse (SendMessage reuse) |
| U0 | resolvers module scaffold (mod.rs+stubs+lib.rs line) | orchestrator | rung-a | verified | resolvers/mod.rs, literal.rs, manifest.rs (empty), lib.rs +1 line | scripts/cx check -p claude-proxy green (14s, in-turn) | check output quoted in transcript | 0 | TAKEOVER rung=a |
| U7 | live E2E drive + capture proof | agent | R3 | pending | scratch + isolated proxy instance | orchestrator re-reads captured request | — | 0 | — |
| U8 | migration-collision repair (rename + template ledger + test count/key-list) | orchestrator | rung-a | verified | migrations/ (git mv), test template DB, feature_toggles.rs test constants | orch re-ran feature_toggles nextest | 14/14 PASS after: git mv →20260706120001, DELETE stale _sqlx_migrations row 20260706000001 from claude_proxy_test_template, test expectations 9→10 + key list +reminder_on_token | 0 | TAKEOVER rung=a |
| U9 | RUSTSEC-2026-0204 advisory (crossbeam-epoch 0.9.18, transitive) failed make-check deny step | orchestrator | rung-a | built | Cargo.lock only | full make check re-run (in flight) | cargo update -p crossbeam-epoch → 0.9.20 (advisory's stated solution); fmt+lint+test-all had passed before deny in first run | 0 | TAKEOVER rung=a |

## Lessons & environment
- OPERATOR ROUTING CONSTRAINT (2026-07-07): workers must run on explicit lower-tier models (haiku/sonnet/opus per R1-R4 ladder) — never let a dispatch inherit the session (fable) model; omitted model: is a defect.
- okf: CLI not installed in this environment; operator marked okf checks IGNORED for this run (2026-07-06).
- Cargo: shared CARGO_TARGET_DIR via make/scripts/cx; parallel rust workers contend on the target-dir lock (blocking wait, not failure) — use scoped `cargo check -p claude-proxy` / `nextest --test <stem>`; never `cargo clean`.
- Capture DB access: scripts/proxy-q sql (MIKE_DATABASE_URL in .env, never DATABASE_URL). tools.uses ord is per-request — session-wide ordering needs (request.created_at, ord).
- Working tree carries uncommitted changes from threads system-prompt-store-all + anti-false (crates/claude-proxy/src/{api/handlers.rs,config/subcommands.rs,store/persistence/*}, artifacts/knowledge/*, two worklogs) — never stage/edit those.
- Injection design ground truth: lazy_sections/injector.rs has its OWN implementation (does NOT call reminders::appender); new module must call reminders::appender::append_block_to_last_user_message directly.
- scripts/hook-eval suite/case accept --hook <path> — a second hook file needs no driver extension; its parse_decision maps additionalContext-only output to 'deny' vocabulary (documented at top of reminder-token.cases.tsv).
- /settings toggle exposure is NOT automatic: FEATURE_COPY map in frontend/src/routes/settings/+page.svelte AND the closed CHECK constraint on config.feature_toggles.key (template: 20260625000002_session_judge.sql) both need entries per new toggle.
- Incident (U5, self-corrected): unscoped `cargo fmt -p claude-proxy` reformatted lib.rs + literal.rs mid-flight; reverted byte-for-byte; orchestrator confirmed lib.rs diff is exactly the +1 scaffold line. Lesson: fmt must be file-scoped while parallel units are in flight.
- Orchestrator takeover (rung a): Pyright None-guard in reminder_token_hook.py _load_gate(); both hook suites re-run green after.
- SPIKE VERDICT (U1, overrides injection-design §5 where they differ): hook additionalContext lands as a SIBLING text block in the same user message as the tool_result, ALREADY <system-reminder>-wrapped with prefix "PostToolUse:Bash hook additional context: ", own cache_control (1h). U6 therefore REPLACES the inner content of that block (keeps the shell) instead of strip+append via reminders::appender; empty shells are dropped, never forwarded. Token is mid-string. Capture fidelity empirically confirmed (raw token in stored body).
- cmux env leaks HTTPS_PROXY=127.0.0.1:9987 into child claude processes — unset HTTPS_PROXY/HTTP_PROXY or the child bypasses the :9988 proxy (spike hit this; documented fix).
- CONCURRENT SESSIONS: another operator session (b315c517, system-prompt workspace phase) is committing to this branch in parallel (b3914af5 landed mid-run); a third ran a restart test at 16:04Z. Phase B committed early (1aa3051a) to shield it. Workers instructed to re-Read-before-Edit and report collisions.
- Phase B committed: 1aa3051a (11 files). Spike doc still uncommitted — goes in the phase C commit.
- MIGRATION VERSION COLLISION (U8): our 20260706000001 collided with the session-coordination worktree's 20260706000001_workspace_core.sql (branch feature/workspace-phase-1) — both trees share the test postgres (infra.local:5440) and its claude_proxy_test_template, so whichever tree migrates the template poisons the other (checksum-mismatch, then applied-but-missing after our rename). Fix: git mv ours to 20260706120001 + DELETE the stale 20260706000001 row from the template's _sqlx_migrations (our migration is idempotent, double-application harmless). The other branch will still fail template validation on our 120001 row until merge — inherent to two branches adding migrations against one shared template; resolves at merge since versions now differ. Also: U5 worker had missed the test's hardcoded count (9) and canonical key list — both updated.
- U6 workflow wf_911c5620-610 FAILED at the reporting step only (StructuredOutput retry cap 5 exceeded — the known verbose-payload parse failure); implementation was complete on disk (reminder_on_token/{mod,scan,token}.rs 30KB, handler.rs +28, tests). Recovery: treated as built, gates re-run by orchestrator (all green), independent verify dispatched via Agent tool with PLAIN-TEXT return. Lesson: R4/opus impl units get plain-text return contracts or tightly capped schema fields — do not resume that workflow (resume would re-run the impl and clobber in-tree work).
- U7-prep baseline: session e350d2e6-63db-468e-8627-ecd2d27a072f, request 105210 (raw token on wire, filtered_request_body NULL) — Phase D comparison point. E2E harness: /private/tmp/spike-u7.gHlVbo; runbook: artifacts/2026-07-06/ast-grep-resolver/e2e-runbook.md.
- Workspace-wide clippy denies unwrap_used/expect_used via .cargo/config.toml rustflags — INCLUDING test binaries. Existing tests pass because their unwraps are on chrono LocalResult (not Option/Result). Convention: Result-returning tests + `?`, no #[allow].
- FOLLOW-UP (non-blocking, verifier finding): resolver timeout leaks the spawn_blocking thread — a hung rg subprocess is never killed (resolvers/literal.rs:265, manifest.rs:116). Satisfies C-INJ fail-open but is a resource risk under repeated timeouts; needs child-process kill-on-timeout in a later unit.

## Post-gate observation (2026-06-10 afternoon sessions)

Gate IS live and firing: ~9 genuine denial events 10:42–11:20 UTC across the operator's follow-up sessions (the log's 10:17–10:26 block is pollution from this session's own test fixtures and corpus replay — adoption metrics must come from tools.uses, not the log). Findings:

1. **Conversion ≈ 0.** Post-gate sessions: 533 Bash calls, 1 mcp__ast-grep call, 1 Grep call. Denials did not convert to ast-grep.
2. **Evasion observed** (predicted by the file-nav mining agent): after the 10:42 denial the session ran `git show HEAD:tools.rs > /tmp/tools_head.rs && awk ... | grep` — copied source to /tmp (non-source path) and grepped there. Sessions also Read the gate script itself (2×). Do NOT chase awk/python with more deny rules (arms race); ship the tools that answer the underlying question instead — the evaded case was exactly the tools.rs tool-counting struggle → `proxy-mcp list-tools` from the catalogue.
3. **3 of 9 genuine denials were FALSE POSITIVES** from a shlex tokenizer bug: unquoted `;` glued to the prior word merged pipeline segments, so `ls | grep -i vite; sed -n ... vitest.config.ts` looked like grep-on-.ts. FP denials train evasion. FIXED: split_command_segments rewritten with shlex punctuation_chars; 18/18 fixtures (3 new semicolon cases); replay confirms the 3 FPs now allow; corpus coverage unchanged (338 denied, 45% full translation, 0 bare).
4. 25 post-gate requests lacked ast-grep MCP tools entirely (other-project session) — deny message references tools some sessions don't have; the Grep-tool fallback line covers them.

Conclusion: deny-without-attractive-alternative produces evasion, not adoption. Priority shifts to the cli-extraction Tier-1 bundle (tools + toolbox card) with the gate as backstop, per the shipping rule.

## Session log
- 2026-06-10 (38adceac) @ fc7e60e — Compared 4.x vs Fable 5 system prompts from proxy captures (Fable ~56% smaller; engineering rules cut, communication/autonomy guidance expanded; no main-loop Opus 4.8 capture exists — used Sonnet 4.6 as the 4.x-era stand-in). Found frontend-developer agent had no ast-grep guidance in its dispatched prompt (frontmatter-only ref); added Code Navigation section to .claude/agents/frontend-developer.md. Investigated `make ast-grep-report` "no uses" complaint: confirmed 1% compliance is real, attributed 330/394 greps to the main loop, recommended PreToolUse deny-hook as the adoption fix.
- 2026-06-10 (6ffbf7aa-6c84-4dcb-928e-4c253f4c3be9) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (9db4e220-b0bd-4f35-b059-a9eee60f6b44) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (38adceac-0458-4c12-8414-b28245ff38bf) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (27e4bc9f-7223-4324-9f76-6a3bba6e1b80) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (d99dbe8f-187a-4082-9a7b-2d0669247d74) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (93b8b53b-2942-4309-8548-7436a73801ed) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (6618279d-2ec1-4928-9dda-298cc990efee) @ fc7e60e — auto-checkpoint on session end
- 2026-06-10 (03c16670-c4db-4d1b-9055-8584dc13a7cf) @ dbf6bc9 — auto-checkpoint on session end
- 2026-06-10 (bb7dad85-a608-4e27-b18a-c70e14d8a5d2) @ dbf6bc9 — auto-checkpoint on session end
- 2026-06-10 (f753317a-7776-457f-b27f-b0a46faeb07e) @ 2e0b8f81 — auto-checkpoint on session end
- 2026-06-11 (98c2abf3-012f-4829-a66e-f1ec493fb57b) @ 2e0b8f81 — auto-checkpoint on session end
- 2026-06-12 (7f8151c4-c7b8-41f9-98f9-e274a7830cba) @ 2e0b8f81 — auto-checkpoint on session end
- 2026-06-12 (1f326481-63e0-45fb-aed1-18f158a2e63b) @ 2e0b8f81 — auto-checkpoint on session end
- 2026-06-12 (df3d7233-32b1-44e1-83fe-1f046c759f40) @ 2e0b8f81 — auto-checkpoint on session end
- 2026-06-12 (6f262acc-0749-4bbc-8fb9-c0900bfcb124) @ 960eda49 — auto-checkpoint on session end
- 2026-06-13 (54be01a4-638b-4e4e-8e40-6ba6937a1ef4) @ 960eda49 — auto-checkpoint on session end
- 2026-06-13 (fc296cb1-1e6a-4cb0-8b9e-9ac61b2f0213) @ b70a8cee — auto-checkpoint on session end
- 2026-06-15 (64b50581-7065-4013-8f77-dd7d8adeddb4) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (4a5e96a5-5445-40e0-986f-0ec27fa04f65) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (a2e3affd-79ee-4f8c-b61f-4c8e9000a0a1) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (055dc892-99bd-46a3-9ec5-1b8ffadf2190) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (23214930-40a5-41bd-b773-3d13e30b10b8) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (827752a4-2dc5-44b6-82d0-4e7ad9dc0b04) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (821c1197-f2a5-43a9-acc7-92e813b9d91c) @ 7bc4110b — auto-checkpoint on session end
- 2026-06-15 (04978e09-efd9-4b57-b1b3-67bbf5e661b0) @ 12997aa9 — auto-checkpoint on session end
- 2026-06-15 (97e9ddcd-d0d2-4791-bb95-5fa512981bdb) @ 12997aa9 — auto-checkpoint on session end
- 2026-06-15 (4b3c987a-bd1c-4ebb-a6bc-87a1e58accc9) @ dc8fac0c — auto-checkpoint on session end
- 2026-06-15 (b0299c6d-5d91-4e57-ac9b-7353ff8590ff) @ 75b2c5a7 — auto-checkpoint on session end
- 2026-06-16 (f463f95e-d9bc-4a9d-a9a6-bf896dbb2963) @ 75b2c5a7 — auto-checkpoint on session end
- 2026-06-16 (a8c5eea2-eb47-45c5-989e-3d975664099b) @ 9e2283a5 — auto-checkpoint on session end
- 2026-06-16 (c116a894-5d6b-4f6e-b054-9a8df33a963e) @ 9e2283a5 — auto-checkpoint on session end
- 2026-06-16 (7e22f86d-742b-423f-ace3-e028c7257328) @ a9460edb — auto-checkpoint on session end
- 2026-06-16 (96016b68-4f39-4716-a134-495b9c904471) @ a9460edb — auto-checkpoint on session end
- 2026-06-16 (636056ed-1bec-4376-88ab-da4698f4e22c) @ 692275f5 — auto-checkpoint on session end
- 2026-06-16 (eddf1186-14a4-4958-8f11-02496a424f1b) @ 97f2cc28 — auto-checkpoint on session end
- 2026-06-16 (0ea207c2-936b-48c8-86a8-a034df3d6dfd) @ cc8c03de — auto-checkpoint on session end
- 2026-06-16 (49098071-085e-47f5-86f9-b5b15e1f54d6) @ 9fe8eaac — auto-checkpoint on session end
- 2026-06-16 (0b591a58-7637-4d3b-92e8-cdbd473cd166) @ 9fe8eaac — auto-checkpoint on session end
- 2026-06-17 (a2cb65fa-2959-4c00-b3bf-c05a1a2692f8) @ dfc2df90 — auto-checkpoint on session end
- 2026-06-17 (ea593008-53e7-43cd-8428-64f42417f30c) @ dfc2df90 — auto-checkpoint on session end
- 2026-06-17 (d0a6fb11-e811-47a5-a4b0-efede52d61ac) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (44c49ca9-13d9-4717-b4f2-f2c75cad03e6) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (4a4c9a7f-e0d3-49cd-8056-df08f202b41b) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (cc8ad6a8-c1e8-4b08-b951-473bf5f4c43b) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (3a014b0c-039b-4757-a148-d98f8f596c23) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (46726b2d-e0e1-4c08-abd5-9870740e7c16) @ d5404deb — auto-checkpoint on session end
- 2026-06-17 (3a014b0c-039b-4757-a148-d98f8f596c23) @ d5404deb — Analyzed post-denial behavior + per-denial alternative quality; designed intent-aware 3-route gate (text→Grep/rg, structure→ast-grep, symbol→LSP)
- 2026-06-17 (3a014b0c-039b-4757-a148-d98f8f596c23) @ d5404deb — Recorded ban-active window (2026-06-10 16:29Z → 2026-06-17 21:52Z, 66 sessions, 1978 denies) + disabling the gate to collect fresh natural-behavior baseline (Arm 1 refresh / control for the resolver)
- 2026-06-18 (881f9a84-832a-4f6b-b438-ecc05c843700) @ d5404deb — auto-checkpoint on session end
- 2026-06-18 (3a014b0c-039b-4757-a148-d98f8f596c23) @ d5404deb — Gate off in MAIN settings only — still live in 4 worktrees (operator: leave them); pivot to serve-resolved-answer holds; analyzed pre-grep tool-call chains from transcripts (grep = navigation primitive, 82% lead to more search/read)
- 2026-06-18 (30164918-451c-4b1b-bbe8-4f2b88c7f871) @ d06cd518 — auto-checkpoint on session end
- 2026-06-18 (4bc1893d-2ce3-40fb-a059-b37bb555ef8d) @ c569f134 — auto-checkpoint on session end
- 2026-06-18 (39bbd8e5-a988-4b5c-8000-df1b76609b02) @ 27921b51 — auto-checkpoint on session end
- 2026-06-18 (cfeb6db0-5b5a-46a3-8073-5130d220c39d) @ 27921b51 — auto-checkpoint on session end
- 2026-06-18 (884c3776-b3f3-4503-a540-e21c71d1f310) @ 0f6fd018 — auto-checkpoint on session end
- 2026-06-19 (6243e2a2-7f62-4cad-9da2-4b01d7a10553) @ 0d2f436e — auto-checkpoint on session end
- 2026-06-19 (b8c65252-be4b-48c2-949b-cd334beeada0) @ 0e39c661 — auto-checkpoint on session end
- 2026-06-19 (0bcf6842-5195-4504-b146-65582e21773a) @ 0e39c661 — auto-checkpoint on session end
- 2026-06-19 (8a7e7181-2c5d-4fb5-8d3f-637a7f1c0030) @ 766041c1 — auto-checkpoint on session end
- 2026-06-19 (e754f6b6-b57d-42cb-8416-4dc8393e4908) @ 766041c1 — auto-checkpoint on session end
- 2026-06-19 (c65a8e51-719c-4380-baa4-a5a43aa3d66c) @ 766041c1 — auto-checkpoint on session end
- 2026-06-19 (fb870209-5901-4a6f-b900-5cf0ebf8b21c) @ 766041c1 — auto-checkpoint on session end
- 2026-06-19 (425abaa9-3bdd-429c-9de1-8557ab352051) @ 1b34329d — auto-checkpoint on session end
- 2026-06-20 (cdcb4ea9-a978-4982-bd44-91404d8da4bf) @ d3c44f75 — auto-checkpoint on session end
- 2026-06-20 (829c1ee3-c0a2-4e78-bd72-e47b0e955b16) @ d3c44f75 — auto-checkpoint on session end
- 2026-06-20 (c5aaf3f4-dd04-4f41-bca4-6b7d6aa2dc1c) @ d3c44f75 — auto-checkpoint on session end
- 2026-06-20 (25a47388-fd57-4c1e-bbb4-ac5fe2b42ab7) @ d3c44f75 — auto-checkpoint on session end
- 2026-06-20 (18292893-6c96-4865-a57f-02985c1058da) @ d3c44f75 — auto-checkpoint on session end
- 2026-07-06 (15150187-e9c0-4206-a97d-fcdac6668389) @ 0c218ccb — /delegate run executed the full Next arc: 3-arm metrics + 10-chain mining (wf_6a73e791-164, verified), resolver/injection/substrate design drafts (wf_d0032e30-f08, code-grounding verified with 3 citation fixes), architect DECISIONS.md (LSP deferred, cold ast-grep+rg v1, R1+R2 via reminder_on_token, R3/R4 separate), adversarial acceptance gate C1-C4 (C1 corrective wave: Arm3 enumeration TSV + D8 frontend-project disclosure). Deliverables: artifacts/2026-07-06/ast-grep-resolver/.
- 2026-07-07 (15150187-e9c0-4206-a97d-fcdac6668389) @ f1e3560c — implementation ACCEPTED 9/9 (see prior entry). Multi-session merge coordination (bb8247f0): confirmed origin/main=ca5981ee already carries phase B + migration via PR #124; only phase C (66c1ee0a) needed landing. Cherry-picked onto a clean branch feat/reminder-on-token-phase-c, resolved 3 conflicts (migration no-op, handler.rs auto-merge verified, manifest.rs SIGNATURE_RE naming), re-ran all gates green. Holding push/PR-open on PR #126 (rg-in-CI) per coordinator.
- 2026-09-03 (thread sweep) — status implementing -> parked: no activity since 2026-07-07; ast-grep is in daily use but this landing sequence was never finished
