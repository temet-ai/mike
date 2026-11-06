CREATE TABLE agents.work_items (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id          bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    wire_tool_use_id    text NOT NULL,
    kind                text NOT NULL CHECK (kind IN ('bash', 'agent')),
    task_id             text NOT NULL,
    output_path         text,
    owner_agent_id      text,
    description         text,
    launched_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    launched_at         timestamptz NOT NULL,
    state               text NOT NULL DEFAULT 'running'
                        CHECK (state IN ('running', 'completed', 'failed', 'killed', 'unknown')),
    finished_at         timestamptz,
    finished_request_id bigint REFERENCES requests.requests(id) ON DELETE SET NULL,
    notified_agent_id   text,
    exit_code           integer,
    summary             text,
    CONSTRAINT uq_work_items_wire_tool_use_id UNIQUE (wire_tool_use_id)
);

CREATE INDEX idx_work_items_session_launched ON agents.work_items (session_id, launched_at);
CREATE INDEX idx_work_items_task_id ON agents.work_items (task_id);
CREATE INDEX idx_work_items_running ON agents.work_items (session_id) WHERE state = 'running';
