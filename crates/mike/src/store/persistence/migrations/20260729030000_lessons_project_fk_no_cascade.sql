-- memory.lessons.project_id was the one outlier on this table's FKs (every
-- other nullable FK -- agent_id, agent_instance_id, duplicate_of_id,
-- request_id, session_id, source_turn_id -- is already ON DELETE SET NULL):
-- deleting a project hard-deleted every lesson scoped to it, a route to
-- destroying a lesson row that this table's archive semantic forbids
-- elsewhere. SET NULL orphans the lesson from project-scoped recall while
-- preserving the row; RESTRICT was rejected as a much wider behaviour change
-- (it would block project deletion outright).

ALTER TABLE memory.lessons DROP CONSTRAINT IF EXISTS lessons_project_id_fkey;
ALTER TABLE memory.lessons ADD CONSTRAINT lessons_project_id_fkey
    FOREIGN KEY (project_id) REFERENCES projects.projects(id) ON DELETE SET NULL;
