CREATE TABLE memory.harvest_runs (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id        bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    session_id        bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    request_id        bigint NOT NULL,
    tags_found        integer NOT NULL,
    lessons_written   integer NOT NULL,
    lessons_skipped   integer NOT NULL,
    lessons_truncated integer NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX harvest_runs_created_at_idx ON memory.harvest_runs (created_at);

COMMENT ON TABLE memory.harvest_runs IS
    'One row per finished model reply in which the memory harvester found at least one '
    '<mem:update> tag (mike#611): how many tags it found, how many became lessons, how many '
    'were skipped (a duplicate lesson, or a failed write) and how many bodies were cut to '
    'max_lesson_bytes. Replies with no tag write no row. The weekly learning audit sums these '
    'to show whether sessions use the grammar. request_id has no foreign key because the '
    'retention sweep deletes old requests and this record is meant to outlive them.';
