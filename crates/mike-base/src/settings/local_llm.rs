pub const LOCAL_URL_KEY: &str = "llm.local.url";
pub const LOCAL_KEY_ENV_KEY: &str = "llm.local.api_key_env";
pub const DEFAULT_LOCAL_KEY_ENV: &str = "MIKE_LOCAL_LLM_API_KEY";
pub const SIDE_CALL_MODELS_KEY: &str = "side_calls.models";
pub const DEFAULT_SIDE_CALL_MODELS: [&str; 3] = [
    "local/gemma-4-12b",
    "openrouter/google/gemini-3.5-flash-lite",
    "claude/haiku",
];

pub fn default_side_call_models() -> Vec<String> {
    DEFAULT_SIDE_CALL_MODELS
        .iter()
        .map(|m| (*m).to_owned())
        .collect()
}
