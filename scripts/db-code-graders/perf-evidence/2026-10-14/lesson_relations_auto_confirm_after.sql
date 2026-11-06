UPDATE memory.lesson_relations r
    SET status_id = cs.id, decided_at = now(), decided_by = 'judge_reinforcement'
   FROM memory.lesson_relation_statuses ps, memory.lesson_relation_statuses cs,
        memory.lesson_relation_kinds k, memory.lesson_relation_methods m,
        memory.lessons f, memory.lessons t,
        memory.lesson_sources fs, memory.lesson_sources ts, memory.lesson_stages fst
  WHERE ps.code = 'proposed' AND cs.code = 'confirmed' AND r.status_id = ps.id
    AND k.id = r.kind_id AND k.code IN ('supersedes', 'refines', 'recurs')
    AND m.id = r.method_id AND m.code = 'write_time'
    AND f.id = r.from_lesson_id AND t.id = r.to_lesson_id
    AND fs.id = f.source_id AND fs.code = 'judge'
    AND ts.id = t.source_id AND ts.code = 'judge'
    AND fst.id = f.stage_id
    AND (fst.code IN ('daily', 'long_term') OR k.code = 'recurs')
