WITH lesson AS (
    INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, agent_id, session_id)
    SELECT 993478, lk.id, 'a carried line', ls.id, lsrc.id,
           (SELECT min(a.id) FROM agents.agents a WHERE lower(a.name) = lower('db-dev')), 1525731
      FROM memory.lesson_kinds lk
     CROSS JOIN memory.lesson_stages ls
     CROSS JOIN memory.lesson_sources lsrc
     WHERE lk.code = 'convention' AND ls.code = 'raw' AND lsrc.code = 'carry'
    RETURNING id
)
INSERT INTO memory.carried_lessons (lesson_id, project_id, discipline, tag, source_note_uuid)
SELECT lesson.id, 993478, 'db-dev', 'carry:perf-probe', '9e8f740e-d809-4bec-9cc2-f9a4148dcc82'::uuid FROM lesson
RETURNING lesson_id AS "lesson_id"
