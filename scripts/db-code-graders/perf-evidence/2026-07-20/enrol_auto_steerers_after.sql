INSERT INTO steering.enrolments
       (session_id, steerer_id, mode_id, authority_id, enrolled_by)
   SELECT s.id, st.id, st.default_mode_id, st.authority_id, 'auto'
     FROM sessions.sessions   s
    CROSS JOIN steering.steerers st
    WHERE s.session_uuid = '6c2eb855-afe9-4ff2-b323-02a1409f0bdb'
      AND st.auto_enrol  = true
      AND st.enabled     = true
      AND st.deleted_at IS NULL
      AND (st.project_id IS NULL OR st.project_id = s.project_id)
   ON CONFLICT (session_id, steerer_id) DO NOTHING
