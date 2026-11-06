SELECT count(*) AS channels, count(local_session_uuid) AS linked FROM remote_control.channels WHERE first_seen_at >= now() - interval '24 hours' AND first_seen_at < now() - interval '15 minutes'
