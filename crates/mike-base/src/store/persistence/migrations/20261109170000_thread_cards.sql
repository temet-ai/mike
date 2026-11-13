-- One rendered card per work thread: what memory-keeper hands a session that
-- continues the thread. Built from work.threads, work.thread_state, the newest
-- handoff and the decisions after it in work.thread_log, the decision and
-- finding notes tagged thread:<slug>, the open dossier promises of the
-- thread's sessions and the unmet asks in their live turn verdicts.
--
-- A card is derived data and is rebuilt whenever its inputs change. `text` is
-- the rendered card, at most 1,500 bytes. `card` is the same content as
-- structured sections, `refs` the reference tokens the text carries
-- (thread:<slug>, entry:<id>, note:<uuid>, promise:<id>, turn:<id>).
-- `content_hash` is the sha256 of `text`: the card worker rewrites the content
-- only when it differs. It is a plain column on purpose, never indexed.
--
-- Two change counters in memory.change_generations:
--   thread_cards         bumped when a card's content (card, text, content_hash,
--                        refs) changes. Every machine's snapshot of the cards
--                        reloads when it moves. Advancing source_max_at or
--                        built_at alone leaves it alone, so a rebuild that
--                        renders the same text costs readers nothing.
--   thread_card_sources  bumped when a thread, its state, its log or its
--                        session links change. The card worker reads it every
--                        tick and rebuilds the cards whose sources are newer
--                        than their source_max_at. Bookkeeping writes leave it
--                        alone: an embedding written onto thread_state, or a
--                        session link's last_seen_at moving.

CREATE TABLE IF NOT EXISTS work.thread_cards (
    thread_id      bigint      PRIMARY KEY REFERENCES work.threads(id) ON DELETE CASCADE,
    card           jsonb       NOT NULL,
    text           text        NOT NULL,
    content_hash   text        NOT NULL,
    refs           jsonb       NOT NULL,
    source_max_at  timestamptz NOT NULL,
    built_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT thread_cards_text_bounded CHECK (octet_length(text) <= 1500)
);

COMMENT ON TABLE work.thread_cards IS
    'One rendered card per work thread, rebuilt by memory-keeper when the thread, its entries, its thread-tagged notes, or its sessions'' promises or verdicts change. Derived data: safe to delete, rebuilt by the next pass.';
COMMENT ON COLUMN work.thread_cards.card IS
    'The card as structured sections, in the order the text renders them.';
COMMENT ON COLUMN work.thread_cards.text IS
    'The rendered card, at most 1,500 bytes, with a reference token on every item.';
COMMENT ON COLUMN work.thread_cards.content_hash IS
    'sha256 of text, hex. Not indexed: the card worker compares it on the row it writes.';
COMMENT ON COLUMN work.thread_cards.refs IS
    'Array of the reference tokens the text carries.';
COMMENT ON COLUMN work.thread_cards.source_max_at IS
    'Newest source change the card has been built against. Advanced on every rebuild, including one that renders the same text.';
COMMENT ON COLUMN work.thread_cards.built_at IS
    'When the content last changed. A rebuild that renders the same text leaves it alone.';

INSERT INTO memory.change_generations (name) VALUES ('thread_cards'), ('thread_card_sources')
ON CONFLICT (name) DO NOTHING;

DROP TRIGGER IF EXISTS trg_thread_cards_change_generation ON work.thread_cards;
DROP TRIGGER IF EXISTS trg_thread_cards_change_generation_upd ON work.thread_cards;
CREATE TRIGGER trg_thread_cards_change_generation
    AFTER INSERT OR DELETE ON work.thread_cards
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('thread_cards');
CREATE TRIGGER trg_thread_cards_change_generation_upd
    AFTER UPDATE ON work.thread_cards
    FOR EACH ROW
    WHEN ((OLD.card, OLD.text, OLD.content_hash, OLD.refs)
          IS DISTINCT FROM (NEW.card, NEW.text, NEW.content_hash, NEW.refs))
    EXECUTE FUNCTION memory.bump_change_generation('thread_cards');

DROP TRIGGER IF EXISTS trg_threads_card_sources ON work.threads;
DROP TRIGGER IF EXISTS trg_threads_card_sources_upd ON work.threads;
CREATE TRIGGER trg_threads_card_sources
    AFTER INSERT OR DELETE ON work.threads
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
CREATE TRIGGER trg_threads_card_sources_upd
    AFTER UPDATE ON work.threads
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');

DROP TRIGGER IF EXISTS trg_thread_state_card_sources ON work.thread_state;
DROP TRIGGER IF EXISTS trg_thread_state_card_sources_upd ON work.thread_state;
CREATE TRIGGER trg_thread_state_card_sources
    AFTER INSERT OR DELETE ON work.thread_state
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
CREATE TRIGGER trg_thread_state_card_sources_upd
    AFTER UPDATE ON work.thread_state
    FOR EACH ROW
    WHEN ((OLD.tldr, OLD.where_we_are, OLD.next_up, OLD.blockers, OLD.open_questions,
           OLD.decisions, OLD.files, OLD.updated_at)
          IS DISTINCT FROM (NEW.tldr, NEW.where_we_are, NEW.next_up, NEW.blockers,
                            NEW.open_questions, NEW.decisions, NEW.files, NEW.updated_at))
    EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');

DROP TRIGGER IF EXISTS trg_thread_log_card_sources ON work.thread_log;
DROP TRIGGER IF EXISTS trg_thread_log_card_sources_upd ON work.thread_log;
CREATE TRIGGER trg_thread_log_card_sources
    AFTER INSERT OR DELETE ON work.thread_log
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
CREATE TRIGGER trg_thread_log_card_sources_upd
    AFTER UPDATE ON work.thread_log
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');

DROP TRIGGER IF EXISTS trg_thread_sessions_card_sources ON work.thread_sessions;
DROP TRIGGER IF EXISTS trg_thread_sessions_card_sources_upd ON work.thread_sessions;
CREATE TRIGGER trg_thread_sessions_card_sources
    AFTER INSERT OR DELETE ON work.thread_sessions
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
CREATE TRIGGER trg_thread_sessions_card_sources_upd
    AFTER UPDATE ON work.thread_sessions
    FOR EACH ROW WHEN (OLD.contribution IS DISTINCT FROM NEW.contribution)
    EXECUTE FUNCTION memory.bump_change_generation('thread_card_sources');
