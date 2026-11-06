INSERT INTO steering.evaluations
       (steerer_id, session_id, operator_turn_id, status)
SELECT st.id, 680448, 12381, 'dispatched'
  FROM steering.steerers st
 WHERE st.kind = 'false_done'
RETURNING uuid;
