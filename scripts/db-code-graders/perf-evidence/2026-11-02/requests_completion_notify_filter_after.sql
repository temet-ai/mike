UPDATE requests.requests r SET status_id = st.id, completed_at = now() FROM requests.statuses st WHERE st.code = 'error' AND r.id = 990004
