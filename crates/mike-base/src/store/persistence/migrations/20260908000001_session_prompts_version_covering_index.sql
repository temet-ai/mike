-- `PersistHandle::get_prompt_class_versions`'s `windowed` CTE scans every
-- `system_prompts.session_prompts` row for a `(lane, subagent_persona)`
-- pair and computes two `substring(sp.blocks->0->>'text' FROM ...)`
-- expressions to parse the Claude Code release/build out of the captured
-- system prompt. `blocks` is a schemaless JSONB capture payload (30-60 KB
-- per row on the live `main`/no-persona class), so evaluating those two
-- expressions detoasts the whole column on every one of the table's 3,895
-- rows just to read block 0 — measured at 149ms best-of-3 against the
-- live class (~1,250 rows), against this route's 50ms read-tier budget
-- (`scripts/db-code-graders/perf-evidence/2026-09-08/prompt-class-versions-main_before.sql`).
--
-- An expression index alone cannot fix this. Verified empirically against
-- this Postgres (16.15): index-only-scan eligibility
-- (`check_index_only`/`pull_varattnos`) requires every RAW column
-- referenced anywhere in the query — target list or WHERE — to be present
-- in the index as a plain (non-expression) key or INCLUDE column, even
-- when an exact-matching expression is *also* an index key. A
-- `(lane, substring(blocks->0->>'text' FROM ...))` expression index, with
-- the query still reading `sp.blocks` to compute `release`/`build`, was
-- measured forcing `enable_bitmapscan/seqscan = off`: the planner still
-- produced a plain `Index Scan` (never `Index Only Scan`), with the same
-- ~17.6k buffer hits (the full TOASTed `blocks` column, detoasted per
-- row) as the un-indexed query — no improvement at all. Putting the raw
-- `blocks` column itself in the index (as key or INCLUDE) does not help
-- either: `blocks` is exactly the large, per-row-toasted payload this
-- migration exists to stop reading, so copying it into the index doubles
-- the storage and pays the same detoast cost from a second location.
--
-- The fix promotes `release`/`build` to real, WRITE-time-materialized
-- columns instead of a read-time expression, following the same pattern
-- `requests.responses.input_tokens` etc. use for `token_usage` JSONB
-- (`20260604000001_baseline.sql`) and `memory.operator_turns.search_tsv`
-- uses for FTS (`20260713000002_operator_turns_search_tsv.sql`): a
-- `GENERATED ALWAYS AS (...) STORED` column is a genuine heap column with
-- no `blocks` reference in downstream queries, so it can be indexed like
-- any other plain column and served index-only. `cc_release`/`cc_build`
-- are computed once per row (at insert, and once here for the table's
-- existing rows) rather than once per read.
--
-- `subagent_persona` is included in the index too — not because it is
-- large (it is a short text value, cheap either way), but because the
-- same raw-column rule applies to it: `get_prompt_class_versions` filters
-- on `COALESCE(subagent_persona, '') = COALESCE($2::text, '')` rather than
-- the `IS NOT DISTINCT FROM $2` form the other prompt-class queries use —
-- verified empirically that a bound `IS NOT DISTINCT FROM` parameter
-- cannot use a btree index at all (plans as a post-scan `Filter`, never an
-- `Index Cond`), so the `COALESCE` rewrite is what makes the persona
-- filter an equality the leading index columns can serve.
-- `extract_subagent_persona` (`extract/request/turn_origin_classify.rs`)
-- only ever returns `None` or a non-empty trimmed string, so the
-- empty-string sentinel never collides with a real persona.
--
-- Plain (non-CONCURRENTLY) CREATE INDEX and a same-migration ADD COLUMN
-- table rewrite: the table is small (3,895 rows, 19 MB) and sqlx runs
-- each migration inside its own transaction, matching this repo's
-- established convention for tables of this size (see
-- 20260713000002_operator_turns_search_tsv.sql,
-- 20260716010000_system_prompt_firing_rules_request_index.sql).

ALTER TABLE system_prompts.session_prompts
    ADD COLUMN cc_release text GENERATED ALWAYS AS (
        substring(blocks->0->>'text' from 'cc_version=(\d+\.\d+\.\d+)')
    ) STORED,
    ADD COLUMN cc_build text GENERATED ALWAYS AS (
        substring(blocks->0->>'text' from 'cc_version=([^;\s]+)')
    ) STORED;

COMMENT ON COLUMN system_prompts.session_prompts.cc_release IS
    'Three-segment Claude Code release (e.g. 2.1.263) parsed from block 0''s cc_version= billing header at write time. Generated from blocks so PersistHandle::get_prompt_class_versions can read it without detoasting blocks; NULL when block 0 carries no such header, same as the read-time expression it replaces.';
COMMENT ON COLUMN system_prompts.session_prompts.cc_build IS
    'Full cc_version= value (release plus build tag, e.g. 2.1.263.abc1) parsed from block 0''s billing header at write time. Generated sibling of cc_release — see that column''s comment.';

CREATE INDEX IF NOT EXISTS idx_session_prompts_lane_persona_version
    ON system_prompts.session_prompts (
        lane,
        (COALESCE(subagent_persona, ''))
    )
    INCLUDE (subagent_persona, cc_release, cc_build, created_at, request_id, session_id);

COMMENT ON INDEX system_prompts.idx_session_prompts_lane_persona_version IS
    'Covers PersistHandle::get_prompt_class_versions: with cc_release/cc_build materialized (see column comments), this index answers the windowed CTE via an index-only scan that never touches blocks. Query must filter with COALESCE(subagent_persona, '''') = COALESCE($2::text, '''') to match the expression key.';
