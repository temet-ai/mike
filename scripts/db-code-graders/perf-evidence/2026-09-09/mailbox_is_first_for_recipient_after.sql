SELECT NOT EXISTS (
    SELECT 1
      FROM mailbox.messages m
      JOIN mailbox.messages p
        ON p.to_session_id = m.to_session_id AND p.id < m.id
     WHERE m.message_uuid = '6f4c1a42-d7d3-429a-a376-2b48ead5fd04'
) AS first_for_recipient
