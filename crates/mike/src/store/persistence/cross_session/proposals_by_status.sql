SELECT id, uuid, source, scope, project_id, class, polarity, status, proposed_kind,
       proposed_text, evidence_count, session_count, first_evidence_at, last_evidence_at, window_days,
       rollup_id, proposer_version, created_at, decided_at, decision_note, approved_text, superseded_by
  FROM memory.directive_proposals
 WHERE status = $1 AND id < $2
 ORDER BY id DESC
 LIMIT $3
