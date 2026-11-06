-- Add next_input_suggestion_sidecar turn origin and repair the spurious
-- operator turns it manufactured (thread: turn-prompt-extraction, inventory
-- 2026-07-02).
--
-- Background: Claude Code's next-input "SUGGESTION MODE" probe re-sends the
-- full conversation history with a synthetic last user message beginning
-- exactly `[SUGGESTION MODE:`, asking the model to predict the operator's
-- next input. It carries the SAME session id as the real conversation and
-- is multi-message (not one-shot), so it fell through every existing
-- sidecar fingerprint and was classified `primary`. That closed the real
-- open operator turn and opened a spurious one: the MCP-tool-JSON heuristic
-- in `extract_human_prompt` blanks content starting with `[`, so the new
-- turn's `user_prompt` was empty, while the probe's own predicted reply won
-- the `stp` LATERAL in `operator_turns.rs` (filtered to
-- `ro.code IN ('primary','primary_first')`) and read back as the operator's
-- `stop_response`. Live data: 570 affected requests, 562 spurious turns.

-- Step 1: register the new origin.
-- Explicit id=12 (OVERRIDING SYSTEM VALUE) — last assigned was id=11
-- ('workflow_subagent', migration 20260618000001). setval advances the
-- sequence past the explicit id so future plain INSERTs don't collide.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (12, 'next_input_suggestion_sidecar')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 12);

-- Step 2: reclassify the request rows.
-- Fingerprint mirrors the Rust extractor: a user-prompt-turn request
-- currently classified `primary` whose extracted `human_prompt` is empty
-- (the bracket-heuristic false negative) and whose raw `last_user_message`
-- opens with the probe's harness envelope. Restricted to current
-- turn_origin='primary' AND turn_type='user_prompt' so correctly-classified
-- rows (and non-turn-opening tool_loop/continuation requests, which never
-- manufacture a spurious turn) are never touched.
UPDATE requests.requests r
SET turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'next_input_suggestion_sidecar')
WHERE r.turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'primary')
  AND r.turn_type_id = (SELECT id FROM requests.turn_types WHERE code = 'user_prompt')
  AND r.human_prompt = ''
  AND r.last_user_message LIKE '[SUGGESTION MODE%';

-- Step 3: remove the spurious operator turns.
-- Their start_request_id now points at a next_input_suggestion_sidecar
-- request. Deleting (not inheriting) follows the monitor_sidecar /
-- count_tokens_sidecar / web_fetch_sidecar precedent: these requests never
-- represented a conversational turn, so the correct model is that they
-- never opened one. operator_turn_chunks cascade; requests.operator_turn_id
-- detaches via ON DELETE SET NULL.
DELETE FROM memory.operator_turns t
WHERE t.start_request_id IN (
    SELECT r.id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id
    WHERE o.code = 'next_input_suggestion_sidecar'
);
