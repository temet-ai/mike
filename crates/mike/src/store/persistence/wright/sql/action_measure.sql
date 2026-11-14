WITH due AS (
    SELECT a.id, a.remedy, (e.closed_at IS NOT NULL) AS effective
      FROM wright.actions a
      JOIN wright.remedies r ON r.name = a.remedy
      JOIN wright.episodes e ON e.id = a.episode_id
      JOIN wright.problems p ON p.id = a.problem_id
     CROSS JOIN LATERAL (
         SELECT count(*) AS passes
           FROM wright.check_runs cr
          WHERE cr.check_name = r.check_name
            AND cr.outcome <> 'unknown'
            AND cr.finished_at > a.verified_at
     ) runs
     WHERE a.state = 'verified'
       AND a.measured_at IS NULL
       AND coalesce(a.effect->>'outcome', '') <> 'told_coordinator'
       AND NOT EXISTS (
           SELECT 1 FROM wright.wright_session_uuids() AS w(session_uuid)
            WHERE w.session_uuid::text = p.resource)
       AND (e.closed_at IS NOT NULL OR runs.passes >= r.measure_after_passes)
       FOR UPDATE OF a SKIP LOCKED
), marked AS (
    UPDATE wright.actions a
       SET measured_at = now(), effective = d.effective,
           state = CASE WHEN d.effective THEN a.state ELSE 'ineffective' END,
           updated_at = now()
      FROM due d
     WHERE a.id = d.id
    RETURNING a.id, a.remedy, d.effective
), ordered AS (
    SELECT m.id, m.remedy, m.effective,
           max(m.id) FILTER (WHERE m.effective) OVER (PARTITION BY m.remedy) AS last_effective
      FROM marked m
), tally AS (
    SELECT o.remedy,
           bool_or(o.effective) AS any_effective,
           count(*) FILTER (WHERE o.effective) AS effective,
           count(*) FILTER (WHERE NOT o.effective) AS ineffective,
           count(*) FILTER (WHERE NOT o.effective AND o.id > coalesce(o.last_effective, 0)) AS trailing,
           max(o.id) FILTER (WHERE NOT o.effective) AS last_ineffective
      FROM ordered o
     GROUP BY o.remedy
), next AS (
    SELECT r.name, r.enabled AS was_enabled, t.effective, t.ineffective, t.last_ineffective,
           (CASE WHEN t.any_effective THEN 0 ELSE r.ineffective_runs END) + t.trailing::int AS runs
      FROM tally t
      JOIN wright.remedies r ON r.name = t.remedy
), updated AS (
    UPDATE wright.remedies r
       SET ineffective_runs = n.runs,
           enabled = CASE WHEN n.runs >= $1 THEN false ELSE r.enabled END,
           paused_at = CASE WHEN n.runs >= $1 AND n.was_enabled THEN now() ELSE r.paused_at END,
           pause_reason = CASE WHEN n.runs >= $1 AND n.was_enabled
                               THEN format('turned off: its problem was still open %s times in a row after it acted; the last, action %s',
                                           n.runs, n.last_ineffective)
                               ELSE r.pause_reason END,
           updated_at = now()
      FROM next n
     WHERE r.name = n.name
    RETURNING r.name, n.runs, n.effective, n.ineffective, (n.runs >= $1 AND n.was_enabled) AS turned_off
)
SELECT name AS remedy, runs AS ineffective_runs, effective, ineffective, turned_off
  FROM updated
 ORDER BY name
