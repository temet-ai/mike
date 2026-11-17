-- An engineer run the Claude API refused (rate or usage limit, overload) ends as 'unavailable'.
-- It is not an attempt on its problem or follow-up, and the engineer waits before the next start.
ALTER TABLE wright.investigations DROP CONSTRAINT IF EXISTS investigations_state_check;
ALTER TABLE wright.investigations ADD CONSTRAINT investigations_state_check
    CHECK (state IN ('starting', 'running', 'finished', 'failed', 'killed', 'reconciled', 'unavailable'));
