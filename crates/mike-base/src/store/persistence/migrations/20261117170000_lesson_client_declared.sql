-- memory.lesson_delivery_arms.declared_at: when the session's client (tether)
-- last declared, through POST /api/sessions/{id}/context with "lessons" in
-- `serves`, that it delivers this session's lessons. A declared session whose
-- client never reached POST /api/lessons/for-prompt is a client failure: the
-- session_lessons stage records session_lessons_client_unreachable and does
-- not inject. Startup rebuilds the declared set from this column.

ALTER TABLE memory.lesson_delivery_arms
    ADD COLUMN IF NOT EXISTS declared_at timestamptz;
