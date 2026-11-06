-- ════════════════════════════════════════════════════════════════════
-- Issue #25 (R3) — skills_used: agent-invoked skill attribution per turn
--
-- subject_command (migration 20260625000003) captures the slash-command /
-- skill the OPERATOR typed to open a turn. But a skill can also become
-- active MID-turn when the agent invokes it via the `Skill` tool — those
-- were captured only as a generic 'Skill' entry in tools_used[], never by
-- name. That is asymmetric with agents_dispatched[], which already
-- materialises sub-agent names invoked via the Agent/Task tool.
--
-- This adds skills_used text[] — the DISTINCT skill names invoked via the
-- Skill tool within the turn — mirroring agents_dispatched[] exactly:
-- a stored column materialised at turn-close (store/persistence/mod.rs
-- materialize_turn_aggregates) AND live-computed in
-- operator_turn_aggregates_v for the open trailing turn.
--
-- Skill name source: tools.uses where tool_name = 'Skill', name read from
-- the verbatim input JSONB (input->>'skill'). tools.uses.input is the FULL
-- tool_use payload (baseline.sql:645). The id (u.id / r.id) is the join
-- key throughout — never a content hash.
-- ════════════════════════════════════════════════════════════════════

ALTER TABLE memory.operator_turns
    ADD COLUMN skills_used text[];

-- ── Live + materialized aggregate view ────────────────────────────────
-- Same shape as migration 20260610000006 (the lateralized form), with
-- skills_used added: closed turns read the materialized column, the open
-- trailing turn computes it live from the already-joined tools.uses `u`.
CREATE OR REPLACE VIEW claude_proxy.operator_turn_aggregates_v AS
SELECT ot.id AS turn_id,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_call_count,       0) ELSE COALESCE(la.tool_call_count,       0) END AS tool_call_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tool_error_count,      0) ELSE COALESCE(la.tool_error_count,      0) END AS tool_error_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.weasel_findings_count, 0) ELSE COALESCE(la.weasel_findings_count, 0) END AS weasel_findings_count,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_input_tokens,    0)::numeric ELSE COALESCE(lt.total_input_tokens,  0) END AS total_input_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.total_output_tokens,   0)::numeric ELSE COALESCE(lt.total_output_tokens, 0) END AS total_output_tokens,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.tools_used,         '{}') ELSE COALESCE(la.tools_used,         '{}') END AS tools_used,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.agents_dispatched,  '{}') ELSE COALESCE(la.agents_dispatched,  '{}') END AS agents_dispatched,
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.files_touched,      '{}') ELSE COALESCE(la.files_touched,      '{}') END AS files_touched,
       -- skills_used is appended LAST so this CREATE OR REPLACE VIEW only ADDS a
       -- column to the existing view (Postgres forbids reordering/renaming
       -- existing view columns under CREATE OR REPLACE). Consumers read by name.
       CASE WHEN ot.closed_at IS NOT NULL THEN COALESCE(ot.skills_used,        '{}') ELSE COALESCE(la.skills_used,        '{}') END AS skills_used
FROM memory.operator_turns ot
LEFT JOIN LATERAL (
    SELECT COUNT(DISTINCT u.id)                                                                                     AS tool_call_count,
           COALESCE(ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name) FILTER (WHERE u.tool_name IS NOT NULL), '{}'::text[]) AS tools_used,
           COUNT(DISTINCT CASE WHEN res.is_error THEN res.id END)                                                   AS tool_error_count,
           COUNT(DISTINCT CASE WHEN d.analyzer = 'weasel' THEN d.id END)                                           AS weasel_findings_count,
           COALESCE(ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name) FILTER (WHERE ag.name IS NOT NULL), '{}'::text[])  AS agents_dispatched,
           COALESCE(ARRAY_AGG(DISTINCT (u.input ->> 'skill') ORDER BY (u.input ->> 'skill'))
                    FILTER (WHERE u.tool_name = 'Skill' AND (u.input ->> 'skill') IS NOT NULL), '{}'::text[])       AS skills_used,
           COALESCE(ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path) FILTER (WHERE u.file_path IS NOT NULL), '{}'::text[]) AS files_touched
    FROM requests.requests r
    LEFT JOIN tools.uses u ON u.request_id = r.id
    LEFT JOIN tools.results res ON res.request_id = r.id
    LEFT JOIN requests.diagnostics d ON d.request_id = r.id
    LEFT JOIN agents.dispatches ad ON ad.tool_use_id = u.id
    LEFT JOIN agents.agents ag ON ag.id = ad.agent_id
    WHERE r.operator_turn_id = ot.id
      AND ot.closed_at IS NULL
) la ON true
LEFT JOIN LATERAL (
    SELECT COALESCE(SUM(resp.input_tokens),  0) AS total_input_tokens,
           COALESCE(SUM(resp.output_tokens), 0) AS total_output_tokens
    FROM requests.requests r
    LEFT JOIN responses.responses resp ON resp.request_id = r.id
    WHERE r.operator_turn_id = ot.id
      AND ot.closed_at IS NULL
) lt ON true;

-- ── Backfill closed turns ─────────────────────────────────────────────
-- Set-based: GROUP skill names per turn, join onto the turn (id join key).
-- Closed turns with no Skill calls are normalised to '{}' below so the
-- stored column matches the at-close materialisation (which writes '{}').
UPDATE memory.operator_turns ot
   SET skills_used = s.skills
  FROM (
      SELECT r.operator_turn_id AS turn_id,
             ARRAY_AGG(DISTINCT (u.input ->> 'skill') ORDER BY (u.input ->> 'skill')) AS skills
      FROM tools.uses u
      JOIN requests.requests r ON r.id = u.request_id
      WHERE u.tool_name = 'Skill'
        AND (u.input ->> 'skill') IS NOT NULL
        AND r.operator_turn_id IS NOT NULL
      GROUP BY r.operator_turn_id
  ) s
 WHERE s.turn_id = ot.id
   AND ot.closed_at IS NOT NULL;

UPDATE memory.operator_turns
   SET skills_used = '{}'::text[]
 WHERE closed_at IS NOT NULL
   AND skills_used IS NULL;
