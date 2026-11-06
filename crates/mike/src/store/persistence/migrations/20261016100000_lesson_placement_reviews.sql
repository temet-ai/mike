CREATE TABLE memory.lesson_placement_reviews (
    lesson_id  bigint PRIMARY KEY REFERENCES memory.lessons(id) ON DELETE CASCADE,
    decision   text NOT NULL CHECK (decision IN ('accepted', 'declined')),
    decided_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.lesson_placement_reviews IS
    'The user''s decision on a lesson placed as a rule or an agent statement, made on the Proposals '
    'page: accepted (the user wants it in the rule or agent file the page names) or declined. A '
    'lesson with no row has not been decided. Clearing a decision deletes the row.';
