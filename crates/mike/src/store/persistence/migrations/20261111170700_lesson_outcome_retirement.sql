-- Lessons retired from supply by their outcome record or by an adjudicated
-- contradiction (issue #168).
--
-- A retired lesson is no longer supplied with requests but keeps its row, its
-- stage, its proposal evidence (the link to its source corrections) and its
-- place in knowledge search. Retirement is not archiving: archiving moves
-- evidence to the superseding lesson and drops the lesson from search.
--
-- retired_kind: 'outcome' when the overnight verdicts on its supplies
-- (memory.supplied_item_verdicts, #164) fell below the floor with enough
-- judged supplies; 'superseded' when a newer lesson contradicts it and the
-- adjudicator agreed. retired_by_lesson_id names that newer lesson.

ALTER TABLE memory.lessons
    ADD COLUMN IF NOT EXISTS retired_at timestamptz NULL,
    ADD COLUMN IF NOT EXISTS retired_kind text NULL,
    ADD COLUMN IF NOT EXISTS retired_reason text NULL,
    ADD COLUMN IF NOT EXISTS retired_by_lesson_id bigint NULL REFERENCES memory.lessons(id) ON DELETE SET NULL;

ALTER TABLE memory.lessons DROP CONSTRAINT IF EXISTS lessons_retired_kind_check;
ALTER TABLE memory.lessons ADD CONSTRAINT lessons_retired_kind_check
    CHECK (retired_kind IS NULL OR retired_kind IN ('outcome', 'superseded'));

ALTER TABLE memory.lessons DROP CONSTRAINT IF EXISTS lessons_retired_shape;
ALTER TABLE memory.lessons ADD CONSTRAINT lessons_retired_shape
    CHECK ((retired_at IS NULL) = (retired_kind IS NULL)
       AND (retired_at IS NULL) = (retired_reason IS NULL));

COMMENT ON COLUMN memory.lessons.retired_at IS
    'When the lesson was retired from supply. NULL while it may be supplied. A retired lesson stays searchable and keeps its evidence.';
COMMENT ON COLUMN memory.lessons.retired_kind IS
    'outcome: its supply verdicts fell below lesson_outcome.retire_floor over at least lesson_outcome.min_judged_supplies judged supplies. superseded: an adjudicated contradiction by a newer lesson.';
COMMENT ON COLUMN memory.lessons.retired_reason IS
    'Why it was retired, in words, with the counts or the contradicting lesson.';
COMMENT ON COLUMN memory.lessons.retired_by_lesson_id IS
    'The newer lesson whose adjudicated contradiction retired this one; NULL for retirement by outcome.';
