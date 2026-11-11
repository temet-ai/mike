SELECT FROM pg_advisory_xact_lock(hashtextextended('wright:' || $1::text, 0))
