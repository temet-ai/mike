WITH cfg AS (
    SELECT coalesce(max((value #>> '{}')::bigint), 0) AS live_id
      FROM config.settings
     WHERE key = 'turn_judge.live_bundle_id'
),
shadow AS (
    SELECT id, components FROM eval.bundles
     WHERE role = 'turn_judge_shadow' AND origin = 'resolver' AND superseded_at IS NULL
     ORDER BY created_at DESC, id DESC
     LIMIT 1
),
live AS (
    SELECT b.id, b.components FROM eval.bundles b JOIN cfg ON b.id = cfg.live_id
)
SELECT live.id AS live_bundle_id, shadow.id AS shadow_bundle_id
  FROM live, shadow
 WHERE (live.components - 'role'::text) = (shadow.components - 'role'::text)