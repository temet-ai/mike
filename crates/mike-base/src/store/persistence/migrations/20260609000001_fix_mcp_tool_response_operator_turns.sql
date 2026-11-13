-- Fix operator turns whose user_prompt contains raw MCP tool-response JSON.
--
-- Background: requests where a single user message carries raw JSON string content
-- (e.g. `{"session":{"limit":10,...}}`) were misclassified as new operator turns
-- instead of tool-loop continuations.  This left existing turns with a JSON blob
-- in user_prompt and the underlying request rows with matching human_prompt /
-- last_user_message values.
--
-- Fix:
--   1. Clear human_prompt / last_user_message on the start request rows (must run
--      FIRST, while operator_turns still has the JSON pattern to identify them).
--   2. Overwrite operator_turns.user_prompt with the predecessor turn's real prompt
--      in the same session (or NULL if the spurious turn is the first in the session,
--      which causes the view fallback to return the now-empty last_user_message).

-- Step 1: clear the request-level fields so the view's COALESCE fallback path is
-- also clean for any turn that has no predecessor.
UPDATE requests.requests r
SET
    human_prompt      = '',
    last_user_message = ''
WHERE r.id IN (
    SELECT t.start_request_id
    FROM memory.operator_turns t
    WHERE t.user_prompt IS NOT NULL
      AND t.user_prompt ~ '^\s*[\{\[]'
);

-- Step 2: patch operator_turns.user_prompt to inherit the predecessor turn's
-- real user_prompt within the same session.
UPDATE memory.operator_turns AS t
SET user_prompt = (
    SELECT p.user_prompt
    FROM memory.operator_turns p
    WHERE p.session_id = t.session_id
      AND p.id          < t.id
      AND (p.user_prompt IS NULL OR p.user_prompt !~ '^\s*[\{\[]')
    ORDER BY p.id DESC
    LIMIT 1
)
WHERE t.user_prompt IS NOT NULL
  AND t.user_prompt ~ '^\s*[\{\[]';
