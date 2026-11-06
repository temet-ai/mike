-- The OpenRouter API key is resolved from the process environment
-- (PROXY_STEERER_OPENROUTER_API_KEY or OPENROUTER_API_KEY) and is never a
-- settings value. Earlier builds wrote the literal key into config.settings;
-- remove that row. The sibling row steerer.openrouter_api_key_env holds only an
-- environment variable name and stays.
DELETE FROM config.settings WHERE key = 'steerer.openrouter_api_key';
