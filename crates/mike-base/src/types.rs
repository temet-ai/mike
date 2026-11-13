use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

impl SortOrder {
    pub fn sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    pub fn sql_reversed(self) -> &'static str {
        match self {
            Self::Asc => "DESC",
            Self::Desc => "ASC",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub struct RequestParams {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub thinking_enabled: bool,
    pub thinking_budget: Option<u32>,
    pub thinking_mode: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub struct ToolCall {
    pub tool_id: String,
    pub name: String,
    pub input_json: String,
    pub complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseError;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn token_usage_json_roundtrip(usage: TokenUsage) {
            let encoded = serde_json::to_string(&usage)
                .map_err(|e| TestCaseError::fail(format!("serialize: {e}")))?;
            let decoded: TokenUsageDto = serde_json::from_str(&encoded)
                .map_err(|e| TestCaseError::fail(format!("deserialize: {e}")))?;
            prop_assert_eq!(usage.input_tokens, decoded.input_tokens);
            prop_assert_eq!(usage.output_tokens, decoded.output_tokens);
            prop_assert_eq!(
                usage.cache_creation_input_tokens,
                decoded.cache_creation_input_tokens
            );
            prop_assert_eq!(
                usage.cache_read_input_tokens,
                decoded.cache_read_input_tokens
            );
        }
    }

    #[derive(serde::Deserialize)]
    struct TokenUsageDto {
        input_tokens: u32,
        output_tokens: u32,
        cache_creation_input_tokens: Option<u32>,
        cache_read_input_tokens: Option<u32>,
    }
}
