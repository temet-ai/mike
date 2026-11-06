WITH scope AS (
    SELECT t.id AS turn_id, t.session_id, t.start_request_id, t.end_request_id,
           t.outcome_evidence_request_id, t.closed_at,
           lead(t.start_request_id) OVER (
               PARTITION BY t.session_id ORDER BY t.start_request_id
           ) AS next_start
      FROM memory.operator_turns t
     WHERE t.session_id IN (SELECT s.session_id FROM memory.operator_turns s WHERE s.id = ANY(ARRAY[17696,17694,17693,17690,17689,17688,17687,17686,17684,17682,17681,17680,17679,17678,17677,17676,17675,17674,17673,17672,17671,17670,17669,17668,17667,17666,17665,17664,17663,17662,17661,17660,17659,17658,17657,17656,17655,17654,17653,17652,17651,17650,17649,17648,17647,17646,17645,17644,17643,17642,17641,17640,17639,17638,17637,17636,17635,17634,17633,17632,17631,17629,17628,17627,17626,17625,17624,17623,17622,17621,17620,17619,17618,17617,17616,17615,17614,17613,17612,17611,17610,17609,17608,17607,17606,17605,17604,17603,17602,17601,17600,17599,17598,17597,17596,17595,17594,17593,17592,17591,17590,17589,17588,17587,17586,17585,17584,17583,17582,17581,17580,17579,17578,17577,17576,17575,17574,17573,17572,17571,17570,17569,17568,17567,17566,17565,17564,17563,17562,17561,17560,17559,17558,17557,17556,17555,17554,17553,17552,17551,17550,17549,17548,17547,17546,17545,17544,17543,17542,17541,17540,17539,17538,17537,17536,17535,17534,17533,17532,17531,17530,17529,17528,17527,17526,17525,17524,17523,17522,17521,17520,17519,17518,17517,17516,17515,17514,17513,17512,17511,17510,17509,17508,17507,17506,17505,17504,17503,17502,17501,17500,17499,17498,17497,17496,17495,17494,17493,17492,17491]::bigint[]))
), wanted AS (
    SELECT * FROM scope WHERE turn_id = ANY(ARRAY[17696,17694,17693,17690,17689,17688,17687,17686,17684,17682,17681,17680,17679,17678,17677,17676,17675,17674,17673,17672,17671,17670,17669,17668,17667,17666,17665,17664,17663,17662,17661,17660,17659,17658,17657,17656,17655,17654,17653,17652,17651,17650,17649,17648,17647,17646,17645,17644,17643,17642,17641,17640,17639,17638,17637,17636,17635,17634,17633,17632,17631,17629,17628,17627,17626,17625,17624,17623,17622,17621,17620,17619,17618,17617,17616,17615,17614,17613,17612,17611,17610,17609,17608,17607,17606,17605,17604,17603,17602,17601,17600,17599,17598,17597,17596,17595,17594,17593,17592,17591,17590,17589,17588,17587,17586,17585,17584,17583,17582,17581,17580,17579,17578,17577,17576,17575,17574,17573,17572,17571,17570,17569,17568,17567,17566,17565,17564,17563,17562,17561,17560,17559,17558,17557,17556,17555,17554,17553,17552,17551,17550,17549,17548,17547,17546,17545,17544,17543,17542,17541,17540,17539,17538,17537,17536,17535,17534,17533,17532,17531,17530,17529,17528,17527,17526,17525,17524,17523,17522,17521,17520,17519,17518,17517,17516,17515,17514,17513,17512,17511,17510,17509,17508,17507,17506,17505,17504,17503,17502,17501,17500,17499,17498,17497,17496,17495,17494,17493,17492,17491]::bigint[])
), stop AS (
    SELECT DISTINCT ON (w.turn_id)
           w.turn_id, r.id AS stop_request_id,
           CASE WHEN resp.id IS NULL THEN 'aborted' ELSE rsr.code END AS stop_reason
      FROM wanted w
      JOIN requests.requests r
        ON r.session_id = w.session_id
       AND r.id >= w.start_request_id
       AND (w.next_start IS NULL OR r.id < w.next_start)
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                                   AND ro.code IN ('primary', 'primary_first')
      LEFT JOIN responses.responses resp ON resp.request_id = r.id
      LEFT JOIN responses.stop_reasons rsr ON rsr.id = resp.stop_reason_id
     WHERE EXISTS (
            SELECT 1
              FROM responses.stream_blocks sb
              JOIN responses.stream_block_types sbt ON sbt.id = sb.block_type_id
             WHERE sb.request_id = r.id AND sbt.code = 'text')
     ORDER BY w.turn_id, r.id DESC
), interrupted AS (
    SELECT DISTINCT w.turn_id
      FROM wanted w
      JOIN requests.requests r
        ON r.session_id = w.session_id
       AND r.id > w.start_request_id
       AND r.id <= COALESCE(w.outcome_evidence_request_id, w.end_request_id)
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
                                   AND ro.code IN ('primary', 'primary_first')
     WHERE strpos(r.human_prompt, '[Request interrupted by user') > 0
)
SELECT w.turn_id,
       stop.stop_request_id,
       stop.stop_reason,
       (interrupted.turn_id IS NOT NULL) AS interrupted,
       CASE
           WHEN w.outcome_evidence_request_id IS NOT NULL THEN 'next_prompt'
           WHEN EXISTS (
                SELECT 1
                  FROM requests.requests ar
                  JOIN requests.errors e ON e.request_id = ar.id
                 WHERE ar.session_id = w.session_id
                   AND ar.completed_at = w.closed_at
                   AND e.message = 'stream aborted') THEN 'abort'
           ELSE 'idle_reap'
       END AS close_cause
  FROM wanted w
  LEFT JOIN stop ON stop.turn_id = w.turn_id
  LEFT JOIN interrupted ON interrupted.turn_id = w.turn_id
 ORDER BY w.turn_id
