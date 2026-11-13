WITH wanted AS (
    SELECT c.lesson_id, l.text
      FROM memory.carried_lessons c
      JOIN memory.lessons l ON l.id = c.lesson_id AND l.archived_at IS NULL
      JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
     WHERE c.project_id = mailbox.fleet_project_id($1)
       AND c.discipline = ANY($2::text[])
),
given AS (
    INSERT INTO memory.carried_lesson_runs (lesson_id, run_id)
    SELECT lesson_id, $3 FROM wanted
    ON CONFLICT (lesson_id, run_id) DO NOTHING
    RETURNING lesson_id
),
counted AS (
    UPDATE memory.carried_lessons c
       SET runs_given = c.runs_given + 1
      FROM given
     WHERE c.lesson_id = given.lesson_id
    RETURNING c.lesson_id
)
SELECT w.text AS "text!" FROM wanted w ORDER BY w.lesson_id
