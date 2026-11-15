ALTER TABLE memory.lessons
    ADD COLUMN revision integer NOT NULL DEFAULT 1;

COMMENT ON COLUMN memory.lessons.revision IS
    'Starts at 1 and goes up by one each time the lesson text changes. The tag [lesson:<id> rev:<revision>] in a request tells the proxy which wording of the lesson the session already holds.';

CREATE FUNCTION memory.bump_lesson_revision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.revision := OLD.revision + 1;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_lessons_revision
    BEFORE UPDATE OF text ON memory.lessons
    FOR EACH ROW WHEN (OLD.text IS DISTINCT FROM NEW.text)
    EXECUTE FUNCTION memory.bump_lesson_revision();

CREATE TABLE memory.lesson_routes (
    lesson_id  bigint PRIMARY KEY REFERENCES memory.lessons(id) ON DELETE CASCADE,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    route      text NOT NULL CHECK (route IN ('standing', 'injection', 'covered')),
    covered_by text,
    revision   integer NOT NULL,
    decided_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT lesson_routes_covered_by_consistent
        CHECK ((route = 'covered') = (covered_by IS NOT NULL))
);

CREATE INDEX idx_lesson_routes_project ON memory.lesson_routes (project_id);

COMMENT ON TABLE memory.lesson_routes IS
    'The one way each accepted lesson reaches sessions of its project, decided by the learning promoter on a machine that holds a checkout. standing: written into the checkout''s .claude/rules/mike-lessons.md; injection: left for the proxy to deliver turn by turn; covered: a hand-written CLAUDE.md, AGENTS.md or rules file already says it, so it is neither written nor injected. A lesson without a row is injection.';

COMMENT ON COLUMN memory.lesson_routes.covered_by IS
    'Path of the hand-written file that already says the lesson. Set exactly when route is covered.';

COMMENT ON COLUMN memory.lesson_routes.revision IS
    'The lesson revision the route was decided for.';

CREATE TRIGGER trg_lesson_routes_change_generation
    AFTER INSERT OR DELETE ON memory.lesson_routes
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('lessons');

CREATE TRIGGER trg_lesson_routes_change_generation_upd
    AFTER UPDATE ON memory.lesson_routes
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('lessons');
