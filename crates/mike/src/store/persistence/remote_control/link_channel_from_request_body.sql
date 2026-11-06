-- $1 = cse_id, $2 = the local session's uuid.
--
-- Single-row UPDATE scoped by cse_id; the session_id subquery reads only
-- the bind parameter $2, not the outer channels row, so it runs once
-- regardless of how many rows cse_id matches (at most one — cse_id is
-- the channel's natural key). A session not yet in sessions.sessions
-- leaves session_id NULL rather than blocking the local_session_uuid
-- write, matching the resolve-alongside-it behaviour the column comment
-- on remote_control.channels.local_session_uuid documents.
UPDATE remote_control.channels
   SET local_session_uuid = $2,
       session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $2)
 WHERE cse_id = $1
   AND local_session_uuid IS NULL
RETURNING id AS "id!"
