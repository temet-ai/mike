WITH qual AS (
    SELECT p.source, p.class, p.polarity, p.proposed_kind,
           count(*) AS scopes, sum(p.evidence_count) AS evidence_count, sum(p.session_count) AS session_count,
           min(p.first_evidence_at) AS first_at, max(p.last_evidence_at) AS last_at,
           string_agg(p.proposed_text, E'\n\n' ORDER BY p.last_evidence_at DESC, p.id DESC) AS texts
      FROM memory.directive_proposals p
     WHERE p.status = 'open' AND p.scope <> 'operator'
     GROUP BY p.source, p.class, p.polarity, p.proposed_kind
    HAVING count(*) >= $1
),
ins AS (
    INSERT INTO memory.directive_proposals
        (source, scope, project_id, class, polarity, proposed_kind, proposed_text, evidence_count,
         session_count, first_evidence_at, last_evidence_at, window_days, rollup_id, proposer_version)
    SELECT q.source, 'operator', NULL, q.class, q.polarity, q.proposed_kind, q.texts,
           q.evidence_count, q.session_count, q.first_at, q.last_at, $2, $3, $4
      FROM qual q
     WHERE NOT EXISTS (
               SELECT 1 FROM memory.directive_proposals o
                WHERE o.status = 'open' AND o.scope = 'operator' AND o.source = q.source
                  AND o.class = q.class AND o.polarity = q.polarity)
    RETURNING id, source, class, polarity
)
UPDATE memory.directive_proposals p
   SET status = 'superseded', superseded_by = i.id
  FROM ins i
 WHERE p.status = 'open' AND p.scope <> 'operator' AND p.source = i.source
   AND p.class = i.class AND p.polarity = i.polarity
RETURNING p.id AS "id!"
