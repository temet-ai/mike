DELETE FROM memory.lesson_relations r
 USING memory.lesson_relation_methods m, memory.lesson_relation_statuses s, memory.lessons a
 WHERE m.id = r.method_id AND m.code = 'adjudicated'
   AND s.id = r.status_id AND s.code = 'proposed'
   AND a.id = r.from_lesson_id AND a.project_id = $1
   AND r.detector_version IS DISTINCT FROM $2
