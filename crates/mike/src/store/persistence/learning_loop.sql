SELECT rq.n AS "requests!", rq.at AS requests_at,
       ot.n AS "operator_turns!", ot.at AS operator_turns_at,
       tv.n AS "turn_verdicts!", tv.at AS turn_verdicts_at,
       sv.n AS "session_verdicts!", sv.at AS session_verdicts_at,
       ls.n AS "lessons!", ls.at AS lessons_at,
       lr.n AS "lesson_relations!", lr.at AS lesson_relations_at,
       cr.n AS "consolidation_runs!", cr.at AS consolidation_runs_at,
       si.n AS "supplied_items!", si.at AS supplied_items_at,
       vd.n AS "supply_verdicts!", vd.at AS supply_verdicts_at,
       sg.n AS "search_grades!", sg.at AS search_grades_at,
       mr.n AS "memory_reviews!", mr.at AS memory_reviews_at
  FROM (SELECT count(*) AS n, max(r.created_at) AS at
          FROM requests.requests r WHERE r.created_at >= $1) rq,
       (SELECT count(*) AS n, max(t.created_at) AS at
          FROM memory.operator_turns t WHERE t.created_at >= $1) ot,
       (SELECT count(*) AS n, max(v.created_at) AS at
          FROM memory.turn_verdicts v WHERE v.created_at >= $1) tv,
       (SELECT count(*) AS n, max(s.judged_at) AS at
          FROM session_judges.verdicts s WHERE s.judged_at >= $1) sv,
       (SELECT count(*) AS n, max(l.created_at) AS at
          FROM memory.lessons l WHERE l.created_at >= $1) ls,
       (SELECT count(*) AS n, max(x.created_at) AS at
          FROM (SELECT x.created_at FROM memory.lesson_relations x ORDER BY x.id DESC LIMIT $2) x
         WHERE x.created_at >= $1) lr,
       (SELECT count(*) AS n, max(c.started_at) AS at
          FROM memory.consolidation_runs c WHERE c.started_at >= $1) cr,
       (SELECT count(*) AS n, max(i.supplied_at) AS at
          FROM memory.supplied_items i WHERE i.supplied_at >= $1) si,
       (SELECT count(*) AS n, max(d.judged_at) AS at
          FROM memory.supplied_item_verdicts d WHERE d.judged_at >= $1) vd,
       (SELECT count(*) AS n, max(g.graded_at) AS at
          FROM memory.search_grades g WHERE g.graded_at >= $1) sg,
       (SELECT count(*) AS n, max(n.created_at) AS at
          FROM workspace.session_notes n
         WHERE n.tags @> ARRAY['memory-review'] AND n.kind = 'reading' AND n.created_at >= $1) mr
