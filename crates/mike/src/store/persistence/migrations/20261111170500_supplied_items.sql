-- What memory mike supplied with each request, one row per item, and the
-- overnight verdict on whether the session used it (issue #164).
--
-- memory.keeper_supplies recorded only the first supply of a card per session
-- and nothing about the request that carried it. It now records every supply
-- (the writer drops its NOT EXISTS) and keeps the marker hash, the score and the
-- chance of the supply, so the capture path can join a card found in a request
-- body back to the supply that produced it.
--
-- memory.supplied_items: one row per lesson or work-thread card carried by a
-- request, written after the request id is minted. A lesson or card a client
-- (the mike mod or tether) added is in every later request of the session, so
-- it is recorded once per session, on the first request that carried it; a
-- proxy-added item is in the forwarded body of one request only and is recorded
-- on each. route says who added it. score is what chose it (the lesson ranker's
-- relevance, the thread resolver's score); chance is the probability that a
-- request like this one is given the item: 1 for a lesson (the ranker is
-- deterministic and there is no lesson holdout), the keeper arm's share for a
-- card (the 20% control group gets none).
--
-- memory.supplied_item_verdicts: one overnight judge verdict per item, and the
-- hand check of a weekly sample. An item judged ignored is a cost: bytes sent
-- for nothing.

ALTER TABLE memory.keeper_supplies
    ADD COLUMN IF NOT EXISTS block_hash text NULL,
    ADD COLUMN IF NOT EXISTS score double precision NULL,
    ADD COLUMN IF NOT EXISTS chance double precision NULL,
    ADD COLUMN IF NOT EXISTS card_text text NULL;

COMMENT ON COLUMN memory.keeper_supplies.block_hash IS
    'The [keeper:<hash>] marker of the block supplied; how the capture path finds the supply a request carried. Not indexed.';
COMMENT ON COLUMN memory.keeper_supplies.score IS
    'The resolver score of the best thread supplied; NULL when nothing was supplied.';
COMMENT ON COLUMN memory.keeper_supplies.chance IS
    'Probability that a session like this one is given a card: the keeper arm share, the same for both arms.';
COMMENT ON COLUMN memory.keeper_supplies.card_text IS
    'The whole block text supplied, for the overnight verdict. NULL when no block was supplied.';

CREATE TABLE IF NOT EXISTS memory.supplied_items (
    id            bigint           GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    session_id    bigint           NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    request_id    bigint           NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    item_kind     text             NOT NULL,
    lesson_id     bigint           NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    thread_id     bigint           NULL REFERENCES work.threads(id) ON DELETE SET NULL,
    card_hash     text             NULL,
    route         text             NOT NULL,
    score         double precision NULL,
    chance        double precision NULL,
    item_text     text             NOT NULL,
    supplied_at   timestamptz      NOT NULL DEFAULT now(),
    CONSTRAINT supplied_items_kind_check CHECK (item_kind IN ('lesson', 'card')),
    CONSTRAINT supplied_items_route_check CHECK (route IN ('proxy', 'mod', 'tether')),
    CONSTRAINT supplied_items_lesson_shape CHECK ((item_kind = 'lesson') = (lesson_id IS NOT NULL)),
    CONSTRAINT supplied_items_card_shape CHECK ((item_kind = 'card') = (card_hash IS NOT NULL)),
    CONSTRAINT supplied_items_chance_range CHECK (chance IS NULL OR (chance >= 0 AND chance <= 1))
);

CREATE INDEX IF NOT EXISTS idx_supplied_items_request
    ON memory.supplied_items (request_id);
CREATE INDEX IF NOT EXISTS idx_supplied_items_session_supplied
    ON memory.supplied_items (session_id, supplied_at);
CREATE INDEX IF NOT EXISTS idx_supplied_items_lesson
    ON memory.supplied_items (lesson_id) WHERE lesson_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_supplied_items_thread
    ON memory.supplied_items (thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_supplied_items_supplied_at
    ON memory.supplied_items (supplied_at);

COMMENT ON TABLE memory.supplied_items IS
    'One row per lesson or work-thread card supplied with a request: route, the score that chose it, its chance of being supplied, and the text supplied.';
COMMENT ON COLUMN memory.supplied_items.route IS
    'proxy: mike added it to the forwarded body. mod: the mike mod added it as hook context. tether: tether added it.';
COMMENT ON COLUMN memory.supplied_items.score IS
    'Lesson: the ranker relevance that chose it. Card: the resolver score of its thread. NULL when the process that supplied it did not record one.';
COMMENT ON COLUMN memory.supplied_items.chance IS
    'Probability that a request like this one is given the item: 1 for a lesson, the keeper arm share for a card.';
COMMENT ON COLUMN memory.supplied_items.item_text IS
    'The text supplied, whole: the lesson text or the card block.';

CREATE TABLE IF NOT EXISTS memory.supplied_item_verdicts (
    supplied_item_id bigint      PRIMARY KEY REFERENCES memory.supplied_items(id) ON DELETE CASCADE,
    verdict          text        NOT NULL,
    rationale        text        NOT NULL,
    evidence         text        NOT NULL DEFAULT '',
    model            text        NOT NULL,
    input_text       text        NOT NULL,
    judged_at        timestamptz NOT NULL DEFAULT now(),
    hand_verdict     text        NULL,
    hand_checked_by  text        NULL,
    hand_checked_at  timestamptz NULL,
    CONSTRAINT supplied_item_verdicts_verdict_check
        CHECK (verdict IN ('used', 'corrected_anyway', 'ignored')),
    CONSTRAINT supplied_item_verdicts_hand_check
        CHECK (hand_verdict IS NULL OR hand_verdict IN ('used', 'corrected_anyway', 'ignored')),
    CONSTRAINT supplied_item_verdicts_hand_consistent
        CHECK ((hand_verdict IS NULL) = (hand_checked_at IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_supplied_item_verdicts_judged_at
    ON memory.supplied_item_verdicts (judged_at);

COMMENT ON TABLE memory.supplied_item_verdicts IS
    'The overnight judge verdict on one supplied item (used, corrected_anyway, ignored), with the exact input sent, and the hand check of a weekly sample.';
COMMENT ON COLUMN memory.supplied_item_verdicts.verdict IS
    'used: the session acted on it. corrected_anyway: the operator corrected the turn on what the item covered. ignored: no sign it was used; a cost.';
COMMENT ON COLUMN memory.supplied_item_verdicts.hand_verdict IS
    'A person''s verdict on the same item, for the weekly agreement rate. NULL until checked.';
