WITH lesson AS (
    INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, agent_id, session_id)
    SELECT $1, lk.id, $3, ls.id, lsrc.id,
           (SELECT min(a.id) FROM agents.agents a WHERE lower(a.name) = lower($5)), $4
      FROM memory.lesson_kinds lk
     CROSS JOIN memory.lesson_stages ls
     CROSS JOIN memory.lesson_sources lsrc
     WHERE lk.code = $2 AND ls.code = 'raw' AND lsrc.code = 'carry'
    RETURNING id
)
INSERT INTO memory.carried_lessons (lesson_id, project_id, discipline, tag, source_note_uuid)
SELECT lesson.id, $1, $5, $6, $7 FROM lesson
RETURNING lesson_id AS "lesson_id!"
