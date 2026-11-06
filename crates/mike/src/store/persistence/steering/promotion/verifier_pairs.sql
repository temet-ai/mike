WITH v AS (
    SELECT DISTINCT ON (e.operator_turn_id) e.operator_turn_id,
           (e.status = 'resolved_correction') AS fired, i.pushed_at
      FROM steering.evaluations e
      LEFT JOIN steering.interventions i ON i.uuid = e.intervention_uuid
     WHERE e.steerer_id = $1 AND e.dispatched_at >= $3 AND e.status <> 'dispatched'
     ORDER BY e.operator_turn_id, e.dispatched_at DESC, e.id DESC
),
q AS (
    SELECT DISTINCT ON (e.operator_turn_id) e.operator_turn_id,
           (e.status = 'resolved_correction') AS fired,
           COALESCE(e.label_evidence ->> 'answer' = 'yes', false) AS relevant_yes
      FROM steering.evaluations e
     WHERE e.steerer_id = $2 AND e.question = 'unobserved_done' AND e.dispatched_at >= $3
       AND e.status <> 'dispatched'
     ORDER BY e.operator_turn_id, e.dispatched_at DESC, e.id DESC
),
pairs AS (
    SELECT v.fired AS v_fired, q.fired AS q_fired, t.reaction, q.relevant_yes,
           (v.pushed_at IS NULL OR f.next_genuine_at IS NULL OR v.pushed_at > f.next_genuine_at) AS unseen
      FROM v
      JOIN q ON q.operator_turn_id = v.operator_turn_id
      JOIN memory.operator_turns t ON t.id = v.operator_turn_id
      LEFT JOIN memory.turn_facts f ON f.operator_turn_id = v.operator_turn_id
),
costs AS (
    SELECT count(*) FILTER (WHERE steerer_id = $1)::bigint AS verifier_calls,
           COALESCE(sum(cost_usd) FILTER (WHERE steerer_id = $1), 0)::float8 AS verifier_usd,
           count(*) FILTER (WHERE steerer_id = $2)::bigint AS guard_calls,
           COALESCE(sum(cost_usd) FILTER (WHERE steerer_id = $2), 0)::float8 AS guard_usd
      FROM llm.calls
     WHERE created_at >= $3 AND steerer_id IN ($1, $2)
),
tally AS (
    SELECT count(*)::bigint AS turns,
           count(*) FILTER (WHERE v_fired AND q_fired)::bigint AS both_fired,
           count(*) FILTER (WHERE v_fired AND NOT q_fired)::bigint AS verifier_only,
           count(*) FILTER (WHERE q_fired AND NOT v_fired)::bigint AS guard_only,
           count(*) FILTER (WHERE NOT v_fired AND NOT q_fired)::bigint AS neither,
           count(*) FILTER (WHERE v_fired AND unseen)::bigint AS verifier_counted,
           count(*) FILTER (WHERE v_fired AND unseen AND reaction = 'corrected')::bigint AS verifier_corrected,
           count(*) FILTER (WHERE q_fired AND reaction = 'corrected')::bigint AS guard_corrected,
           count(*) FILTER (WHERE reaction = 'corrected' AND relevant_yes)::bigint AS relevant,
           count(*) FILTER (WHERE reaction = 'corrected' AND relevant_yes AND v_fired)::bigint AS verifier_hits,
           count(*) FILTER (WHERE reaction = 'corrected' AND relevant_yes AND q_fired)::bigint AS guard_hits
      FROM pairs
)
SELECT tally.turns AS "turns!", tally.both_fired AS "both_fired!", tally.verifier_only AS "verifier_only!", tally.guard_only AS "guard_only!", tally.neither AS "neither!", tally.verifier_counted AS "verifier_counted!", tally.verifier_corrected AS "verifier_corrected!", tally.guard_corrected AS "guard_corrected!", tally.relevant AS "relevant!", tally.verifier_hits AS "verifier_hits!", tally.guard_hits AS "guard_hits!",
       costs.verifier_calls AS "verifier_calls!", costs.verifier_usd AS "verifier_usd!", costs.guard_calls AS "guard_calls!", costs.guard_usd AS "guard_usd!"
  FROM tally CROSS JOIN costs
