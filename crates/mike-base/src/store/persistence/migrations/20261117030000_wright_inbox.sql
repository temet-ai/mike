-- Wright's inbox (temet-ai/mike#321): what happened to a known fix, a repair or a follow-up is
-- written here when it happens, by the database itself as the row changes, so every writer
-- posts it. The next engineer run for the check reads the unread events in its evidence and
-- marks them read when it stores its finding; it does not query to rediscover them.
CREATE TABLE IF NOT EXISTS wright.inbox (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    check_name  text        NOT NULL,
    problem_id  bigint      REFERENCES wright.problems(id) ON DELETE CASCADE,
    kind        text        NOT NULL CHECK (kind IN ('remedy', 'repair', 'followup')),
    ref_id      bigint      NOT NULL,
    state       text        NOT NULL,
    summary     text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    read_by     bigint      REFERENCES wright.investigations(id) ON DELETE SET NULL,
    read_at     timestamptz,
    CONSTRAINT inbox_read_has_time CHECK ((read_by IS NULL) OR (read_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS wright_inbox_unread ON wright.inbox (check_name, id) WHERE read_at IS NULL;

COMMENT ON TABLE wright.inbox IS
    'Events for Wright''s engineer, written by triggers as known-fix actions, repairs and follow-ups change state; read into the next run''s evidence for the check (temet-ai/mike#321).';

CREATE OR REPLACE FUNCTION wright.inbox_from_repairs() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO wright.inbox (check_name, problem_id, kind, ref_id, state, summary)
    SELECT p.check_name, n.problem_id, 'repair', n.id, n.state,
           format('repair %s of finding %s (%s) is %s%s', n.id, n.finding_id, n.owning_part, n.state,
                  COALESCE(': ' || left(n.reason, 600), '') || COALESCE(' ' || n.pr_url, ''))
      FROM new_rows n
      JOIN old_rows o ON o.id = n.id
      JOIN wright.problems p ON p.id = n.problem_id
     WHERE o.state IS DISTINCT FROM n.state
       AND n.state IN ('gate_failed', 'refused', 'landed', 'deployed', 'healthy');
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION wright.inbox_from_actions() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO wright.inbox (check_name, problem_id, kind, ref_id, state, summary)
    SELECT p.check_name, n.problem_id, 'remedy', n.id, n.state,
           format('known fix %s, action %s on problem %s, is %s%s', n.remedy, n.id, n.problem_id, n.state,
                  COALESCE(': ' || left(n.refusal, 600), ''))
      FROM new_rows n
      JOIN old_rows o ON o.id = n.id
      JOIN wright.problems p ON p.id = n.problem_id
     WHERE o.state IS DISTINCT FROM n.state
       AND n.state IN ('verified', 'failed', 'refused', 'ineffective', 'undone', 'undo_conflict');
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION wright.inbox_from_followups() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO wright.inbox (check_name, problem_id, kind, ref_id, state, summary)
    SELECT n.check_name, NULL, 'followup', n.id, n.state,
           format('follow-up %s (%s, %s) is %s%s%s', n.id, n.kind, left(n.title, 160), n.state,
                  COALESCE(', issue #' || n.issue_number, ''), COALESCE(': ' || left(n.closed_reason, 600), ''))
      FROM new_rows n
      JOIN old_rows o ON o.id = n.id
     WHERE o.state IS DISTINCT FROM n.state
       AND n.check_name IS NOT NULL
       AND n.state IN ('filed', 'handed_off', 'fixed', 'dropped', 'stuck');
    RETURN NULL;
END $$;

DROP TRIGGER IF EXISTS repairs_to_inbox ON wright.repairs;
CREATE TRIGGER repairs_to_inbox AFTER UPDATE ON wright.repairs
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION wright.inbox_from_repairs();

DROP TRIGGER IF EXISTS actions_to_inbox ON wright.actions;
CREATE TRIGGER actions_to_inbox AFTER UPDATE ON wright.actions
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION wright.inbox_from_actions();

DROP TRIGGER IF EXISTS followups_to_inbox ON wright.followups;
CREATE TRIGGER followups_to_inbox AFTER UPDATE ON wright.followups
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION wright.inbox_from_followups();
