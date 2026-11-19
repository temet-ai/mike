-- Statement-level AFTER triggers fire even when the statement touched no row, so an
-- INSERT ... ON CONFLICT DO NOTHING that adds nothing bumped seat_memory_sources (the
-- launch-brief mail check runs one every few seconds) and an empty DELETE bumped
-- thread_card_sources. These triggers now read their transition table and bump only when
-- a row was really added or removed.
CREATE OR REPLACE FUNCTION memory.bump_change_generation_if_rows() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM changed_rows) THEN
        UPDATE memory.change_generations
           SET generation = generation + 1, changed_at = now()
         WHERE name = ANY (TG_ARGV);
    END IF;
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_launches_seat_memory_generation ON launch.launches;
CREATE TRIGGER trg_launches_seat_memory_generation
    AFTER INSERT ON launch.launches REFERENCING NEW TABLE AS changed_rows
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation_if_rows('seat_memory_sources');

DROP TRIGGER IF EXISTS trg_fleet_events_seat_memory_generation ON launch.fleet_events;
CREATE TRIGGER trg_fleet_events_seat_memory_generation
    AFTER INSERT ON launch.fleet_events REFERENCING NEW TABLE AS changed_rows
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation_if_rows('seat_memory_sources');

DROP TRIGGER IF EXISTS trg_mailbox_messages_seat_memory_generation ON mailbox.messages;
CREATE TRIGGER trg_mailbox_messages_seat_memory_generation
    AFTER INSERT ON mailbox.messages REFERENCING NEW TABLE AS changed_rows
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation_if_rows('seat_memory_sources');

DROP TRIGGER IF EXISTS trg_thread_card_candidates_card_sources ON work.thread_card_candidates;
DROP TRIGGER IF EXISTS trg_thread_card_candidates_card_sources_ins ON work.thread_card_candidates;
DROP TRIGGER IF EXISTS trg_thread_card_candidates_card_sources_del ON work.thread_card_candidates;
CREATE TRIGGER trg_thread_card_candidates_card_sources_ins
    AFTER INSERT ON work.thread_card_candidates REFERENCING NEW TABLE AS changed_rows
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation_if_rows('thread_card_sources');
CREATE TRIGGER trg_thread_card_candidates_card_sources_del
    AFTER DELETE ON work.thread_card_candidates REFERENCING OLD TABLE AS changed_rows
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation_if_rows('thread_card_sources');
