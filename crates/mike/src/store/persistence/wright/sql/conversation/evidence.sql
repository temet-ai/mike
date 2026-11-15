SELECT jsonb_build_object(
           'problem_id', p.id, 'check', p.check_name, 'source', p.source, 'scope', p.scope,
           'resource', p.resource, 'state', p.state, 'opened_at', p.opened_at,
           'last_seen_at', p.last_seen_at, 'healthy_at', p.healthy_at,
           'named_by_operator', p.id = ANY($1),
           'episodes', COALESCE(e.episodes, '[]'::jsonb),
           'findings', COALESCE(f.findings, '[]'::jsonb),
           'actions', COALESCE(a.actions, '[]'::jsonb),
           'repairs', COALESCE(r.repairs, '[]'::jsonb))
  FROM wright.problems p
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'episode_id', x.id, 'opened_at', x.opened_at, 'closed_at', x.closed_at,
                 'observations', x.observations, 'evidence', x.evidence) ORDER BY x.opened_at DESC) AS episodes
        FROM (SELECT id, opened_at, closed_at, observations, evidence
                FROM wright.episodes
               WHERE problem_id = p.id
               ORDER BY opened_at DESC, id DESC
               LIMIT 3) x
  ) e ON true
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'finding_id', x.id, 'investigation_id', x.investigation_id, 'episode_id', x.episode_id,
                 'kind', x.kind, 'cause_established', x.cause_established,
                 'owning_part', x.owning_part, 'symptom', x.symptom,
                 'suspected_cause', x.suspected_cause, 'confirmed_cause', x.confirmed_cause,
                 'remedy', x.remedy, 'evidence', x.evidence, 'created_at', x.created_at)
                 ORDER BY x.id DESC) AS findings
        FROM (SELECT * FROM wright.findings WHERE problem_id = p.id ORDER BY id DESC LIMIT 3) x
  ) f ON true
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'action_id', x.id, 'remedy', x.remedy, 'state', x.state, 'by', x.by,
                 'effect', x.effect, 'refusal', x.refusal, 'created_at', x.created_at)
                 ORDER BY x.id DESC) AS actions
        FROM (SELECT * FROM wright.actions WHERE problem_id = p.id ORDER BY id DESC LIMIT 5) x
  ) a ON true
  LEFT JOIN LATERAL (
      SELECT jsonb_agg(jsonb_build_object(
                 'repair_id', x.id, 'finding_id', x.finding_id, 'state', x.state,
                 'branch', x.branch, 'pr_url', x.pr_url, 'merge_sha', x.merge_sha,
                 'reason', x.reason, 'created_at', x.created_at)
                 ORDER BY x.id DESC) AS repairs
        FROM (SELECT * FROM wright.repairs WHERE problem_id = p.id ORDER BY id DESC LIMIT 3) x
  ) r ON true
 WHERE p.id = ANY($1) OR p.state = 'open'
 ORDER BY p.id = ANY($1) DESC, p.last_seen_at DESC, p.id DESC
 LIMIT $2
