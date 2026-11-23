-- `proxy.row_changed` (feed table `thread_card_busy_sources`) when a live
-- verdict is inserted into `memory.turn_verdicts` for a session linked to a
-- work thread. Verdicts are append-only, so inserts are enough. The function
-- comes from 20261120170520; see it for why the memory keeper waits on this.

CREATE OR REPLACE TRIGGER trg_turn_verdicts_thread_card_busy_inserted
AFTER INSERT ON memory.turn_verdicts
FOR EACH ROW
WHEN (NEW.mode = 'live')
EXECUTE FUNCTION mike.notify_row_changed_thread_card_busy_session();
