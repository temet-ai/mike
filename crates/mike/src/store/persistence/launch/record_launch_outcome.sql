UPDATE launch.launches
   SET result             = $2,
       reason             = $3,
       opened             = opened OR $4,
       delivered          = $5,
       confirmed          = $6,
       channel_check      = $7,
       channel_checked_at = CASE WHEN $7::text IS NULL THEN channel_checked_at ELSE now() END,
       stderr_text        = $8,
       stderr_truncated   = $9
 WHERE id = $1
RETURNING id
