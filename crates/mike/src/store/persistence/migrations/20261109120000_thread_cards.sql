-- One rendered card per work thread: what memory-keeper hands a session that
-- continues the thread. Built from work.threads, work.thread_state, the newest
-- handoff and the decisions after it in work.thread_log, the decision and
-- finding notes of the thread's sessions, their open dossier promises and the
-- unmet asks in their live turn verdicts.
--
-- A card is derived data and is rebuilt whenever its inputs change. `text` is
-- the rendered card, at most 1,500 bytes. `card` is the same content as
-- structured sections, `refs` the reference tokens the text carries
-- (thread:<slug>, entry:<id>, note:<uuid>, promise:<id>, turn:<id>).
-- `content_hash` is the sha256 of `text`: the refresh worker writes a row only
-- when it differs. It is a plain column on purpose, never indexed. The
-- embedding column arrives with the embed worker's knowledge source kind.

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
    'One rendered card per work thread, rebuilt by memory-keeper when the thread, its entries, its sessions'' notes, promises or verdicts change. Derived data: safe to delete, rebuilt by the next sweep.';
COMMENT ON COLUMN work.thread_cards.card IS
    'The card as structured sections, in the order the text renders them.';
COMMENT ON COLUMN work.thread_cards.text IS
    'The rendered card, at most 1,500 bytes, framed as data with a reference token on every item.';
COMMENT ON COLUMN work.thread_cards.content_hash IS
    'sha256 of text, hex. Not indexed: the refresh worker compares it on the row it already read.';
COMMENT ON COLUMN work.thread_cards.refs IS
    'Array of the reference tokens the text carries.';
COMMENT ON COLUMN work.thread_cards.source_max_at IS
    'Newest timestamp among the inputs the card was built from.';
COMMENT ON COLUMN work.thread_cards.built_at IS
    'When the text last changed. A rebuild that renders the same text leaves the row alone.';
