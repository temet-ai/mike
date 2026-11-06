INSERT INTO memory.lesson_archived_reasons (code) VALUES ('not_a_lesson')
ON CONFLICT (code) DO NOTHING;

INSERT INTO memory.lesson_relation_methods (code) VALUES ('write_time'), ('rewritten')
ON CONFLICT (code) DO NOTHING;

CREATE TABLE memory.lesson_shape_findings (
    lesson_id             bigint PRIMARY KEY REFERENCES memory.lessons(id) ON DELETE CASCADE,
    shape                 text NOT NULL CHECK (shape IN ('rule', 'status', 'over_length')),
    detail                text NOT NULL,
    chars                 integer NOT NULL CHECK (chars >= 0),
    found_at              timestamptz NOT NULL DEFAULT now(),
    attempts              integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error            text,
    last_attempt_at       timestamptz,
    resolution            text CHECK (resolution IN ('none_needed', 'rewritten', 'not_a_lesson', 'kept')),
    resolved_at           timestamptz,
    replacement_lesson_id bigint REFERENCES memory.lessons(id) ON DELETE SET NULL,
    CONSTRAINT lesson_shape_findings_resolution_consistent
        CHECK ((resolution IS NULL) = (resolved_at IS NULL)),
    CONSTRAINT lesson_shape_findings_rule_needs_nothing
        CHECK ((shape = 'rule') = COALESCE(resolution = 'none_needed', false))
);

CREATE INDEX idx_lesson_shape_findings_unresolved
    ON memory.lesson_shape_findings (found_at, lesson_id)
    WHERE resolution IS NULL;

COMMENT ON TABLE memory.lesson_shape_findings IS
    'One row per lesson that has been checked for rule shape: a durable instruction in the imperative '
    'with its trigger and what to do, not a status report about a session. shape is rule, status '
    '(opens with a status word, carries a session uuid or commit sha as its subject, or is a '
    'verifier: carryover note) or over_length (longer than the lessons.shape.max_chars setting). A '
    'lesson with an unresolved non-rule row (resolution IS NULL) is not consolidated to daily, not '
    'auto-promoted and, while raw, not injected. The shape pass on the lesson proposer tick '
    'rewrites it into one or more rules (resolution rewritten, replacement_lesson_id set, a '
    'confirmed supersedes relation of method rewritten from each replacement to the original, the '
    'original archived as superseded with its text kept), moves it out of lessons when it is a '
    'status note (resolution not_a_lesson, archived as not_a_lesson, a handover note written to '
    'its session when it has one), or gives up after three failed attempts (resolution kept). kept '
    'is also written at once when the lesson is recorded while the judge_lessons toggle is off, '
    'and by the nightly run for a hold open for 14 days.';

COMMENT ON COLUMN memory.lesson_shape_findings.chars IS
    'Length of the lesson text in characters when it was checked.';

COMMENT ON COLUMN memory.lesson_shape_findings.attempts IS
    'Rewrite attempts that failed: a model call failed or its answer did not pass the shape check. '
    'The shape pass waits 2^attempts hours after each failure and keeps the lesson as it is after the third.';
