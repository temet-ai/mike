-- Phase 19 efficiency-steerer seeds (path A): seed 5 pattern-gated steerers
-- that nudge away from known tool-usage anti-patterns, firing through the
-- existing generic LLM-judged fleet (no new invocation mode, no widened
-- `steerers_kind_check` — all 5 use kind='refocus').
--
-- Idempotent: agent INSERT ... WHERE NOT EXISTS; steerer INSERT ... WHERE NOT
-- EXISTS keyed by unique `name`.

-- ─── 1. Agents prerequisite row ────────────────────────────────────────────
-- One generic agent backs all 5 rows — they share a family (tool-usage
-- efficiency nudges) distinct from refocus/false-done/memory-keeper.

INSERT INTO agents.agents (name, description)
SELECT
    'efficiency',
    'Efficiency steerer family — flags tool-usage anti-patterns (spill reads, cat, redundant grep, sleep polling, re-run gates) and nudges the faster path'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'efficiency');

-- ─── 2. Seed the 5 steerer registry rows ───────────────────────────────────
-- All 5: kind='refocus' (path A — no new kind), trigger_point='response'
-- (evaluate the assistant turn just completed), invocation='pattern' (LLM
-- judge only when a keyword pre-filter hits), auto_enrol=false (operator
-- opts sessions in explicitly, matching refocus/reorienter/memory-keeper —
-- not the false-done always-on special case), enabled=true (no-default-off).
-- authority=command / mode=drive mirrors refocus/false-done (a directive
-- nudge, not passive context supply).
-- Priorities 10-14 slot after the existing roster (refocus=1, reorienter=2,
-- memory-keeper=3, false-done=5) — priority has no UNIQUE constraint, just
-- an ordering hint, so this only avoids visual confusion with the original four.

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'no-full-spill-read',
    'No full spill read',
    'Detects a full Read (no offset/limit) on a harness spill/persisted-output file where grep/jq would do.',
    'refocus',
    (SELECT id FROM agents.agents WHERE name = 'efficiency' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    10,
    true,
    'response',
    'pattern',
    E'tool-results/\npersisted-output\noutput too large\nfull output saved to',
    false,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You judge whether an assistant turn just performed an unnecessarily expensive read of a harness spill/persisted-output file (paths like tool-results/*.txt or *.json, or output flagged "Output too large ... Full output saved to:") — specifically a full Read with no offset/limit/pagination, where a targeted grep/jq/scripts/spill-query projection would have found the needed content far more cheaply.

Look at the "Tool calls this turn" section: a Read tool call whose file_path points at such a spill file is the signal. A Bash grep/jq/scripts/spill-query invocation over the same path is NOT the anti-pattern — that is the correct behavior.

Be conservative: only flag when a full-file Read of a spill/persisted-output path actually occurred this turn. Do not flag ordinary Reads of source files, docs, or small config files.

Return a JSON verdict: fires (bool), confidence (0.0-1.0), reason (why), nudge (when fires=true, a short instruction to re-do the lookup with a targeted grep/jq/scripts/spill-query call instead of a full Read; empty otherwise).$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'no-full-spill-read');

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'no-cat-in-bash',
    'No cat in Bash',
    'Detects `cat` invoked via Bash on a repo file instead of the Read tool, bypassing harness read-tracking and dedup.',
    'refocus',
    (SELECT id FROM agents.agents WHERE name = 'efficiency' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    11,
    true,
    'response',
    'pattern',
    E'cat \ncat<<\n| cat',
    false,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You judge whether an assistant turn used a Bash `cat` command to read a repository file's contents, instead of the dedicated Read tool.

Look at the "Tool calls this turn" section: a Bash tool call whose command contains `cat <path>` (reading a file) is the anti-pattern. `cat` used only to pipe/concatenate command output (e.g. `foo | cat`), heredocs (`cat <<EOF`) for writing, or `cat` on a non-repo path (e.g. /dev/*, process substitution) is NOT the anti-pattern.

Be conservative: only flag when `cat` is genuinely substituting for a Read of a real file on disk.

Return a JSON verdict: fires (bool), confidence (0.0-1.0), reason (why), nudge (when fires=true, a short instruction to use the Read tool instead of Bash cat; empty otherwise).$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'no-cat-in-bash');

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'ast-grep-over-grep',
    'ast-grep over grep',
    'Detects repeated grep/rg reference-hunts on one symbol where ast-grep or LSP find-references would be faster and more precise.',
    'refocus',
    (SELECT id FROM agents.agents WHERE name = 'efficiency' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    12,
    true,
    'response',
    'pattern',
    E'grep\nrg -\nripgrep',
    false,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You judge whether an assistant turn is grep/rg-hunting for a code symbol (a function, struct, trait, or call site) by text search, where a structural tool (ast-grep) or an LSP find-references/go-to-definition call would answer the same question faster and without false positives from comments or strings.

Look at the "Tool calls this turn" section: a Grep tool call or a Bash grep/rg invocation whose query looks like a symbol name (an identifier, not a plain English phrase or a config/log string search) is the signal. Grep/rg used to search prose, logs, config values, or non-code text is NOT the anti-pattern — that is the correct tool for that job.

Be conservative: only flag when the query is plausibly a code symbol and the task is locating its definition or references.

Return a JSON verdict: fires (bool), confidence (0.0-1.0), reason (why), nudge (when fires=true, a short instruction to use ast-grep or the LSP find-references/go-to-definition tool for this symbol instead; empty otherwise).$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'ast-grep-over-grep');

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'monitor-over-sleep',
    'Monitor over sleep',
    'Detects a hand-rolled sleep/poll loop where the Monitor tool or scripts/task-wait would block correctly and notify on completion.',
    'refocus',
    (SELECT id FROM agents.agents WHERE name = 'efficiency' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    13,
    true,
    'response',
    'pattern',
    E'sleep \nsleep(\nuntil grep\nwhile true',
    false,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You judge whether an assistant turn used a hand-rolled `sleep`/polling loop (a Bash `sleep N` chain, an `until ...; do sleep ...; done` loop, or repeated short sleeps to wait on a background process or file) instead of the Monitor tool or `scripts/task-wait`, which block correctly and notify on completion without busy-waiting.

Look at the "Tool calls this turn" section: a Bash command containing `sleep` used to wait for another process/file/condition is the signal. A single short `sleep` unrelated to waiting on background work (e.g. a deliberate rate-limit delay documented as such) is NOT the anti-pattern.

Be conservative: only flag when the sleep is clearly standing in for a wait-until-done loop.

Return a JSON verdict: fires (bool), confidence (0.0-1.0), reason (why), nudge (when fires=true, a short instruction to use Monitor or scripts/task-wait instead of a sleep loop; empty otherwise).$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'monitor-over-sleep');

INSERT INTO steering.steerers
    (name, display_name, description, kind, agent_id,
     authority_id, default_mode_id, model, priority, enabled,
     trigger_point, invocation, pattern, auto_enrol, config, prompt)
SELECT
    'tee-then-slice-gates',
    'Tee then slice gates',
    'Detects re-running an expensive gate binary (make check, cargo nextest --all) a second time to inspect output already captured in a log.',
    'refocus',
    (SELECT id FROM agents.agents WHERE name = 'efficiency' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes       WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    14,
    true,
    'response',
    'pattern',
    E'make check\nmake test-all\ncargo nextest run --all\ncargo test --all',
    false,
    '{"debounce_turns": 2, "confidence_threshold": 0.7}'::jsonb,
    $$You judge whether an assistant turn re-ran an expensive gate command (make check, make test-all, cargo nextest run --all, cargo test --all, or similar full-suite/full-gate invocations) a second time within the same investigation, when the prior run's output was already captured to a log/file and could have been filtered/grepped instead (e.g. via scripts/expensive-cmd-cache).

Look at the "Tool calls this turn" and "User ask" sections: a repeat full-gate invocation with no code change in between, run only to re-read or re-filter its output, is the signal. The first run of a gate, or a re-run after real edits landed, is NOT the anti-pattern.

Be conservative: only flag when the same expensive gate is clearly being re-run to inspect already-known output.

Return a JSON verdict: fires (bool), confidence (0.0-1.0), reason (why), nudge (when fires=true, a short instruction to cache/filter the prior run's output — e.g. via scripts/expensive-cmd-cache — instead of re-running the gate; empty otherwise).$$
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'tee-then-slice-gates');
