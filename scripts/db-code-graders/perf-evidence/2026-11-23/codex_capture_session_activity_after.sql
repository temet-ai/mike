UPDATE sessions.sessions SET last_seen = GREATEST(last_seen + interval '1 microsecond', clock_timestamp()) WHERE id = 1;
