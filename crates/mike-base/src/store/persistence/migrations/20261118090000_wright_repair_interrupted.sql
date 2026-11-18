-- A repair that mike's own stop interrupted (a deploy's SIGTERM, a restart) ended as
-- gate_failed or refused, and because one repair per finding was allowed, its finding was never
-- tried again. A repair that startup recovery settles as interrupted is marked now, so the
-- claim can try the finding again (up to three interrupted attempts), and the wright_repairs
-- self-check stops naming it and any repair whose problem has since closed.
SET LOCAL lock_timeout = '10s';

ALTER TABLE wright.repairs DROP CONSTRAINT IF EXISTS repairs_one_per_finding;
ALTER TABLE wright.repairs ADD COLUMN IF NOT EXISTS interrupted boolean NOT NULL DEFAULT false;

UPDATE wright.repairs SET interrupted = true WHERE reason LIKE 'interrupted:%' AND NOT interrupted;

CREATE INDEX IF NOT EXISTS idx_wright_repairs_finding ON wright.repairs (finding_id);

UPDATE wright.checks
   SET expects = 'No Wright repair ended gate_failed or refused in the last window_hours while its problem is still open and the stop was not an interruption; each one is named with its reason and the tail of its gate output',
       query = $q$
SELECT 'repair'::text AS scope, r.id::text AS resource,
       jsonb_build_object('repair_id', r.id, 'finding_id', r.finding_id, 'problem_id', r.problem_id,
           'owning_part', r.owning_part, 'state', r.state, 'reason', left(r.reason, 2000),
           'gate_exit', r.gate_exit, 'gate_output_tail', right(r.gate_output, 4000),
           'machine_uuid', r.machine_uuid, 'updated_at', r.updated_at) AS evidence
  FROM wright.repairs r
  JOIN wright.problems p ON p.id = r.problem_id AND p.state = 'open'
 WHERE r.state IN ('gate_failed', 'refused')
   AND NOT r.interrupted
   AND r.updated_at >= now() - make_interval(hours => ($1->>'window_hours')::int)
 ORDER BY r.id DESC
 LIMIT 200
$q$,
       updated_at = now()
 WHERE name = 'wright_repairs';
