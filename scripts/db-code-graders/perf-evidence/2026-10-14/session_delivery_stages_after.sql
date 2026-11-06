SELECT st.stage->>'stage' AS stage,
       st.stage->>'outcome' AS outcome,
       st.stage->>'reason_code' AS reason_code,
       count(*) AS request_count,
       min(r.id) AS first_request_id,
       max(r.id) AS last_request_id
  FROM requests.requests r,
       jsonb_array_elements(r.transformation_manifest->'stages') AS st(stage)
 WHERE r.session_id = 1776684
 GROUP BY 1, 2, 3
 ORDER BY min(r.id), 1, 2, 3
