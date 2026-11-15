INSERT INTO wright.followups (kind, check_name, title, body)
VALUES ('root_cause', $1::text,
        format('Find why %s keeps failing: %s has closed %s in %s days', $1::text, $2::text, $3::bigint, $4::int),
        format('The remedy %s has been verified %s times in %s days and the check %s still opens new problems. Find why the check keeps failing, not this one instance.',
               $2::text, $3::bigint, $4::int, $1::text))
ON CONFLICT (kind, COALESCE(check_name, ''), COALESCE(owning_part, ''))
    WHERE state IN ('open', 'filed', 'handed_off', 'stuck')
DO NOTHING
RETURNING id
