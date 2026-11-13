-- Add trigger_point, invocation, pattern to steering.steerers.
-- Reseed false_done (pattern-gated) and refocus (every_turn) rows.
--
-- Column naming: trigger_point not trigger — trigger is a Postgres reserved word.
-- Idempotent: ADD COLUMN IF NOT EXISTS; UPDATE by name (safe on re-run).

-- ─── 1. Add trigger_point column ─────────────────────────────────────────────
-- Which event type fires this steerer. DEFAULT 'response' covers existing rows
-- (refocus, memory_keeper, false_done all fire on completed responses).

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS trigger_point text NOT NULL DEFAULT 'response';

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_trigger_point_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_trigger_point_check
        CHECK (trigger_point IN ('user_prompt', 'response'));

-- ─── 2. Add invocation column ─────────────────────────────────────────────────
-- Whether the steerer judges every matched event ('every_turn') or only when
-- pattern keywords appear in the evaluated text ('pattern').
-- DEFAULT 'every_turn' covers refocus and memory_keeper.

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS invocation text NOT NULL DEFAULT 'every_turn';

ALTER TABLE steering.steerers
    DROP CONSTRAINT IF EXISTS steerers_invocation_check;

ALTER TABLE steering.steerers
    ADD CONSTRAINT steerers_invocation_check
        CHECK (invocation IN ('every_turn', 'pattern'));

-- ─── 3. Add pattern column ────────────────────────────────────────────────────
-- Newline-separated keyword/regex list. NULL means no pattern gate (invocation
-- must be 'every_turn'). Not null only when invocation='pattern'.

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS pattern text;

-- ─── 4. Reseed false_done row ─────────────────────────────────────────────────
-- trigger_point='response': evaluates completed assistant responses.
-- invocation='pattern': calls the LLM judge only when a completion-claim keyword
-- appears in the response text (fast pre-filter; the LLM does the precision cut).
-- pattern: completion-claim keywords from false_done.rs COMPLETION_KEYWORDS,
--   one per line, lowercase (compared against to_lowercase() output).
--   Leading space on " done" etc. is intentional — it avoids matching mid-word.

UPDATE steering.steerers
   SET trigger_point = 'response',
       invocation    = 'pattern',
       pattern       = E' done\n done.\n done,\ntests pass\ntest pass\n verified\n complete\n completed\n fixed\n it works\nnow working\nsuccessfully\nthe issue is resolved\nissue resolved\nall tests\ncargo check\ncargo build\nmake check\nmake test\nnextest run'
 WHERE name = 'false-done';

-- ─── 5. Reseed refocus row ────────────────────────────────────────────────────
-- trigger_point='response': evaluates completed assistant responses.
-- invocation='every_turn': topic drift is not keyword-detectable; judge every
-- response against the session goal.
-- pattern=NULL: not used for every_turn invocation.

UPDATE steering.steerers
   SET trigger_point = 'response',
       invocation    = 'every_turn',
       pattern       = NULL
 WHERE name = 'refocus';
