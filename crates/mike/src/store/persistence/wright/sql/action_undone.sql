UPDATE wright.actions
   SET state = 'undone', undone_at = now(), updated_at = now(),
       effect = coalesce(effect, '{}'::jsonb) || jsonb_build_object('undo', $2::jsonb)
 WHERE id = $1
   AND state IN ('verified', 'ineffective')
RETURNING id
