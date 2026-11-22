-- A deleted session sends its `sessions` row_changed notice too, so readers
-- that keep a session's identity in memory drop it. The update trigger only
-- fires on a changed column; a delete had no notice at all.
DROP TRIGGER IF EXISTS trg_sessions_row_changed_delete ON sessions.sessions;
CREATE TRIGGER trg_sessions_row_changed_delete
AFTER DELETE ON sessions.sessions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_sessions();
