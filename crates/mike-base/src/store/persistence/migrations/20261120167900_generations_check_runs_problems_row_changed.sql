-- `proxy.row_changed` for memory.change_generations, wright.check_runs and
-- wright.problems, so three kinds of reader wait for a change instead of
-- asking the database on a timer.
--
-- memory.change_generations: the memory keeper, seat memory, lesson and
-- knowledge workers read a generation every few seconds to see whether
-- their sources moved, about 3,500 reads an hour across the machines, nearly
-- all unchanged. Each reader now keeps the generation it read until this
-- notice names a bumped generation. The table is bumped a few hundred times
-- an hour, one notice per bump.
--
-- wright.check_runs: the remedies worker read max(id) every 15 s on every
-- machine (1,200 an hour) to learn whether a check had run. A new run now
-- arrives as a notice. About 200 runs an hour.
--
-- wright.problems: the repair follow asked every minute whether a deployed
-- repair's problem had become healthy. It now waits for a problem's state to
-- change. Problems change state a few times an hour.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_change_generations()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'change_generations',
        'id',         NEW.name,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_change_generations_row_changed
AFTER INSERT ON memory.change_generations
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_change_generations();

CREATE OR REPLACE TRIGGER trg_change_generations_row_changed_update
AFTER UPDATE OF generation ON memory.change_generations
FOR EACH ROW WHEN (OLD.generation IS DISTINCT FROM NEW.generation)
EXECUTE FUNCTION mike.notify_row_changed_change_generations();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_check_runs()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_check_runs',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_check_runs_row_changed
AFTER INSERT ON wright.check_runs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_check_runs();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_problems()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_problems',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_problems_row_changed
AFTER INSERT ON wright.problems
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_problems();

CREATE OR REPLACE TRIGGER trg_wright_problems_row_changed_update
AFTER UPDATE OF state, healthy_at ON wright.problems
FOR EACH ROW WHEN (OLD.state IS DISTINCT FROM NEW.state
                   OR OLD.healthy_at IS DISTINCT FROM NEW.healthy_at)
EXECUTE FUNCTION mike.notify_row_changed_wright_problems();
