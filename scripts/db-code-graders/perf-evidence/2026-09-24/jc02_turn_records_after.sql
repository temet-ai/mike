SELECT t.id AS turn_id, t.session_id, t.start_request_id,
        t.end_request_id, t.created_at, t.closed_at, t.opened_by,
        t.outcome_evidence_request_id,
        lead(t.start_request_id) OVER (
            PARTITION BY t.session_id ORDER BY t.start_request_id
        ) AS next_start
   FROM memory.operator_turns t
  WHERE t.session_id = ANY(ARRAY[1202255,1202152,1173418,1139628,1158718,1173693,1117541,1096395,1035456,1035527,1035386,1050117,1035156,1047553,1034189,981647,1035047,981652,956801,931197,933623,947779,925594,934506,934158]::bigint[])
  ORDER BY t.session_id, t.start_request_id
