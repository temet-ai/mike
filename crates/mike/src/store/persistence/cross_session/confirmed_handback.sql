WITH fires AS (
    SELECT e.id, e.session_id, e.operator_turn_id, e.dispatched_at,
           e.verdict_json -> 'questions' -> 'authorised_handback' ->> 'class' AS class,
           COALESCE(e.verdict_json -> 'questions' -> 'authorised_handback' ->> 'quote', '') AS quote,
           CASE WHEN s.project_resolution = 'unresolved' THEN 'no_project' ELSE 'project' END AS scope,
           s.project_id
      FROM steering.evaluations e
      JOIN sessions.sessions s ON s.id = e.session_id
     WHERE e.question = 'authorised_handback'
       AND e.outcome_label = 'right'
       AND e.status = 'resolved_correction'
       AND e.dispatched_at >= now() - make_interval(days => $1)
),
keys AS (
    SELECT f.scope, f.project_id, f.class,
           count(*) AS evidence_count, count(DISTINCT f.session_id) AS session_count,
           min(f.dispatched_at) AS first_at, max(f.dispatched_at) AS last_at,
           string_agg(format('session %s, turn %s: %s', f.session_id, f.operator_turn_id, f.quote), E'\n'
                      ORDER BY f.dispatched_at, f.id) AS quotes
      FROM fires f
     WHERE f.class IS NOT NULL
     GROUP BY f.scope, f.project_id, f.class
    HAVING count(*) >= $2
),
open_count AS (
    SELECT count(*) AS n FROM memory.directive_proposals WHERE status = 'open'
),
candidates AS (
    SELECT k.*, COALESCE(pr.project_path, '(no project)') AS project_name,
           row_number() OVER (ORDER BY k.evidence_count DESC, k.last_at DESC) AS rk
      FROM keys k
      LEFT JOIN projects.projects pr ON pr.id = k.project_id
     WHERE NOT EXISTS (
               SELECT 1 FROM memory.directive_proposals p
                WHERE p.status = 'open' AND p.source = 'confirmed_handback' AND p.scope = k.scope
                  AND p.project_id IS NOT DISTINCT FROM k.project_id AND p.class = k.class)
       AND NOT EXISTS (
               SELECT 1 FROM memory.directive_proposals p
                WHERE p.status = 'rejected' AND p.source = 'confirmed_handback' AND p.scope = k.scope
                  AND p.project_id IS NOT DISTINCT FROM k.project_id AND p.class = k.class
                  AND p.decided_at > now() - make_interval(days => $5)
                  AND k.evidence_count < 2 * p.evidence_count)
),
ins AS (
    INSERT INTO memory.directive_proposals
        (source, scope, project_id, class, polarity, proposed_kind, proposed_text, evidence_count,
         session_count, first_evidence_at, last_evidence_at, window_days, rollup_id, proposer_version)
    SELECT 'confirmed_handback', c.scope, c.project_id, c.class, 'allow', 'convention',
           format('You may %s in %s without asking first. You were authorised for this in %s sessions:',
                  c.class, c.project_name, c.session_count) || E'\n' || c.quotes,
           c.evidence_count, c.session_count, c.first_at, c.last_at, $1, $3, $4
      FROM candidates c
      CROSS JOIN open_count oc
     WHERE c.rk <= LEAST($6::bigint, GREATEST($7::bigint - oc.n, 0))
    RETURNING id, scope, project_id, class
)
INSERT INTO memory.directive_proposal_evidence
    (proposal_id, kind, ref_id, operator_turn_id, session_id, observed_at, detail)
SELECT i.id, 'evaluation', f.id, f.operator_turn_id, f.session_id, f.dispatched_at,
       jsonb_build_object('class', f.class, 'quote', f.quote)
  FROM ins i
  JOIN fires f ON f.scope = i.scope AND f.project_id IS NOT DISTINCT FROM i.project_id AND f.class = i.class
RETURNING proposal_id AS "proposal_id!"
