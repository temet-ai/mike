-- Add web_fetch_sidecar turn origin and repair misclassified operator-turn
-- prompts (thread: turn-prompt-extraction, inventory 2026-06-10).
--
-- Background: when an agent uses the WebFetch tool, the harness fires a
-- one-shot request whose single user message is the fetched page plus the
-- caller's question: `\nWeb page content:\n---\n<page>\n---\n<prompt>`.
-- It runs on the active model and carries the SAME session id as the real
-- conversation, so it was classified turn_origin='primary' and opened a
-- spurious operator turn whose user_prompt is a full web-page dump
-- (live data: 48 such turns; one session had 28 of its 30 turns from this).
--
-- This migration also repairs the two other prompt-accuracy defects found in
-- the same inventory:
--   - 45 legacy rows written before snapshot_request switched from raw
--     last_user_message to the tag-stripped human_prompt (commit b54f274):
--     backfilled from requests.human_prompt.
--   - Local-command / slash-command envelope tags (<local-command-caveat>,
--     <command-name>, <command-message>, <command-args>,
--     <local-command-stdout>) surfacing as the prompt: repaired with the
--     same rules the Rust extractor now applies (residue text wins; pure
--     command invocations surface the command name + args).

-- Step 1: register the new origin.
-- Explicit id=9 (OVERRIDING SYSTEM VALUE) consistent with the baseline that
-- seeded ids 1-6, monitor_sidecar=7, count_tokens_sidecar=8. setval advances
-- the sequence past the explicit id so future plain INSERTs don't collide.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (9, 'web_fetch_sidecar')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 9);

-- Step 2: reclassify the request rows.
-- Fingerprint mirrors the Rust extractor: one-shot (message_count = 1) and
-- the harness envelope anchored at the (whitespace-trimmed) start of the
-- extracted prompt. The leading anchor rejects genuine prompts that merely
-- quote the marker deeper in their text. Restricted to current
-- turn_origin='primary' so correctly-classified rows are never touched.
UPDATE requests.requests r
SET turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'web_fetch_sidecar')
WHERE r.turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'primary')
  AND r.message_count = 1
  AND r.human_prompt ~ '^[[:space:]]*Web page content:[[:space:]]*\n---';

-- Step 3: remove the spurious operator turns.
-- Their start_request_id now points at a web_fetch_sidecar request. Deleting
-- (not inheriting) follows the monitor_sidecar / count_tokens_sidecar
-- precedent: these requests never represented a conversational turn, so the
-- correct model is that they never opened one. operator_turn_chunks cascade;
-- requests.operator_turn_id detaches via ON DELETE SET NULL.
DELETE FROM memory.operator_turns t
WHERE t.start_request_id IN (
    SELECT r.id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id
    WHERE o.code = 'web_fetch_sidecar'
);

-- Step 4: backfill legacy turn prompts from the start request.
-- Rows written before commit b54f274 carry the raw last_user_message
-- (leading <system-reminder>/<task-notification> blocks, tool noise). The
-- start request's human_prompt is the already-stripped value the current
-- write path would have stored — adopt it wherever the two differ. Runs
-- after Step 3 so deleted WebFetch turns are never touched.
--
-- 4a: only where the request-level human_prompt is non-empty. An empty
-- human_prompt is NOT always the better value: migration
-- 20260609000001_fix_mcp_tool_response_operator_turns deliberately set some
-- turns to inherit their predecessor's prompt while clearing the start
-- request's human_prompt to '' — adopting '' here would undo that repair.
UPDATE memory.operator_turns ot
SET user_prompt = r.human_prompt
FROM requests.requests r
WHERE r.id = ot.start_request_id
  AND r.human_prompt <> ''
  AND ot.user_prompt IS DISTINCT FROM r.human_prompt;

-- 4b: where human_prompt IS empty, clear the turn prompt only when the
-- stored value is recognizably raw harness injection (the pre-b54f274
-- last_user_message shape) — inherited plain-prose prompts from the
-- 20260609000001 repair don't match and are preserved.
UPDATE memory.operator_turns ot
SET user_prompt = ''
FROM requests.requests r
WHERE r.id = ot.start_request_id
  AND r.human_prompt = ''
  AND ot.user_prompt ~ '^[[:space:]]*(<system-reminder>|<task-notification>|[\{\[])';

-- Step 5: repair local-command / slash-command envelope prompts, mirroring
-- the Rust extractor's new rules:
--   a) command-name present, no local-command envelope  -> command + args
--      (the untagged remainder is the harness-expanded skill body);
--   b) envelope present, residue text after stripping   -> the residue
--      (the operator typed their prompt below the command noise);
--   c) envelope present, no residue                     -> command + args
--      (never an empty prompt, never the stdout dump).
-- Postgres ARE note: each pattern's first quantifier is non-greedy, so the
-- whole RE matches non-greedily; '.' spans newlines by default (no 'n' flag).
WITH affected AS (
    SELECT id, user_prompt AS up
    FROM memory.operator_turns
    WHERE user_prompt ~ '<(local-command-caveat|local-command-stdout|command-name)>'
), parsed AS (
    SELECT id,
        -- btrim's default trim set is the SPACE character only — newlines
        -- between the stripped tags would survive a bare btrim(), leaving
        -- whitespace-garbage residues that defeat the `residue = ''` check.
        btrim(
            regexp_replace(
            regexp_replace(
            regexp_replace(
            regexp_replace(
            regexp_replace(up,
                '<local-command-caveat>.*?</local-command-caveat>', '', 'g'),
                '<local-command-stdout>.*?</local-command-stdout>', '', 'g'),
                '<command-message>.*?</command-message>', '', 'g'),
                '<command-args>.*?</command-args>', '', 'g'),
                '<command-name>.*?</command-name>', '', 'g'),
            E' \t\r\n'
        ) AS residue,
        (regexp_match(up, '<command-name>(.*?)</command-name>'))[1] AS cmd_name,
        (regexp_match(up, '<command-args>(.*?)</command-args>'))[1] AS cmd_args,
        up ~ '<(local-command-caveat|local-command-stdout)>' AS has_local
    FROM affected
)
UPDATE memory.operator_turns ot
SET user_prompt = CASE
        WHEN p.cmd_name IS NOT NULL AND (NOT p.has_local OR p.residue = '')
            THEN btrim(btrim(p.cmd_name, E' \t\r\n') || ' ' || COALESCE(btrim(p.cmd_args, E' \t\r\n'), ''), E' \t\r\n')
        ELSE p.residue
    END
FROM parsed p
WHERE p.id = ot.id;
