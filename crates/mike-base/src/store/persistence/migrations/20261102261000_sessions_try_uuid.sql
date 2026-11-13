-- Filters written `s.session_uuid::text = $n` cast the indexed column, so
-- sessions_session_uuid_key could not serve them. `s.session_uuid = sessions.try_uuid($n)`
-- casts the parameter instead; a value that is not a UUID becomes NULL and matches nothing,
-- as the text comparison did.
CREATE OR REPLACE FUNCTION sessions.try_uuid(value text) RETURNS uuid
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT CASE
             WHEN value ~* '^[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}$'
             THEN value::uuid
           END
$$;
