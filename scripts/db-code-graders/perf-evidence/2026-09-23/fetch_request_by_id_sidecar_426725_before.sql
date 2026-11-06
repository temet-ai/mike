SELECT r.id, s.session_uuid::text AS session_id,
            m.code AS method, r.path, r.model, r.is_streaming, r.message_count, r.is_tool_loop,
            st.code AS status,
            r.request_headers,
            r.params,
            r.request_body,
            r.filtered_request_body,
            r.last_user_message, r.human_prompt,
            tt.code AS turn_type, torg.code AS turn_origin, r.subagent_persona,
            r.created_at, r.completed_at,
            r.intercepted,
            r.correction_request_id,
            r.agent_instance_id,
            r.claude_code_agent_id,
            r.transformation_manifest,
            rsp.prompt_sha256        AS rsp_prompt_sha256,
            rsp.override_id          AS rsp_composition_id,
            rsp.override_applied     AS rsp_composition_applied,
            rsp.unknown_blocks       AS rsp_unknown_blocks,
            rsp.filter_modified      AS rsp_filter_modified,
            -- firing_rule_ids: reassembled as bigint[] from the FK junction,
            -- preserving fire order; empty bigint[] (not NULL) when no rules
            -- fired. The LATERAL lets the planner hash-join the junction
            -- table once per request row rather than run a correlated
            -- scalar subquery per outer row.
            COALESCE(fru.rsp_firing_rule_ids, '{}'::bigint[]) AS rsp_firing_rule_ids,
            -- Class-sha exact match (sp.blocks) wins when present -- a
            -- byte-identical match by content hash. Next is spl_exact, this
            -- request's own session_prompts capture row (UNIQUE on
            -- request_id): the verbatim system array captured for this
            -- exact request when no class-sha match was ever computed for
            -- it. The remaining session_prompts fallbacks are
            -- conversation-level approximations for a request with none of
            -- the above -- spl_sub keyed by this request's resolved agent
            -- instance, spl_main keyed by the session's one main-lane row.
            -- Neither is guaranteed byte-identical to this specific
            -- request's system array (the session's prompt can drift
            -- across turns); each is a close-enough fallback, not an exact
            -- match.
            COALESCE(sp.blocks, spl_exact.blocks, spl_sub.blocks, spl_main.blocks) AS sp_blocks,
            p.project_path           AS project_path,
            -- Response-phase fields (from request_responses — NULL when in-flight)
            rr.upstream_request_id   AS rr_upstream_request_id,
            rr.upstream_message_id   AS rr_upstream_message_id,
            rr.response_status       AS rr_response_status,
            rr.response_headers      AS rr_response_headers,
            rr.response_body         AS rr_response_body,
            rr.token_usage           AS rr_token_usage,
            -- stop_reason is normalized: responses.responses carries a
            -- stop_reason_id bigint FK -> responses.stop_reasons(id);
            -- project the text code via the lookup JOIN. Nullable because
            -- stop_reason_id is nullable and rr itself is a LEFT JOIN.
            rsr.code                 AS rr_stop_reason,
            rr.stop_details          AS rr_stop_details
     FROM requests.requests r
     JOIN requests.methods m ON m.id = r.method_id
     JOIN requests.statuses st ON st.id = r.status_id
     JOIN requests.turn_types tt ON tt.id = r.turn_type_id
     JOIN requests.turn_origins torg ON torg.id = r.turn_origin_id
     LEFT JOIN responses.responses rr
            ON rr.request_id = r.id
     LEFT JOIN responses.stop_reasons rsr ON rsr.id = rr.stop_reason_id
     LEFT JOIN requests.system_prompts rsp
            ON rsp.request_id = r.id
     LEFT JOIN overrides.overrides pc ON pc.id = rsp.override_id
     LEFT JOIN system_prompts.system_prompts sp ON sp.prompt_sha256 = rsp.prompt_sha256
     LEFT JOIN system_prompts.session_prompts spl_exact
            ON spl_exact.request_id = r.id
     LEFT JOIN system_prompts.session_prompts spl_sub
            ON spl_sub.instance_id = r.agent_instance_id
     LEFT JOIN system_prompts.session_prompts spl_main
            ON spl_main.session_id = r.session_id AND spl_main.lane = 'main'
     LEFT JOIN sessions.sessions s ON s.id = r.session_id
     LEFT JOIN projects.projects p ON p.id = s.project_id
     LEFT JOIN LATERAL (
         SELECT ARRAY_AGG(fr.id ORDER BY j.ord) AS rsp_firing_rule_ids
         FROM requests.system_prompt_firing_rules j
         JOIN rules.rules fr ON fr.id = j.filter_rule_id
         WHERE j.request_system_prompt_id = rsp.id
     ) fru ON true
     WHERE r.id = 426725;
