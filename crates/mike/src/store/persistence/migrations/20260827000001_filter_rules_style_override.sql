-- Add style_override to filter rules (issue #195).
-- false = the operator's response-style vocabulary check applies normally.
-- true = the operator has explicitly accepted a rule whose authored text
-- (text/inject_text) contains vocabulary banned by the operator's personal
-- writing-style rules, and that acceptance is durable: a later re-save of
-- the same rule does not need to re-supply the override.
ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS style_override BOOLEAN NOT NULL DEFAULT false;
