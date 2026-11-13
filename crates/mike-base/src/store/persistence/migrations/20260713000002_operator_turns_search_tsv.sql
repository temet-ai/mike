-- recall_turns_by_fts (the FTS fallback for POST /api/recall/similar-turns)
-- ranked via pg_trgm word_similarity() against claude_proxy.operator_turn_text_v,
-- a view whose COALESCE/CASE + self-join + LATERAL shape blocks index use
-- entirely, so the view had to reconstruct every candidate turn's text from
-- scratch (measured 11.35s on a 4k-turn project). Re-pointing word_similarity
-- at the raw base columns didn't help either: word_similarity's GIN support is
-- lossy on multi-KB documents (avg final_assistant_text = 4.9KB) — the bitmap
-- scan returned ~50% of rows as candidates, so the heap recheck touched the
-- same block count as a full scan.
--
-- tsvector + GIN is the properly selective structure for long-document search
-- (exact posting lists per lexeme, not a lossy trigram bitmap): measured 84
-- candidate rows out of ~4k via idx_operator_turns_search_tsv vs ~50% via the
-- trigram approach. Generated STORED column (not an expression index) because
-- the Rust query layer reads this table with `SELECT *`-shaped projections
-- elsewhere and a stored column keeps the tsvector visible/inspectable via
-- plain SQL rather than only through a specific index's opclass.
--
-- Only covers `user_prompt` / `final_assistant_text` as they sit on THIS row —
-- it does not see the view's two fallbacks (legacy NULL user_prompt resolved
-- from requests.requests.last_user_message; a still-open turn's live reply
-- reconstructed from stream_blocks). The query layer routes those narrow,
-- rare cases through a separate exception path that still reads the view;
-- see recall_turns_by_fts in store/persistence/mod.rs.
--
-- to_tsvector('english', ...) as a GENERATED STORED expression is Postgres's
-- own documented pattern for this exact case (textsearch-tables.html); the
-- config name is resolved once at DDL time, not re-resolved per row.
--
-- Plain (non-CONCURRENTLY) CREATE INDEX, matching this repo's convention for
-- small/medium tables (e.g. 20260713000001_operator_turn_chunks_text_trgm.sql):
-- sqlx runs each migration in its own transaction and CONCURRENTLY cannot run
-- inside one; memory.operator_turns is ~7k rows, well under a second to build,
-- with no active writers during the startup migration window.

ALTER TABLE memory.operator_turns
    ADD COLUMN search_tsv tsvector
    GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(user_prompt, '') || ' ' || coalesce(final_assistant_text, ''))
    ) STORED;

CREATE INDEX idx_operator_turns_search_tsv
    ON memory.operator_turns USING gin (search_tsv);

COMMENT ON COLUMN memory.operator_turns.search_tsv IS
    'Generated tsvector over this row''s own user_prompt/final_assistant_text, '
    'backing the FTS fallback''s fast path. Does not include the '
    'operator_turn_text_v fallback text (legacy NULL user_prompt, still-open '
    'turn live reply) — recall_turns_by_fts unions in a narrow exception path '
    'over the view for those rows.';
