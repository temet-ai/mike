SELECT count(*)
   FROM memory.lesson_proposal_evidence e
   JOIN (SELECT 16::bigint AS lesson_id
         UNION
         SELECT r.to_lesson_id
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
           JOIN memory.lesson_relation_methods m ON m.id = r.method_id
          WHERE r.from_lesson_id = 16
            AND k.code IN ('supersedes', 'refines')
            AND m.code = 'write_time') backed
     ON backed.lesson_id = e.lesson_id
