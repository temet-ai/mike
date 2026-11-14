-- Wright conversation (temet-ai/mike#76, backend): the operator's conversation with Wright on its
-- page, the reply jobs that answer it, and the typed steers an answer applies.
--
-- wright.messages: one row per message. author 'operator' only through the Bearer-token route;
-- 'wright' only from a reply job. (author, client_msg_id) is unique, so a retried post stores one
-- message and a reply job's answer is stored once whatever the number of attempts.
-- needs_decision marks an answer the operator must act on; notice_sent_at records the one-line
-- notice sent to the operator's session for it, at most once.
-- wright.reply_jobs: one per operator message. 'running' carries a lease the worker renews; a job
-- whose lease lapsed (its process died) is claimed again after a restart. answer_id is set in the
-- same transaction as the answer, so a job is answered at most once.
-- wright.steers: one per answer that proposed a change from the closed set. prior is the
-- resource's state before the change, effect its state after; an undo compares the resource with
-- effect and refuses when it changed. refusal holds why a proposal outside the set or out of range
-- was not applied. version guards an undo against a stale page.

SET LOCAL lock_timeout = '10s';

CREATE TABLE wright.messages (
    id             bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author         text        NOT NULL CHECK (author IN ('operator', 'wright')),
    body           text        NOT NULL CHECK (btrim(body) <> ''),
    client_msg_id  text        NOT NULL CHECK (btrim(client_msg_id) <> ''),
    reply_to       bigint      REFERENCES wright.messages(id) ON DELETE SET NULL,
    links          jsonb       NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(links) = 'array'),
    needs_decision boolean     NOT NULL DEFAULT false,
    notice_sent_at timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT messages_one_per_client_msg UNIQUE (author, client_msg_id),
    CONSTRAINT messages_decision_is_wright CHECK (NOT needs_decision OR author = 'wright'),
    CONSTRAINT messages_notice_needs_decision CHECK (notice_sent_at IS NULL OR needs_decision)
);

CREATE INDEX idx_wright_messages_reply_to ON wright.messages (reply_to) WHERE reply_to IS NOT NULL;
CREATE INDEX idx_wright_messages_notice_due ON wright.messages (id)
    WHERE needs_decision AND notice_sent_at IS NULL;

CREATE TABLE wright.reply_jobs (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id  bigint      NOT NULL UNIQUE REFERENCES wright.messages(id) ON DELETE CASCADE,
    state       text        NOT NULL DEFAULT 'queued'
                    CHECK (state IN ('queued', 'running', 'answered', 'failed')),
    attempts    integer     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    not_before  timestamptz NOT NULL DEFAULT now(),
    lease_until timestamptz,
    answer_id   bigint      REFERENCES wright.messages(id) ON DELETE SET NULL,
    error       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT reply_jobs_running_has_lease CHECK ((state = 'running') = (lease_until IS NOT NULL)),
    CONSTRAINT reply_jobs_failed_has_error CHECK ((state = 'failed') = (error IS NOT NULL))
);

CREATE INDEX idx_wright_reply_jobs_due ON wright.reply_jobs (not_before, id)
    WHERE state IN ('queued', 'running');
CREATE INDEX idx_wright_reply_jobs_answer ON wright.reply_jobs (answer_id) WHERE answer_id IS NOT NULL;

CREATE TABLE wright.steers (
    id         bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id bigint      NOT NULL UNIQUE REFERENCES wright.messages(id) ON DELETE CASCADE,
    change     jsonb       NOT NULL CHECK (jsonb_typeof(change) = 'object'),
    prior      jsonb,
    effect     jsonb,
    refusal    text,
    applied_at timestamptz,
    undone_at  timestamptz,
    version    integer     NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT steers_refused_never_applied CHECK (refusal IS NULL OR applied_at IS NULL),
    CONSTRAINT steers_undone_after_applied CHECK (undone_at IS NULL OR applied_at IS NOT NULL),
    CONSTRAINT steers_applied_has_states CHECK (applied_at IS NULL OR (prior IS NOT NULL AND effect IS NOT NULL)),
    CONSTRAINT steers_pending_has_prior CHECK (refusal IS NOT NULL OR prior IS NOT NULL)
);

CREATE INDEX idx_wright_steers_pending ON wright.steers (id)
    WHERE applied_at IS NULL AND refusal IS NULL;
CREATE INDEX idx_wright_steers_live_kind ON wright.steers ((change->>'kind'))
    WHERE applied_at IS NOT NULL AND undone_at IS NULL;
