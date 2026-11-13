-- ════════════════════════════════════════════════════════════════════
-- Issue #25 (R3) — resolve subject_agent from the turn's sub-agent dispatch
--
-- subject_agent (migration 20260625000003) was set at turn-OPEN from the
-- opening request's subagent_persona, defaulting to 'main'. Operator turns
-- open only on primary origins, so that default is 'main' for ~every turn —
-- a near-constant column. But a turn's real "subject agent" is knowable at
-- turn-CLOSE: if the turn delegated to exactly one distinct sub-agent (via
-- the Agent/Task tool, resolved through agents.dispatches -> agents.agents),
-- that persona IS the agent that did the work.
--
-- This backfills closed turns: when a turn dispatched exactly ONE distinct
-- agent, surface that persona as subject_agent (overwriting only the 'main'
-- default, so any start-request persona is preserved). Turns that ran direct
-- or orchestrated several agents keep 'main' — the full set remains in
-- agents_dispatched[]. The capture path applies the same rule at close
-- (store/persistence/mod.rs materialize_turn_aggregates).
--
-- Set-based: GROUP dispatched agents per turn, join onto the turn (id join
-- key). Not a per-row correlated subquery.
-- ════════════════════════════════════════════════════════════════════

UPDATE memory.operator_turns ot
   SET subject_agent = s.agent
  FROM (
      SELECT r.operator_turn_id AS turn_id,
             CASE WHEN COUNT(DISTINCT ag.name) = 1 THEN MIN(ag.name) END AS agent
      FROM agents.dispatches ad
      JOIN tools.uses u        ON u.id = ad.tool_use_id
      JOIN requests.requests r ON r.id = u.request_id
      JOIN agents.agents ag    ON ag.id = ad.agent_id
      WHERE r.operator_turn_id IS NOT NULL
      GROUP BY r.operator_turn_id
  ) s
 WHERE s.turn_id = ot.id
   AND ot.closed_at IS NOT NULL
   AND s.agent IS NOT NULL
   AND ot.subject_agent = 'main';
