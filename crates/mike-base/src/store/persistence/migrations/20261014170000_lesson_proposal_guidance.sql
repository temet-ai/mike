CREATE TABLE memory.lesson_proposal_guidance (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    verdict_id bigint NOT NULL,
    project_id bigint NOT NULL REFERENCES projects.projects(id),
    reference text NOT NULL,
    source_kind text NOT NULL CHECK (source_kind IN ('memory', 'skill')),
    name text NOT NULL CHECK (btrim(name) <> ''),
    relative_path text NOT NULL,
    similarity double precision,
    cited boolean NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (verdict_id, reference)
);
CREATE INDEX lesson_proposal_guidance_project_created
    ON memory.lesson_proposal_guidance (project_id, created_at);
COMMENT ON TABLE memory.lesson_proposal_guidance IS
    'The Claude Code memory files and project skills the lesson proposer was shown beside one correction, and whether its stated reason named the file. One row per correction and shown document.';
COMMENT ON COLUMN memory.lesson_proposal_guidance.cited IS
    'True when the proposer''s reason names the file path, the file name without extension, or the skill name.';
