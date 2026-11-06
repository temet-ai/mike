-- Add inject_text column to rules.rules.
-- For lazy_section rules, inject_text overrides the re-injected content:
-- when trigger_token fires, the proxy injects inject_text instead of the
-- extracted section_text. This allows sub-agents with only a stub in their
-- system prompt to receive the full canonical section on !!CHROME!!.
-- NULL means fall back to extracting the section from the request body
-- (existing behaviour, preserved for all existing rules).
ALTER TABLE rules.rules ADD COLUMN IF NOT EXISTS inject_text TEXT;
