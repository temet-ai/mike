"""Bounded, read-only weekly learning measurements; raw captures stay in Mike."""

MIGRATION_DEPENDENT = frozenset({
    'lesson_shape', 'lesson_shape_injected', 'lesson_shape_pending', 'lesson_shape_resolved',
    'lesson_relations_write_time', 'lesson_replacement_effect',
    'lesson_placement', 'lesson_placement_stuck', 'lesson_placement_proposals', 'skill_live'})

RECALL_STAGE ='{"stages":[{"stage":"recall_injector"}]}'
RECALL_NO_TAGS = '{"stages":[{"stage":"recall_injector","reason_code":"recall_no_tags"}]}'
RECALL_INJECTED = '{"stages":[{"stage":"recall_injector","reason_code":"recall_lessons_injected"}]}'
RECALL_OUT_OF_BAND = '{"stages":[{"stage":"recall_injector","reason_code":"recall_delivered_out_of_band"}]}'


def queries(start, end):
    window = f"r.created_at >= '{start}' AND r.created_at < '{end}'"
    return {
        'sessions': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,
          grouping(p.project_path)::boolean AS is_global,
          count(DISTINCT r.session_id) AS sessions
          FROM requests.requests r LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} GROUP BY GROUPING SETS ((p.project_path),())""",
        'requests': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,
          o.code AS origin,r.model,count(*) AS requests,count(DISTINCT r.session_id) AS sessions,
          sum(a.input_tokens) AS input_tokens,sum(a.output_tokens) AS output_tokens,
          sum(a.cache_read_input_tokens) AS cache_read_tokens,
          sum(a.cache_creation_input_tokens) AS cache_write_tokens,
          count(*) FILTER(WHERE a.input_tokens IS NULL) AS missing_usage
          FROM requests.requests r LEFT JOIN responses.responses a ON a.request_id=r.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          JOIN requests.turn_origins o ON o.id=r.turn_origin_id WHERE {window}
          GROUP BY 1,2,3 ORDER BY 1,2,3""",
        'tools': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,u.tool_name,
          count(*) AS calls,count(*) FILTER(WHERE a.is_error) AS errors,
          sum(a.content_bytes) AS result_bytes,count(*) FILTER(WHERE a.tool_use_id IS NULL) AS missing_results
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          LEFT JOIN tools.results a ON a.tool_use_id=u.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} GROUP BY 1,2 ORDER BY calls DESC,1,2""",
        'skill_loads': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,
          p.project_uuid AS project_uuid,u.input->>'skill' AS skill,count(*) AS loads,
          count(DISTINCT r.session_id) AS sessions
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} AND u.tool_name='Skill' GROUP BY 1,2,3 ORDER BY loads DESC,1,2""",
        'skill_live': f"""SELECT p.project_path AS project,o.folder,o.first_shipped_on,
          o.window_start,o.window_end,o.loads,o.load_sessions,o.loaded_turns,o.judged_turns,
          o.corrected_turns,o.base_judged_turns,o.base_corrected_turns,o.retired_at,
          o.retired_reason,o.measured_at
          FROM memory.skill_live_outcomes o JOIN projects.projects p ON p.id=o.project_id
          WHERE o.retired_at IS NULL OR o.retired_at >= '{start}'
          ORDER BY o.loads DESC,p.project_path,o.folder""",
        'repeated_results': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,u.tool_name,
          md5(u.input::text) AS input_fingerprint,a.content_sha256 AS result_fingerprint,
          count(*) AS calls,count(DISTINCT r.session_id) AS sessions,
          min(u.id) AS example_tool_use_id,min(r.id) AS example_request_id,
          sum(a.content_bytes)-max(a.content_bytes) AS repeated_result_bytes
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          JOIN tools.results a ON a.tool_use_id=u.id AND NOT a.is_error
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} GROUP BY 1,2,3,4 HAVING count(*) >= 3
          ORDER BY repeated_result_bytes DESC NULLS LAST,1,2,3,4""",
        'created_tools': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,
          u.input->>'file_path' AS file_path,count(*) AS writes,
          count(DISTINCT r.session_id) AS sessions,min(r.id) AS example_request_id
          FROM requests.requests r JOIN tools.uses u ON u.request_id=r.id
          LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} AND u.tool_name IN ('Write','write_file')
          AND u.input->>'file_path' ~ '(scripts/|\\.(py|sh|bash|js|ts)$)'
          GROUP BY 1,2 HAVING count(DISTINCT r.session_id)>1 ORDER BY writes DESC,1,2""",
        'turns': f"""SELECT p.project_path AS project,count(*) AS turns,
          count(*) FILTER(WHERE f.prompt_kind='genuine') AS genuine_turns,
          count(*) FILTER(WHERE f.operator_turn_id IS NULL) AS missing_provenance,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND t.live_verdict_id IS NOT NULL AND v.id IS NULL) AS stale_outcome_labels,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='corrected') AS corrected,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='accepted') AS accepted,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='corrected' AND coalesce(t.final_assistant_text,'')='') AS empty_corrected,
          count(*) FILTER(WHERE t.closed_at IS NOT NULL AND t.embedded_at IS NULL) AS pending_embeddings,
          sum(t.tool_call_count) AS tool_calls,sum(t.tool_error_count) AS tool_errors
          FROM memory.operator_turns t LEFT JOIN projects.projects p ON p.id=t.project_id
          LEFT JOIN memory.turn_facts f ON f.operator_turn_id=t.id
          LEFT JOIN memory.turn_verdicts v ON v.id=t.live_verdict_id AND v.projected
            AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id
          WHERE t.created_at >= '{start}' AND t.created_at < '{end}' GROUP BY 1 ORDER BY 1""",
        'lessons': """SELECT p.project_path AS project,st.code AS stage,src.code AS source,
          count(*) AS lessons,count(*) FILTER(WHERE l.embedding IS NULL) AS pending_embeddings,
          min(l.created_at) FILTER(WHERE l.embedding IS NULL) AS oldest_pending
          FROM memory.lessons l JOIN projects.projects p ON p.id=l.project_id
          JOIN memory.lesson_stages st ON st.id=l.stage_id
          JOIN memory.lesson_sources src ON src.id=l.source_id
          WHERE l.archived_at IS NULL GROUP BY 1,2,3 ORDER BY 1,2,3""",
        'lesson_shape': """SELECT coalesce(f.shape,'unchecked') AS shape,count(*) AS lessons,
          sum(char_length(l.text)) AS stored_chars,
          count(*) FILTER(WHERE char_length(l.text)>600) AS over_600_chars,
          count(*) FILTER(WHERE char_length(l.text)<80) AS under_80_chars
          FROM memory.lessons l JOIN memory.lesson_stages st ON st.id=l.stage_id
          LEFT JOIN memory.lesson_shape_findings f ON f.lesson_id=l.id
          WHERE l.archived_at IS NULL AND st.code IN ('daily','long_term')
          GROUP BY 1 ORDER BY 1""",
        'lesson_shape_injected': f"""SELECT coalesce(f.shape,'unchecked') AS shape,count(*) AS injections,
          sum(char_length(l.text)) AS injected_chars
          FROM memory.lesson_injections i JOIN memory.lessons l ON l.id=i.lesson_id
          LEFT JOIN memory.lesson_shape_findings f ON f.lesson_id=l.id
          WHERE i.injected_at >= '{start}' AND i.injected_at < '{end}'
          GROUP BY 1 ORDER BY 1""",
        'lesson_placement': """SELECT coalesce(pl.code,'unplaced') AS placement,count(*) AS lessons,
          count(*) FILTER(WHERE l.agent_id IS NOT NULL) AS agent_scoped,
          count(*) FILTER(WHERE l.placement_covered_by IS NOT NULL) AS naming_a_document
          FROM memory.lessons l LEFT JOIN memory.lesson_placements pl ON pl.id=l.placement_id
          WHERE l.archived_at IS NULL GROUP BY 1 ORDER BY 1""",
        'lesson_placement_stuck': """SELECT count(*) AS lessons
          FROM memory.lesson_placement_failures f JOIN memory.lessons l ON l.id=f.lesson_id
          WHERE f.attempts >= 3 AND l.placement_id IS NULL AND l.archived_at IS NULL""",
        'lesson_placement_proposals': """SELECT l.id,pl.code AS placement,p.project_path AS project,
          a.name AS agent,l.text,l.placement_covered_by AS covered_by,l.placement_reason AS reason
          FROM memory.lessons l JOIN memory.lesson_placements pl ON pl.id=l.placement_id
          JOIN projects.projects p ON p.id=l.project_id LEFT JOIN agents.agents a ON a.id=l.agent_id
          WHERE l.archived_at IS NULL AND pl.code IN ('rule','agent')
          ORDER BY pl.code,p.project_path,l.id""",
        'project_names': "SELECT project_uuid,project_path FROM projects.projects",
        'skill_cap_setting': "SELECT value FROM config.settings WHERE key='lessons.skills.max_per_project'",
        'lesson_shape_pending': """SELECT shape,count(*) AS lessons,min(found_at) AS oldest_found_at
          FROM memory.lesson_shape_findings WHERE resolution IS NULL GROUP BY 1 ORDER BY 1""",
        'lesson_shape_resolved': f"""SELECT resolution,count(*) AS lessons
          FROM memory.lesson_shape_findings
          WHERE resolved_at >= '{start}' AND resolved_at < '{end}' AND resolution <> 'none_needed'
          GROUP BY 1 ORDER BY 1""",
        'lesson_relations_write_time': """SELECT k.code AS kind,m.code AS method,s.code AS status,
          count(*) AS relations,count(*) FILTER(WHERE r.evidence_json ? 'applied_at') AS applied
          FROM memory.lesson_relations r JOIN memory.lesson_relation_kinds k ON k.id=r.kind_id
          JOIN memory.lesson_relation_methods m ON m.id=r.method_id
          JOIN memory.lesson_relation_statuses s ON s.id=r.status_id
          WHERE m.code IN ('write_time','rewritten') GROUP BY 1,2,3 ORDER BY 1,2,3""",
        'lesson_replacement_effect': """WITH pairs AS (
          SELECT r.from_lesson_id AS replacement_id,r.to_lesson_id AS replaced_id
          FROM memory.lesson_relations r JOIN memory.lesson_relation_methods m ON m.id=r.method_id
          WHERE m.code='write_time' AND r.evidence_json ? 'applied_at'),
          sides AS (SELECT replacement_id AS lesson_id,'replacement' AS side FROM pairs
                    UNION ALL SELECT replaced_id,'replaced' FROM pairs)
          SELECT x.side,count(DISTINCT t.id) AS turns,
          count(DISTINCT t.id) FILTER(WHERE t.reaction='corrected') AS corrected_turns
          FROM sides x JOIN memory.lesson_injections i ON i.lesson_id=x.lesson_id
          JOIN memory.operator_turns t ON t.session_id=i.session_id
            AND i.request_id BETWEEN t.start_request_id AND t.end_request_id
          GROUP BY 1 ORDER BY 1""",
        'injections': f"""SELECT p.project_path AS project,count(*) AS injections,
          count(DISTINCT i.lesson_id) AS lessons,count(DISTINCT i.session_id) AS sessions
          FROM memory.lesson_injections i JOIN sessions.sessions s ON s.id=i.session_id
          JOIN projects.projects p ON p.id=s.project_id
          WHERE i.injected_at >= '{start}' AND i.injected_at < '{end}' GROUP BY 1 ORDER BY 1""",
        'memory_recall_tags': f"""SELECT coalesce(p.project_path,'(unresolved)') AS project,
          count(*) FILTER(WHERE r.transformation_manifest @> '{RECALL_NO_TAGS}') AS no_tag_requests,
          count(*) FILTER(WHERE r.transformation_manifest @> '{RECALL_INJECTED}') AS injected,
          count(*) FILTER(WHERE r.transformation_manifest @> '{RECALL_OUT_OF_BAND}') AS delivered_out_of_band,
          count(*) FILTER(WHERE r.transformation_manifest @> '{RECALL_STAGE}'
            AND NOT r.transformation_manifest @> '{RECALL_NO_TAGS}') AS requests_with_recall_tags
          FROM requests.requests r LEFT JOIN sessions.sessions s ON s.id=r.session_id
          LEFT JOIN projects.projects p ON p.id=coalesce(r.project_id,s.project_id)
          WHERE {window} AND r.transformation_manifest @> '{RECALL_STAGE}'
          GROUP BY 1 ORDER BY 1""",
        'memory_update_tags': f"""SELECT p.project_path AS project,
          count(*) AS replies_with_tags,sum(h.tags_found) AS tags_found,
          sum(h.lessons_written) AS lessons_written,sum(h.lessons_skipped) AS lessons_skipped,
          sum(h.lessons_truncated) AS lessons_truncated
          FROM memory.harvest_runs h JOIN projects.projects p ON p.id=h.project_id
          WHERE h.created_at >= '{start}' AND h.created_at < '{end}' GROUP BY 1 ORDER BY 1""",
        'memory_harvested_lessons': f"""SELECT p.project_path AS project,count(*) AS lessons
          FROM memory.lessons l JOIN memory.lesson_sources src ON src.id=l.source_id
          JOIN projects.projects p ON p.id=l.project_id
          WHERE src.code='harvester' AND l.created_at >= '{start}' AND l.created_at < '{end}'
          GROUP BY 1 ORDER BY 1""",
        'proposer': f"""SELECT created_at::date AS day,count(*) AS calls,sum(prompt_tokens) AS input_tokens,
          sum(completion_tokens) AS output_tokens FROM llm.calls WHERE role='lesson_proposer'
          AND created_at >= '{start}' AND created_at < '{end}' GROUP BY 1 ORDER BY 1""",
        'effects': """SELECT family,count(*) AS rows,max(window_end) AS latest
          FROM eval.rates GROUP BY family ORDER BY family""",
        'proposal_jobs': """SELECT status,count(*) AS jobs,
          min(created_at) AS oldest_created_at,min(updated_at) AS oldest_updated_at,
          min(next_attempt_at) FILTER(WHERE status='awaiting_embedding') AS next_attempt_at,
          count(*) FILTER(WHERE status='awaiting_embedding' AND next_attempt_at <= now()) AS retry_ready,
          max(retry_count) AS max_retries
          FROM memory.lesson_proposal_jobs GROUP BY status ORDER BY status""",
        'proposal_recovery': """SELECT recovery_state,count(*) AS jobs,
          min(created_at) AS oldest_created_at
          FROM memory.lesson_proposal_recovery GROUP BY recovery_state ORDER BY recovery_state""",
        'proposer_budget': """WITH days AS (
          SELECT ((now() AT TIME ZONE 'UTC')::date - offset_days)::date AS day
          FROM generate_series(0,6) AS offset_days
          ), calls AS (
          SELECT (created_at AT TIME ZONE 'UTC')::date AS day,count(*) AS actual_calls
          FROM llm.calls WHERE role='lesson_proposer'
          AND created_at >= (((now() AT TIME ZONE 'UTC')::date - 6)::timestamp AT TIME ZONE 'UTC')
          AND created_at < (((now() AT TIME ZONE 'UTC')::date + 1)::timestamp AT TIME ZONE 'UTC')
          GROUP BY 1)
          SELECT d.day,coalesce(b.consumed,0) AS consumed,coalesce(c.actual_calls,0) AS recorded_calls
          FROM days d LEFT JOIN memory.lesson_proposer_daily_budget b ON b.day=d.day
          LEFT JOIN calls c ON c.day=d.day ORDER BY d.day""",
        'governor_settings': """SELECT key,value,updated_at FROM config.settings
          WHERE jsonb_typeof(value) IN ('number','boolean') AND key IN (
            'llm.rate_limit_per_minute','llm.ceiling_max_calls','llm.ceiling_window_secs',
            'llm.daily_spend_ceiling_usd','llm.spend_alert_fraction',
            'llm.budget.steerer.share_pct','llm.budget.session_judge.share_pct',
            'llm.budget.judge_worker.share_pct','llm.budget.lesson_relations.share_pct',
            'llm.budget.lesson_proposer.share_pct','llm.budget.eval.share_pct','llm.budget.other.share_pct',
            'judge_lessons.min_confidence','judge_lessons.max_per_day',
            'judge_lessons.auto_promote_min_evidence','judge_lessons.quiet_max_per_day',
            'judge_lessons.quiet_start_hour_utc','judge_lessons.quiet_end_hour_utc',
            'judge_lessons.quiet_idle_secs') ORDER BY key""",
        'embedding_setting': "SELECT key,value,updated_at FROM config.settings WHERE key='embedding.enabled'",
        'llm_roles': """WITH recent AS (
          SELECT role,model,outcome,http_status,created_at,
                 row_number() OVER (PARTITION BY role ORDER BY created_at DESC,id DESC) AS rn
          FROM llm.calls WHERE created_at > now() - interval '7 days' AND role <> 'eval')
          SELECT role,(array_agg(model ORDER BY rn))[1] AS model,
                 (array_agg(outcome ORDER BY rn))[1] AS last_outcome,
                 (array_agg(http_status ORDER BY rn))[1] AS last_http_status,
                 max(created_at) AS last_call_at,
                 max(created_at) FILTER (WHERE outcome='ok') AS last_ok_at,
                 coalesce(min(rn) FILTER (WHERE outcome='ok')-1,count(*))::bigint AS failed_streak
          FROM recent GROUP BY role ORDER BY role""",
        'worker_toggles': """SELECT key,enabled,updated_at,
          round((extract(epoch FROM now()-updated_at)/86400)::numeric,1) AS days_in_state
          FROM config.feature_toggles
          WHERE key IN ('steerer','judge_worker','session_judge','judge_lessons') ORDER BY key""",
        'proposer_active_days': """SELECT to_char(day,'YYYY-MM') AS month,count(*) AS days_with_calls
          FROM (SELECT (created_at AT TIME ZONE 'UTC')::date AS day FROM llm.calls
                WHERE role='lesson_proposer'
                AND created_at >= date_trunc('month',now()) - interval '2 months' GROUP BY 1) d
          GROUP BY 1 ORDER BY 1""",
    }
