-- The judge claim starts from a queue column instead of joining every turn to its facts.
-- judge_queued_at is set when a turn is judge-eligible (closed, genuine, non-synthetic, not
-- opened by an agent request, follow-up settled, not permanently failed) and has no live
-- verdict matching its facts; it is cleared when such a verdict lands or the turn fails
-- permanently. Writers: the turn_facts upsert, the turn_verdicts inserts, the failure
-- markers and the re-judge reset. The judge worker reconciles the column at boot, when the
-- live bundle changes and once an hour.
ALTER TABLE memory.operator_turns ADD COLUMN judge_queued_at timestamptz;

CREATE INDEX operator_turns_judge_queue_idx
    ON memory.operator_turns (closed_at, id)
    WHERE judge_queued_at IS NOT NULL;

UPDATE memory.operator_turns t
   SET judge_queued_at = now()
  FROM memory.turn_facts f, requests.requests r
 WHERE f.operator_turn_id = t.id
   AND r.id = t.start_request_id
   AND t.closed_at IS NOT NULL
   AND f.prompt_kind = 'genuine'
   AND f.session_class <> 'synthetic'
   AND r.claude_code_agent_id IS NULL
   AND f.next_state IN ('resolved', 'none_session_settled')
   AND t.judge_failure_reason IS DISTINCT FROM 'permanent'
   AND NOT EXISTS (
       SELECT 1 FROM memory.turn_verdicts v
        WHERE v.operator_turn_id = t.id
          AND v.mode = 'live'
          AND v.reaction IS NOT NULL
          AND v.bundle_id = (SELECT NULLIF((value #>> '{}')::bigint, 0)
                               FROM config.settings
                              WHERE key = 'turn_judge.live_bundle_id')
          AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id);
