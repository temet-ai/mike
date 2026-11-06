-- Add count_tokens_sidecar turn origin and repair spurious operator turns
-- created by Claude Code's /v1/messages/count_tokens token-counting probe.
--
-- Background: Claude Code fires POST /v1/messages/count_tokens before sending
-- large payloads to check token usage. The response is {"input_tokens": N} —
-- never a real chat message. These requests have no system prompt and carry
-- a single user message whose content is a raw file dump (e.g. Rust source).
-- They were misclassified as turn_origin='primary' and opened spurious
-- operator turns whose user_prompt shows raw file contents.
--
-- Fix:
--   1. Register the new turn origin code.
--   2. Reclassify the misidentified request rows.
--   3. Delete the spurious operator_turns rows.

-- Step 1: register the new origin.
-- Use explicit id=8 (OVERRIDING SYSTEM VALUE) consistent with the baseline
-- migration that seeded ids 1-6 without advancing the identity sequence.
-- setval advances the sequence past the explicit id so future plain INSERTs
-- don't collide.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (8, 'count_tokens_sidecar')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 8);

-- Step 2: reclassify the request rows.
-- The authoritative signal is the path column — '/v1/messages/count_tokens'
-- cannot be a genuine user turn. We restrict to current turn_origin='primary'
-- to avoid touching rows that were already correctly classified.
UPDATE requests.requests r
SET turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'count_tokens_sidecar')
WHERE r.turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'primary')
  AND r.path = '/v1/messages/count_tokens';

-- Step 3: remove the spurious operator turns.
-- Their start_request_id now points to a count_tokens_sidecar request.
-- These turns have user_prompt = raw file content, not user-typed text.
-- Deleting is correct: the count_tokens probe is not a conversational turn
-- and there is no real predecessor prompt to inherit.
DELETE FROM memory.operator_turns t
WHERE t.start_request_id IN (
    SELECT r.id
    FROM requests.requests r
    JOIN requests.turn_origins to_code ON to_code.id = r.turn_origin_id
    WHERE to_code.code = 'count_tokens_sidecar'
);
