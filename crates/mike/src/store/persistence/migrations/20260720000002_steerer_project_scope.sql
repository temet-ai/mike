-- Project scope for steerers: NULL project_id means global (fires/enrols
-- across every project, the existing behavior); a set project_id confines
-- auto-enrolment to sessions of that project. Expand-only — no default, no
-- backfill, so every existing row reads back as global unchanged.
--
-- ON DELETE CASCADE: a scoped steerer has no meaning once its project is
-- gone (unlike sessions/requests, which are historical records worth
-- keeping under a NULL/orphan project). Deleting the project deletes the
-- steerer row along with its enrolments (steering.enrolments.steerer_id is
-- ON DELETE CASCADE already).

ALTER TABLE steering.steerers
    ADD COLUMN IF NOT EXISTS project_id bigint
        REFERENCES projects.projects (id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_steerers_project_id
    ON steering.steerers (project_id)
    WHERE project_id IS NOT NULL;
