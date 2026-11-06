WITH turn_ordinals AS (
    SELECT
        ot.id                AS turn_id,
        ot.session_id,
        ot.start_request_id,
        ot.end_request_id,
        ot.outcome_id,
        ot.classified_at,
        row_number() OVER (PARTITION BY ot.session_id ORDER BY ot.start_request_id) AS turn_ordinal,
        count(*) OVER (PARTITION BY ot.session_id) AS session_turn_count
    FROM memory.operator_turns ot
),
fires AS (
    SELECT
        i.id                 AS intervention_id,
        i.session_id,
        i.trigger_request_id,
        i.created_at         AS fired_at,
        t.turn_id            AS fire_turn_id,
        t.turn_ordinal        AS fire_turn_ordinal,
        t.session_turn_count
    FROM steering.interventions i
    JOIN steering.steerers st ON st.id = i.steerer_id AND st.name = 'false-done'
    JOIN turn_ordinals t
        ON t.session_id = i.session_id
       AND t.start_request_id <= i.trigger_request_id
       AND t.end_request_id >= i.trigger_request_id
),
corrections AS (
    SELECT
        t.turn_id            AS correction_turn_id,
        t.session_id,
        t.turn_ordinal        AS correction_turn_ordinal,
        t.classified_at       AS corrected_at,
        t.session_turn_count
    FROM turn_ordinals t
    JOIN memory.turn_outcomes o ON o.id = t.outcome_id
    WHERE o.code = 'corrected'
),
fire_correction_pairs AS (
    SELECT
        f.intervention_id, f.session_id, f.trigger_request_id, f.fired_at,
        f.fire_turn_id, f.fire_turn_ordinal, f.session_turn_count,
        c.correction_turn_id, c.correction_turn_ordinal, c.corrected_at,
        row_number() OVER (
            PARTITION BY f.intervention_id
            ORDER BY abs(c.correction_turn_ordinal - f.fire_turn_ordinal), c.correction_turn_ordinal
        ) AS rn
    FROM fires f
    LEFT JOIN corrections c ON c.session_id = f.session_id
),
correction_fire_pairs AS (
    SELECT
        c.correction_turn_id, c.session_id, c.correction_turn_ordinal, c.corrected_at, c.session_turn_count,
        f.intervention_id, f.fire_turn_id, f.fire_turn_ordinal, f.fired_at, f.trigger_request_id,
        row_number() OVER (
            PARTITION BY c.correction_turn_id
            ORDER BY f.fire_turn_ordinal DESC NULLS LAST
        ) AS rn
    FROM corrections c
    LEFT JOIN fires f ON f.session_id = c.session_id AND f.fire_turn_ordinal <= c.correction_turn_ordinal
)
SELECT
    'fire'::text                                    AS row_kind,
    intervention_id, session_id, trigger_request_id,
    fired_at                                        AS event_at,
    fire_turn_id, fire_turn_ordinal,
    correction_turn_id, correction_turn_ordinal, corrected_at,
    (correction_turn_ordinal - fire_turn_ordinal)   AS signed_gap,
    CASE
        WHEN correction_turn_ordinal IS NULL THEN 'never'
        WHEN correction_turn_ordinal - fire_turn_ordinal > 0 THEN 'early'
        WHEN correction_turn_ordinal - fire_turn_ordinal = 0 THEN 'on-time'
        ELSE 'late'
    END                                              AS timing_class,
    session_turn_count,
    CASE WHEN session_turn_count < 8 THEN 'short' ELSE 'long' END AS session_length_segment
FROM fire_correction_pairs
WHERE rn = 1
UNION ALL
SELECT
    'correction'::text                              AS row_kind,
    intervention_id, session_id, trigger_request_id,
    fired_at                                        AS event_at,
    fire_turn_id, fire_turn_ordinal,
    correction_turn_id, correction_turn_ordinal, corrected_at,
    (correction_turn_ordinal - fire_turn_ordinal)   AS signed_gap,
    CASE
        WHEN intervention_id IS NULL THEN 'no_fire_preceded'
        ELSE 'fire_preceded'
    END                                              AS timing_class,
    session_turn_count,
    CASE WHEN session_turn_count < 8 THEN 'short' ELSE 'long' END AS session_length_segment
FROM correction_fire_pairs
WHERE rn = 1;
