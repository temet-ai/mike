WITH participant_ids AS (
                       SELECT thread_id, from_session_id AS session_id
                         FROM mailbox.messages
                        WHERE thread_id IS NOT NULL AND from_session_id IS NOT NULL
                       UNION
                       SELECT thread_id, to_session_id AS session_id
                         FROM mailbox.messages
                        WHERE thread_id IS NOT NULL AND to_session_id IS NOT NULL
                       UNION
                       SELECT id AS thread_id, created_by_session_id AS session_id
                         FROM mailbox.threads
                        WHERE created_by_session_id IS NOT NULL
                   ),
                   participants AS (
                       SELECT pi.thread_id, ARRAY_AGG(DISTINCT s.session_uuid) AS participant_uuids
                         FROM participant_ids pi
                         JOIN sessions.sessions s ON s.id = pi.session_id
                        GROUP BY pi.thread_id
                   ),
                   message_stats AS (
                       SELECT
                           m.thread_id,
                           COUNT(*) AS message_count,
                           COUNT(*) FILTER (
                               WHERE m.to_session_id IS NOT NULL
                                 AND (m.sent_request_id IS NOT NULL OR m.channel_delivered_at IS NOT NULL)
                                 AND m.consumed_at IS NULL
                                 AND m.dead_lettered_at IS NULL
                           ) AS unread_count,
                           MIN(m.queued_at) FILTER (
                               WHERE m.to_session_id IS NOT NULL
                                 AND m.sent_request_id IS NULL
                                 AND m.channel_delivered_at IS NULL
                                 AND m.consumed_at IS NULL
                                 AND m.dead_lettered_at IS NULL
                           ) AS oldest_pending_queued_at,
                           MIN(
                               CASE
                                   WHEN m.to_session_id IS NULL THEN NULL
                                   WHEN m.dead_lettered_at IS NOT NULL THEN 0
                                   WHEN m.consumed_at IS NOT NULL THEN 4
                                   WHEN m.sent_request_id IS NOT NULL THEN 3
                                   WHEN m.channel_delivered_at IS NOT NULL THEN 3
                                   WHEN m.claimed_at IS NOT NULL THEN 2
                                   ELSE 1
                               END
                           ) AS worst_rank
                         FROM mailbox.messages m
                        WHERE m.thread_id IS NOT NULL
                        GROUP BY m.thread_id
                   ),
                   thread_tag_codes AS (
                       SELECT tt.thread_id, ARRAY_AGG(tg.code ORDER BY tg.code) AS tag_codes
                         FROM mailbox.thread_tags tt
                         JOIN mailbox.tags tg ON tg.id = tt.tag_id
                        GROUP BY tt.thread_id
                   ),
                   creator_fleet AS (
                       SELECT DISTINCT ON (l.session_uuid) l.session_uuid, f.fleet_uuid
                         FROM launch.launches l
                         JOIN launch.fleets f ON f.run_id = l.run_id
                        ORDER BY l.session_uuid, l.created_at DESC, l.id DESC
                   )
                   SELECT
                       t.id,
                       t.thread_uuid,
                       t.subject,
                       t.created_at,
                       t.last_message_at,
                       t.resolved_at,
                       creator.session_uuid AS created_by_session_uuid,
                       COALESCE(p.participant_uuids, ARRAY[]::uuid[]) AS participant_uuids,
                       COALESCE(ttc.tag_codes, ARRAY[]::text[]) AS tag_codes,
                       COALESCE(ms.message_count, 0) AS message_count,
                       COALESCE(ms.unread_count, 0) AS unread_count,
                       ms.oldest_pending_queued_at,
                       ms.worst_rank,
                       pr.project_uuid,
                       COALESCE(NULLIF(btrim(pr.custom_name), ''), pr.display_name) AS project_name,
                       pr.project_path,
                       fl.fleet_uuid,
                       fl.slug AS fleet_slug,
                       fl.state AS fleet_state
                     FROM mailbox.threads t
                     LEFT JOIN sessions.sessions creator ON creator.id = t.created_by_session_id
                     LEFT JOIN creator_fleet cf ON cf.session_uuid = creator.session_uuid
                     LEFT JOIN launch.fleets fl ON fl.fleet_uuid = COALESCE(t.fleet_uuid, cf.fleet_uuid)
                     LEFT JOIN projects.projects pr0 ON pr0.id = COALESCE(t.project_id, creator.project_id)
                     LEFT JOIN projects.projects pr ON pr.id = COALESCE(pr0.merged_into_id, pr0.id)
                     LEFT JOIN participants p ON p.thread_id = t.id
                     LEFT JOIN message_stats ms ON ms.thread_id = t.id
                     LEFT JOIN thread_tag_codes ttc ON ttc.thread_id = t.id
                    WHERE (NULL::uuid IS NULL OR t.thread_uuid = NULL)
                      AND (NULL::uuid IS NULL OR EXISTS (
                           SELECT 1 FROM mailbox.messages pm
                           LEFT JOIN sessions.sessions sender ON sender.id = pm.from_session_id
                           JOIN sessions.sessions recip ON recip.id = pm.to_session_id
                          WHERE pm.thread_id = t.id AND (sender.session_uuid = NULL OR recip.session_uuid = NULL)
                      ))
                      AND (NULL::bigint[] IS NULL OR t.project_id = ANY(NULL::bigint[]))
                      AND (NULL::boolean IS NULL OR (t.resolved_at IS NOT NULL) = NULL)
                      -- Reuses thread_tag_codes (already computed for the SELECT list's
                      -- tag_codes column) instead of a second EXISTS join against
                      -- mailbox.thread_tags/tags — avoids a redundant SubPlan for a
                      -- value this query already aggregates.
                      AND (NULL::text IS NULL OR NULL::text = ANY(COALESCE(ttc.tag_codes, ARRAY[]::text[])))
                    ORDER BY t.last_message_at DESC
