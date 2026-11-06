-- A change counter per derived dataset, bumped by triggers on the tables the dataset reads.
-- Workers that cache a dataset in memory read one row by primary key to learn whether to
-- reload, instead of hashing the source tables on every tick. The triggers are row-level and
-- UPDATE fires only when a watched value really changed, so no-op writes (re-projection,
-- upserts that change nothing, last_seen bumps) leave the counters alone.
CREATE TABLE IF NOT EXISTS memory.change_generations (
    name       text PRIMARY KEY,
    generation bigint NOT NULL DEFAULT 0,
    changed_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO memory.change_generations (name) VALUES ('lessons'), ('session_projects')
ON CONFLICT (name) DO NOTHING;

CREATE OR REPLACE FUNCTION memory.bump_change_generation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE memory.change_generations
       SET generation = generation + 1, changed_at = now()
     WHERE name = ANY (TG_ARGV);
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_lessons_change_generation ON memory.lessons;
DROP TRIGGER IF EXISTS trg_lessons_change_generation_upd ON memory.lessons;
CREATE TRIGGER trg_lessons_change_generation
    AFTER INSERT OR DELETE ON memory.lessons
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('lessons');
CREATE TRIGGER trg_lessons_change_generation_upd
    AFTER UPDATE ON memory.lessons
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('lessons');

DROP TRIGGER IF EXISTS trg_lesson_relations_change_generation ON memory.lesson_relations;
DROP TRIGGER IF EXISTS trg_lesson_relations_change_generation_upd ON memory.lesson_relations;
CREATE TRIGGER trg_lesson_relations_change_generation
    AFTER INSERT OR DELETE ON memory.lesson_relations
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('lessons');
CREATE TRIGGER trg_lesson_relations_change_generation_upd
    AFTER UPDATE ON memory.lesson_relations
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('lessons');

DROP TRIGGER IF EXISTS trg_lesson_shape_findings_change_generation ON memory.lesson_shape_findings;
DROP TRIGGER IF EXISTS trg_lesson_shape_findings_change_generation_upd ON memory.lesson_shape_findings;
CREATE TRIGGER trg_lesson_shape_findings_change_generation
    AFTER INSERT OR DELETE ON memory.lesson_shape_findings
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('lessons');
CREATE TRIGGER trg_lesson_shape_findings_change_generation_upd
    AFTER UPDATE ON memory.lesson_shape_findings
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
    EXECUTE FUNCTION memory.bump_change_generation('lessons');

DROP TRIGGER IF EXISTS trg_projects_path_change_generation ON projects.projects;
DROP TRIGGER IF EXISTS trg_projects_path_change_generation_upd ON projects.projects;
CREATE TRIGGER trg_projects_path_change_generation
    AFTER INSERT OR DELETE ON projects.projects
    FOR EACH ROW EXECUTE FUNCTION memory.bump_change_generation('lessons', 'session_projects');
CREATE TRIGGER trg_projects_path_change_generation_upd
    AFTER UPDATE OF project_path ON projects.projects
    FOR EACH ROW WHEN (OLD.project_path IS DISTINCT FROM NEW.project_path)
    EXECUTE FUNCTION memory.bump_change_generation('lessons', 'session_projects');

DROP TRIGGER IF EXISTS trg_sessions_project_change_generation ON sessions.sessions;
DROP TRIGGER IF EXISTS trg_sessions_project_change_generation_del ON sessions.sessions;
DROP TRIGGER IF EXISTS trg_sessions_project_change_generation_upd ON sessions.sessions;
CREATE TRIGGER trg_sessions_project_change_generation
    AFTER INSERT ON sessions.sessions
    FOR EACH ROW
    WHEN (NEW.location_uuid IS NOT NULL OR NEW.project_resolution = 'operator')
    EXECUTE FUNCTION memory.bump_change_generation('session_projects');
CREATE TRIGGER trg_sessions_project_change_generation_del
    AFTER DELETE ON sessions.sessions
    FOR EACH ROW
    WHEN (OLD.location_uuid IS NOT NULL OR OLD.project_resolution = 'operator')
    EXECUTE FUNCTION memory.bump_change_generation('session_projects');
CREATE TRIGGER trg_sessions_project_change_generation_upd
    AFTER UPDATE OF project_id, location_uuid, project_resolution ON sessions.sessions
    FOR EACH ROW
    WHEN ((OLD.project_id, OLD.location_uuid, OLD.project_resolution)
          IS DISTINCT FROM (NEW.project_id, NEW.location_uuid, NEW.project_resolution))
    EXECUTE FUNCTION memory.bump_change_generation('session_projects');

-- Lessons also leave the injectable set by time; counting the expired ones through this index
-- tells the worker an expiry passed without a write.
CREATE INDEX IF NOT EXISTS idx_lessons_expires_at
    ON memory.lessons (expires_at) WHERE expires_at IS NOT NULL;
