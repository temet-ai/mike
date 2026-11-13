-- Add monitor_sidecar turn origin and repair spurious operator turns created
-- by background session-monitoring skills (e.g. harden-session-work).
--
-- Background: one-shot requests from skills like harden-session-work carry a
-- system prompt starting with "A user kicked off a Claude Code agent to do a
-- coding task and walked away."  Their single user message contains plain-text
-- skill output ("Current state: done (for 31m)\n...") — never user-typed.
-- These were misclassified as turn_origin='primary' and opened spurious
-- operator turns whose user_prompt shows the skill dump instead of the real
-- user prompt.
--
-- Fix:
--   1. Register the new turn origin code.
--   2. Update the misclassified request rows to turn_origin='monitor_sidecar'.
--   3. Delete the spurious operator_turns rows whose start_request_id now
--      resolves to monitor_sidecar (no predecessor turn to inherit from —
--      the skill output is not a user prompt at all; deleting is correct here
--      rather than inheriting, because these turns have no conversational
--      meaning and there may be no real predecessor turn in the same session).

-- Step 1: register the new origin.
-- Use explicit id=7 (OVERRIDING SYSTEM VALUE) consistent with the baseline
-- migration that seeded ids 1-6 without advancing the identity sequence.
-- setval advances the sequence past the explicit id so future plain INSERTs
-- don't collide.
INSERT INTO requests.turn_origins (id, code) OVERRIDING SYSTEM VALUE VALUES (7, 'monitor_sidecar')
ON CONFLICT DO NOTHING;
SELECT setval('requests.turn_origins_id_seq', 7);

-- Step 2: reclassify the request rows.
-- We identify them by joining to request_bodies and checking the system prompt
-- text (stored in system_prompt_blocks via the system_prompts FK).  However,
-- the system prompt text lives in system_prompt_blocks and requires a multi-hop
-- join.  The simpler discriminator that is safe here: the human_prompt on the
-- request row is the plain-text skill dump, which always starts with
-- "Current state:" — a string that cannot appear in a genuine human-typed
-- turn (those are stripped of hook injections, but "Current state:" is not a
-- hook; it is skill-generated text that was never typed).
UPDATE requests.requests r
SET turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'monitor_sidecar')
WHERE r.turn_origin_id = (SELECT id FROM requests.turn_origins WHERE code = 'primary')
  AND r.human_prompt LIKE 'Current state:%';

-- Step 3: remove the spurious operator turns.
-- These turns have user_prompt = the skill dump.  Unlike the MCP-JSON fix
-- (which patches user_prompt to inherit the predecessor), here we delete
-- because:
--   a) The content is skill-generated, not a real user interaction.
--   b) There is no meaningful predecessor prompt to inherit in most cases.
--   c) The turn_type was 'user_prompt' only because the origin was wrong;
--      the correct model is: these requests never opened a turn at all.
DELETE FROM memory.operator_turns t
WHERE t.user_prompt LIKE 'Current state:%';
