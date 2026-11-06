-- Seed the steer-author agent and its initial composition.
--
-- The steer-author is a dedicated prompt-smith agent that AUTHORS steering
-- anchors (behavioral constraints) from plain-language descriptions via
-- POST /api/steerers/assist. Its system prompt lives here so it can be
-- improved over time via the composition editor without a recompile.
--
-- Resolution: resolve_steer_author_system_prompt() reads blocks[2].text of
-- the newest composition linked to the 'steer-author' agent, and falls back
-- to the Phase-A inline literal when absent.
--
-- Schema reuse: same pattern as 20260615000001_steerer_roster_seed.sql.
-- No new table — reuse agents.agents + overrides.overrides per the steering
-- schema decision (20260613000002_steering_schema.sql:53-54).
--
-- Prerequisites guaranteed by prior migrations:
--   agents.agents (20260604000001_baseline) — agent_id FK target
--   overrides.overrides (20260604000001_baseline) — composition store
--   system_prompts.system_prompts (20260604000001_baseline) — system_prompt_id FK
--   pgcrypto (20260604000001_baseline CREATE EXTENSION) — encode(digest(...))

-- ─── steer-author agent row ───────────────────────────────────────────────

INSERT INTO agents.agents (name, description)
SELECT 'steer-author',
       'Steer-author prompt-smith — authors effective steering anchors (behavioral constraints) ' ||
       'from plain-language operator descriptions. Backs POST /api/steerers/assist.'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'steer-author');

-- ─── steer-author composition (blocks[2].text = the Phase-A tuned prompt) ─
--
-- blocks is a JSON array of three elements matching the composition block
-- convention used by resolve_verdict_system_prompt (refocus.rs:39-47):
--   blocks[0] — header / label text
--   blocks[1] — preamble / identity text
--   blocks[2] — the system prompt body (the text resolve_steer_author_system_prompt reads)
--
-- system_prompt_id FK: overrides.overrides.system_prompt_id is NOT NULL.
-- We insert a system_prompts row for the composed prompt text and carry its
-- surrogate id forward via RETURNING so the FK is satisfied.  prompt_sha256 is
-- computed via pgcrypto so it stays in sync with the stored text — no
-- hard-coded hex that can drift.  The sha is NOT unique (plain index only);
-- the surrogate id is the key, so there is no ON CONFLICT here.

DO $$
DECLARE
    v_agent_id     bigint;
    v_prompt_text  text;
    v_sha256       text;
    v_sp_id        bigint;
    v_mode_id      bigint;
    v_blocks       jsonb;
BEGIN
    -- Resolve agent surrogate id.
    SELECT id INTO v_agent_id
      FROM agents.agents
     WHERE name = 'steer-author'
     ORDER BY updated_at DESC
     LIMIT 1;

    IF v_agent_id IS NULL THEN
        RAISE EXCEPTION 'steer-author agent row missing — INSERT above must have failed';
    END IF;

    -- Already seeded? Idempotency guard.
    IF EXISTS (SELECT 1 FROM overrides.overrides WHERE agent_id = v_agent_id) THEN
        RAISE NOTICE 'steer-author composition already seeded — skipping';
        RETURN;
    END IF;

    -- The steer-author system prompt (Phase-A inline literal).
    -- KEEP IN SYNC with the fallback in resolve_steer_author_system_prompt().
    -- NOTE: concatenation is EXPLICIT (||). PostgreSQL's implicit
    -- adjacent-literal concatenation only joins two PLAIN quoted strings;
    -- an E'...' escape-string literal anywhere in the chain breaks the lexer
    -- ("syntax error at or near E'...'"). The || operator joins plain and
    -- E-strings alike, so the E'\n\n' paragraph separators are safe here.
    v_prompt_text :=
        'You are a steer-author: a prompt-smith that writes steering anchors '
        || 'for a runtime session steerer. A runtime steerer watches a live Claude Code session '
        || 'and injects a correction when a turn violates its anchor. '
        || 'You do NOT talk to the session and you do NOT continue its conversation. '
        || E'\n\n'
        || 'Your ONLY output is an effective steering anchor: a directive, in the imperative, '
        || 'stating what the session must be held to and what behaviors it must not exhibit. '
        || 'Write it as a rule the steerer evaluates a turn AGAINST — never as a reply TO a turn, '
        || 'and never as a description of what the session is doing. '
        || E'\n\n'
        || 'The anchor must forbid the failure behaviors this steer targets: '
        || 'do not dismiss work as pre-existing or out-of-scope; do not shift ownership of a failure; '
        || 'do not claim success without a working, verified deliverable; finish what was actually requested. '
        || E'\n\n'
        || 'Good anchor examples: '
        || '''Hold the session to the originally requested deliverable. The session MUST NOT dismiss '
        || 'unresolved errors as pre-existing or out of scope; every error raised must be addressed '
        || 'before any completion claim.'' '
        || '''The session MUST NOT claim a deliverable works unless tests were run and their output '
        || 'appears in the same turn. Premature success claims are a violation.'' '
        || E'\n\n'
        || 'Bad output examples (NEVER produce these — they are conversation, not anchors): '
        || '''The session is working on implementing the steerer fleet.'' '
        || '''You are right that a RIGHT JOIN would be better here.'' '
        || E'\n\n'
        || 'When diagnostic (weasel) findings are provided, the anchor MUST name each detected '
        || 'behavior explicitly so the steerer recognizes the recurrence. '
        || E'\n\n'
        || 'Return a JSON object with fields: suggestion (prompt, authority, mode, config) and '
        || 'rationale (one sentence per field explaining the recommendation). '
        || 'suggestion.prompt is the steering anchor the runtime steerer will enforce. '
        || 'authority must be one of: supply_context, redirect, command. '
        || 'mode must be one of: augment, drive. '
        || 'config.confidence_threshold should be 0.7-0.95 for behavioral violations (they are specific). '
        || 'config.debounce_turns should be 1-3 (re-check frequently when behavior violations are detected).';

    -- Compute SHA-256 via pgcrypto (always available — CREATE EXTENSION in baseline).
    v_sha256 := encode(digest(v_prompt_text, 'sha256'), 'hex');

    -- Build the blocks array: [header, preamble, system-prompt-body].
    -- blocks[2].text is what resolve_steer_author_system_prompt() reads.
    v_blocks := jsonb_build_array(
        jsonb_build_object('text', 'steer-author'),
        jsonb_build_object('text', 'Prompt-smith for steering anchor authoring'),
        jsonb_build_object('text', v_prompt_text)
    );

    -- Insert the system_prompts row for the FK and carry its surrogate id
    -- forward via RETURNING — no ON CONFLICT, no re-resolution by sha.
    -- prompt_sha256 is deliberately NON-unique post-squash (plain btree index
    -- only); the surrogate id is the key. The DO-block idempotency guard above
    -- (no overrides row for this agent) means we only reach here on a genuine
    -- first seed, so a fresh insert is correct.
    INSERT INTO system_prompts.system_prompts
        (prompt_sha256, blocks, first_seen_at, last_seen_at)
    VALUES (v_sha256, v_blocks, NOW(), NOW())
    RETURNING id INTO v_sp_id;

    -- Resolve mode_id for 'observe' (the safe default for seeded compositions).
    SELECT id INTO v_mode_id
      FROM overrides.modes
     WHERE code = 'observe'
     LIMIT 1;

    -- Insert the composition row.
    INSERT INTO overrides.overrides
        (prompt_sha256, label, blocks, mode_id,
         agent_name, agent_definition_path, agent_id, system_prompt_id)
    VALUES
        (v_sha256,
         'steer-author initial prompt (Phase A)',
         v_blocks,
         v_mode_id,
         'steer-author',
         NULL,
         v_agent_id,
         v_sp_id);

    -- Record initial revision.
    INSERT INTO overrides.revisions
        (override_id, version, label, blocks, mode_id, notes)
    SELECT o.id, o.version, o.label, o.blocks, o.mode_id, NULL
      FROM overrides.overrides o
     WHERE o.agent_id = v_agent_id
     ORDER BY o.created_at DESC
     LIMIT 1;

    RAISE NOTICE 'steer-author agent (id=%) composition seeded (sha256=%..) ', v_agent_id, left(v_sha256, 16);
END;
$$;
