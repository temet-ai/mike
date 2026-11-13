WITH window_rows AS (
    SELECT e.uuid, e.session_id, e.operator_turn_id, e.outcome_label, e.label_source, e.label_evidence,
           e.verdict_json, e.trigger_request_id
      FROM steering.evaluations e
     WHERE e.steerer_id = $1
       AND e.question = $2
       AND e.mode = 'shadow'
       AND e.status = 'resolved_correction'
       AND e.dispatched_at >= $3
),
classed AS (
    SELECT w.outcome_label, w.label_source, w.verdict_json, w.label_evidence,
           COALESCE(tf.session_class, 'unknown') AS session_class,
           (r.claude_code_agent_id IS NOT NULL OR r.agent_instance_id IS NOT NULL) AS non_actor,
           (w.label_evidence ->> 'next_genuine_after_secs')::bigint AS next_after
      FROM window_rows w
      LEFT JOIN requests.requests r ON r.id = w.trigger_request_id
      LEFT JOIN memory.turn_facts tf ON tf.operator_turn_id = w.operator_turn_id
)
SELECT count(*)::bigint AS "would_fire!",
       count(*) FILTER (WHERE outcome_label = 'right' AND NOT non_actor)::bigint AS "labelled_right!",
       count(*) FILTER (WHERE outcome_label = 'wrong' OR non_actor)::bigint AS "labelled_wrong!",
       count(*) FILTER (WHERE outcome_label = 'indeterminate' AND NOT non_actor)::bigint AS "labelled_indeterminate!",
       count(*) FILTER (WHERE label_source = 'next_message')::bigint AS "source_next_message!",
       count(*) FILTER (WHERE label_source = 'session_end')::bigint AS "source_session_end!",
       count(*) FILTER (WHERE label_source = 'operator')::bigint AS "source_operator!",
       count(*) FILTER (WHERE label_source = 'replay')::bigint AS "source_replay!",
       count(*) FILTER (WHERE (verdict_json -> 'quorum' -> $2 ->> 'votes_total')::int > 1)::bigint AS "with_quorum!",
       count(*) FILTER (WHERE session_class = 'operator')::bigint AS "operator_fires!",
       count(*) FILTER (WHERE session_class = 'template')::bigint AS "template_fires!",
       count(*) FILTER (WHERE session_class NOT IN ('operator', 'template'))::bigint AS "other_fires!",
       count(*) FILTER (WHERE non_actor)::bigint AS "non_actor_fires!",
       count(*) FILTER (WHERE next_after IS NOT NULL)::bigint AS "timing_n!",
       count(*) FILTER (WHERE next_after <= $4)::bigint AS "within_grace!",
       count(*) FILTER (WHERE next_after <= $4 + 60)::bigint AS "within_grace_60!"
  FROM classed
