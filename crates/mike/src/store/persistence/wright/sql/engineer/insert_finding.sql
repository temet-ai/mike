WITH ended AS (
    UPDATE wright.investigations
       SET state = 'finished', ended_at = now(), exit_code = $10, end_reason = NULL
     WHERE id = $1 AND state IN ('starting', 'running')
    RETURNING id, problem_id, episode_id
)
INSERT INTO wright.findings
       (investigation_id, problem_id, episode_id, symptom, suspected_cause, confirmed_cause,
        cause_established, owning_part, kind, remedy, evidence)
SELECT ended.id, ended.problem_id, ended.episode_id, $2, $3, $4, $5, $6, $7, $8, $9
  FROM ended
RETURNING id
