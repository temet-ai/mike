WITH target AS (
    SELECT DISTINCT unnest(ARRAY[17777,17790,17803]::bigint[]::bigint[]) AS turn_id
),
live AS (
    SELECT DISTINCT ON (v.operator_turn_id)
           v.operator_turn_id AS turn_id, v.id, v.bundle_id, v.reaction, v.delivery, v.confidence,
           v.model, v.rubric_version, v.rationale, v.unmet_ask, v.evidence_quality
      FROM memory.turn_verdicts v
      JOIN target t ON t.turn_id = v.operator_turn_id
      LEFT JOIN memory.turn_facts f ON f.operator_turn_id = v.operator_turn_id
      JOIN memory.operator_turns ot ON ot.id = v.operator_turn_id
      JOIN requests.requests r ON r.id = ot.start_request_id
     WHERE v.mode = 'live'
       AND v.bundle_id = 14
       AND v.reaction IS NOT NULL
       AND r.claude_code_agent_id IS NULL
       AND (f.operator_turn_id IS NULL OR (
           f.prompt_kind = 'genuine' AND f.session_class <> 'synthetic'
           AND f.next_state IN ('resolved', 'none_session_settled')
           AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id))
     ORDER BY v.operator_turn_id, v.id DESC
),
gold AS (
    SELECT DISTINCT ON (l.subject_id, l.axis) l.subject_id AS turn_id, l.axis, l.label
      FROM eval.labels_as_of('infinity'::timestamptz) l
      JOIN target t ON t.turn_id = l.subject_id
     WHERE l.subject_kind = 'turn'
       AND l.tier = 'gold'
       AND l.axis IN ('reaction', 'delivery')
     ORDER BY l.subject_id, l.axis, l.created_at DESC, l.id DESC
),
end_gold AS (
    SELECT DISTINCT ON (l.subject_id) l.subject_id AS turn_id, l.label
      FROM eval.labels_as_of('infinity'::timestamptz) l
      JOIN target t ON t.turn_id = l.subject_id
     WHERE l.subject_kind = 'session_end' AND l.axis = 'session_end' AND l.tier = 'gold'
     ORDER BY l.subject_id, l.created_at DESC, l.id DESC
),
end_live AS (
    SELECT DISTINCT ON (v.end_turn_id) v.end_turn_id AS turn_id, v.end_label AS label
      FROM session_judges.verdicts v
      JOIN target t ON t.turn_id = v.end_turn_id
     WHERE v.mode = 'live' AND v.bundle_id = 13 AND v.end_label IS NOT NULL
     ORDER BY v.end_turn_id, v.judged_at DESC, v.id DESC
),
ends AS (
    SELECT t.turn_id
      FROM target t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.turn_id AND f.next_state = 'none_session_settled'
      JOIN memory.operator_turns ot ON ot.id = t.turn_id
      JOIN requests.requests r ON r.id = ot.start_request_id
      LEFT JOIN end_gold eg ON eg.turn_id = t.turn_id
      LEFT JOIN end_live el ON el.turn_id = t.turn_id
     WHERE COALESCE(eg.label, el.label) = 'closed_in_frustration'
       AND f.prompt_kind = 'genuine' AND f.session_class <> 'synthetic'
       AND r.claude_code_agent_id IS NULL
),
gold_axes AS (
    SELECT g.turn_id,
           max(g.label) FILTER (WHERE g.axis = 'reaction') AS reaction,
           max(g.label) FILTER (WHERE g.axis = 'delivery') AS delivery
      FROM gold g
     GROUP BY g.turn_id
),
inputs AS (
    SELECT t.turn_id,
           ga.reaction AS gold_reaction,
           ga.delivery AS gold_delivery,
           lv.id AS live_id,
           lv.bundle_id AS live_bundle_id,
           lv.reaction AS live_reaction,
           lv.delivery AS live_delivery,
           lv.confidence,
           lv.model,
           lv.rubric_version,
           lv.rationale,
           lv.unmet_ask,
           lv.evidence_quality,
           (en.turn_id IS NOT NULL AND ga.reaction IS NULL) AS frustration_close
      FROM target t
      LEFT JOIN gold_axes ga ON ga.turn_id = t.turn_id
      LEFT JOIN live lv ON lv.turn_id = t.turn_id
      LEFT JOIN ends en ON en.turn_id = t.turn_id
     WHERE ga.turn_id IS NOT NULL OR lv.id IS NOT NULL OR en.turn_id IS NOT NULL OR true::boolean
),
sourced AS (
    SELECT i.*,
           COALESCE(i.gold_reaction, i.live_reaction) AS reaction,
           COALESCE(i.gold_delivery, i.live_delivery) AS delivery,
           CASE
               WHEN i.frustration_close THEN 'frustration_close'
               WHEN i.gold_reaction IS NOT NULL OR i.gold_delivery IS NOT NULL THEN 'operator'
               WHEN i.live_id IS NOT NULL THEN 'judge'
               ELSE 'no_verdict'
           END AS source,
           CASE
               WHEN i.gold_reaction IS NOT NULL THEN i.gold_reaction
               WHEN i.live_reaction IS NOT NULL AND i.confidence >= 0.6::real THEN i.live_reaction
               WHEN i.live_reaction IS NOT NULL THEN 'unclear'
           END AS effective_reaction
      FROM inputs i
),
computed AS (
    SELECT s.turn_id, s.reaction, s.delivery, s.source,
           CASE WHEN s.source IN ('judge', 'frustration_close') THEN s.live_id END AS live_verdict_id,
           CASE WHEN s.source IN ('judge', 'frustration_close') THEN s.live_bundle_id END AS bundle_id,
           s.model, s.rubric_version, s.rationale, s.unmet_ask, s.evidence_quality,
           CASE
               WHEN s.frustration_close THEN 'abandoned'
               WHEN s.effective_reaction = 'corrected' THEN 'corrected'
               WHEN s.effective_reaction IN ('accepted', 'redirected') AND s.delivery = 'shipped' THEN 'shipped'
               WHEN s.effective_reaction IN ('accepted', 'redirected') THEN 'accepted'
               WHEN s.delivery = 'shipped' THEN 'shipped'
               ELSE 'unknown'
           END AS outcome_code
      FROM sourced s
),
snap AS (
    INSERT INTO memory.operator_turn_legacy_labels
        (turn_id, outcome_id, classified_at, judge_model, judge_rubric_version, rationale,
         unmet_ask, evidence_quality, judge_failure_reason)
    SELECT ot.id, ot.outcome_id, ot.classified_at, ot.judge_model, ot.judge_rubric_version,
           ot.rationale, ot.unmet_ask, ot.evidence_quality, ot.judge_failure_reason
      FROM memory.operator_turns ot
      JOIN computed c ON c.turn_id = ot.id
     WHERE ot.outcome_source IS NULL
       AND ot.closed_at IS NOT NULL
    ON CONFLICT (turn_id) DO NOTHING
    RETURNING turn_id
)
UPDATE memory.operator_turns ot
   SET outcome_id                 = tot.id,
       reaction                   = c.reaction,
       delivery                   = c.delivery,
       live_verdict_id            = c.live_verdict_id,
       outcome_source             = c.source,
       outcome_bundle_id          = c.bundle_id,
       outcome_projection_version = 1::smallint,
       outcome_projected_at       = now(),
       judge_model                = c.model,
       judge_rubric_version       = c.rubric_version,
       rationale                  = c.rationale,
       unmet_ask                  = c.unmet_ask,
       evidence_quality           = c.evidence_quality
  FROM computed c
  JOIN memory.turn_outcomes tot ON tot.code = c.outcome_code
 WHERE ot.id = c.turn_id
   AND ot.closed_at IS NOT NULL
   AND (ot.outcome_id, ot.reaction, ot.delivery, ot.outcome_source, ot.live_verdict_id,
        ot.outcome_bundle_id, ot.outcome_projection_version)
       IS DISTINCT FROM (tot.id, c.reaction, c.delivery, c.source, c.live_verdict_id,
        c.bundle_id, 1::smallint)
RETURNING ot.id, tot.code
;
