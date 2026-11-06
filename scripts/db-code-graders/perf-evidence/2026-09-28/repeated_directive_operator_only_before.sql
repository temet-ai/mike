WITH dirs AS (
    SELECT e.id AS entry_id, e.session_id, e.operator_turn_id, e.effective_at,
           e.data ->> 'class' AS class, e.data ->> 'polarity' AS polarity, e.data -> 'span' AS span,
           split_part(e.source_ref, ':', 2)::bigint AS request_id,
           CASE WHEN s.project_resolution = 'unresolved' THEN 'no_project' ELSE 'project' END AS scope,
           s.project_id
      FROM memory.dossier_entries e
      JOIN sessions.sessions s ON s.id = e.session_id
     WHERE e.kind = 'directive'
       AND e.source = 'capture'
       AND e.data ->> 'class' IN ('commit', 'push', 'merge', 'deploy', 'pr', 'proceed', 'ask_first')
       AND e.data ->> 'polarity' IN ('allow', 'forbid')
       AND e.source_ref LIKE 'requests.requests:%'
       AND e.effective_at >= now() - make_interval(days => 30)
),
correcting AS (
    SELECT DISTINCT d.entry_id
      FROM dirs d
      JOIN memory.turn_facts f ON f.next_genuine_request_id = d.request_id
      JOIN memory.turn_verdicts v ON v.operator_turn_id = f.operator_turn_id
                                 AND v.mode = 'live' AND v.reaction = 'corrected'
),
keys AS (
    SELECT d.scope, d.project_id, d.class, d.polarity,
           count(*) AS evidence_count, count(DISTINCT d.session_id) AS session_count,
           min(d.effective_at) AS first_at, max(d.effective_at) AS last_at
      FROM dirs d
      LEFT JOIN correcting c ON c.entry_id = d.entry_id
     GROUP BY d.scope, d.project_id, d.class, d.polarity
    HAVING count(DISTINCT d.session_id) >= 3 AND bool_or(c.entry_id IS NOT NULL)
),
latest AS (
    SELECT DISTINCT ON (d.scope, d.project_id, d.class, d.polarity)
           d.scope, d.project_id, d.class, d.polarity, d.operator_turn_id
      FROM dirs d
     ORDER BY d.scope, d.project_id, d.class, d.polarity, d.effective_at DESC, d.entry_id DESC
),
open_count AS (
    SELECT count(*) AS n FROM memory.directive_proposals WHERE status = 'open'
),
candidates AS (
    SELECT k.*, t.user_prompt AS proposed_text,
           row_number() OVER (ORDER BY k.evidence_count DESC, k.last_at DESC) AS rk
      FROM keys k
      JOIN latest l ON l.scope = k.scope AND l.project_id IS NOT DISTINCT FROM k.project_id
                   AND l.class = k.class AND l.polarity = k.polarity
      JOIN memory.operator_turns t ON t.id = l.operator_turn_id
     WHERE NOT EXISTS (
               SELECT 1 FROM memory.directive_proposals p
                WHERE p.status = 'open' AND p.source = 'repeated_directive' AND p.scope = k.scope
                  AND p.project_id IS NOT DISTINCT FROM k.project_id AND p.class = k.class
                  AND p.polarity = k.polarity)
       AND NOT EXISTS (
               SELECT 1 FROM memory.directive_proposals p
                WHERE p.status = 'rejected' AND p.source = 'repeated_directive' AND p.scope = k.scope
                  AND p.project_id IS NOT DISTINCT FROM k.project_id AND p.class = k.class
                  AND p.polarity = k.polarity
                  AND p.decided_at > now() - make_interval(days => 14)
                  AND k.evidence_count < 2 * p.evidence_count)
),
ins AS (
    INSERT INTO memory.directive_proposals
        (source, scope, project_id, class, polarity, proposed_kind, proposed_text, evidence_count,
         session_count, first_evidence_at, last_evidence_at, window_days, rollup_id, proposer_version)
    SELECT 'repeated_directive', c.scope, c.project_id, c.class, c.polarity, 'preference',
           COALESCE(c.proposed_text, ''), c.evidence_count, c.session_count, c.first_at, c.last_at,
           30, 125, 'perf-probe'
      FROM candidates c
      CROSS JOIN open_count oc
     WHERE c.rk <= LEAST(3::bigint, GREATEST(10::bigint - oc.n, 0))
    RETURNING id, scope, project_id, class, polarity
)
INSERT INTO memory.directive_proposal_evidence
    (proposal_id, kind, ref_id, operator_turn_id, session_id, observed_at, detail)
SELECT i.id, 'dossier_entry', d.entry_id, d.operator_turn_id, d.session_id, d.effective_at,
       jsonb_build_object('class', d.class, 'polarity', d.polarity, 'span', d.span, 'request_id', d.request_id)
  FROM ins i
  JOIN dirs d ON d.scope = i.scope AND d.project_id IS NOT DISTINCT FROM i.project_id
             AND d.class = i.class AND d.polarity = i.polarity
RETURNING proposal_id
