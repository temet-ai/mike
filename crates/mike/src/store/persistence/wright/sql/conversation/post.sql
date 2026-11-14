WITH inserted AS (
    INSERT INTO wright.messages (author, body, client_msg_id, reply_to)
    VALUES ('operator', $1, $2, $3)
    ON CONFLICT (author, client_msg_id) DO NOTHING
    RETURNING id, author, body, client_msg_id, reply_to, links, created_at
), job AS (
    INSERT INTO wright.reply_jobs (message_id)
    SELECT id FROM inserted
    RETURNING message_id
)
SELECT id, author, body, client_msg_id, reply_to, links, created_at FROM inserted
